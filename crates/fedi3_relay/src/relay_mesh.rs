/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use anyhow::{Context, Result};
use async_trait::async_trait;
use libp2p::futures::{future::Either, StreamExt};
use libp2p::{
    core::muxing::StreamMuxerBox,
    core::upgrade,
    dns, identify, identity, kad, noise, ping, quic, relay, request_response,
    swarm::dial_opts::{DialOpts, PeerCondition},
    swarm::{derive_prelude::*, SwarmEvent},
    tcp, websocket, yamux, Multiaddr, PeerId, Swarm, Transport,
};
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use ed25519_dalek::{Signer as _, Verifier as _};

use crate::relay_notes::{
    note_to_index, RelayActorIndex, RelayMediaIndex, RelayMeshPeerHint, RelaySyncActorItem,
    RelaySyncBundle, RelaySyncMediaItem, RelaySyncNoteItem,
};
use crate::{now_ms, relay_p2p_infra_multiaddrs, AppState, RelayTelemetry};

const RELAY_REPUTATION_MIN_SCORE: i32 = -3;
const RELAY_REPUTATION_MAX_SCORE: i32 = 10;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RelayMeshSyncRequest {
    since: Option<i64>,
    cursor: Option<i64>,
    limit: u32,
    relay_url: Option<String>,
    created_at_ms: Option<i64>,
    sign_pubkey_b64: Option<String>,
    signature_b64: Option<String>,
}

#[derive(Clone)]
struct RelayMeshConfig {
    listen: Vec<Multiaddr>,
    bootstrap: Vec<Multiaddr>,
    relay_reserve: Vec<Multiaddr>,
    key_path: PathBuf,
    enable_quic: bool,
    sync_interval_secs: u64,
    sync_limit: u32,
    reputation_ttl_ms: i64,
    diagnostics: bool,
    diagnostics_sample_n: u64,
}

pub fn spawn_relay_mesh(state: AppState) {
    if !state.cfg.relay_mesh_enable {
        return;
    }
    tokio::spawn(async move {
        if let Err(e) = run_relay_mesh(state).await {
            error!("relay mesh failed: {e:#}");
        }
    });
}

fn load_or_generate_keypair(path: &PathBuf) -> Result<identity::Keypair> {
    if let Ok(bytes) = std::fs::read(path) {
        return identity::Keypair::from_protobuf_encoding(&bytes).context("decode keypair");
    }
    let kp = identity::Keypair::generate_ed25519();
    let bytes = kp.to_protobuf_encoding().context("encode keypair")?;
    write_private_file(path, &bytes).context("write keypair")?;
    Ok(kp)
}

fn write_private_file(path: &PathBuf, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("create parent dir")?;
    }
    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .context("open private file")?;
        file.write_all(bytes).context("write private file")?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes).context("write private file")?;
    }
    Ok(())
}

fn load_mesh_config(cfg: &crate::RelayConfig) -> RelayMeshConfig {
    let listen = cfg
        .relay_mesh_listen
        .iter()
        .filter_map(|s| s.parse::<Multiaddr>().ok())
        .filter(|addr| is_supported_mesh_addr(addr, cfg.relay_mesh_enable_quic))
        .collect::<Vec<_>>();
    let bootstrap = cfg
        .relay_mesh_bootstrap
        .iter()
        .filter_map(|s| s.parse::<Multiaddr>().ok())
        .filter(|addr| is_supported_mesh_addr(addr, cfg.relay_mesh_enable_quic))
        .collect::<Vec<_>>();
    let mut relay_reserve = Vec::new();
    for s in relay_p2p_infra_multiaddrs(cfg) {
        if let Ok(addr) = s.parse::<Multiaddr>() {
            if is_supported_mesh_addr(&addr, cfg.relay_mesh_enable_quic) {
                relay_reserve.push(addr);
            }
        }
    }
    RelayMeshConfig {
        listen,
        bootstrap,
        relay_reserve,
        key_path: cfg.relay_mesh_key_path.clone(),
        enable_quic: cfg.relay_mesh_enable_quic,
        sync_interval_secs: cfg.relay_sync_interval_secs.max(30),
        sync_limit: cfg.relay_sync_limit.min(200).max(1),
        reputation_ttl_ms: (cfg.relay_reputation_ttl_secs as i64) * 1000,
        diagnostics: cfg.relay_mesh_diagnostics,
        diagnostics_sample_n: cfg.relay_mesh_diagnostics_sample_n.max(1),
    }
}

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "BehaviourEvent")]
struct Behaviour {
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    kad: kad::Behaviour<kad::store::MemoryStore>,
    relay: relay::client::Behaviour,
    rr: request_response::Behaviour<RelayMeshCodec>,
}

#[derive(Debug)]
enum BehaviourEvent {
    Identify(()),
    Ping(()),
    Kad(()),
    Relay(()),
    Rr(request_response::Event<RelayMeshSyncRequest, RelaySyncBundle>),
}

impl From<identify::Event> for BehaviourEvent {
    fn from(_: identify::Event) -> Self {
        Self::Identify(())
    }
}
impl From<ping::Event> for BehaviourEvent {
    fn from(_: ping::Event) -> Self {
        Self::Ping(())
    }
}
impl From<kad::Event> for BehaviourEvent {
    fn from(_: kad::Event) -> Self {
        Self::Kad(())
    }
}
impl From<relay::client::Event> for BehaviourEvent {
    fn from(_: relay::client::Event) -> Self {
        Self::Relay(())
    }
}
impl From<request_response::Event<RelayMeshSyncRequest, RelaySyncBundle>> for BehaviourEvent {
    fn from(v: request_response::Event<RelayMeshSyncRequest, RelaySyncBundle>) -> Self {
        Self::Rr(v)
    }
}

