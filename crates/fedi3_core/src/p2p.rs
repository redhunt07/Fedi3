/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use crate::relay_bridge::handle_relay_http_request;
use anyhow::{Context, Result};
use async_trait::async_trait;
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

fn addr_is_ipv4_only(addr: &Multiaddr) -> bool {
    for p in addr.iter() {
        match p {
            Protocol::Ip6(_) | Protocol::Dns6(_) | Protocol::Dns(_) | Protocol::Dnsaddr(_) => {
                return false
            }
            _ => {}
        }
    }
    true
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct P2pConfig {
    pub enable: bool,
    pub listen: Option<String>,
    pub bootstrap: Option<Vec<String>>,
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

    /// Force IPv4-only addresses for P2P transport and discovery.
    pub ipv4_only: Option<bool>,

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
            ipv4_only: Some(true),
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

#[async_trait]
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

const MAX_P2P_PAYLOAD_BYTES: usize = 2 * 1024 * 1024;

async fn read_len_prefixed_json<T, V>(io: &mut T) -> std::io::Result<V>
where
    T: futures_util::AsyncRead + Unpin + Send,
    V: serde::de::DeserializeOwned,
{
    use futures_util::AsyncReadExt as _;
    let mut len_buf = [0u8; 4];
    io.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len == 0 || len > MAX_P2P_PAYLOAD_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("p2p payload too large: {len}"),
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
    Identify(identify::Event),
    Ping(ping::Event),
    Rr(request_response::Event<RelayHttpRequest, RelayHttpResponse>),
    Kad(kad::Event),
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
        Self::Identify(v)
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
    mdns: mdns::tokio::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    rr: request_response::Behaviour<Fedi3Codec>,
    kad: kad::Behaviour<kad::store::MemoryStore>,
    autonat: autonat::Behaviour,
    relay: relay::client::Behaviour,
    dcutr: dcutr::Behaviour,
    gossip: gossipsub::Behaviour,
}

async fn run_p2p_with_shutdown(
    cfg: P2pConfig,
    keypair: identity::Keypair,
    did: String,
    self_actor_url: String,
    internal_token: String,
    private_key_pem: String,
    social: Arc<SocialDb>,
    metrics: Arc<NetMetrics>,
    mut out_rx: mpsc::Receiver<OutboundMsg>,
    mut handler: impl Clone
        + Send
        + 'static
        + tower::Service<
            Request<Body>,
            Response = http::Response<Body>,
            Error = std::convert::Infallible,
        >,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    if !cfg.enable {
        return Ok(());
    }

    metrics.set_p2p_enabled(true);
    let peer_id = PeerId::from(keypair.public());
    let ipv4_only = cfg.ipv4_only.unwrap_or(true);
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
    let raw_ws = websocket::WsConfig::new(tcp::tokio::Transport::new(
        tcp::Config::default().nodelay(true),
    ));
    let raw_ws_or_tcp = libp2p::core::transport::choice::OrTransport::new(raw_ws, raw_tcp);
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

    let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?;
    let identify = identify::Behaviour::new(identify::Config::new(
        "/fedi3/identify/1".to_string(),
        keypair.public(),
    ));
    let ping = ping::Behaviour::new(ping::Config::new());
    let rr = request_response::Behaviour::new(
        [(Fedi3Protocol, request_response::ProtocolSupport::Full)],
        request_response::Config::default(),
    );
    let kad = {
        let store = kad::store::MemoryStore::new(peer_id);
        kad::Behaviour::new(peer_id, store)
    };
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
        autonat,
        relay: relay_behaviour,
        dcutr,
        gossip,
    };

    let mut swarm = Swarm::new(
        transport,
        behaviour,
        peer_id,
        libp2p::swarm::Config::with_tokio_executor(),
    );

    if !force_relay_only {
        if let Some(listen) = cfg.listen.as_deref() {
            for s in listen
                .split(',')
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
            {
                let addr: Multiaddr = s.parse()?;
                if ipv4_only && !addr_is_ipv4_only(&addr) {
                    continue;
                }
                swarm.listen_on(addr)?;
            }
        }
    }

    // Ask for circuit relay reservations (best-effort). We listen on `/p2p-circuit` via each relay.
    if let Some(relays) = cfg.relay_reserve.as_ref() {
        for s in relays {
            if let Ok(mut addr) = s.parse::<Multiaddr>() {
                if ipv4_only && !addr_is_ipv4_only(&addr) {
                    continue;
                }
                // Ensure it ends with /p2p/<relay_peer_id>
                let has_peer = addr.iter().any(|p| matches!(p, Protocol::P2p(_)));
                if !has_peer {
                    continue;
                }
                addr.push(Protocol::P2pCircuit);
                let _ = swarm.listen_on(addr);
            }
        }
    }

    let mut boot: HashSet<Multiaddr> = HashSet::new();
    if let Some(items) = &cfg.bootstrap {
        for s in items {
            if let Ok(a) = s.parse::<Multiaddr>() {
                if ipv4_only && !addr_is_ipv4_only(&a) {
                    continue;
                }
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
    if ipv4_only {
        self_addrs.retain(|s| {
            s.parse::<Multiaddr>()
                .ok()
                .map(|a| addr_is_ipv4_only(&a))
                .unwrap_or(false)
        });
    }
    if let Some(relays) = cfg.relay_reserve.as_ref() {
        for r in relays {
            let r = r.trim();
            if r.is_empty() {
                continue;
            }
            if ipv4_only {
                let Ok(base_addr) = r.parse::<Multiaddr>() else {
                    continue;
                };
                if !addr_is_ipv4_only(&base_addr) {
                    continue;
                }
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
    let base_self_addrs = self_record.addrs.clone();
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
                if ipv4_only && !addr_is_ipv4_only(&addr) {
                    continue;
                }
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
            _ = shutdown.changed() => {
                if *shutdown.borrow() { break; }
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
                        let peer: PeerId = peer_id.parse().context("parse peer_id")?;
                        if let Ok(bytes) = serde_json::to_vec(&req) {
                            metrics.p2p_tx_add(bytes.len() as u64);
                        }
                        metrics.p2p_peer_seen(&peer.to_string());
                        let req_id = swarm.behaviour_mut().rr.send_request(&peer, req);
                        pending.insert(req_id, resp_tx);
                    }
                    OutboundMsg::AddAddrs { peer_id, addrs } => {
                        let peer: PeerId = peer_id.parse().context("parse peer_id")?;
                        for a in addrs {
                            if let Ok(addr) = a.parse::<Multiaddr>() {
                                if ipv4_only && !addr_is_ipv4_only(&addr) {
                                    continue;
                                }
                                if dynamic_relay_only {
                                    let ok = addr.iter().any(|p| matches!(p, Protocol::P2pCircuit));
                                    if !ok {
                                        continue;
                                    }
                                }
                                swarm.add_peer_address(peer, addr.clone());
                                let _ = swarm.dial(addr);
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
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!(%peer_id, %address, "p2p listening");
                    }
                    SwarmEvent::ExternalAddrConfirmed { address } => {
                        if !prefer_relay_addrs {
                            if !ipv4_only || addr_is_ipv4_only(&address) {
                                observed_addrs.insert(address);
                            }
                        }
                    }
                    SwarmEvent::ConnectionEstablished { peer_id: remote_peer, .. } => {
                        connected_peers.insert(remote_peer);
                        metrics.p2p_connected_peers_set(connected_peers.len() as u64);
                        if !kad_bootstrapped {
                            let _ = swarm.behaviour_mut().kad.bootstrap();
                            kad_bootstrapped = true;
                        }
                    }
                    SwarmEvent::ConnectionClosed { peer_id: remote_peer, .. } => {
                        connected_peers.remove(&remote_peer);
                        metrics.p2p_connected_peers_set(connected_peers.len() as u64);
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received { info, .. })) => {
                        if !prefer_relay_addrs {
                            if !ipv4_only || addr_is_ipv4_only(&info.observed_addr) {
                                swarm.add_external_address(info.observed_addr.clone());
                                observed_addrs.insert(info.observed_addr);
                            }
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                        for (p, addr) in list {
                            if ipv4_only && !addr_is_ipv4_only(&addr) {
                                continue;
                            }
                            info!(peer=%p, %addr, "p2p discovered");
                            swarm.add_peer_address(p, addr);
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Expired(_list))) => {}
                    SwarmEvent::Behaviour(BehaviourEvent::Rr(request_response::Event::Message { peer, message })) => {
                        match message {
                            request_response::Message::Request { request, channel, .. } => {
                                if let Ok(bytes) = serde_json::to_vec(&request) {
                                    metrics.p2p_rx_add(bytes.len() as u64);
                                }
                                metrics.p2p_peer_seen(&peer.to_string());
                                let request = maybe_decrypt(&private_key_pem, request);
                                let resp = handle_relay_http_request(&mut handler, request).await;
                                if let Ok(bytes) = serde_json::to_vec(&resp) {
                                    metrics.p2p_tx_add(bytes.len() as u64);
                                }
                                if swarm.behaviour_mut().rr.send_response(channel, resp).is_err() {
                                    error!(peer=%peer, "p2p send_response failed");
                                }
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
                                    let mut ack_ids: Vec<String> = Vec::new();
                                    for msg in poll.messages {
                                        let dedup_key = format!("urn:fedi3:mbx:{}", msg.id);
                                        if !social.mark_inbox_seen(&dedup_key).unwrap_or(false) {
                                            continue;
                                        }
                                        metrics.mailbox_peer_seen(&mbx_peer.to_string());
                                        let req = maybe_decrypt(&private_key_pem, msg.req);
                                        let resp = handle_relay_http_request(&mut handler, req).await;
                                        if (200..300).contains(&resp.status) || resp.status == 202 {
                                            ack_ids.push(msg.id);
                                        }
                                    }
                                    if !ack_ids.is_empty() {
                                        let ack_body = serde_json::json!({
                                            "for_peer_id": peer_id.to_string(),
                                            "ids": ack_ids,
                                        });
                                        let ack_bytes = serde_json::to_vec(&ack_body).unwrap_or_default();
                                        let ack_req = RelayHttpRequest {
                                            id: format!("mbx-ack-{}", random_id()),
                                            method: "POST".to_string(),
                                            path: "/.fedi3/mailbox/ack".to_string(),
                                            query: "".to_string(),
                                            headers: vec![("content-type".to_string(), "application/json".to_string())],
                                            body_b64: B64.encode(ack_bytes),
                                        };
                                        metrics.mailbox_tx_add(ack_req.body_b64.len() as u64);
                                        let req_id = swarm.behaviour_mut().rr.send_request(&mbx_peer, ack_req);
                                        mailbox_pending.insert(req_id, mbx_peer.clone());
                                        mailbox_sent_at.insert(req_id, now_ms());
                                    }
                                }
                            }
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Rr(request_response::Event::OutboundFailure { request_id, error, .. })) => {
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
                        let _ = message.topic == gossip_topic_global.hash();
                        // Ingest via internal endpoint to store & apply policy.
                        let req = http::Request::builder()
                            .method(http::Method::POST)
                            .uri("http://localhost/_fedi3/global/ingest")
                            .header("Content-Type", "application/activity+json")
                            .header("X-Fedi3-Internal", internal_token.clone())
                            .body(Body::from(message.data))
                            .unwrap();
                         let _ = handler.clone().oneshot(req).await;
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
                    SwarmEvent::Behaviour(BehaviourEvent::Relay(_)) => {}
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
        return Ok(kp);
    }
    let kp = identity::Keypair::generate_ed25519();
    let bytes = kp.to_protobuf_encoding().context("encode p2p keypair")?;
    std::fs::write(&path, bytes).context("write p2p keypair")?;
    Ok(kp)
}

pub fn peer_id_string(keypair: &identity::Keypair) -> String {
    PeerId::from(keypair.public()).to_string()
}
