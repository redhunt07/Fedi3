/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use fedi3_protocol::{RelayHttpRequest, RelayHttpResponse};
use libp2p::{
    core::upgrade,
    identify, identity,
    kad,
    noise,
    ping,
    quic,
    relay,
    request_response,
    swarm::{derive_prelude::*, SwarmEvent},
    core::muxing::StreamMuxerBox,
    tcp, websocket, yamux, Multiaddr, PeerId, Swarm, Transport,
};
use libp2p::futures::{future::Either, StreamExt};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::Duration,
};
use tokio::sync::Mutex;
use tracing::{info, warn};

mod mailbox;
use mailbox::{MailboxMessage, MailboxStore};

#[derive(Debug, Clone)]
struct Config {
    listen: Vec<Multiaddr>,
    enable_relay_server: bool,
    enable_kad_server: bool,
    bootstrap: Vec<Multiaddr>,
    key_path: PathBuf,
    mailbox_db: PathBuf,
    mailbox_max_per_peer: u32,
    mailbox_max_bytes_per_peer: u64,
    mailbox_max_ttl_secs: u64,
    mailbox_max_puts_per_min: u32,
    mailbox_max_put_bytes_per_min: u64,
    mailbox_max_body_bytes: usize,
    peer_id_file: PathBuf,
}

fn load_config() -> Result<Config> {
    let listen = std::env::var("FEDI3_P2P_LISTEN")
        .unwrap_or_else(|_| "/ip4/0.0.0.0/tcp/4001,/ip4/0.0.0.0/udp/4001/quic-v1".to_string());
    let listen = listen
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<Multiaddr>().context("parse listen addr"))
        .collect::<Result<Vec<_>>>()?;

    let bootstrap = std::env::var("FEDI3_P2P_BOOTSTRAP").unwrap_or_default();
    let bootstrap = bootstrap
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<Multiaddr>().ok())
        .collect::<Vec<_>>();

    let enable_relay_server = std::env::var("FEDI3_P2P_ENABLE_RELAY_SERVER")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(true);

    let enable_kad_server = std::env::var("FEDI3_P2P_ENABLE_KAD_SERVER")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(true);

    let key_path = std::env::var("FEDI3_P2P_KEY")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("fedi3_p2p_infra_keypair.pb"));

    let mailbox_db = std::env::var("FEDI3_P2P_MAILBOX_DB")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("fedi3_p2p_mailbox.sqlite"));

    let mailbox_max_per_peer = std::env::var("FEDI3_P2P_MAILBOX_MAX_PER_PEER")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(1000);

    let mailbox_max_bytes_per_peer = std::env::var("FEDI3_P2P_MAILBOX_MAX_BYTES_PER_PEER")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10 * 1024 * 1024);

    let mailbox_max_ttl_secs = std::env::var("FEDI3_P2P_MAILBOX_MAX_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(7 * 24 * 3600);

    let mailbox_max_puts_per_min = std::env::var("FEDI3_P2P_MAILBOX_MAX_PUTS_PER_MIN")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(120);

    let mailbox_max_put_bytes_per_min = std::env::var("FEDI3_P2P_MAILBOX_MAX_PUT_BYTES_PER_MIN")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(2 * 1024 * 1024);

    let mailbox_max_body_bytes = std::env::var("FEDI3_P2P_MAILBOX_MAX_BODY_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(512 * 1024);

    let peer_id_file = std::env::var("FEDI3_P2P_PEER_ID_FILE")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/data/fedi3_p2p_peer_id"));

    Ok(Config {
        listen,
        enable_relay_server,
        enable_kad_server,
        bootstrap,
        key_path,
        mailbox_db,
        mailbox_max_per_peer,
        mailbox_max_bytes_per_peer,
        mailbox_max_ttl_secs,
        mailbox_max_puts_per_min,
        mailbox_max_put_bytes_per_min,
        mailbox_max_body_bytes,
        peer_id_file,
    })
}

fn load_or_generate_keypair(path: &PathBuf) -> Result<identity::Keypair> {
    if let Ok(bytes) = std::fs::read(path) {
        return identity::Keypair::from_protobuf_encoding(&bytes).context("decode keypair");
    }
    let kp = identity::Keypair::generate_ed25519();
    let bytes = kp.to_protobuf_encoding().context("encode keypair")?;
    std::fs::write(path, bytes).context("write keypair")?;
    Ok(kp)
}

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "BehaviourEvent")]
struct Behaviour {
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    kad: kad::Behaviour<kad::store::MemoryStore>,
    relay: relay::Behaviour,
    rr: request_response::Behaviour<Fedi3Codec>,
}