#[derive(Clone)]
struct RelayMeshProtocol;

impl AsRef<str> for RelayMeshProtocol {
    fn as_ref(&self) -> &str {
        "/fedi3/relay-sync/1"
    }
}

#[derive(Clone, Default)]
struct RelayMeshCodec;

#[async_trait]
impl request_response::Codec for RelayMeshCodec {
    type Protocol = RelayMeshProtocol;
    type Request = RelayMeshSyncRequest;
    type Response = RelaySyncBundle;

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

const MAX_SYNC_BYTES: usize = 2 * 1024 * 1024;

async fn read_len_prefixed_json<T, V>(io: &mut T) -> std::io::Result<V>
where
    T: futures_util::AsyncRead + Unpin + Send,
    V: serde::de::DeserializeOwned,
{
    use futures_util::AsyncReadExt as _;
    let mut len_buf = [0u8; 4];
    io.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len == 0 || len > MAX_SYNC_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("sync payload too large: {len}"),
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

struct PendingSync {
    relay_url: String,
    peer_id: PeerId,
    last_seen: i64,
    max_seen: i64,
}

fn mesh_diag_enabled(cfg: &RelayMeshConfig, tick: u64) -> bool {
    cfg.diagnostics && (tick % cfg.diagnostics_sample_n.max(1) == 0)
}

async fn run_relay_mesh(state: AppState) -> Result<()> {
    let cfg = load_mesh_config(&state.cfg);
    let keypair = load_or_generate_keypair(&cfg.key_path)?;
    let peer_id = PeerId::from(keypair.public());
    *state.relay_mesh_peer_id.write().await = Some(peer_id.to_string());

    let (relay_transport, relay_behaviour) = relay::client::new(peer_id);
    let raw_tcp = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true));
    let raw_ws = websocket::WsConfig::new(tcp::tokio::Transport::new(
        tcp::Config::default().nodelay(true),
    ));
    let raw_ws_or_tcp = libp2p::core::transport::choice::OrTransport::new(raw_ws, raw_tcp);
    let dns_ws_or_tcp = dns::tokio::Transport::system(raw_ws_or_tcp)?;
    let raw_stream =
        libp2p::core::transport::choice::OrTransport::new(relay_transport, dns_ws_or_tcp);
    let tcp_transport = raw_stream
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(&keypair)?)
        .multiplex(yamux::Config::default())
        .timeout(Duration::from_secs(20))
        .boxed();
    let transport = if cfg.enable_quic {
        let quic_transport = quic::tokio::Transport::new(quic::Config::new(&keypair));
        libp2p::core::transport::choice::OrTransport::new(quic_transport, tcp_transport)
            .map(|either, _| match either {
                Either::Left((peer, conn)) => (peer, StreamMuxerBox::new(conn)),
                Either::Right((peer, muxer)) => (peer, muxer),
            })
            .boxed()
    } else {
        tcp_transport
    };

    let identify = identify::Behaviour::new(identify::Config::new(
        "/fedi3/relay-mesh/1".to_string(),
        keypair.public(),
    ));
    let ping = ping::Behaviour::new(ping::Config::new());
    let mut kad = {
        let store = kad::store::MemoryStore::new(peer_id);
        kad::Behaviour::new(peer_id, store)
    };
    kad.set_mode(Some(kad::Mode::Client));
    let rr_cfg = request_response::Config::default().with_request_timeout(Duration::from_secs(20));
    let rr = request_response::Behaviour::new(
        [(RelayMeshProtocol, request_response::ProtocolSupport::Full)],
        rr_cfg,
    );

    let behaviour = Behaviour {
        identify,
        ping,
        kad,
        relay: relay_behaviour,
        rr,
    };
    let mut swarm = Swarm::new(
        transport,
        behaviour,
        peer_id,
        libp2p::swarm::Config::with_tokio_executor().with_idle_connection_timeout(
            Duration::from_secs(cfg.sync_interval_secs.max(120) + 30),
        ),
    );

    for addr in &cfg.listen {
        if let Err(e) = swarm.listen_on(addr.clone()) {
            warn!(%addr, "relay mesh listen failed: {e}");
        }
    }

    for mut addr in cfg.relay_reserve.clone() {
        let has_peer = addr
            .iter()
            .any(|p| matches!(p, libp2p::multiaddr::Protocol::P2p(_)));
        if !has_peer {
            continue;
        }
        addr.push(libp2p::multiaddr::Protocol::P2pCircuit);
        if let Err(e) = swarm.listen_on(addr.clone()) {
            warn!(%addr, "relay mesh reserve listen failed: {e}");
        }
    }

    let mut bootstrap_peers: HashSet<Multiaddr> = HashSet::new();
    for addr in &cfg.bootstrap {
        bootstrap_peers.insert(addr.clone());
    }
    for addr in &bootstrap_peers {
        if let Some(pid) = extract_peer_id(addr) {
            swarm.behaviour_mut().kad.add_address(&pid, addr.clone());
        }
        if cfg.diagnostics {
            debug!(%addr, "relay mesh bootstrap dial candidate");
        }
        let _ = swarm.dial(addr.clone());
    }

    info!(%peer_id, "relay mesh enabled");

    let mut pending: HashMap<request_response::OutboundRequestId, PendingSync> = HashMap::new();
    let mut inflight_relays: HashSet<String> = HashSet::new();
    let mut connected_peers: HashSet<PeerId> = HashSet::new();
    let mut bootstrapped = false;
    let mut diag_tick: u64 = 0;
    let mut sync_tick = tokio::time::interval(Duration::from_secs(cfg.sync_interval_secs));
    sync_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = sync_tick.tick() => {
                if let Err(e) = queue_sync_requests(&state, &mut swarm, &cfg, &mut pending, &mut inflight_relays, &connected_peers).await {
                    warn!("relay mesh sync tick failed: {e:#}");
                }
            }
            ev = swarm.select_next_some() => {
                diag_tick = diag_tick.wrapping_add(1);
                match ev {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!(%peer_id, %address, "relay mesh listening");
                    }
                    SwarmEvent::Dialing { peer_id: target_peer_id, connection_id } => {
                        if mesh_diag_enabled(&cfg, diag_tick) {
                            debug!(
                                local_peer_id = %peer_id,
                                target_peer_id = ?target_peer_id,
                                ?connection_id,
                                "relay mesh dialing"
                            );
                        }
                    }
                    SwarmEvent::ConnectionEstablished { peer_id: remote_peer_id, connection_id, endpoint, num_established, .. } => {
                        connected_peers.insert(remote_peer_id);
                        if mesh_diag_enabled(&cfg, diag_tick) {
                            info!(
                                local_peer_id = %peer_id,
                                remote_peer_id = %remote_peer_id,
                                ?connection_id,
                                ?endpoint,
                                num_established,
                                "relay mesh connection established"
                            );
                        }
                        if !bootstrapped && !bootstrap_peers.is_empty() {
                            let _ = swarm.behaviour_mut().kad.bootstrap();
                            bootstrapped = true;
                        }
                        if let Err(e) = queue_sync_requests_for_peer(
                            &state,
                            &mut swarm,
                            &cfg,
                            &mut pending,
                            &mut inflight_relays,
                            &connected_peers,
                            remote_peer_id,
                        ).await {
                            warn!(
                                peer_id = %remote_peer_id,
                                "relay mesh immediate sync queue failed: {e:#}"
                            );
                        }
                    }
                    SwarmEvent::ConnectionClosed { peer_id: remote_peer_id, connection_id, endpoint, cause, num_established, .. } => {
                        if num_established == 0 {
                            connected_peers.remove(&remote_peer_id);
                        }
                        if mesh_diag_enabled(&cfg, diag_tick) {
                            debug!(
                                local_peer_id = %peer_id,
                                remote_peer_id = %remote_peer_id,
                                ?connection_id,
                                ?endpoint,
                                ?cause,
                                num_established,
                                "relay mesh connection closed"
                            );
                        }
                    }
                    SwarmEvent::OutgoingConnectionError { peer_id: target_peer_id, connection_id, error } => {
                        if let Some(pid) = target_peer_id {
                            connected_peers.remove(&pid);
                        }
                        if mesh_diag_enabled(&cfg, diag_tick) {
                            warn!(
                                local_peer_id = %peer_id,
                                target_peer_id = ?target_peer_id,
                                ?connection_id,
                                error = ?error,
                                "relay mesh outgoing connection error"
                            );
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Rr(request_response::Event::Message { peer, message })) => {
                        match message {
                            request_response::Message::Request { request, channel, .. } => {
                                let _ = peer;
                                let resp = handle_sync_request(&state, &cfg, request).await;
                                let _ = swarm.behaviour_mut().rr.send_response(channel, resp);
                            }
                            request_response::Message::Response { request_id, response } => {
                                handle_sync_response(&state, &cfg, &mut swarm, &mut pending, &mut inflight_relays, request_id, response).await;
                            }
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Rr(request_response::Event::OutboundFailure { request_id, error, .. })) => {
                        if let Some(pending) = pending.remove(&request_id) {
                            inflight_relays.remove(&pending.relay_url);
                            if mesh_diag_enabled(&cfg, diag_tick) {
                                warn!(
                                    relay = %pending.relay_url,
                                    peer_id = %pending.peer_id,
                                    request_id = ?request_id,
                                    error = ?error,
                                    "relay mesh outbound failure detailed"
                                );
                            }
                            warn!(relay=%pending.relay_url, "relay mesh request failed: {error}");
                        }
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Rr(request_response::Event::ResponseSent { .. })) => {}
                    _ => {}
                }
            }
        }
    }
}

