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
    identify, identity, kad, noise, ping, quic, relay, request_response,
    swarm::{derive_prelude::*, SwarmEvent},
    tcp, websocket, yamux, Multiaddr, PeerId, Swarm, Transport,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{error, info, warn};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use ed25519_dalek::{Signer as _, Verifier as _};

use crate::relay_notes::{
    note_to_index, RelayActorIndex, RelayMediaIndex, RelaySyncActorItem, RelaySyncBundle,
    RelaySyncMediaItem, RelaySyncNoteItem,
};
use crate::{now_ms, relay_p2p_infra_multiaddrs, AppState, RelayTelemetry};

const RELAY_REPUTATION_MIN_SCORE: i32 = -3;
const RELAY_REPUTATION_MAX_SCORE: i32 = 10;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RelayMeshSyncRequest {
    since: Option<i64>,
    cursor: Option<i64>,
    limit: u32,
}

#[derive(Clone)]
struct RelayMeshConfig {
    listen: Vec<Multiaddr>,
    bootstrap: Vec<Multiaddr>,
    relay_reserve: Vec<Multiaddr>,
    key_path: PathBuf,
    sync_interval_secs: u64,
    sync_limit: u32,
    reputation_ttl_ms: i64,
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
    std::fs::write(path, bytes).context("write keypair")?;
    Ok(kp)
}

fn load_mesh_config(cfg: &crate::RelayConfig) -> RelayMeshConfig {
    let listen = cfg
        .relay_mesh_listen
        .iter()
        .filter_map(|s| s.parse::<Multiaddr>().ok())
        .collect::<Vec<_>>();
    let bootstrap = cfg
        .relay_mesh_bootstrap
        .iter()
        .filter_map(|s| s.parse::<Multiaddr>().ok())
        .collect::<Vec<_>>();
    let mut relay_reserve = Vec::new();
    for s in relay_p2p_infra_multiaddrs(cfg) {
        if let Ok(addr) = s.parse::<Multiaddr>() {
            relay_reserve.push(addr);
        }
    }
    RelayMeshConfig {
        listen,
        bootstrap,
        relay_reserve,
        key_path: cfg.relay_mesh_key_path.clone(),
        sync_interval_secs: cfg.relay_sync_interval_secs.max(30),
        sync_limit: cfg.relay_sync_limit.min(200).max(1),
        reputation_ttl_ms: (cfg.relay_reputation_ttl_secs as i64) * 1000,
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
    let rr_cfg = request_response::Config::default()
        .with_request_timeout(Duration::from_secs(20));
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
        libp2p::swarm::Config::with_tokio_executor(),
    );

    for addr in &cfg.listen {
        if let Err(e) = swarm.listen_on(addr.clone()) {
            warn!(%addr, "relay mesh listen failed: {e}");
        }
    }

    for mut addr in cfg.relay_reserve.clone() {
        let has_peer = addr.iter().any(|p| matches!(p, libp2p::multiaddr::Protocol::P2p(_)));
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
        let _ = swarm.dial(addr.clone());
    }

    info!(%peer_id, "relay mesh enabled");

    let mut pending: HashMap<request_response::OutboundRequestId, PendingSync> = HashMap::new();
    let mut inflight_relays: HashSet<String> = HashSet::new();
    let mut bootstrapped = false;
    let mut sync_tick = tokio::time::interval(Duration::from_secs(cfg.sync_interval_secs));
    sync_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = sync_tick.tick() => {
                if let Err(e) = queue_sync_requests(&state, &mut swarm, &cfg, &mut pending, &mut inflight_relays).await {
                    warn!("relay mesh sync tick failed: {e:#}");
                }
            }
            ev = swarm.select_next_some() => {
                match ev {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!(%peer_id, %address, "relay mesh listening");
                    }
                    SwarmEvent::ConnectionEstablished { .. } => {
                        if !bootstrapped && !bootstrap_peers.is_empty() {
                            let _ = swarm.behaviour_mut().kad.bootstrap();
                            bootstrapped = true;
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
                .map(|note| RelaySyncNoteItem { note, created_at_ms })
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
    if let Some(next) = note_page.next.as_deref().and_then(|v| v.parse::<i64>().ok()) {
        next_candidates.push(next);
    }
    if let Some(next) = media_page.next.as_deref().and_then(|v| v.parse::<i64>().ok()) {
        next_candidates.push(next);
    }
    if let Some(next) = actor_page.next.as_deref().and_then(|v| v.parse::<i64>().ok()) {
        next_candidates.push(next);
    }
    let next = next_candidates
        .into_iter()
        .min()
        .map(|v| v.to_string());

    let relay_url = state
        .cfg
        .public_url
        .as_deref()
        .map(|v| v.trim_end_matches('/').to_string())
        .unwrap_or_default();
    let mut bundle = RelaySyncBundle {
        relay_url,
        created_at_ms: now_ms(),
        notes,
        media,
        actors,
        next,
        signature_b64: None,
    };
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

    let mut item_count = 0usize;
    if !response.notes.is_empty() || !response.media.is_empty() || !response.actors.is_empty() {
        let db = state.db.lock().await;
        for item in response.notes {
            item_count += 1;
            if item.created_at_ms > pend.max_seen {
                pend.max_seen = item.created_at_ms;
            }
            if let Some(mut indexed) = note_to_index(&item.note) {
                indexed.created_at_ms = item.created_at_ms;
                let _ = db.upsert_relay_note(&indexed);
            }
        }
        for item in response.media {
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
        for item in response.actors {
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

    if let Some(next) = response.next.and_then(|v| v.parse::<i64>().ok()) {
        let req = RelayMeshSyncRequest {
            since: None,
            cursor: Some(next),
            limit: cfg.sync_limit,
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
        if peer_id.to_string() == local_peer_id {
            continue;
        }

        let since = last_map.get(&relay_url).copied();
        for addr in peer_circuit_addrs(&cfg.relay_reserve, &peer_id) {
            swarm.add_peer_address(peer_id, addr.clone());
            let _ = swarm.dial(addr);
        }
        let req = RelayMeshSyncRequest {
            since,
            cursor: None,
            limit,
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

fn extract_peer_id(addr: &Multiaddr) -> Option<PeerId> {
    for p in addr.iter() {
        if let libp2p::multiaddr::Protocol::P2p(pid) = p {
            return Some(pid);
        }
    }
    None
}

fn peer_circuit_addrs(base: &[Multiaddr], peer_id: &PeerId) -> Vec<Multiaddr> {
    let mut out = Vec::new();
    for addr in base {
        let mut a = addr.clone();
        let has_peer = a.iter().any(|p| matches!(p, libp2p::multiaddr::Protocol::P2p(_)));
        if !has_peer {
            continue;
        }
        a.push(libp2p::multiaddr::Protocol::P2pCircuit);
        a.push(libp2p::multiaddr::Protocol::P2p(peer_id.clone()));
        out.push(a);
    }
    out
}

fn relay_peer_id_from_telemetry(telemetry_json: &str) -> Option<PeerId> {
    let t = serde_json::from_str::<RelayTelemetry>(telemetry_json).ok()?;
    let peer_id = t.relay_p2p_peer_id.as_ref()?.trim();
    if peer_id.is_empty() {
        return None;
    }
    peer_id.parse::<PeerId>().ok()
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

async fn update_reputation(
    state: &AppState,
    relay_url: &str,
    delta: i32,
    retention_ms: i64,
) {
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
