/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use crate::relay_bridge::handle_relay_http_request;
use anyhow::{Context, Result};
use axum::body::Body;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use fedi3_protocol::{RelayHttpRequest, RelayHttpResponse};
use futures_util::{future::Either, StreamExt};
use http::Request;
use libp2p::kad::{Quorum, Record};
use libp2p::multiaddr::Protocol;
use libp2p::Transport;
use libp2p::{
    autonat,
    core::muxing::StreamMuxerBox,
    core::upgrade,
    dcutr, gossipsub, identify, identity, kad, mdns, noise, ping, quic, relay, request_response,
    swarm::{derive_prelude::*, SwarmEvent},
    tcp, websocket, yamux, Multiaddr, PeerId, Swarm,
};
use std::sync::Arc;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use tokio::sync::watch;
use tokio::sync::{mpsc, oneshot};
use tower::util::ServiceExt;
use tracing::{error, info};

use crate::net_metrics::NetMetrics;
use crate::social_db::SocialDb;

mod discovery;
use discovery::{PeerBook, SignedContact};

/// Maximum accepted P2P frame size (H-06).
pub const MAX_P2P_FRAME: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct P2pConfig {
    pub enable: bool,
    pub listen: Option<String>,
    pub bootstrap: Option<Vec<String>>,
    /// Independent public IPFS provider discovery (never stores Fedi3 records).
    pub public_discovery_enable: Option<bool>,
    /// Override IPFS bootstrap multiaddresses; an empty list disables seeds.
    pub public_bootstrap: Option<Vec<String>>,
    /// Persist and reconnect known Fedi3 peers, and exchange signed contacts.
    pub peer_exchange_enable: Option<bool>,
    pub mdns_enable: Option<bool>,
    pub announce: Option<Vec<String>>,
    /// Multiaddr dei relay (server) su cui chiedere una reservation (circuit relay v2).
    /// Deve includere anche `/p2p/<relay_peer_id>`, es:
    /// `/ip4/1.2.3.4/tcp/4001/p2p/12D3KooW...`
    pub relay_reserve: Option<Vec<String>>,
    /// Poll interval (seconds) for P2P mailbox store-and-forward.
    pub mailbox_poll_secs: Option<u64>,
    /// Enable global timeline gossip (best-effort).
    pub gossip_enable: Option<bool>,
    /// Enable DHT discovery record publishing and lookups.
    pub discovery_enable: Option<bool>,
    /// Enable P2P outbox sync worker (pull from followed peers).
    pub sync_enable: Option<bool>,
    /// Sync interval seconds.
    pub sync_poll_secs: Option<u64>,
    /// Max items fetched per peer per tick.
    pub sync_batch_limit: Option<u32>,
    /// Prefer relay circuit addresses (skip publishing observed external addrs).
    pub prefer_relay_addrs: Option<bool>,
    /// Force relay-circuit-only connectivity (TURN-like): skip direct listen + publish only relayed addrs.
    pub force_relay_only: Option<bool>,
    /// If true and relays are configured, switch to relay-preferred mode when AutoNAT reports non-public status.
    pub auto_force_relay_only: Option<bool>,
    /// Enable device-sync between peers with same DID (requires shared identity key).
    pub device_sync_enable: Option<bool>,
    /// Device-sync interval seconds.
    pub device_sync_poll_secs: Option<u64>,

    /// Enable WebRTC (ICE/TURN) as NAT traversal fallback for P2P request/response delivery.
    pub webrtc_enable: Option<bool>,
    /// Poll interval seconds for relay signaling inbox.
    pub webrtc_poll_secs: Option<u64>,
    /// ICE server URLs (e.g. `stun:stun.l.google.com:19302`, `turn:turn.example:3478?transport=udp`).
    pub webrtc_ice_urls: Option<Vec<String>>,
    /// Optional ICE username (TURN).
    pub webrtc_ice_username: Option<String>,
    /// Optional ICE credential (TURN).
    pub webrtc_ice_credential: Option<String>,
    /// Connection timeout seconds (offer/answer + datachannel open).
    pub webrtc_connect_timeout_secs: Option<u64>,
    /// Idle TTL seconds for open sessions.
    pub webrtc_idle_ttl_secs: Option<u64>,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            enable: false,
            listen: Some("/ip4/0.0.0.0/tcp/0,/ip4/0.0.0.0/udp/0/quic-v1".to_string()),
            bootstrap: Some(Vec::new()),
            public_discovery_enable: Some(true),
            public_bootstrap: None,
            peer_exchange_enable: Some(true),
            mdns_enable: Some(true),
            announce: Some(Vec::new()),
            relay_reserve: Some(Vec::new()),
            mailbox_poll_secs: Some(15),
            gossip_enable: Some(true),
            discovery_enable: Some(true),
            sync_enable: Some(true),
            sync_poll_secs: Some(30),
            sync_batch_limit: Some(50),
            prefer_relay_addrs: Some(false),
            force_relay_only: Some(false),
            auto_force_relay_only: Some(true),
            device_sync_enable: Some(false),
            device_sync_poll_secs: Some(30),
            webrtc_enable: Some(false),
            webrtc_poll_secs: Some(2),
            webrtc_ice_urls: Some(Vec::new()),
            webrtc_ice_username: None,
            webrtc_ice_credential: None,
            webrtc_connect_timeout_secs: Some(20),
            webrtc_idle_ttl_secs: Some(300),
        }
    }
}