async fn handle_sync_request(
    state: &AppState,
    cfg: &RelayMeshConfig,
    req: RelayMeshSyncRequest,
) -> RelaySyncBundle {
    if let Err(e) = verify_mesh_request(state, cfg, &req).await {
        let relay_url = req.relay_url.as_deref().unwrap_or("").trim_end_matches('/');
        if !relay_url.is_empty() {
            update_reputation(state, relay_url, -2, cfg.reputation_ttl_ms).await;
        }
        warn!(relay_url = %relay_url, "relay mesh request rejected: {e:#}");
        return build_empty_bundle(state).await;
    }

    let limit = req.limit.min(cfg.sync_limit).max(1);
    let db = state.db.lock().await;
    let note_page = db
        .list_relay_notes_sync(limit, req.since, req.cursor)
        .unwrap_or_else(|_| crate::CollectionPage {
            total: 0,
            items: Vec::new(),
            next: None,
        });
    let notes = note_page
        .items
        .into_iter()
        .filter_map(|(note_json, created_at_ms)| {
            serde_json::from_str::<serde_json::Value>(&note_json)
                .ok()
                .map(|note| RelaySyncNoteItem {
                    note,
                    created_at_ms,
                })
        })
        .collect::<Vec<_>>();
    let media_page = db
        .list_relay_media_sync(limit, req.since, req.cursor)
        .unwrap_or_else(|_| crate::CollectionPage {
            total: 0,
            items: Vec::new(),
            next: None,
        });
    let media = media_page
        .items
        .into_iter()
        .map(|item| RelaySyncMediaItem {
            url: item.url,
            media_type: item.media_type,
            name: item.name,
            width: item.width,
            height: item.height,
            blurhash: item.blurhash,
            created_at_ms: item.created_at_ms,
        })
        .collect::<Vec<_>>();
    let actor_page = db
        .list_relay_actor_sync(limit, req.since, req.cursor)
        .unwrap_or_else(|_| crate::CollectionPage {
            total: 0,
            items: Vec::new(),
            next: None,
        });
    let actors = actor_page
        .items
        .into_iter()
        .map(|item| RelaySyncActorItem {
            actor_url: item.actor_url,
            username: item.username,
            actor_json: item.actor_json,
            updated_at_ms: item.updated_at_ms,
        })
        .collect::<Vec<_>>();

    let mut next_candidates = Vec::new();
    if let Some(next) = note_page
        .next
        .as_deref()
        .and_then(|v| v.parse::<i64>().ok())
    {
        next_candidates.push(next);
    }
    if let Some(next) = media_page
        .next
        .as_deref()
        .and_then(|v| v.parse::<i64>().ok())
    {
        next_candidates.push(next);
    }
    if let Some(next) = actor_page
        .next
        .as_deref()
        .and_then(|v| v.parse::<i64>().ok())
    {
        next_candidates.push(next);
    }
    let next = next_candidates.into_iter().min().map(|v| v.to_string());

    let mut bundle = build_sync_bundle(state, notes, media, actors, next).await;
    if !bundle.relay_url.is_empty() {
        if let Ok((_, sk_b64)) = db.load_or_create_signing_keypair_b64() {
            if let Ok(sig) = sign_bundle_b64(&bundle, &sk_b64) {
                bundle.signature_b64 = Some(sig);
            }
        }
    }
    bundle
}