#[derive(Debug)]
enum BehaviourEvent {
    Identify(()),
    Ping(()),
    Kad(()),
    Relay(relay::Event),
    Rr(request_response::Event<RelayHttpRequest, RelayHttpResponse>),
}

impl From<identify::Event> for BehaviourEvent {
    fn from(v: identify::Event) -> Self {
        let _ = v;
        Self::Identify(())
    }
}
impl From<ping::Event> for BehaviourEvent {
    fn from(v: ping::Event) -> Self {
        let _ = v;
        Self::Ping(())
    }
}
impl From<kad::Event> for BehaviourEvent {
    fn from(v: kad::Event) -> Self {
        let _ = v;
        Self::Kad(())
    }
}
impl From<relay::Event> for BehaviourEvent {
    fn from(v: relay::Event) -> Self {
        Self::Relay(v)
    }
}

impl From<request_response::Event<RelayHttpRequest, RelayHttpResponse>> for BehaviourEvent {
    fn from(v: request_response::Event<RelayHttpRequest, RelayHttpResponse>) -> Self {
        Self::Rr(v)
    }
}

#[derive(Clone)]
struct Fedi3Protocol;

impl AsRef<str> for Fedi3Protocol {
    fn as_ref(&self) -> &str {
        "/fedi3/relay-http/1"
    }
}

#[derive(Clone, Default)]
struct Fedi3Codec;

#[async_trait]
impl request_response::Codec for Fedi3Codec {
    type Protocol = Fedi3Protocol;
    type Request = RelayHttpRequest;
    type Response = RelayHttpResponse;

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> std::io::Result<Self::Request>
    where
        T: futures_util::AsyncRead + Unpin + Send,
    {
        read_len_prefixed_json(io).await
    }

    async fn read_response<T>(&mut self, _: &Self::Protocol, io: &mut T) -> std::io::Result<Self::Response>
    where
        T: futures_util::AsyncRead + Unpin + Send,
    {
        read_len_prefixed_json(io).await
    }

    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> std::io::Result<()>
    where
        T: futures_util::AsyncWrite + Unpin + Send,
    {
        write_len_prefixed_json(io, &req).await
    }

    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        resp: Self::Response,
    ) -> std::io::Result<()>
    where
        T: futures_util::AsyncWrite + Unpin + Send,
    {
        write_len_prefixed_json(io, &resp).await
    }
}