impl P2pConfig {
    fn validate_bootstraps(&self) -> Result<()> {
        for (name, seeds, limit) in [
            ("bootstrap", self.bootstrap.as_ref(), 32),
            ("public_bootstrap", self.public_bootstrap.as_ref(), 8),
        ] {
            if let Some(seeds) = seeds {
                anyhow::ensure!(seeds.len() <= limit, "p2p.{name}: at most {limit} seeds");
                for seed in seeds {
                    let addr: Multiaddr = seed
                        .trim()
                        .parse()
                        .with_context(|| format!("invalid p2p.{name} multiaddress"))?;
                    anyhow::ensure!(
                        matches!(addr.iter().last(), Some(Protocol::P2p(_))),
                        "p2p.{name}: each seed must end with /p2p/<peer-id>"
                    );
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PeerDiscoveryRecord {
    pub peer_id: String,
    pub actor: String,
    pub addrs: Vec<String>,
    pub updated_at_ms: i64,
    pub v: u32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DidDiscoveryRecord {
    pub did: String,
    pub actor: String,
    pub peers: Vec<DidPeer>,
    pub updated_at_ms: i64,
    pub v: u32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DidPeer {
    pub peer_id: String,
    pub addrs: Vec<String>,
    pub last_seen_ms: i64,
}

#[derive(Clone)]
pub struct P2pHandle {
    pub peer_id: String,
    tx: mpsc::Sender<OutboundMsg>,
}

impl P2pHandle {
    pub async fn request(&self, peer_id: &str, req: RelayHttpRequest) -> Result<RelayHttpResponse> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(OutboundMsg::Request {
                peer_id: peer_id.to_string(),
                req,
                resp_tx: tx,
            })
            .await
            .context("p2p send outbound")?;
        rx.await.context("p2p response dropped")?
    }

    pub async fn add_peer_addrs(&self, peer_id: &str, addrs: Vec<String>) -> Result<()> {
        self.tx
            .send(OutboundMsg::AddAddrs {
                peer_id: peer_id.to_string(),
                addrs,
            })
            .await
            .context("p2p send add_addrs")?;
        Ok(())
    }

    pub async fn publish(&self, topic: &str, data: Vec<u8>) -> Result<()> {
        self.tx
            .send(OutboundMsg::Publish {
                topic: topic.to_string(),
                data,
            })
            .await
            .context("p2p send publish")?;
        Ok(())
    }

    pub async fn kad_get_peer(&self, peer_id: &str) -> Result<Option<PeerDiscoveryRecord>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(OutboundMsg::KadGetPeer {
                peer_id: peer_id.to_string(),
                resp_tx: tx,
            })
            .await
            .context("p2p send kad_get_peer")?;
        rx.await.context("p2p kad response dropped")?
    }

    pub async fn kad_get_did(&self, did: &str) -> Result<Option<DidDiscoveryRecord>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(OutboundMsg::KadGetDid {
                did: did.to_string(),
                resp_tx: tx,
            })
            .await
            .context("p2p send kad_get_did")?;
        rx.await.context("p2p kad response dropped")?
    }
}

enum OutboundMsg {
    Request {
        peer_id: String,
        req: RelayHttpRequest,
        resp_tx: oneshot::Sender<Result<RelayHttpResponse>>,
    },
    AddAddrs {
        peer_id: String,
        addrs: Vec<String>,
    },
    Publish {
        topic: String,
        data: Vec<u8>,
    },
    KadGetPeer {
        peer_id: String,
        resp_tx: oneshot::Sender<Result<Option<PeerDiscoveryRecord>>>,
    },
    KadGetDid {
        did: String,
        resp_tx: oneshot::Sender<Result<Option<DidDiscoveryRecord>>>,
    },
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

impl request_response::Codec for Fedi3Codec {
    type Protocol = Fedi3Protocol;
    type Request = RelayHttpRequest;
    type Response = RelayHttpResponse;

    async fn read_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> std::io::Result<Self::Request>
    where
        T: futures_util::AsyncRead + Unpin + Send,
    {
        read_len_prefixed_json(io).await
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> std::io::Result<Self::Response>
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
    read_json_with_limit(io, MAX_P2P_FRAME).await
}

async fn read_json_with_limit<T, V>(io: &mut T, limit: usize) -> std::io::Result<V>
where
    T: futures_util::AsyncRead + Unpin + Send,
    V: serde::de::DeserializeOwned,
{
    use futures_util::AsyncReadExt as _;
    let mut len_buf = [0u8; 4];
    io.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    // H-06/P-01: never allocate the length declared by the peer before validating it.
    if len > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("p2p frame too large: {len} bytes (max {limit})"),
        ));
    }
    let mut buf = vec![0u8; len];
    io.read_exact(&mut buf).await?;
    serde_json::from_slice(&buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

async fn write_len_prefixed_json<T, V>(io: &mut T, value: &V) -> std::io::Result<()>
where
    T: futures_util::AsyncWrite + Unpin + Send,
    V: serde::Serialize,
{
    use futures_util::AsyncWriteExt as _;
    let bytes = serde_json::to_vec(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if bytes.len() > MAX_P2P_FRAME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("p2p frame too large: {} bytes", bytes.len()),
        ));
    }
    let len = (bytes.len() as u32).to_be_bytes();
    io.write_all(&len).await?;
    io.write_all(&bytes).await?;
    io.flush().await?;
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
struct MailboxPollResp {
    messages: Vec<MailboxMessage>,
}

#[derive(Debug, serde::Deserialize)]
struct MailboxMessage {
    id: String,
    req: RelayHttpRequest,
}

fn random_id() -> String {
    let mut b = [0u8; 16];
    use rand::RngCore as _;
    rand::rngs::OsRng.fill_bytes(&mut b);
    b.iter().map(|v| format!("{v:02x}")).collect()
}

#[derive(Debug)]
#[allow(dead_code)]
enum BehaviourEvent {
    Mdns(mdns::Event),
    Identify(Box<identify::Event>),
    Ping(ping::Event),
    Rr(request_response::Event<RelayHttpRequest, RelayHttpResponse>),
    Kad(kad::Event),
    PublicKad(kad::Event),
    Exchange(request_response::Event<Vec<SignedContact>, Vec<SignedContact>>),
    Autonat(autonat::Event),
    Relay(relay::client::Event),
    Dcutr(dcutr::Event),
    Gossip(gossipsub::Event),
}

impl From<mdns::Event> for BehaviourEvent {
    fn from(v: mdns::Event) -> Self {
        Self::Mdns(v)
    }
}
impl From<identify::Event> for BehaviourEvent {
    fn from(v: identify::Event) -> Self {
        Self::Identify(Box::new(v))
    }
}
impl From<ping::Event> for BehaviourEvent {
    fn from(v: ping::Event) -> Self {
        Self::Ping(v)
    }
}
impl From<request_response::Event<RelayHttpRequest, RelayHttpResponse>> for BehaviourEvent {
    fn from(v: request_response::Event<RelayHttpRequest, RelayHttpResponse>) -> Self {
        Self::Rr(v)
    }
}
impl From<kad::Event> for BehaviourEvent {
    fn from(v: kad::Event) -> Self {
        Self::Kad(v)
    }
}
impl From<request_response::Event<Vec<SignedContact>, Vec<SignedContact>>> for BehaviourEvent {
    fn from(v: request_response::Event<Vec<SignedContact>, Vec<SignedContact>>) -> Self {
        Self::Exchange(v)
    }
}

// A wrapper gives the second DHT a distinct event type. Both DHTs use the
// standard libp2p implementation, with separate protocols and record stores.
#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "PublicDhtEvent")]
struct PublicDht {
    kad: kad::Behaviour<kad::store::MemoryStore>,
}
enum PublicDhtEvent {
    Kad(kad::Event),
}
impl From<kad::Event> for PublicDhtEvent {
    fn from(v: kad::Event) -> Self {
        Self::Kad(v)
    }
}
impl From<PublicDhtEvent> for BehaviourEvent {
    fn from(v: PublicDhtEvent) -> Self {
        match v {
            PublicDhtEvent::Kad(e) => Self::PublicKad(e),
        }
    }
}
impl From<autonat::Event> for BehaviourEvent {
    fn from(v: autonat::Event) -> Self {
        Self::Autonat(v)
    }
}
impl From<relay::client::Event> for BehaviourEvent {
    fn from(v: relay::client::Event) -> Self {
        Self::Relay(v)
    }
}
impl From<dcutr::Event> for BehaviourEvent {
    fn from(v: dcutr::Event) -> Self {
        Self::Dcutr(v)
    }
}
impl From<gossipsub::Event> for BehaviourEvent {
    fn from(v: gossipsub::Event) -> Self {
        Self::Gossip(v)
    }
}

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "BehaviourEvent")]
struct Behaviour {
    mdns: libp2p::swarm::behaviour::toggle::Toggle<mdns::tokio::Behaviour>,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    rr: request_response::Behaviour<Fedi3Codec>,
    kad: kad::Behaviour<kad::store::MemoryStore>,
    public_dht: libp2p::swarm::behaviour::toggle::Toggle<PublicDht>,
    exchange: libp2p::swarm::behaviour::toggle::Toggle<
        request_response::Behaviour<discovery::ExchangeCodec>,
    >,
    autonat: autonat::Behaviour,
    relay: relay::client::Behaviour,
    dcutr: dcutr::Behaviour,
    gossip: gossipsub::Behaviour,
}

enum InboundResult {
    Response {
        peer: PeerId,
        channel: request_response::ResponseChannel<RelayHttpResponse>,
        response: RelayHttpResponse,
    },
    Mailbox {
        peer: PeerId,
        ids: Vec<String>,
    },
    Gossip,
}

async fn run_p2p_with_shutdown<S>(
    cfg: P2pConfig,
    keypair: identity::Keypair,
    did: String,
    self_actor_url: String,
    internal_token: String,
    private_key_pem: String,
    social: Arc<SocialDb>,
    metrics: Arc<NetMetrics>,
    mut out_rx: mpsc::Receiver<OutboundMsg>,
    handler: S,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()>
where
    S: Clone
        + Send
        + 'static
        + tower::Service<
            Request<Body>,
            Response = http::Response<Body>,
            Error = std::convert::Infallible,
        >,
    S::Future: Send,
{
    if !cfg.enable {
        return Ok(());
    }

    metrics.set_p2p_enabled(true);
    let peer_id = PeerId::from(keypair.public());
    let exchange_enabled = cfg.peer_exchange_enable.unwrap_or(true);
    let public_enabled = cfg.public_discovery_enable.unwrap_or(true);
    let mut peer_book = PeerBook::load(social.data_dir(), peer_id, now_ms());
    metrics
        .p2p_known_peers
        .store(peer_book.len() as u64, std::sync::atomic::Ordering::Relaxed);
    let mut connected_peers: std::collections::HashSet<PeerId> = std::collections::HashSet::new();
    let mut mailbox_sent_at: HashMap<request_response::OutboundRequestId, i64> = HashMap::new();

    let (relay_transport, relay_behaviour) = relay::client::new(peer_id);
    let force_relay_only = cfg.force_relay_only.unwrap_or(false);
    let auto_force_relay_only = cfg.auto_force_relay_only.unwrap_or(true);
    let has_any_relay = cfg
        .relay_reserve
        .as_ref()
        .map(|v| v.iter().any(|s| !s.trim().is_empty()))
        .unwrap_or(false);
    if force_relay_only {
        if !has_any_relay {
            anyhow::bail!("p2p.force_relay_only=true requires at least one relay_reserve");
        }
    }
    let auto_relay_allowed = !force_relay_only && auto_force_relay_only && has_any_relay;

    // Circuit relay transport must be combined at the "raw" (pre-upgrade) layer, then upgraded.
    let raw_tcp = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true));
    let raw_ws = websocket::Config::new(tcp::tokio::Transport::new(
        tcp::Config::default().nodelay(true),
    ));
    let raw_ws_or_tcp = libp2p::core::transport::choice::OrTransport::new(raw_ws, raw_tcp);
    let raw_ws_or_tcp = libp2p::dns::tokio::Transport::system(raw_ws_or_tcp)?;
    let raw_stream =
        libp2p::core::transport::choice::OrTransport::new(relay_transport, raw_ws_or_tcp);
    let tcp_transport = raw_stream
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(&keypair)?)
        .multiplex(yamux::Config::default())
        .timeout(Duration::from_secs(20))
        .boxed();

    let quic_transport = quic::tokio::Transport::new(quic::Config::new(&keypair));
    let transport =
        libp2p::core::transport::choice::OrTransport::new(quic_transport, tcp_transport)
            .map(|either, _| match either {
                Either::Left((peer, conn)) => (peer, StreamMuxerBox::new(conn)),
                Either::Right((peer, muxer)) => (peer, muxer),
            })
            .boxed();

    let mdns = if cfg.mdns_enable.unwrap_or(true) {
        Some(mdns::tokio::Behaviour::new(
            mdns::Config::default(),
            peer_id,
        )?)
    } else {
        None
    }
    .into();
    let identify = identify::Behaviour::new(identify::Config::new(
        "/fedi3/identify/1".to_string(),
        keypair.public(),
    ));
    let ping = ping::Behaviour::new(ping::Config::new());
    let rr = request_response::Behaviour::new(
        [(Fedi3Protocol, request_response::ProtocolSupport::Full)],
        request_response::Config::default(),
    );
    let kad = discovery::private_kad(peer_id);
    let public_dht = public_enabled
        .then(|| {
            let mut kad = kad::Behaviour::new(peer_id, kad::store::MemoryStore::new(peer_id));
            // This node is a public-DHT client, not an IPFS storage/record server.
            kad.set_mode(Some(kad::Mode::Client));
            PublicDht { kad }
        })
        .into();
    let exchange = exchange_enabled
        .then(|| {
            request_response::Behaviour::with_codec(
                discovery::ExchangeCodec,
                [(
                    libp2p::StreamProtocol::new(discovery::EXCHANGE_PROTOCOL),
                    request_response::ProtocolSupport::Full,
                )],
                request_response::Config::default().with_request_timeout(Duration::from_secs(10)),
            )
        })
        .into();
    let autonat = autonat::Behaviour::new(peer_id, Default::default());
    let dcutr = dcutr::Behaviour::new(peer_id);