async fn handle_sync_response(
    state: &AppState,
    cfg: &RelayMeshConfig,
    swarm: &mut Swarm<Behaviour>,
    pending: &mut HashMap<request_response::OutboundRequestId, PendingSync>,
    inflight_relays: &mut HashSet<String>,
    request_id: request_response::OutboundRequestId,
    response: RelaySyncBundle,
) {
    let Some(mut pend) = pending.remove(&request_id) else {
        return;
    };

    let expected_relay = pend.relay_url.trim_end_matches('/');
    let response_relay = response.relay_url.trim_end_matches('/');
    if expected_relay != response_relay {
        update_reputation(state, &pend.relay_url, -2, cfg.reputation_ttl_ms).await;
        inflight_relays.remove(&pend.relay_url);
        warn!(
            relay_url = %pend.relay_url,
            response_relay = %response.relay_url,
            "relay mesh response relay_url mismatch"
        );
        return;
    }

    let signature_ok = {
        let db = state.db.lock().await;
        let pk_b64 = db.get_relay_pubkey_b64(&pend.relay_url).ok().flatten();
        if response.signature_b64.is_none() {
            false
        } else if let Some(pk_b64) = pk_b64 {
            verify_bundle_signature(&response, &pk_b64).is_ok()
        } else {
            false
        }
    };

    if !signature_ok {
        update_reputation(state, &pend.relay_url, -2, cfg.reputation_ttl_ms).await;
        inflight_relays.remove(&pend.relay_url);
        warn!(
            relay_url = %pend.relay_url,
            "relay mesh response signature invalid"
        );
        return;
    }
    update_reputation(state, &pend.relay_url, 1, cfg.reputation_ttl_ms).await;
    let RelaySyncBundle {
        notes,
        media,
        actors,
        peer_hints,
        next,
        ..
    } = response;
    let discovered = ingest_peer_hints(state, &peer_hints).await;
    if discovered > 0 {
        info!(
            relay_url = %pend.relay_url,
            discovered,
            "relay mesh peer hints discovered"
        );
    }

    let mut item_count = 0usize;
    if !notes.is_empty() || !media.is_empty() || !actors.is_empty() {
        let db = state.db.lock().await;
        for item in notes {
            item_count += 1;
            if item.created_at_ms > pend.max_seen {
                pend.max_seen = item.created_at_ms;
            }
            if let Some(mut indexed) = note_to_index(&item.note) {
                indexed.created_at_ms = item.created_at_ms;
                let _ = db.upsert_relay_note(&indexed);
            }
        }
        for item in media {
            item_count += 1;
            if item.created_at_ms > pend.max_seen {
                pend.max_seen = item.created_at_ms;
            }
            let idx = RelayMediaIndex {
                url: item.url,
                media_type: item.media_type,
                name: item.name,
                width: item.width,
                height: item.height,
                blurhash: item.blurhash,
                created_at_ms: item.created_at_ms,
            };
            let _ = db.upsert_relay_media(&idx);
        }
        for item in actors {
            item_count += 1;
            if item.updated_at_ms > pend.max_seen {
                pend.max_seen = item.updated_at_ms;
            }
            let idx = RelayActorIndex {
                actor_url: item.actor_url,
                username: item.username,
                actor_json: item.actor_json,
                updated_at_ms: item.updated_at_ms,
            };
            let _ = db.upsert_relay_actor(&idx);
        }
        info!(
            relay_url = %pend.relay_url,
            items = item_count,
            max_seen = pend.max_seen,
            "relay mesh sync applied"
        );
    }

    if let Some(next) = next.and_then(|v| v.parse::<i64>().ok()) {
        let req = match build_signed_mesh_request(state, None, Some(next), cfg.sync_limit).await {
            Ok(v) => v,
            Err(_) => {
                inflight_relays.remove(&pend.relay_url);
                return;
            }
        };
        let req_id = swarm.behaviour_mut().rr.send_request(&pend.peer_id, req);
        pending.insert(
            req_id,
            PendingSync {
                relay_url: pend.relay_url,
                peer_id: pend.peer_id,
                last_seen: pend.last_seen,
                max_seen: pend.max_seen,
            },
        );
        return;
    }

    inflight_relays.remove(&pend.relay_url);
    if pend.max_seen > pend.last_seen {
        let db = state.db.lock().await;
        let key = format!("relay_sync_last_ms:{}", pend.relay_url);
        let _ = db.relay_meta_set(&key, &pend.max_seen.to_string());
    }
}