async fn read_len_prefixed_json<T, V>(io: &mut T) -> std::io::Result<V>
where
    T: futures_util::AsyncRead + Unpin + Send,
    V: serde::de::DeserializeOwned,
{
    use futures_util::AsyncReadExt as _;
    let mut len_buf = [0u8; 4];
    io.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    io.read_exact(&mut buf).await?;
    serde_json::from_slice(&buf).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

async fn write_len_prefixed_json<T, V>(io: &mut T, value: &V) -> std::io::Result<()>
where
    T: futures_util::AsyncWrite + Unpin + Send,
    V: serde::Serialize,
{
    use futures_util::AsyncWriteExt as _;
    let bytes = serde_json::to_vec(value).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let len = (bytes.len() as u32).to_be_bytes();
    io.write_all(&len).await?;
    io.write_all(&bytes).await?;
    io.flush().await?;
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
struct MailboxPutReq {
    to_peer_id: String,
    req: RelayHttpRequest,
    ttl_secs: Option<u64>,
}

#[derive(Debug, serde::Deserialize)]
struct MailboxPollReq {
    for_peer_id: String,
    limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize)]
struct MailboxAckReq {
    for_peer_id: String,
    ids: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
struct MailboxPollResp {
    messages: Vec<MailboxMessage>,
}

#[derive(Debug, serde::Serialize)]
struct MailboxPutResp {
    ok: bool,
    id: String,
}

#[derive(Debug, serde::Serialize)]
struct MailboxAckResp {
    deleted: u64,
}

#[derive(Debug, Default)]
struct RateState {
    window_start_ms: i64,
    puts: u32,
    bytes: u64,
}

#[derive(Debug)]
struct RateLimiter {
    max_puts_per_min: u32,
    max_put_bytes_per_min: u64,
    inner: Mutex<HashMap<String, RateState>>,
}

impl RateLimiter {
    fn new(max_puts_per_min: u32, max_put_bytes_per_min: u64) -> Self {
        Self {
            max_puts_per_min: max_puts_per_min.max(1),
            max_put_bytes_per_min: max_put_bytes_per_min.max(1024),
            inner: Mutex::new(HashMap::new()),
        }
    }

    async fn allow_put(&self, peer_id: &str, bytes: u64) -> bool {
        let now = now_ms();
        let mut guard = self.inner.lock().await;
        let st = guard.entry(peer_id.to_string()).or_default();
        if now.saturating_sub(st.window_start_ms) >= 60_000 {
            st.window_start_ms = now;
            st.puts = 0;
            st.bytes = 0;
        }
        if st.puts.saturating_add(1) > self.max_puts_per_min {
            return false;
        }
        if st.bytes.saturating_add(bytes) > self.max_put_bytes_per_min {
            return false;
        }
        st.puts = st.puts.saturating_add(1);
        st.bytes = st.bytes.saturating_add(bytes);
        true
    }
}

async fn handle_mailbox_request(
    store: &MailboxStore,
    limiter: &RateLimiter,
    mailbox_max_body_bytes: usize,
    from_peer: &PeerId,
    req: RelayHttpRequest,
) -> RelayHttpResponse {
    let resp_headers = vec![("content-type".to_string(), "application/json".to_string())];

    let bad = |id: String, status: u16, msg: String| RelayHttpResponse {
        id,
        status,
        headers: resp_headers.clone(),
        body_b64: B64.encode(format!(r#"{{"error":"{}"}}"#, msg.replace('"', "\\\""))),
    };

    if req.method.to_ascii_uppercase() != "POST" {
        return bad(req.id, 405, "method not allowed".to_string());
    }

    let body = match B64.decode(req.body_b64.as_bytes()) {
        Ok(b) => b,
        Err(_) => return bad(req.id, 400, "invalid body".to_string()),
    };
    if body.len() > mailbox_max_body_bytes.max(8 * 1024) {
        return bad(req.id, 413, "body too large".to_string());
    }

    let from_peer_id = from_peer.to_string();
    match req.path.as_str() {
        "/.fedi3/mailbox/put" => {
            let put: MailboxPutReq = match serde_json::from_slice(&body) {
                Ok(v) => v,
                Err(_) => return bad(req.id, 400, "bad json".to_string()),
            };
            if put.to_peer_id.trim().is_empty() || put.to_peer_id.len() > 128 {
                return bad(req.id, 400, "invalid to_peer_id".to_string());
            }
            let msg_id = if put.req.id.trim().is_empty() {
                format!("mbx-{}", random_id())
            } else {
                put.req.id.clone()
            };
            let approx_size = serde_json::to_vec(&put.req).map(|v| v.len() as u64).unwrap_or(0);
            if !limiter.allow_put(&from_peer_id, approx_size).await {
                return bad(req.id, 429, "rate limited".to_string());
            }
            let ttl = put.ttl_secs.unwrap_or(7 * 24 * 3600);
            if let Err(e) = store
                .put(&from_peer_id, &put.to_peer_id, &msg_id, &put.req, ttl)
                .await
            {
                return bad(req.id, 502, format!("store failed: {e:#}"));
            }
            let out = MailboxPutResp { ok: true, id: msg_id };
            let json = serde_json::to_vec(&out).unwrap_or_default();
            RelayHttpResponse {
                id: req.id,
                status: 200,
                headers: resp_headers,
                body_b64: B64.encode(json),
            }
        }
        "/.fedi3/mailbox/poll" => {
            let poll: MailboxPollReq = match serde_json::from_slice(&body) {
                Ok(v) => v,
                Err(_) => return bad(req.id, 400, "bad json".to_string()),
            };
            if poll.for_peer_id != from_peer_id {
                return bad(req.id, 403, "peer mismatch".to_string());
            }
            let limit = poll.limit.unwrap_or(50);
            let msgs = match store.poll(&from_peer_id, limit).await {
                Ok(v) => v,
                Err(e) => return bad(req.id, 502, format!("poll failed: {e:#}")),
            };
            let out = MailboxPollResp { messages: msgs };
            let json = serde_json::to_vec(&out).unwrap_or_default();
            RelayHttpResponse {
                id: req.id,
                status: 200,
                headers: resp_headers,
                body_b64: B64.encode(json),
            }
        }
        "/.fedi3/mailbox/ack" => {
            let ack: MailboxAckReq = match serde_json::from_slice(&body) {
                Ok(v) => v,
                Err(_) => return bad(req.id, 400, "bad json".to_string()),
            };
            if ack.for_peer_id != from_peer_id {
                return bad(req.id, 403, "peer mismatch".to_string());
            }
            let deleted = match store.ack(&from_peer_id, &ack.ids).await {
                Ok(v) => v,
                Err(e) => return bad(req.id, 502, format!("ack failed: {e:#}")),
            };
            let out = MailboxAckResp { deleted };
            let json = serde_json::to_vec(&out).unwrap_or_default();
            RelayHttpResponse {
                id: req.id,
                status: 200,
                headers: resp_headers,
                body_b64: B64.encode(json),
            }
        }
        _ => bad(req.id, 404, "not found".to_string()),
    }
}

fn random_id() -> String {
    let mut b = [0u8; 16];
    use rand::RngCore as _;
    rand::rngs::OsRng.fill_bytes(&mut b);
    b.iter().map(|v| format!("{v:02x}")).collect()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    let cfg = load_config()?;
    let keypair = load_or_generate_keypair(&cfg.key_path)?;
    let peer_id = PeerId::from(keypair.public());

    info!(%peer_id, key=%cfg.key_path.display(), "fedi3_p2p_infra starting");
    if let Some(parent) = cfg.peer_id_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&cfg.peer_id_file, peer_id.to_string()) {
        warn!(path=%cfg.peer_id_file.display(), "peer_id write failed: {e}");
    } else {
        info!(path=%cfg.peer_id_file.display(), "peer_id written");
    }
    info!(
        mailbox_db=%cfg.mailbox_db.display(),
        max_per_peer=%cfg.mailbox_max_per_peer,
        max_bytes_per_peer=%cfg.mailbox_max_bytes_per_peer,
        max_ttl_secs=%cfg.mailbox_max_ttl_secs,
        "mailbox enabled"
    );

    let mailbox = MailboxStore::open(
        &cfg.mailbox_db,
        cfg.mailbox_max_per_peer,
        cfg.mailbox_max_bytes_per_peer,
        cfg.mailbox_max_ttl_secs,
    )?;
    let limiter = RateLimiter::new(cfg.mailbox_max_puts_per_min, cfg.mailbox_max_put_bytes_per_min);

    let tcp_transport = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true))
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(&keypair)?)
        .multiplex(yamux::Config::default())
        .timeout(Duration::from_secs(20))
        .boxed();
    let ws_transport = websocket::WsConfig::new(tcp::tokio::Transport::new(tcp::Config::default().nodelay(true)))
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(&keypair)?)
        .multiplex(yamux::Config::default())
        .timeout(Duration::from_secs(20))
        .boxed();
    let quic_transport = quic::tokio::Transport::new(quic::Config::new(&keypair));
    let tcp_or_ws = libp2p::core::transport::choice::OrTransport::new(ws_transport, tcp_transport);
    let transport = libp2p::core::transport::choice::OrTransport::new(quic_transport, tcp_or_ws)
        .map(|either, _| match either {
            Either::Left((peer, conn)) => (peer, StreamMuxerBox::new(conn)),
            Either::Right(inner) => match inner {
                Either::Left((peer, muxer)) => (peer, muxer),
                Either::Right((peer, muxer)) => (peer, muxer),
            },
        })
        .boxed();

    let identify = identify::Behaviour::new(identify::Config::new(
        "/fedi3/infra-identify/1".to_string(),
        keypair.public(),
    ));
    let ping = ping::Behaviour::new(ping::Config::new());

    let mut kad_behaviour = {
        let store = kad::store::MemoryStore::new(peer_id);
        let mut k = kad::Behaviour::new(peer_id, store);
        if cfg.enable_kad_server {
            k.set_mode(Some(kad::Mode::Server));
        }
        k
    };

    // Seed bootstrap nodes if provided.
    let mut bootstrap_peers: HashSet<PeerId> = HashSet::new();
    for addr in &cfg.bootstrap {
        let mut pid: Option<PeerId> = None;
        for p in addr.iter() {
            if let libp2p::multiaddr::Protocol::P2p(h) = p {
                pid = Some(h);
            }
        }
        if let Some(p) = pid {
            bootstrap_peers.insert(p);
            kad_behaviour.add_address(&p, addr.clone());
        }
    }

    let relay_cfg = relay::Config {
        // Keep defaults; infra nodes can tune via env later.
        ..Default::default()
    };
    let relay_behaviour = relay::Behaviour::new(peer_id, relay_cfg);

    let rr = request_response::Behaviour::new(
        [(Fedi3Protocol, request_response::ProtocolSupport::Full)],
        request_response::Config::default(),
    );

    let behaviour = Behaviour {
        identify,
        ping,
        kad: kad_behaviour,
        relay: relay_behaviour,
        rr,
    };

    let mut swarm = Swarm::new(
        transport,
        behaviour,
        peer_id,
        libp2p::swarm::Config::with_tokio_executor(),
    );

    for addr in cfg.listen {
        swarm.listen_on(addr)?;
    }

    for addr in cfg.bootstrap {
        let _ = swarm.dial(addr);
    }

    // Bootstrap the DHT when possible.
    let mut bootstrapped = false;

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            ev = swarm.select_next_some() => {
                match ev {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!(%peer_id, %address, "listening");
                    }
                    SwarmEvent::ConnectionEstablished { .. } => {
                        if !bootstrapped && cfg.enable_kad_server && !bootstrap_peers.is_empty() {
                            let _ = swarm.behaviour_mut().kad.bootstrap();
                            bootstrapped = true;
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Relay(ev)) => {
                        if cfg.enable_relay_server {
                            match ev {
                                relay::Event::ReservationReqAccepted { src_peer_id, .. } => {
                                    info!(peer=%src_peer_id, "relay reservation accepted");
                                }
                                relay::Event::CircuitReqAccepted { src_peer_id, dst_peer_id, .. } => {
                                    info!(src=%src_peer_id, dst=%dst_peer_id, "relay circuit accepted");
                                }
                                _ => {}
                            }
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Rr(request_response::Event::Message { peer, message })) => {
                        match message {
                            request_response::Message::Request { request, channel, .. } => {
                                let resp = handle_mailbox_request(
                                    &mailbox,
                                    &limiter,
                                    cfg.mailbox_max_body_bytes,
                                    &peer,
                                    request,
                                )
                                .await;
                                let _ = swarm.behaviour_mut().rr.send_response(channel, resp);
                            }
                            request_response::Message::Response { .. } => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}