    let gossip = {
        let cfg = gossipsub::ConfigBuilder::default()
            .max_transmit_size(256 * 1024)
            .build()
            .expect("gossipsub config");
        let auth = gossipsub::MessageAuthenticity::Signed(keypair.clone());
        gossipsub::Behaviour::new(auth, cfg).expect("gossipsub")
    };

    let behaviour = Behaviour {
        mdns,
        identify,
        ping,
        rr,
        kad,
        public_dht,
        exchange,
        autonat,
        relay: relay_behaviour,
        dcutr,
        gossip,
    };

    let swarm_config = libp2p::swarm::Config::with_tokio_executor()
        .with_idle_connection_timeout(Duration::from_secs(120));
    let mut swarm = Swarm::new(transport, behaviour, peer_id, swarm_config);

    if !force_relay_only {
        // Deserializing an omitted Option does not use P2pConfig::default().
        // The Flutter payload omits listen; still open direct TCP/QUIC ports.
        if let Some(listen) = cfg
            .listen
            .as_deref()
            .or(Some("/ip4/0.0.0.0/tcp/0,/ip4/0.0.0.0/udp/0/quic-v1"))
        {
            for s in listen
                .split(',')
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
            {
                let addr: Multiaddr = s.parse()?;
                swarm.listen_on(addr)?;
            }
        }
    }

    // The relay transport dials on listen_on itself. Sleeping here prevents
    // the swarm from being polled and cannot establish a connection.
    let mut relay_listeners = HashMap::new();
    let mut relay_retry = HashMap::new();
    for s in cfg.relay_reserve.iter().flatten() {
        if let Ok(mut addr) = s.trim().parse::<Multiaddr>() {
            if !matches!(addr.iter().last(), Some(Protocol::P2p(_))) {
                tracing::warn!(%addr, "relay reservation requires a terminal peer ID");
                continue;
            }
            addr.push(Protocol::P2pCircuit);
            match swarm.listen_on(addr.clone()) {
                Ok(id) => {
                    relay_listeners.insert(id, addr);
                }
                Err(e) => {
                    tracing::warn!(%addr, error=%e, "relay reservation listen failed");
                    relay_retry.insert(addr, tokio::time::Instant::now() + Duration::from_secs(10));
                }
            }
        }
    }
    let mut relay_tick = tokio::time::interval(Duration::from_secs(5));
    relay_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Request handlers can use this same swarm (e.g. chat delivery receipts).
    // Keep them outside the event loop so nested requests can make progress.
    let mut inbound = tokio::task::JoinSet::new();

    let mut boot: HashSet<Multiaddr> = HashSet::new();
    if let Some(items) = &cfg.bootstrap {
        for s in items {
            if let Ok(a) = s.parse::<Multiaddr>() {
                boot.insert(a);
            }
        }
    }
    for addr in boot {
        // If the multiaddr includes /p2p/<peer>, register it for kad.
        let mut peer_for_kad: Option<PeerId> = None;
        for p in addr.iter() {
            if let libp2p::multiaddr::Protocol::P2p(h) = p {
                peer_for_kad = Some(h);
            }
        }
        if let Some(pid) = peer_for_kad {
            swarm.behaviour_mut().kad.add_address(&pid, addr.clone());
        }
        let _ = swarm.dial(addr);
    }

    info!(%peer_id, "p2p enabled");
    let gossip_enabled = cfg.gossip_enable.unwrap_or(true);
    let discovery_enabled = cfg.discovery_enable.unwrap_or(true);
    let gossip_topic_global: gossipsub::IdentTopic = gossipsub::IdentTopic::new("/fedi3/global/1");
    if gossip_enabled {
        let _ = swarm.behaviour_mut().gossip.subscribe(&gossip_topic_global);
        info!(%peer_id, "gossip enabled");
    }

    // DHT self discovery record (best-effort).
    let mut self_addrs: Vec<String> = if force_relay_only {
        Vec::new()
    } else {
        cfg.announce.clone().unwrap_or_default()
    };
    if let Some(relays) = cfg.relay_reserve.as_ref() {
        for r in relays {
            let r = r.trim();
            if r.is_empty() {
                continue;
            }
            self_addrs.push(format!("{r}/p2p-circuit/p2p/{peer_id}"));
        }
    }
    self_addrs.sort();
    self_addrs.dedup();
    let mut self_record = PeerDiscoveryRecord {
        peer_id: peer_id.to_string(),
        actor: self_actor_url,
        addrs: self_addrs,
        updated_at_ms: now_ms(),
        v: 1,
    };
    let prefer_relay_addrs_cfg = cfg.prefer_relay_addrs.unwrap_or(false);
    let mut dynamic_relay_only = force_relay_only;
    let mut prefer_relay_addrs = prefer_relay_addrs_cfg || dynamic_relay_only;
    let mut base_self_addrs = self_record.addrs.clone();
    let mut observed_addrs: HashSet<Multiaddr> = HashSet::new();
    let mut discovery_tick = tokio::time::interval(Duration::from_secs(600));
    discovery_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Mailbox targets: infra peers that provide store-and-forward.
    let mailbox_poll_secs = cfg.mailbox_poll_secs.unwrap_or(15).max(5).min(300);
    let mut mailbox_targets: Vec<PeerId> = Vec::new();
    if let Some(relays) = cfg.relay_reserve.as_ref() {
        let mut by_peer: HashMap<PeerId, Vec<Multiaddr>> = HashMap::new();
        for s in relays {
            if let Ok(addr) = s.parse::<Multiaddr>() {
                let mut pid: Option<PeerId> = None;
                for p in addr.iter() {
                    if let Protocol::P2p(h) = p {
                        pid = Some(h);
                    }
                }
                if let Some(pid) = pid {
                    by_peer.entry(pid).or_default().push(addr);
                }
            }
        }
        for (pid, addrs) in by_peer {
            for a in &addrs {
                swarm.add_peer_address(pid, a.clone());
                let _ = swarm.dial(a.clone());
            }
            mailbox_targets.push(pid);
        }
    }
    let mut mailbox_tick = tokio::time::interval(Duration::from_secs(mailbox_poll_secs));
    mailbox_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut mailbox_pending: HashMap<request_response::OutboundRequestId, PeerId> = HashMap::new();
    // Persist mailbox dedup using the main inbox_seen table via a reserved namespace.

    let mut pending: HashMap<
        request_response::OutboundRequestId,
        oneshot::Sender<Result<RelayHttpResponse>>,
    > = HashMap::new();
    // Only one dial per peer while disconnected. request_response treats any
    // concurrent dial failure as failure of every pending request for that peer.
    let mut connecting: HashMap<
        PeerId,
        Vec<(RelayHttpRequest, oneshot::Sender<Result<RelayHttpResponse>>)>,
    > = HashMap::new();
    let mut kad_pending: HashMap<
        kad::QueryId,
        oneshot::Sender<Result<Option<PeerDiscoveryRecord>>>,
    > = HashMap::new();
    let mut kad_pending_did: HashMap<
        kad::QueryId,
        oneshot::Sender<Result<Option<DidDiscoveryRecord>>>,
    > = HashMap::new();
    let mut did_publish_pending: HashSet<kad::QueryId> = HashSet::new();
    let mut kad_bootstrapped = false;
    let public_seeds: Vec<String> = cfg.public_bootstrap.clone().unwrap_or_else(|| {
        discovery::PUBLIC_BOOTSTRAPS
            .iter()
            .map(|s| s.to_string())
            .collect()
    });
    let mut public_tick = tokio::time::interval(Duration::from_secs(300));
    public_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut peers_tick = tokio::time::interval(Duration::from_secs(30));
    peers_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut persist_tick = tokio::time::interval(Duration::from_secs(60));
    persist_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut exchange_last: HashMap<PeerId, i64> = HashMap::new();
    let mut exchange_requested: HashMap<PeerId, i64> = HashMap::new();
    let mut fedi_peers = HashSet::new();
    let mut public_ready = false;
    let mut provider_candidates = HashSet::new();
    let mut public_relays = HashSet::new();
    let configured_relays: HashSet<_> = cfg
        .relay_reserve
        .iter()
        .flatten()
        .filter_map(|s| s.parse::<Multiaddr>().ok())
        .filter_map(|addr| {
            addr.iter()
                .filter_map(|p| {
                    if let Protocol::P2p(peer) = p {
                        Some(peer)
                    } else {
                        None
                    }
                })
                .last()
        })
        .collect();
    let mut public_relay_cooldowns = HashMap::new();

    if discovery_enabled {
        // Try to publish immediately (will replicate when peers are known).
        self_record.updated_at_ms = now_ms();
        if let Ok(value) = serde_json::to_vec(&self_record) {
            let key = kad_key_for_peer(&peer_id.to_string());
            let record = Record {
                key,
                value,
                publisher: None,
                expires: None,
            };
            let _ = swarm.behaviour_mut().kad.put_record(record, Quorum::One);
        }

        // Merge-publish DID record (best-effort) so multiple devices with same identity can coexist.
        let q = swarm.behaviour_mut().kad.get_record(kad_key_for_did(&did));
        did_publish_pending.insert(q);
    }