async fn queue_sync_requests(
    state: &AppState,
    swarm: &mut Swarm<Behaviour>,
    cfg: &RelayMeshConfig,
    pending: &mut HashMap<request_response::OutboundRequestId, PendingSync>,
    inflight_relays: &mut HashSet<String>,
    connected_peers: &HashSet<PeerId>,
) -> Result<()> {
    let limit = cfg.sync_limit.min(200).max(1);
    let local_peer_id = state
        .relay_mesh_peer_id
        .read()
        .await
        .clone()
        .unwrap_or_default();
    let (relays, last_map) = {
        let db = state.db.lock().await;
        let relays = db.list_relays(500).unwrap_or_default();
        let last = db.list_relay_sync_state().unwrap_or_default();
        let mut map = HashMap::new();
        for (relay_url, last_ms) in last {
            map.insert(relay_url, last_ms);
        }
        (relays, map)
    };

    for (relay_url, _base_domain, _last_seen, telemetry_json, _sig) in relays {
        if inflight_relays.contains(&relay_url) {
            continue;
        }
        if !reputation_allows(state, &relay_url, cfg.reputation_ttl_ms).await {
            continue;
        }
        let Some(telemetry_json) = telemetry_json else {
            continue;
        };
        let Some(peer_id) = relay_peer_id_from_telemetry(&telemetry_json) else {
            continue;
        };
        if cfg.diagnostics {
            debug!(
                relay_url = %relay_url,
                telemetry_peer_id = %peer_id,
                "relay mesh telemetry peer id selected"
            );
        }
        if peer_id.to_string() == local_peer_id {
            continue;
        }

        let since = last_map.get(&relay_url).copied();
        let direct_addrs = peer_direct_addrs(&relay_url);
        let circuit_addrs = peer_circuit_addrs(&cfg.relay_reserve);
        if cfg.diagnostics {
            debug!(
                relay_url = %relay_url,
                peer_id = %peer_id,
                direct_candidates = ?direct_addrs,
                circuit_candidates = ?circuit_addrs,
                "relay mesh dial candidates prepared"
            );
        }
        if !connected_peers.contains(&peer_id) {
            let dial_addrs = peer_direct_dial_addrs(&relay_url, peer_id);
            if !dial_addrs.is_empty() {
                let opts = DialOpts::peer_id(peer_id)
                    .condition(PeerCondition::DisconnectedAndNotDialing)
                    .addresses(dial_addrs.clone())
                    .build();
                if let Err(e) = swarm.dial(opts) {
                    if cfg.diagnostics {
                        let msg = e.to_string();
                        if msg.contains("dial condition was configured") {
                            debug!(
                                relay_url = %relay_url,
                                peer_id = %peer_id,
                                candidates = ?dial_addrs,
                                "relay mesh explicit direct dial skipped: {msg}"
                            );
                        } else {
                            warn!(
                                relay_url = %relay_url,
                                peer_id = %peer_id,
                                candidates = ?dial_addrs,
                                "relay mesh explicit direct dial failed: {msg}"
                            );
                        }
                    }
                } else if cfg.diagnostics {
                    debug!(
                        relay_url = %relay_url,
                        peer_id = %peer_id,
                        candidates = ?dial_addrs,
                        "relay mesh explicit direct dial queued"
                    );
                }
            }
            // Let the connection establish first; send_request is gated to active peers.
            continue;
        }
        let req = match build_signed_mesh_request(state, since, None, limit).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let req_id = swarm.behaviour_mut().rr.send_request(&peer_id, req);
        info!(
            relay_url = %relay_url,
            peer_id = %peer_id,
            since = ?since,
            "relay mesh sync request queued"
        );
        pending.insert(
            req_id,
            PendingSync {
                relay_url: relay_url.clone(),
                peer_id,
                last_seen: since.unwrap_or(0),
                max_seen: since.unwrap_or(0),
            },
        );
        inflight_relays.insert(relay_url);
    }

    Ok(())
}

async fn queue_sync_requests_for_peer(
    state: &AppState,
    swarm: &mut Swarm<Behaviour>,
    cfg: &RelayMeshConfig,
    pending: &mut HashMap<request_response::OutboundRequestId, PendingSync>,
    inflight_relays: &mut HashSet<String>,
    connected_peers: &HashSet<PeerId>,
    target_peer_id: PeerId,
) -> Result<()> {
    let limit = cfg.sync_limit.min(200).max(1);
    let local_peer_id = state
        .relay_mesh_peer_id
        .read()
        .await
        .clone()
        .unwrap_or_default();
    if target_peer_id.to_string() == local_peer_id || !connected_peers.contains(&target_peer_id) {
        return Ok(());
    }

    let (relays, last_map) = {
        let db = state.db.lock().await;
        let relays = db.list_relays(500).unwrap_or_default();
        let last = db.list_relay_sync_state().unwrap_or_default();
        let mut map = HashMap::new();
        for (relay_url, last_ms) in last {
            map.insert(relay_url, last_ms);
        }
        (relays, map)
    };

    for (relay_url, _base_domain, _last_seen, telemetry_json, _sig) in relays {
        if inflight_relays.contains(&relay_url) {
            continue;
        }
        if !reputation_allows(state, &relay_url, cfg.reputation_ttl_ms).await {
            continue;
        }
        let Some(telemetry_json) = telemetry_json else {
            continue;
        };
        let Some(peer_id) = relay_peer_id_from_telemetry(&telemetry_json) else {
            continue;
        };
        if peer_id != target_peer_id {
            continue;
        }

        let since = last_map.get(&relay_url).copied();
        let req = match build_signed_mesh_request(state, since, None, limit).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let req_id = swarm.behaviour_mut().rr.send_request(&peer_id, req);
        info!(
            relay_url = %relay_url,
            peer_id = %peer_id,
            since = ?since,
            "relay mesh sync request queued (on connect)"
        );
        pending.insert(
            req_id,
            PendingSync {
                relay_url: relay_url.clone(),
                peer_id,
                last_seen: since.unwrap_or(0),
                max_seen: since.unwrap_or(0),
            },
        );
        inflight_relays.insert(relay_url);
        break;
    }

    Ok(())
}

fn extract_peer_id(addr: &Multiaddr) -> Option<PeerId> {
    for p in addr.iter() {
        if let libp2p::multiaddr::Protocol::P2p(pid) = p {
            return Some(pid);
        }
    }
    None
}

fn peer_circuit_addrs(base: &[Multiaddr]) -> Vec<Multiaddr> {
    let mut out = Vec::new();
    for addr in base {
        let has_peer = addr
            .iter()
            .any(|p| matches!(p, libp2p::multiaddr::Protocol::P2p(_)));
        if !has_peer {
            continue;
        }
        let mut a = addr.clone();
        a.push(libp2p::multiaddr::Protocol::P2pCircuit);
        out.push(a);
    }
    out
}

fn peer_direct_addrs(relay_url: &str) -> Vec<Multiaddr> {
    let mut out = Vec::new();
    let Ok(url) = reqwest::Url::parse(relay_url) else {
        return out;
    };
    let Some(host) = url.host_str() else {
        return out;
    };
    if let Ok(addrs) = (host, 4011).to_socket_addrs() {
        for sock in addrs {
            let ip = sock.ip();
            if is_local_dial_candidate(ip) {
                continue;
            }
            let host_proto = if ip.is_ipv6() {
                format!("/ip6/{ip}")
            } else {
                format!("/ip4/{ip}")
            };
            let tcp = format!("{host_proto}/tcp/4011");
            if let Ok(addr) = tcp.parse::<Multiaddr>() {
                out.push(addr);
            }
        }
    }
    if out.is_empty() {
        let host_proto = if host.contains(':') {
            format!("/ip6/{host}")
        } else {
            format!("/dns4/{host}")
        };
        let tcp = format!("{host_proto}/tcp/4011");
        if let Ok(addr) = tcp.parse::<Multiaddr>() {
            out.push(addr);
        }
    }
    out
}

fn peer_direct_dial_addrs(relay_url: &str, peer_id: PeerId) -> Vec<Multiaddr> {
    let mut out = Vec::new();
    for mut addr in peer_direct_addrs(relay_url) {
        addr.push(libp2p::multiaddr::Protocol::P2p(peer_id.into()));
        out.push(addr);
    }
    out
}

fn is_supported_mesh_addr(addr: &Multiaddr, enable_quic: bool) -> bool {
    let mut has_tcp = false;
    for p in addr.iter() {
        match p {
            libp2p::multiaddr::Protocol::QuicV1 => {
                if !enable_quic {
                    return false;
                }
            }
            libp2p::multiaddr::Protocol::Udp(_) => {
                if !enable_quic {
                    return false;
                }
            }
            libp2p::multiaddr::Protocol::Tcp(_) => has_tcp = true,
            _ => {}
        }
    }
    enable_quic || has_tcp
}

fn is_local_dial_candidate(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_unspecified(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
    }
}

fn relay_peer_id_from_telemetry(telemetry_json: &str) -> Option<PeerId> {
    let t = serde_json::from_str::<RelayTelemetry>(telemetry_json).ok()?;
    let peer_id = t.relay_p2p_peer_id.as_ref()?.trim();
    if peer_id.is_empty() {
        return None;
    }
    peer_id.parse::<PeerId>().ok()
}

fn synthesize_peer_hint_telemetry(relay_url: &str, peer_id: &str) -> String {
    let now = now_ms();
    serde_json::json!({
        "relay_url": relay_url,
        "timestamp_ms": now,
        "online_users": 0,
        "online_peers": 0,
        "total_users": 0,
        "total_peers_seen": 0,
        "peers_seen_window_ms": 0,
        "peers_seen_cutoff_ms": now,
        "relays": [],
        "relay_p2p_peer_id": peer_id
    })
    .to_string()
}