    loop {
        tokio::select! {
            _ = persist_tick.tick(), if exchange_enabled => {
                metrics.p2p_known_peers.store(peer_book.len() as u64, std::sync::atomic::Ordering::Relaxed);
                let snapshot = peer_book.snapshot();
                if let Err(e) = tokio::task::spawn_blocking(move || discovery::save_snapshot(snapshot)).await? {
                    tracing::warn!(error=%e, "could not save known P2P peers");
                }
            }
            _ = peers_tick.tick(), if exchange_enabled => {
                let now = now_ms();
                exchange_last.retain(|id, _| connected_peers.contains(id));
                exchange_requested.retain(|id, _| connected_peers.contains(id));
                if connected_peers.len() < 32 {
                    for (peer, addrs) in peer_book.candidates(now, &connected_peers) {
                        for addr in addrs.into_iter().filter_map(|s| s.parse::<Multiaddr>().ok()) {
                            swarm.add_peer_address(peer, addr);
                        }
                        let opts = libp2p::swarm::dial_opts::DialOpts::peer_id(peer)
                            .condition(libp2p::swarm::dial_opts::PeerCondition::DisconnectedAndNotDialing).build();
                        let _ = swarm.dial(opts);
                    }
                }
                let mut contacts = peer_book.contacts(now);
                if let Some(own) = SignedContact::create(&keypair, &self_record.addrs, now) { contacts.insert(0, own); }
                let targets: Vec<_> = fedi_peers.iter().filter(|id| connected_peers.contains(id)
                    && exchange_requested.get(id).map(|at| now - at >= 120_000).unwrap_or(true)).take(4).copied().collect();
                for peer in targets {
                    if let Some(exchange) = swarm.behaviour_mut().exchange.as_mut() {
                        exchange.send_request(&peer, contacts.clone());
                        exchange_requested.insert(peer, now);
                    }
                }
            }
            _ = public_tick.tick(), if public_enabled => {
                public_relay_cooldowns.retain(|_, until| *until > tokio::time::Instant::now());
                for seed in public_seeds.iter().take(8) {
                    if let Ok(addr) = seed.parse::<Multiaddr>() {
                        if let Some(Protocol::P2p(peer)) = addr.iter().last() {
                            if let Some(public) = swarm.behaviour_mut().public_dht.as_mut() {
                                public.kad.add_address(&peer, addr.clone());
                            }
                            if !swarm.is_connected(&peer) {
                                let _ = swarm.dial(libp2p::swarm::dial_opts::DialOpts::peer_id(peer)
                                    .condition(libp2p::swarm::dial_opts::PeerCondition::DisconnectedAndNotDialing)
                                    .addresses(vec![addr]).build());
                            }
                        }
                    }
                }
                if public_ready {
                    if let Some(public) = swarm.behaviour_mut().public_dht.as_mut() {
                        let _ = public.kad.bootstrap();
                        let _ = public.kad.start_providing(discovery::public_provider_key());
                        public.kad.get_providers(discovery::public_provider_key());
                    }
                }
                provider_candidates.clear();
            }
            result = inbound.join_next(), if !inbound.is_empty() => {
                match result {
                    Some(Ok(InboundResult::Response { peer, channel, response })) => {
                        if let Ok(bytes) = serde_json::to_vec(&response) {
                            metrics.p2p_tx_add(bytes.len() as u64);
                        }
                        if swarm.behaviour_mut().rr.send_response(channel, response).is_err() {
                            error!(peer=%peer, "p2p send_response failed");
                        }
                    }
                    Some(Ok(InboundResult::Mailbox { peer, ids })) if !ids.is_empty() => {
                        let ack_req = RelayHttpRequest {
                            id: format!("mbx-ack-{}", random_id()),
                            method: "POST".to_string(),
                            path: "/.fedi3/mailbox/ack".to_string(),
                            query: String::new(),
                            headers: vec![("content-type".to_string(), "application/json".to_string())],
                            body_b64: B64.encode(serde_json::to_vec(&serde_json::json!({
                                "for_peer_id": peer_id.to_string(), "ids": ids,
                            }))?),
                        };
                        metrics.mailbox_tx_add(ack_req.body_b64.len() as u64);
                        let id = swarm.behaviour_mut().rr.send_request(&peer, ack_req);
                        mailbox_pending.insert(id, peer);
                        mailbox_sent_at.insert(id, now_ms());
                    }
                    Some(Err(e)) => tracing::warn!(error=%e, "p2p inbound handler failed"),
                    _ => {}
                }
            }
            _ = relay_tick.tick() => {
                let now = tokio::time::Instant::now();
                let due: Vec<_> = relay_retry.iter()
                    .filter(|(_, at)| **at <= now).map(|(addr, _)| addr.clone()).collect();
                for addr in due {
                    relay_retry.remove(&addr);
                    match swarm.listen_on(addr.clone()) {
                        Ok(id) => { relay_listeners.insert(id, addr); }
                        Err(e) => {
                            tracing::warn!(%addr, error=%e, "relay reservation retry failed");
                            relay_retry.insert(addr, now + Duration::from_secs(10));
                        }
                    }
                }
                pending.retain(|_, tx| !tx.is_closed());
                connecting.retain(|_, requests| {
                    requests.retain(|(_, tx)| !tx.is_closed());
                    !requests.is_empty()
                });
                kad_pending.retain(|_, tx| !tx.is_closed());
                kad_pending_did.retain(|_, tx| !tx.is_closed());
            }
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    if exchange_enabled {
                        let snapshot = peer_book.snapshot();
                        let _ = tokio::task::spawn_blocking(move || discovery::save_snapshot(snapshot)).await;
                    }
                    break;
                }
            }
            _ = discovery_tick.tick() => {
                if discovery_enabled {
                    self_record.addrs = base_self_addrs.clone();
                    if !prefer_relay_addrs && !observed_addrs.is_empty() {
                        // Publish a bounded set of observed external addrs (best-effort).
                        let mut extra = observed_addrs.iter().take(8).map(|a| a.to_string()).collect::<Vec<_>>();
                        extra.sort();
                        for a in extra {
                            if !self_record.addrs.contains(&a) {
                                self_record.addrs.push(a);
                            }
                        }
                        self_record.addrs.sort();
                        self_record.addrs.dedup();
                        if self_record.addrs.len() > 32 {
                            self_record.addrs.truncate(32);
                        }
                    }
                    self_record.updated_at_ms = now_ms();
                    if let Ok(value) = serde_json::to_vec(&self_record) {
                        let key = kad_key_for_peer(&peer_id.to_string());
                        let record = Record {
                            key,
                            value,
                            publisher: None,
                            expires: None,
                        };
                        let _ = swarm.behaviour_mut().kad.put_record(record, Quorum::One);
                    }

                    // Merge-publish DID record (best-effort) so multiple devices with same identity can coexist.
                    let q = swarm.behaviour_mut().kad.get_record(kad_key_for_did(&did));
                    did_publish_pending.insert(q);
                }
            }
            _ = mailbox_tick.tick() => {
                if mailbox_targets.is_empty() {
                    continue;
                }
                for pid in &mailbox_targets {
                    let poll_body = serde_json::json!({
                        "for_peer_id": peer_id.to_string(),
                        "limit": 50,
                    });
                    let poll_bytes = serde_json::to_vec(&poll_body).unwrap_or_default();
                    let poll_req = RelayHttpRequest {
                        id: format!("mbx-poll-{}", random_id()),
                        method: "POST".to_string(),
                        path: "/.fedi3/mailbox/poll".to_string(),
                        query: "".to_string(),
                        headers: vec![("content-type".to_string(), "application/json".to_string())],
                        body_b64: B64.encode(poll_bytes),
                    };
                    metrics.mailbox_tx_add(poll_req.body_b64.len() as u64);
                    metrics.mailbox_peer_seen(&pid.to_string());
                    let req_id = swarm.behaviour_mut().rr.send_request(pid, poll_req);
                    mailbox_pending.insert(req_id, pid.clone());
                    mailbox_sent_at.insert(req_id, now_ms());
                }
            }
            msg = out_rx.recv() => {
                let Some(msg) = msg else { break; };
                match msg {
                    OutboundMsg::Request { peer_id, req, resp_tx } => {
                        let peer: PeerId = match peer_id.parse() {
                            Ok(peer) => peer,
                            Err(e) => {
                                let _ = resp_tx.send(Err(anyhow::anyhow!("invalid peer ID: {e}")));
                                continue;
                            }
                        };
                        if let Ok(bytes) = serde_json::to_vec(&req) {
                            metrics.p2p_tx_add(bytes.len() as u64);
                        }
                        metrics.p2p_peer_seen(&peer.to_string());
                        if swarm.is_connected(&peer) {
                            let req_id = swarm.behaviour_mut().rr.send_request(&peer, req);
                            pending.insert(req_id, resp_tx);
                        } else {
                            let queue = connecting.entry(peer).or_default();
                            queue.push((req, resp_tx));
                            if queue.len() == 1 {
                                let opts = libp2p::swarm::dial_opts::DialOpts::peer_id(peer)
                                    .condition(libp2p::swarm::dial_opts::PeerCondition::NotDialing).build();
                                if let Err(e) = swarm.dial(opts) {
                                    if !matches!(e, libp2p::swarm::DialError::DialPeerConditionFalse(_)) {
                                        for (_, tx) in connecting.remove(&peer).unwrap_or_default() {
                                            let _ = tx.send(Err(anyhow::anyhow!("p2p dial failed: {e}")));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    OutboundMsg::AddAddrs { peer_id, addrs } => {
                        let Ok(peer) = peer_id.parse::<PeerId>() else { continue; };
                        for a in addrs {
                            if let Ok(addr) = a.parse::<Multiaddr>() {
                                if dynamic_relay_only {
                                    let ok = addr.iter().any(|p| matches!(p, Protocol::P2pCircuit));
                                    if !ok {
                                        continue;
                                    }
                                }
                                // request_response dials when needed. An eager
                                // dial here races its dial and fails pending requests.
                                swarm.add_peer_address(peer, addr);
                            }
                        }
                    }
                    OutboundMsg::Publish { topic, data } => {
                        if !gossip_enabled {
                            continue;
                        }
                        let t = gossipsub::IdentTopic::new(topic);
                        metrics.p2p_tx_add(data.len() as u64);
                        let _ = swarm.behaviour_mut().gossip.publish(t, data);
                    }
                    OutboundMsg::KadGetPeer { peer_id, resp_tx } => {
                        let key = kad_key_for_peer(&peer_id);
                        let q = swarm.behaviour_mut().kad.get_record(key);
                        kad_pending.insert(q, resp_tx);
                    }
                    OutboundMsg::KadGetDid { did, resp_tx } => {
                        let key = kad_key_for_did(&did);
                        let q = swarm.behaviour_mut().kad.get_record(key);
                        kad_pending_did.insert(q, resp_tx);
                    }
                }
            }
            ev = swarm.select_next_some() => {
                match ev {
                    SwarmEvent::ListenerClosed { listener_id, reason, .. } => {
                        if let Some(addr) = relay_listeners.remove(&listener_id) {
                            let prefix = addr.to_string();
                            base_self_addrs.retain(|a| !a.starts_with(&prefix));
                            self_record.addrs.retain(|a| !a.starts_with(&prefix));
                            let relay_peer = addr.iter().filter_map(|p| if let Protocol::P2p(peer) = p { Some(peer) } else { None }).last();
                            if let Some(peer) = relay_peer.filter(|p| public_relays.remove(p)) {
                                // Public relays can reject reservations when full. Try
                                // other discovered servers instead of pinning both slots
                                // to two unavailable nodes and retrying them forever.
                                public_relay_cooldowns.insert(peer, tokio::time::Instant::now() + Duration::from_secs(600));
                                continue;
                            }
                            tracing::warn!(%addr, ?reason, "relay reservation closed; scheduling retry");
                            relay_retry.insert(addr, tokio::time::Instant::now() + Duration::from_secs(10));
                        }
                    }
                    SwarmEvent::ListenerError { listener_id, error } => {
                        tracing::warn!(?listener_id, %error, "p2p listener error");
                    }
                    SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                        tracing::debug!(?peer_id, %error, "p2p dial failed");
                        if let Some(peer) = peer_id {
                            if !swarm.is_connected(&peer) && !matches!(error, libp2p::swarm::DialError::DialPeerConditionFalse(_)) {
                                for (_, tx) in connecting.remove(&peer).unwrap_or_default() {
                                    let _ = tx.send(Err(anyhow::anyhow!("p2p dial failed: {error}")));
                                }
                            }
                        }
                    }
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!(%peer_id, %address, "p2p listening");
                        // Includes reservations discovered on community relays.
                        let normalized = discovery::sanitize_addrs(peer_id, &[address.to_string()], true);
                        for addr in normalized {
                            if !base_self_addrs.contains(&addr) { base_self_addrs.push(addr.clone()); }
                            if !self_record.addrs.contains(&addr) { self_record.addrs.push(addr); }
                        }
                    }
                    SwarmEvent::ExternalAddrConfirmed { address } => {
                        if !prefer_relay_addrs {
                            observed_addrs.insert(address);
                        }
                    }
                    SwarmEvent::ConnectionEstablished { peer_id: remote_peer, .. } => {
                        connected_peers.insert(remote_peer);
                        for (req, tx) in connecting.remove(&remote_peer).unwrap_or_default() {
                            if !tx.is_closed() {
                                let id = swarm.behaviour_mut().rr.send_request(&remote_peer, req);
                                pending.insert(id, tx);
                            }
                        }
                        metrics.p2p_connected_peers_set(connected_peers.len() as u64);
                        if !kad_bootstrapped {
                            kad_bootstrapped = swarm.behaviour_mut().kad.bootstrap().is_ok();
                        }
                    }
                    SwarmEvent::ConnectionClosed { peer_id: remote_peer, num_established, .. } => {
                        if num_established == 0 {
                            connected_peers.remove(&remote_peer);
                            fedi_peers.remove(&remote_peer);
                            exchange_last.remove(&remote_peer);
                            exchange_requested.remove(&remote_peer);
                        }
                        metrics.p2p_connected_peers_set(connected_peers.len() as u64);
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Identify(event)) => {
                        let identify::Event::Received { peer_id: remote_peer, info, .. } = *event else { continue; };
                        let is_fedi = info.protocols.iter().any(|p| p.as_ref() == "/fedi3/relay-http/1");
                        if is_fedi && exchange_enabled {
                            fedi_peers.insert(remote_peer);
                            let addrs = info.listen_addrs.iter().map(ToString::to_string).collect::<Vec<_>>();
                            peer_book.learned(remote_peer, addrs, now_ms(), true);
                            // Start exchange immediately after Identify, not only after the next tick.
                            if info.protocols.iter().any(|p| p.as_ref() == discovery::EXCHANGE_PROTOCOL)
                                && !exchange_requested.contains_key(&remote_peer) {
                                let now = now_ms();
                                let mut contacts = peer_book.contacts(now);
                                if let Some(own) = SignedContact::create(&keypair, &self_record.addrs, now) { contacts.insert(0, own); }
                                if let Some(exchange) = swarm.behaviour_mut().exchange.as_mut() {
                                    exchange.send_request(&remote_peer, contacts);
                                    exchange_requested.insert(remote_peer, now);
                                }
                            }
                        }
                        if info.protocols.iter().any(|p| p.as_ref() == discovery::PRIVATE_DHT) {
                            for addr in &info.listen_addrs {
                                swarm.behaviour_mut().kad.add_address(&remote_peer, addr.clone());
                            }
                            if !kad_bootstrapped { kad_bootstrapped = swarm.behaviour_mut().kad.bootstrap().is_ok(); }
                            if discovery_enabled {
                                discovery_tick.reset_immediately();
                            }
                        }
                        if info.protocols.iter().any(|p| p.as_ref() == discovery::PRIVATE_DHT) {
                            // Our infrastructure also speaks the old IPFS-named
                            // protocol for old Fedi3 clients. It is not a public
                            // IPFS router: don't join the two routing tables.
                            if let Some(public) = swarm.behaviour_mut().public_dht.as_mut() { public.kad.remove_peer(&remote_peer); }
                        } else if info.protocols.iter().any(|p| p.as_ref() == "/ipfs/kad/1.0.0") {
                            if let Some(public) = swarm.behaviour_mut().public_dht.as_mut() {
                                for addr in &info.listen_addrs { public.kad.add_address(&remote_peer, addr.clone()); }
                                if !public_ready {
                                    public_ready = public.kad.bootstrap().is_ok();
                                    if public_ready { info!(peer=%remote_peer, "public IPFS discovery bootstrap connected"); }
                                    public_tick.reset_immediately();
                                }
                            }
                        }
                        // Public relay reservations are opportunistic and bounded.
                        // Never assume that an IPFS bootstrap is also a relay.
                        if public_enabled && public_relays.len() < 2 && !public_relays.contains(&remote_peer)
                            && !configured_relays.contains(&remote_peer)
                            && public_relay_cooldowns.get(&remote_peer).map(|until| *until <= tokio::time::Instant::now()).unwrap_or(true)
                            && info.protocols.iter().any(|p| p.as_ref() == "/libp2p/circuit/relay/0.2.0/hop") {
                            let addrs = info.listen_addrs.iter().map(ToString::to_string).collect::<Vec<_>>();
                            if let Some(addr) = discovery::sanitize_addrs(remote_peer, &addrs, true).first() {
                                let mut circuit = addr.parse::<Multiaddr>()?;
                                circuit.push(Protocol::P2pCircuit);
                                if let Ok(id) = swarm.listen_on(circuit.clone()) {
                                    relay_listeners.insert(id, circuit);
                                    public_relays.insert(remote_peer);
                                }
                            }
                        }
                        if !prefer_relay_addrs {
                            swarm.add_external_address(info.observed_addr.clone());
                            observed_addrs.insert(info.observed_addr);
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                        for (p, addr) in list {
                            info!(peer=%p, %addr, "p2p discovered");
                            swarm.add_peer_address(p, addr);
                            if connected_peers.len() < 32 {
                                let _ = swarm.dial(libp2p::swarm::dial_opts::DialOpts::peer_id(p)
                                    .condition(libp2p::swarm::dial_opts::PeerCondition::DisconnectedAndNotDialing).build());
                            }
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Expired(_list))) => {}
                    SwarmEvent::Behaviour(BehaviourEvent::Exchange(request_response::Event::Message { peer, message, .. })) => {
                        let now = now_ms();
                        match message {
                            request_response::Message::Request { request, channel, .. } => {
                                // Bound signature work and response amplification per authenticated peer.
                                if exchange_last.get(&peer).map(|at| now - at < 30_000).unwrap_or(false) {
                                    if let Some(exchange) = swarm.behaviour_mut().exchange.as_mut() { let _ = exchange.send_response(channel, Vec::new()); }
                                    continue;
                                }
                                exchange_last.insert(peer, now);
                                let learned = learn_contacts(request, &mut peer_book, &mut swarm, now);
                                metrics.p2p_peer_exchange_contacts.fetch_add(learned as u64, std::sync::atomic::Ordering::Relaxed);
                                let mut contacts = peer_book.contacts(now);
                                if let Some(own) = SignedContact::create(&keypair, &self_record.addrs, now) { contacts.insert(0, own); }
                                if let Some(exchange) = swarm.behaviour_mut().exchange.as_mut() { let _ = exchange.send_response(channel, contacts); }
                            }
                            request_response::Message::Response { response, .. } => {
                                let learned = learn_contacts(response, &mut peer_book, &mut swarm, now);
                                metrics.p2p_peer_exchange_contacts.fetch_add(learned as u64, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::PublicKad(kad::Event::OutboundQueryProgressed {
                        result: kad::QueryResult::Bootstrap(Ok(_)), ..
                    })) => {
                        metrics.p2p_public_discovery_ready.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::PublicKad(kad::Event::OutboundQueryProgressed {
                        result: kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders { providers, .. })), ..
                    })) => {
                        for peer in providers {
                            if peer == peer_id || provider_candidates.len() >= 8 || connected_peers.len() >= 32 || !provider_candidates.insert(peer) { continue; }
                            let _ = swarm.dial(libp2p::swarm::dial_opts::DialOpts::peer_id(peer)
                                .condition(libp2p::swarm::dial_opts::PeerCondition::DisconnectedAndNotDialing).build());
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Rr(request_response::Event::Message { peer, message, .. })) => {
                        match message {
                            request_response::Message::Request { request, channel, .. } => {
                                if let Ok(bytes) = serde_json::to_vec(&request) {
                                    metrics.p2p_rx_add(bytes.len() as u64);
                                }
                                metrics.p2p_peer_seen(&peer.to_string());
                                let request = maybe_decrypt(&private_key_pem, request);
                                if inbound.len() >= 64 {
                                    let _ = swarm.behaviour_mut().rr.send_response(channel, RelayHttpResponse {
                                        id: request.id, status: 503, headers: Vec::new(), body_b64: String::new(),
                                    });
                                    continue;
                                }
                                let mut handler = handler.clone();
                                inbound.spawn(async move {
                                    let response = handle_relay_http_request(&mut handler, request).await;
                                    InboundResult::Response { peer, channel, response }
                                });
                            }
                            request_response::Message::Response { request_id, response } => {
                                if let Ok(bytes) = serde_json::to_vec(&response) {
                                    metrics.p2p_rx_add(bytes.len() as u64);
                                }
                                metrics.p2p_peer_seen(&peer.to_string());
                                if let Some(tx) = pending.remove(&request_id) {
                                    let _ = tx.send(Ok(response));
                                } else if let Some(mbx_peer) = mailbox_pending.remove(&request_id) {
                                    metrics.mailbox_peer_seen(&mbx_peer.to_string());
                                    if let Some(sent) = mailbox_sent_at.remove(&request_id) {
                                        let rtt = now_ms().saturating_sub(sent);
                                        if rtt > 0 {
                                            metrics.mailbox_rtt_update(rtt as u64);
                                        }
                                    }
                                    if !(200..300).contains(&response.status) {
                                        continue;
                                    }
                                    metrics.mailbox_rx_add(response.body_b64.len() as u64);
                                    let body = match B64.decode(response.body_b64.as_bytes()) {
                                        Ok(b) => b,
                                        Err(_) => continue,
                                    };
                                    let poll: MailboxPollResp = match serde_json::from_slice(&body) {
                                        Ok(v) => v,
                                        Err(_) => continue,
                                    };
                                    if poll.messages.is_empty() {
                                        continue;
                                    }
                                    if inbound.len() >= 64 { continue; }
                                    let social = social.clone();
                                    let private_key_pem = private_key_pem.clone();
                                    let mut handler = handler.clone();
                                    inbound.spawn(async move {
                                        let mut ids = Vec::new();
                                        for msg in poll.messages {
                                            let dedup_key = format!("urn:fedi3:mbx:{}", msg.id);
                                            if social.has_inbox_seen(&dedup_key).unwrap_or(false) {
                                                ids.push(msg.id);
                                                continue;
                                            }
                                            let req = maybe_decrypt(&private_key_pem, msg.req);
                                            let resp = handle_relay_http_request(&mut handler, req).await;
                                            if (200..300).contains(&resp.status) {
                                                let _ = social.mark_inbox_seen(&dedup_key);
                                                ids.push(msg.id);
                                            }
                                        }
                                        InboundResult::Mailbox { peer: mbx_peer, ids }
                                    });
                                }
                            }
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Rr(request_response::Event::OutboundFailure { request_id, error, .. })) => {
                        tracing::debug!(?request_id, %error, "p2p outbound request failed");
                        if let Some(tx) = pending.remove(&request_id) {
                            let _ = tx.send(Err(anyhow::anyhow!("p2p outbound failure: {error:?}")));
                        } else {
                            let _ = mailbox_pending.remove(&request_id);
                            let _ = mailbox_sent_at.remove(&request_id);
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Gossip(gossipsub::Event::Message { message, .. })) => {
                        if !gossip_enabled {
                            continue;
                        }
                        metrics.p2p_rx_add(message.data.len() as u64);
                        metrics.p2p_peer_seen("gossip");
                        // Best-effort: only accept global topic (we subscribe only to it anyway).
                        if message.topic != gossip_topic_global.hash() { continue; }
                        // Ingest via internal endpoint to store & apply policy.
                        let req = http::Request::builder()
                            .method(http::Method::POST)
                            .uri("http://localhost/_fedi3/global/ingest")
                            .header("Content-Type", "application/activity+json")
                            .header("X-Fedi3-Internal", internal_token.clone())
                            .body(Body::from(message.data))
                            .unwrap();
                         if inbound.len() < 64 {
                             let handler = handler.clone();
                             inbound.spawn(async move {
                                 let _ = handler.oneshot(req).await;
                                 InboundResult::Gossip
                             });
                         }
                     }
                    SwarmEvent::Behaviour(BehaviourEvent::Ping(ev)) => {
                        if let Ok(rtt) = ev.result {
                            metrics.p2p_rtt_update(rtt.as_millis() as u64);
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Kad(ev)) => {
                        match ev {
                            kad::Event::OutboundQueryProgressed { id, result, .. } => {
                                let tx_peer = kad_pending.remove(&id);
                                let tx_did = kad_pending_did.remove(&id);
                                let publish_did = did_publish_pending.remove(&id);
                                if tx_peer.is_none() && tx_did.is_none() && !publish_did {
                                    continue;
                                }

                                let value_opt: Option<Vec<u8>> = match result {
                                    kad::QueryResult::GetRecord(Ok(ok)) => match ok {
                                        kad::GetRecordOk::FoundRecord(r) => Some(r.record.value),
                                        kad::GetRecordOk::FinishedWithNoAdditionalRecord { .. } => None,
                                    },
                                    kad::QueryResult::GetRecord(Err(_)) => None,
                                    _ => None,
                                };

                                if let Some(tx) = tx_peer {
                                    let rec = value_opt
                                        .as_deref()
                                        .and_then(|b| serde_json::from_slice::<PeerDiscoveryRecord>(b).ok());
                                    let _ = tx.send(Ok(rec));
                                }
                                if let Some(tx) = tx_did {
                                    let rec = value_opt
                                        .as_deref()
                                        .and_then(|b| serde_json::from_slice::<DidDiscoveryRecord>(b).ok());
                                    let _ = tx.send(Ok(rec));
                                }

                                if publish_did {
                                    let merged = merge_did_record(value_opt.as_deref(), &did, &self_record);
                                    if let Ok(value) = serde_json::to_vec(&merged) {
                                        let record = Record {
                                            key: kad_key_for_did(&did),
                                            value,
                                            publisher: None,
                                            expires: None,
                                        };
                                        let _ = swarm.behaviour_mut().kad.put_record(record, Quorum::One);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Autonat(ev)) => {
                        if !auto_relay_allowed {
                            continue;
                        }
                        match ev {
                            autonat::Event::StatusChanged { new, .. } => {
                                let want_relay = !matches!(new, autonat::NatStatus::Public(_));
                                if want_relay != dynamic_relay_only {
                                    dynamic_relay_only = want_relay;
                                    prefer_relay_addrs = prefer_relay_addrs_cfg || dynamic_relay_only;
                                    if dynamic_relay_only {
                                        observed_addrs.clear();
                                    }
                                    info!(%peer_id, dynamic_relay_only, "p2p auto relay-only toggled");
                                }
                            }
                            _ => {}
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Relay(ev)) => {
                        tracing::info!(?ev, "relay client event");
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Dcutr(_)) => {}
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

pub fn start_p2p<S>(
    cfg: P2pConfig,
    keypair: identity::Keypair,
    did: String,
    self_actor_url: String,
    internal_token: String,
    private_key_pem: String,
    social: Arc<SocialDb>,
    metrics: Arc<NetMetrics>,
    handler: S,
    shutdown: watch::Receiver<bool>,
) -> Result<Option<P2pHandle>>
where
    S: Clone
        + Send
        + 'static
        + tower::Service<
            Request<Body>,
            Response = http::Response<Body>,
            Error = std::convert::Infallible,
        >,
    S::Future: Send,
{
    if !cfg.enable {
        return Ok(None);
    }
    cfg.validate_bootstraps()?;
    let peer_id = PeerId::from(keypair.public()).to_string();
    let (tx, rx) = mpsc::channel::<OutboundMsg>(64);
    tokio::spawn(async move {
        if let Err(e) = run_p2p_with_shutdown(
            cfg,
            keypair,
            did,
            self_actor_url,
            internal_token,
            private_key_pem,
            social,
            metrics,
            rx,
            handler,
            shutdown,
        )
        .await
        {
            error!("p2p task stopped: {e:#}");
        }
    });
    Ok(Some(P2pHandle { peer_id, tx }))
}

fn learn_contacts(
    contacts: Vec<SignedContact>,
    book: &mut PeerBook,
    swarm: &mut Swarm<Behaviour>,
    now: i64,
) -> usize {
    let mut learned = 0;
    for contact in contacts.into_iter().take(discovery::MAX_CONTACTS) {
        if let Some((peer, addrs)) = book.introduce(contact, now) {
            learned += 1;
            for addr in addrs
                .into_iter()
                .filter_map(|a| a.parse::<Multiaddr>().ok())
            {
                swarm.add_peer_address(peer, addr);
            }
        }
    }
    learned
}

fn kad_key_for_peer(peer_id: &str) -> kad::RecordKey {
    // Stable key namespace for peer discovery.
    kad::RecordKey::new(&format!("/fedi3/peer/{peer_id}"))
}

fn kad_key_for_did(did: &str) -> kad::RecordKey {
    // Stable key namespace for DID discovery.
    kad::RecordKey::new(&format!("/fedi3/did/{did}"))
}

fn merge_did_record(
    existing: Option<&[u8]>,
    did: &str,
    self_record: &PeerDiscoveryRecord,
) -> DidDiscoveryRecord {
    let now = now_ms();
    let mut peers: Vec<DidPeer> = Vec::new();
    let mut actor: Option<String> = None;
    let mut updated_at_ms: Option<i64> = None;
    if let Some(bytes) = existing {
        if let Ok(r) = serde_json::from_slice::<DidDiscoveryRecord>(bytes) {
            peers = r.peers;
            actor = Some(r.actor);
            updated_at_ms = Some(r.updated_at_ms);
        }
    }

    // Keep only relatively recent peers to reduce staleness.
    let cutoff_ms: i64 = now.saturating_sub(7 * 24 * 3600 * 1000);
    peers.retain(|p| !p.peer_id.trim().is_empty() && p.last_seen_ms >= cutoff_ms);

    // Upsert self peer.
    if let Some(p) = peers.iter_mut().find(|p| p.peer_id == self_record.peer_id) {
        p.addrs = self_record.addrs.clone();
        p.last_seen_ms = now;
    } else {
        peers.push(DidPeer {
            peer_id: self_record.peer_id.clone(),
            addrs: self_record.addrs.clone(),
            last_seen_ms: now,
        });
    }

    peers.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
    peers.dedup_by(|a, b| a.peer_id == b.peer_id);
    if peers.len() > 16 {
        peers.truncate(16);
    }

    // Keep actor stable to avoid oscillations across devices.
    // If the existing record is stale and has no peers, prefer our current actor URL.
    let actor = match actor {
        Some(a) if !a.trim().is_empty() => {
            let stale_cutoff_ms: i64 = now.saturating_sub(7 * 24 * 3600 * 1000);
            let is_stale = updated_at_ms.unwrap_or(0) < stale_cutoff_ms;
            if is_stale && peers.is_empty() {
                self_record.actor.clone()
            } else {
                a
            }
        }
        _ => self_record.actor.clone(),
    };

    DidDiscoveryRecord {
        did: did.to_string(),
        actor,
        peers,
        updated_at_ms: now,
        v: 1,
    }
}

fn maybe_decrypt(private_key_pem: &str, req: RelayHttpRequest) -> RelayHttpRequest {
    crate::crypto_envelope::decrypt_relay_http_request_body(private_key_pem, req)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub fn load_or_generate_keypair(data_dir: &std::path::Path) -> Result<identity::Keypair> {
    let path = data_dir.join("p2p_keypair.pb");
    if let Ok(bytes) = std::fs::read(&path) {
        let kp = identity::Keypair::from_protobuf_encoding(&bytes).context("decode p2p keypair")?;
        let _ = crate::keys::secure_file(&path);
        return Ok(kp);
    }
    let kp = identity::Keypair::generate_ed25519();
    let bytes = kp.to_protobuf_encoding().context("encode p2p keypair")?;
    crate::keys::write_secret_atomic(&path, &bytes).context("write p2p keypair")?;
    Ok(kp)
}

pub fn peer_id_string(keypair: &identity::Keypair) -> String {
    PeerId::from(keypair.public()).to_string()
}

#[cfg(test)]
mod frame_tests {
    use super::*;

    #[tokio::test]
    async fn oversized_frame_header_is_rejected_without_allocation() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&u32::MAX.to_be_bytes());
        let mut cursor = futures_util::io::Cursor::new(buf);
        let res: std::io::Result<serde_json::Value> = read_len_prefixed_json(&mut cursor).await;
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert!(err.contains("too large"), "{err}");
    }

    #[tokio::test]
    async fn valid_frame_round_trips() {
        let value = serde_json::json!({"hello": "world"});
        let mut writer = futures_util::io::Cursor::new(Vec::<u8>::new());
        write_len_prefixed_json(&mut writer, &value).await.unwrap();
        let bytes = writer.into_inner();
        let mut reader = futures_util::io::Cursor::new(bytes);
        let out: serde_json::Value = read_len_prefixed_json(&mut reader).await.unwrap();
        assert_eq!(out, value);
    }
}

#[cfg(test)]
mod connectivity_tests {
    use super::*;
    use axum::{routing::get, Router};

    #[derive(NetworkBehaviour)]
    struct TestRelay {
        relay: relay::Behaviour,
        identify: identify::Behaviour,
    }

    fn port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    fn request(path: &str) -> RelayHttpRequest {
        RelayHttpRequest {
            id: random_id(),
            method: "GET".into(),
            path: path.into(),
            query: String::new(),
            headers: vec![],
            body_b64: String::new(),
        }
    }

    fn task_node(
        cfg: P2pConfig,
        key: identity::Keypair,
        dir: &std::path::Path,
    ) -> (
        P2pHandle,
        watch::Sender<bool>,
        tokio::task::JoinHandle<Result<()>>,
        Arc<NetMetrics>,
    ) {
        let db = Arc::new(SocialDb::open(dir.join("social.db")).unwrap());
        let metrics = Arc::new(NetMetrics::new());
        let (stop, shutdown) = watch::channel(false);
        let (tx, rx) = mpsc::channel(64);
        let handle = P2pHandle {
            peer_id: key.public().to_peer_id().to_string(),
            tx,
        };
        let task = tokio::spawn(run_p2p_with_shutdown(
            cfg,
            key,
            "did:fedi3:test".into(),
            "https://test.invalid/users/test".into(),
            "internal-test-token".into(),
            String::new(),
            db,
            metrics.clone(),
            rx,
            Router::new().route("/echo", get(|| async { "echo" })),
            shutdown,
        ));
        (handle, stop, task, metrics)
    }

    #[tokio::test]
    async fn peers_exchange_signed_contacts_and_reconnect_after_restart_without_seeds() {
        use std::sync::atomic::Ordering;
        let a_dir = crate::test_support::TempDir::new("bootstrap-a");
        let b_dir = crate::test_support::TempDir::new("bootstrap-b");
        let a_key = identity::Keypair::generate_ed25519();
        let b_key = identity::Keypair::generate_ed25519();
        let a_id = a_key.public().to_peer_id();
        let a_port = port();
        let base = P2pConfig {
            enable: true,
            public_discovery_enable: Some(false),
            mdns_enable: Some(false),
            gossip_enable: Some(false),
            ..Default::default()
        };
        let a_cfg = P2pConfig {
            listen: Some(format!("/ip4/127.0.0.1/tcp/{a_port}")),
            // Public announcements here are signature fixtures only; the test
            // connects through the loopback bootstrap, never these addresses.
            announce: Some(vec!["/ip4/8.8.8.8/tcp/4001".into()]),
            ..base.clone()
        };
        let (_a, a_stop, a_task, _) = task_node(a_cfg, a_key, a_dir.path());
        let b_cfg = P2pConfig {
            listen: None,
            bootstrap: Some(vec![format!("/ip4/127.0.0.1/tcp/{a_port}/p2p/{a_id}")]),
            ..base.clone()
        };
        let (b, b_stop, b_task, b_metrics) = task_node(b_cfg, b_key.clone(), b_dir.path());
        tokio::time::timeout(Duration::from_secs(12), async {
            while b_metrics.p2p_peer_exchange_contacts.load(Ordering::Relaxed) == 0 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("signed contact was not exchanged over the network");
        assert_eq!(
            b.request(&a_id.to_string(), request("/echo"))
                .await
                .unwrap()
                .status,
            200
        );
        b_stop.send(true).unwrap();
        b_task.await.unwrap().unwrap();
        drop(b);
        let cached = std::fs::read_to_string(b_dir.path().join("p2p_known_peers.json")).unwrap();
        assert!(cached.contains(&a_id.to_string()));
        // Remove the public signature fixture before the autonomous redial:
        // tests must not attempt a connection to an unrelated public service.
        let mut cache: serde_json::Value = serde_json::from_str(&cached).unwrap();
        for entry in cache.as_array_mut().unwrap() {
            entry["addrs"]
                .as_array_mut()
                .unwrap()
                .retain(|addr| !addr.as_str().unwrap().contains("8.8.8.8"));
        }
        std::fs::write(
            b_dir.path().join("p2p_known_peers.json"),
            serde_json::to_vec(&cache).unwrap(),
        )
        .unwrap();
        // No bootstrap, mDNS, public IPFS, or relay is available to the restarted
        // node. Its only route to A is the endpoint cache learned via Identify.
        let (b, stop, task, metrics) = task_node(base, b_key, b_dir.path());
        tokio::time::timeout(Duration::from_secs(8), async {
            while metrics.p2p_connected_peers.load(Ordering::Relaxed) == 0 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("cached peer did not reconnect without bootstrap");
        assert_eq!(
            b.request(&a_id.to_string(), request("/echo"))
                .await
                .unwrap()
                .status,
            200
        );
        stop.send(true).unwrap();
        task.await.unwrap().unwrap();
        a_stop.send(true).unwrap();
        a_task.await.unwrap().unwrap();
    }

    #[tokio::test]
    #[ignore = "contacts public IPFS bootstrap infrastructure; run explicitly"]
    async fn public_ipfs_bootstrap_smoke() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("fedi3_core::p2p=debug,libp2p_dns=debug,libp2p_kad=debug")
            .with_test_writer()
            .try_init();
        use std::sync::atomic::Ordering;
        let dir = crate::test_support::TempDir::new("public-ipfs-smoke");
        let cfg = P2pConfig {
            enable: true,
            mdns_enable: Some(false),
            discovery_enable: Some(false),
            gossip_enable: Some(false),
            ..Default::default()
        };
        let (_handle, stop, task, metrics) =
            task_node(cfg, identity::Keypair::generate_ed25519(), dir.path());
        let result = tokio::time::timeout(Duration::from_secs(120), async {
            while !metrics.p2p_public_discovery_ready.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await;
        stop.send(true).unwrap();
        task.await.unwrap().unwrap();
        result.expect("no public IPFS bootstrap identified a compatible DHT");
    }

    fn node(
        cfg: P2pConfig,
        router: Router,
        shutdown: watch::Receiver<bool>,
    ) -> (P2pHandle, crate::test_support::TempDir) {
        let dir = crate::test_support::TempDir::new("p2p-connectivity");
        let db = Arc::new(SocialDb::open(dir.path().join("social.db")).unwrap());
        let key = identity::Keypair::generate_ed25519();
        let handle = start_p2p(
            cfg,
            key,
            "did:fedi3:test".into(),
            "https://test.invalid/users/test".into(),
            "internal-test-token".into(),
            String::new(),
            db,
            Arc::new(NetMetrics::new()),
            router,
            shutdown,
        )
        .unwrap()
        .unwrap();
        (handle, dir)
    }

    #[tokio::test]
    async fn public_gossip_replicates_to_disk_without_a_relay() {
        let (stop, shutdown) = watch::channel(false);
        let db_dir = crate::test_support::TempDir::new("gossip-replica");
        let replica = Arc::new(SocialDb::open(db_dir.path().join("replica.db")).unwrap());
        let db = replica.clone();
        let router = Router::new().route(
            "/_fedi3/global/ingest",
            axum::routing::post(move |body: axum::body::Bytes| {
                let db = db.clone();
                async move {
                    db.insert_global_feed_item(
                        "gossip-test",
                        Some("https://test.invalid/users/a"),
                        body.to_vec(),
                    )
                    .unwrap();
                    "ok"
                }
            }),
        );
        let listen = port();
        let base = P2pConfig {
            enable: true,
            public_discovery_enable: Some(false),
            mdns_enable: Some(false),
            discovery_enable: Some(false),
            gossip_enable: Some(true),
            ..Default::default()
        };
        let (a, _a_dir) = node(
            P2pConfig {
                listen: Some(format!("/ip4/127.0.0.1/tcp/{listen}")),
                ..base.clone()
            },
            router,
            shutdown.clone(),
        );
        let (b, _b_dir) = node(
            P2pConfig {
                bootstrap: Some(vec![format!(
                    "/ip4/127.0.0.1/tcp/{listen}/p2p/{}",
                    a.peer_id
                )]),
                ..base
            },
            Router::new(),
            shutdown,
        );
        let payload = br#"{"id":"https://test.invalid/notes/gossip-test","type":"Create","to":["https://www.w3.org/ns/activitystreams#Public"]}"#.to_vec();
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                b.publish("/fedi3/global/1", payload.clone()).await.unwrap();
                if replica
                    .global_feed_actor_stats_since("https://test.invalid/users/a", 0)
                    .unwrap()
                    .0
                    > 0
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        })
        .await
        .expect("gossip did not reach the other core over its direct TCP connection");
        let reopened = SocialDb::open(db_dir.path().join("replica.db")).unwrap();
        assert_eq!(
            reopened
                .global_feed_actor_stats_since("https://test.invalid/users/a", 0)
                .unwrap()
                .0,
            1
        );
        stop.send(true).unwrap();
    }

    // Both nodes have no direct listener. The relay initially does not exist;
    // reservations must recover, including resolving its DNS multiaddress.
    // The remote handler then makes a request back through the same swarm.
    #[tokio::test]
    async fn circuit_recovers_from_unavailable_relay_and_handles_nested_requests() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("fedi3_core::p2p=debug,libp2p_relay=debug")
            .with_test_writer()
            .try_init();
        let (stop, shutdown) = watch::channel(false);
        let relay_key = identity::Keypair::generate_ed25519();
        let relay_peer = PeerId::from(relay_key.public());
        let relay_port = port();
        let relay_addr = format!("/dns4/localhost/tcp/{relay_port}/p2p/{relay_peer}");
        let cfg = P2pConfig {
            enable: true,
            force_relay_only: Some(true),
            public_discovery_enable: Some(false),
            mdns_enable: Some(false),
            relay_reserve: Some(vec![relay_addr.clone()]),
            gossip_enable: Some(false),
            discovery_enable: Some(false),
            mailbox_poll_secs: Some(300),
            ..Default::default()
        };
        let (a, _a_dir) = node(
            cfg.clone(),
            Router::new().route("/echo", get(|| async { "echo" })),
            shutdown.clone(),
        );
        let callback = Arc::new(tokio::sync::OnceCell::<P2pHandle>::new());
        let slot = callback.clone();
        let a_id = a.peer_id.clone();
        let (b, _b_dir) = node(
            cfg,
            Router::new().route(
                "/nested",
                get(move || {
                    let slot = slot.clone();
                    let a_id = a_id.clone();
                    async move {
                        let response = slot
                            .get()
                            .unwrap()
                            .request(&a_id, request("/echo"))
                            .await
                            .unwrap();
                        assert_eq!(response.status, 200);
                        B64.decode(response.body_b64).unwrap()
                    }
                }),
            ),
            shutdown.clone(),
        );
        callback
            .set(b.clone())
            .unwrap_or_else(|_| panic!("callback already set"));
        // Let the first reservation attempt fail before bringing up the relay.
        tokio::time::sleep(Duration::from_millis(300)).await;
        let transport = tcp::tokio::Transport::new(tcp::Config::default())
            .upgrade(upgrade::Version::V1)
            .authenticate(noise::Config::new(&relay_key).unwrap())
            .multiplex(yamux::Config::default())
            .boxed();
        let behaviour = TestRelay {
            relay: relay::Behaviour::new(relay_peer, Default::default()),
            identify: identify::Behaviour::new(identify::Config::new(
                "/test/1".into(),
                relay_key.public(),
            )),
        };
        let mut swarm = Swarm::new(
            transport,
            behaviour,
            relay_peer,
            libp2p::swarm::Config::with_tokio_executor()
                .with_idle_connection_timeout(Duration::from_secs(60)),
        );
        let listen_addr: Multiaddr = format!("/ip4/127.0.0.1/tcp/{relay_port}").parse().unwrap();
        swarm.add_external_address(listen_addr.clone());
        swarm.listen_on(listen_addr).unwrap();
        let (reserved_tx, mut reserved_rx) = mpsc::channel(8);
        let relay_task = tokio::spawn(async move {
            loop {
                if let SwarmEvent::Behaviour(TestRelayEvent::Relay(
                    relay::Event::ReservationReqAccepted { src_peer_id, .. },
                )) = swarm.select_next_some().await
                {
                    let _ = reserved_tx.send(src_peer_id).await;
                }
            }
        });
        let mut reserved = HashSet::new();
        tokio::time::timeout(Duration::from_secs(25), async {
            while reserved.len() < 2 {
                reserved.insert(reserved_rx.recv().await.unwrap());
            }
        })
        .await
        .expect("relay reservations did not recover");
        a.add_peer_addrs(
            &b.peer_id,
            vec![format!("{relay_addr}/p2p-circuit/p2p/{}", b.peer_id)],
        )
        .await
        .unwrap();
        b.add_peer_addrs(
            &a.peer_id,
            vec![format!("{relay_addr}/p2p-circuit/p2p/{}", a.peer_id)],
        )
        .await
        .unwrap();
        assert!(a.request("invalid-peer", request("/echo")).await.is_err());
        let responses = tokio::time::timeout(Duration::from_secs(8), async {
            futures_util::future::join_all(
                (0..16).map(|_| a.request(&b.peer_id, request("/nested"))),
            )
            .await
        })
        .await
        .expect("concurrent nested P2P requests deadlocked");
        for response in responses {
            let response = response.expect("cold-peer concurrent request failed");
            assert_eq!(response.status, 200);
            assert_eq!(B64.decode(response.body_b64).unwrap(), b"echo");
        }
        stop.send(true).unwrap();
        relay_task.abort();
    }
}