async fn build_peer_hints(state: &AppState, self_relay_url: &str) -> Vec<RelayMeshPeerHint> {
    let relays = {
        let db = state.db.lock().await;
        db.list_relays(200).unwrap_or_default()
    };
    let self_url = self_relay_url.trim_end_matches('/');
    let mut out = Vec::new();
    for (relay_url, _base_domain, _last_seen, telemetry_json, _sig) in relays {
        let relay_url = relay_url.trim_end_matches('/').to_string();
        if relay_url.is_empty() || relay_url == self_url {
            continue;
        }
        let Some(telemetry_json) = telemetry_json else {
            continue;
        };
        let Some(peer_id) = relay_peer_id_from_telemetry(&telemetry_json) else {
            continue;
        };
        out.push(RelayMeshPeerHint {
            relay_url,
            peer_id: peer_id.to_string(),
        });
    }
    out
}

async fn ingest_peer_hints(state: &AppState, hints: &[RelayMeshPeerHint]) -> usize {
    let self_url = state
        .cfg
        .public_url
        .as_deref()
        .map(|v| v.trim_end_matches('/').to_string())
        .unwrap_or_default();
    let mut discovered = 0usize;
    let mut db = state.db.lock().await;
    for hint in hints.iter().take(200) {
        let relay_url = hint.relay_url.trim_end_matches('/');
        let peer_id = hint.peer_id.trim();
        if relay_url.is_empty() || relay_url == self_url || peer_id.is_empty() {
            continue;
        }
        if peer_id.parse::<PeerId>().is_err() {
            continue;
        }
        let base_domain = reqwest::Url::parse(relay_url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()));
        let telemetry_json = Some(synthesize_peer_hint_telemetry(relay_url, peer_id));
        if db
            .upsert_relay(relay_url, base_domain, telemetry_json, None)
            .is_ok()
        {
            discovered += 1;
        }
    }
    discovered
}

async fn build_sync_bundle(
    state: &AppState,
    notes: Vec<RelaySyncNoteItem>,
    media: Vec<RelaySyncMediaItem>,
    actors: Vec<RelaySyncActorItem>,
    next: Option<String>,
) -> RelaySyncBundle {
    let relay_url = state
        .cfg
        .public_url
        .as_deref()
        .map(|v| v.trim_end_matches('/').to_string())
        .unwrap_or_default();
    let peer_hints = build_peer_hints(state, &relay_url).await;
    RelaySyncBundle {
        relay_url,
        created_at_ms: now_ms(),
        notes,
        media,
        actors,
        peer_hints,
        next,
        signature_b64: None,
    }
}

async fn build_empty_bundle(state: &AppState) -> RelaySyncBundle {
    let mut bundle = build_sync_bundle(state, Vec::new(), Vec::new(), Vec::new(), None).await;
    if !bundle.relay_url.is_empty() {
        let db = state.db.lock().await;
        if let Ok((_, sk_b64)) = db.load_or_create_signing_keypair_b64() {
            if let Ok(sig) = sign_bundle_b64(&bundle, &sk_b64) {
                bundle.signature_b64 = Some(sig);
            }
        }
    }
    bundle
}

async fn build_signed_mesh_request(
    state: &AppState,
    since: Option<i64>,
    cursor: Option<i64>,
    limit: u32,
) -> Result<RelayMeshSyncRequest> {
    let self_relay_url = state
        .cfg
        .public_url
        .as_deref()
        .map(|v| v.trim_end_matches('/').to_string())
        .unwrap_or_default();
    if self_relay_url.is_empty() {
        return Err(anyhow::anyhow!("missing public_url"));
    }
    let (self_pk_b64, self_sk_b64) = {
        let db = state.db.lock().await;
        db.load_or_create_signing_keypair_b64()?
    };
    let mut req = RelayMeshSyncRequest {
        since,
        cursor,
        limit,
        relay_url: Some(self_relay_url),
        created_at_ms: Some(now_ms()),
        sign_pubkey_b64: Some(self_pk_b64),
        signature_b64: None,
    };
    let sig = sign_mesh_request_b64(&req, &self_sk_b64)?;
    req.signature_b64 = Some(sig);
    Ok(req)
}

async fn verify_mesh_request(
    state: &AppState,
    cfg: &RelayMeshConfig,
    req: &RelayMeshSyncRequest,
) -> Result<()> {
    let relay_url = req.relay_url.as_deref().unwrap_or("").trim_end_matches('/');
    if relay_url.is_empty() {
        return Err(anyhow::anyhow!("missing relay_url"));
    }
    let created_at_ms = req.created_at_ms.unwrap_or(0);
    if created_at_ms <= 0 {
        return Err(anyhow::anyhow!("missing created_at_ms"));
    }
    let now = now_ms();
    if (created_at_ms - now).abs() > 24 * 3600 * 1000 {
        return Err(anyhow::anyhow!("request timestamp out of range"));
    }
    let sig_b64 = req
        .signature_b64
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing signature"))?
        .trim();
    if sig_b64.is_empty() {
        return Err(anyhow::anyhow!("missing signature"));
    }

    let pk_b64 = {
        let db = state.db.lock().await;
        db.get_relay_pubkey_b64(relay_url).ok().flatten()
    };

    let pk_b64 = if let Some(pk_b64) = pk_b64 {
        pk_b64
    } else if let Some(pk_b64) = req
        .sign_pubkey_b64
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let mut db = state.db.lock().await;
        let _ = db.upsert_relay(relay_url, None, None, Some(pk_b64.to_string()));
        pk_b64.to_string()
    } else {
        return Err(anyhow::anyhow!("missing sign_pubkey_b64"));
    };

    verify_mesh_request_signature(req, &pk_b64)?;
    update_reputation(state, relay_url, 1, cfg.reputation_ttl_ms).await;
    Ok(())
}

fn mesh_request_bytes_for_signing(req: &RelayMeshSyncRequest) -> Result<Vec<u8>> {
    let mut clone = req.clone();
    clone.signature_b64 = None;
    Ok(serde_json::to_vec(&clone)?)
}

fn sign_mesh_request_b64(req: &RelayMeshSyncRequest, sk_b64: &str) -> Result<String> {
    let sk_bytes = B64.decode(sk_b64.as_bytes())?;
    if sk_bytes.len() != 32 {
        return Err(anyhow::anyhow!("bad signing key length"));
    }
    let mut sk = [0u8; 32];
    sk.copy_from_slice(&sk_bytes);
    let signing = ed25519_dalek::SigningKey::from_bytes(&sk);
    let bytes = mesh_request_bytes_for_signing(req)?;
    let sig: ed25519_dalek::Signature = signing.sign(&bytes);
    Ok(B64.encode(sig.to_bytes()))
}

fn verify_mesh_request_signature(req: &RelayMeshSyncRequest, pk_b64: &str) -> Result<()> {
    let pk_bytes = B64.decode(pk_b64.as_bytes())?;
    if pk_bytes.len() != 32 {
        return Err(anyhow::anyhow!("bad pubkey length"));
    }
    let mut pk = [0u8; 32];
    pk.copy_from_slice(&pk_bytes);
    let verifying = ed25519_dalek::VerifyingKey::from_bytes(&pk)?;

    let sig_b64 = req
        .signature_b64
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing signature"))?
        .trim();
    let sig_bytes = B64.decode(sig_b64.as_bytes())?;
    if sig_bytes.len() != 64 {
        return Err(anyhow::anyhow!("bad signature length"));
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let sig = ed25519_dalek::Signature::from_bytes(&sig_arr);

    let bytes = mesh_request_bytes_for_signing(req)?;
    verifying.verify(&bytes, &sig)?;
    Ok(())
}

fn bundle_bytes_for_signing(bundle: &RelaySyncBundle) -> Result<Vec<u8>> {
    let mut clone = bundle.clone();
    clone.signature_b64 = None;
    Ok(serde_json::to_vec(&clone)?)
}

fn sign_bundle_b64(bundle: &RelaySyncBundle, sk_b64: &str) -> Result<String> {
    let sk_bytes = B64.decode(sk_b64.as_bytes())?;
    if sk_bytes.len() != 32 {
        return Err(anyhow::anyhow!("bad signing key length"));
    }
    let mut sk = [0u8; 32];
    sk.copy_from_slice(&sk_bytes);
    let signing = ed25519_dalek::SigningKey::from_bytes(&sk);
    let bytes = bundle_bytes_for_signing(bundle)?;
    let sig: ed25519_dalek::Signature = signing.sign(&bytes);
    Ok(B64.encode(sig.to_bytes()))
}

fn verify_bundle_signature(bundle: &RelaySyncBundle, pk_b64: &str) -> Result<()> {
    let pk_bytes = B64.decode(pk_b64.as_bytes())?;
    if pk_bytes.len() != 32 {
        return Err(anyhow::anyhow!("bad pubkey length"));
    }
    let mut pk = [0u8; 32];
    pk.copy_from_slice(&pk_bytes);
    let verifying = ed25519_dalek::VerifyingKey::from_bytes(&pk)?;

    let sig_b64 = bundle
        .signature_b64
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing signature"))?
        .trim();
    let sig_bytes = B64.decode(sig_b64.as_bytes())?;
    if sig_bytes.len() != 64 {
        return Err(anyhow::anyhow!("bad signature length"));
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let sig = ed25519_dalek::Signature::from_bytes(&sig_arr);

    let now = now_ms();
    if (bundle.created_at_ms - now).abs() > 24 * 3600 * 1000 {
        return Err(anyhow::anyhow!("bundle timestamp out of range"));
    }
    let bytes = bundle_bytes_for_signing(bundle)?;
    verifying.verify(&bytes, &sig)?;
    Ok(())
}

async fn reputation_allows(state: &AppState, relay_url: &str, retention_ms: i64) -> bool {
    let now = now_ms();
    let mut rep = state.relay_reputation.lock().await;
    if retention_ms > 0 {
        rep.retain(|_, v| now.saturating_sub(v.last_ms) <= retention_ms);
    }
    let key = relay_url.trim_end_matches('/');
    rep.get(key)
        .map(|v| v.score > RELAY_REPUTATION_MIN_SCORE)
        .unwrap_or(true)
}

async fn update_reputation(state: &AppState, relay_url: &str, delta: i32, retention_ms: i64) {
    let now = now_ms();
    let mut rep = state.relay_reputation.lock().await;
    if retention_ms > 0 {
        rep.retain(|_, v| now.saturating_sub(v.last_ms) <= retention_ms);
    }
    let key = relay_url.trim_end_matches('/').to_string();
    let entry = rep.entry(key.clone()).or_insert(crate::RelayReputation {
        score: 0,
        last_ms: now,
    });
    entry.score = (entry.score + delta)
        .min(RELAY_REPUTATION_MAX_SCORE)
        .max(-10);
    entry.last_ms = now;
    let score = entry.score;
    drop(rep);
    let db = state.db.lock().await;
    let _ = db.upsert_relay_reputation(&key, score, now);
}
