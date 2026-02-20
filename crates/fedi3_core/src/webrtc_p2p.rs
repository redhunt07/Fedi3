/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use anyhow::{anyhow, Context, Result};
use axum::body::Body;
use bytes::Bytes;
use fedi3_protocol::{RelayHttpRequest, RelayHttpResponse};
use http::{HeaderMap, Method, Uri};
use tokio::sync::{mpsc, oneshot, watch, Mutex};
use tracing::{info, warn};

use crate::crypto_envelope::decrypt_relay_http_request_body;
use crate::http_retry::send_with_retry;
use crate::http_sig::sign_request_rsa_sha256;
use crate::net_metrics::NetMetrics;
use crate::p2p::P2pConfig;
use crate::relay_bridge::handle_relay_http_request;

use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::time::Duration;

use tower::util::BoxCloneService;
use webrtc::api::setting_engine::SettingEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::ice::network_type::NetworkType;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

#[derive(Clone)]
pub struct WebrtcHandle {
    tx: mpsc::Sender<OutboundMsg>,
}

impl WebrtcHandle {
    pub async fn request(
        &self,
        peer_actor_url: &str,
        peer_id: &str,
        req: RelayHttpRequest,
    ) -> Result<RelayHttpResponse> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(OutboundMsg::Request {
                peer_actor_url: peer_actor_url.to_string(),
                peer_id: peer_id.to_string(),
                req,
                resp_tx: tx,
            })
            .await
            .context("webrtc send request")?;
        rx.await.context("webrtc response dropped")?
    }
}

enum OutboundMsg {
    Request {
        peer_actor_url: String,
        peer_id: String,
        req: RelayHttpRequest,
        resp_tx: oneshot::Sender<Result<RelayHttpResponse>>,
    },
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "t")]
enum WireMsg {
    #[serde(rename = "req")]
    Req { id: String, req: RelayHttpRequest },
    #[serde(rename = "resp")]
    Resp { id: String, resp: RelayHttpResponse },
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SignalMsg {
    id: String,
    from_actor: String,
    session_id: String,
    kind: String,
    payload: serde_json::Value,
}

#[derive(Debug, serde::Deserialize)]
struct PollResp {
    ok: bool,
    messages: Vec<SignalMsg>,
}

#[derive(Clone)]
struct ManagerCfg {
    self_peer_id: String,
    self_relay_base: String,
    private_key_pem: String,
    key_id: String,
    poll_secs: u64,
    connect_timeout_secs: u64,
    idle_ttl_secs: u64,
    ice_urls: Vec<String>,
    ice_username: Option<String>,
    ice_credential: Option<String>,
    ipv4_only: bool,
}

#[allow(dead_code)]
struct Session {
    session_id: String,
    remote_actor: String,
    remote_peer_id: String,
    remote_relay_base: String,
    pc: Arc<RTCPeerConnection>,
    last_used_ms: i64,
    // For offerer flow:
    pending_resp: Option<oneshot::Sender<Result<RelayHttpResponse>>>,
    pending_req_id: Option<String>,
    pending_since_ms: Option<i64>,
}

const FRAME_V1: u8 = 1;
// WebRTC DataChannel messages are practically limited (~16KB). Keep headroom for overhead.
const CHUNK_PAYLOAD_MAX: usize = 12 * 1024;
const ASSEMBLY_TTL_MS: i64 = 60_000;
const MAX_INFLIGHT_ASSEMBLIES: usize = 64;
const MAX_WIRE_BYTES: usize = 2 * 1024 * 1024;
const REQUEST_TIMEOUT_SECS: u64 = 30;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn random_id() -> String {
    let mut b = [0u8; 16];
    use rand::RngCore as _;
    rand::rngs::OsRng.fill_bytes(&mut b);
    b.iter().map(|v| format!("{v:02x}")).collect()
}

fn random_msg_id_16() -> [u8; 16] {
    let mut b = [0u8; 16];
    use rand::RngCore as _;
    rand::rngs::OsRng.fill_bytes(&mut b);
    b
}

fn msg_id_hex(id: &[u8; 16]) -> String {
    id.iter().map(|v| format!("{v:02x}")).collect()
}

fn build_frame(msg_id: &[u8; 16], total_len: u32, offset: u32, chunk: &[u8]) -> Bytes {
    // Frame format:
    // [0]      = version (1)
    // [1..17)  = msg_id (16 bytes)
    // [17..21) = total_len u32 LE
    // [21..25) = offset u32 LE
    // [25..]   = payload bytes
    let mut out = Vec::with_capacity(25 + chunk.len());
    out.push(FRAME_V1);
    out.extend_from_slice(msg_id);
    out.extend_from_slice(&total_len.to_le_bytes());
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(chunk);
    Bytes::from(out)
}

fn parse_frame(bytes: &[u8]) -> Option<([u8; 16], u32, u32, &[u8])> {
    if bytes.len() < 25 {
        return None;
    }
    if bytes[0] != FRAME_V1 {
        return None;
    }
    let mut msg_id = [0u8; 16];
    msg_id.copy_from_slice(&bytes[1..17]);
    let total_len = u32::from_le_bytes(bytes[17..21].try_into().ok()?);
    let offset = u32::from_le_bytes(bytes[21..25].try_into().ok()?);
    Some((msg_id, total_len, offset, &bytes[25..]))
}

#[derive(Default)]
struct Assembler {
    parts: HashMap<String, Assembly>,
}

struct Assembly {
    total_len: usize,
    received: usize,
    chunks: HashMap<u32, Vec<u8>>,
    created_at_ms: i64,
}

impl Assembler {
    fn cleanup(&mut self) {
        let now = now_ms();
        self.parts
            .retain(|_, a| now.saturating_sub(a.created_at_ms) < ASSEMBLY_TTL_MS);
        // Bound the number of in-flight assemblies.
        if self.parts.len() > MAX_INFLIGHT_ASSEMBLIES {
            let mut keys = self
                .parts
                .iter()
                .map(|(k, v)| (k.clone(), v.created_at_ms))
                .collect::<Vec<_>>();
            keys.sort_by_key(|(_, ts)| *ts);
            for (k, _) in keys
                .into_iter()
                .take(self.parts.len().saturating_sub(MAX_INFLIGHT_ASSEMBLIES))
            {
                self.parts.remove(&k);
            }
        }
    }

    fn ingest(&mut self, bytes: &[u8]) -> Option<Vec<u8>> {
        self.cleanup();
        let (msg_id, total_len_u32, offset_u32, chunk) = parse_frame(bytes)?;
        let total_len = total_len_u32 as usize;
        if total_len == 0 || total_len > MAX_WIRE_BYTES {
            return None;
        }
        let offset = offset_u32 as usize;
        if offset >= total_len {
            return None;
        }
        let max_write = total_len.saturating_sub(offset);
        let chunk = &chunk[..chunk.len().min(max_write)];

        let key = msg_id_hex(&msg_id);
        let entry = self.parts.entry(key.clone()).or_insert_with(|| Assembly {
            total_len,
            received: 0,
            chunks: HashMap::new(),
            created_at_ms: now_ms(),
        });

        // Reject mismatched total_len.
        if entry.total_len != total_len {
            self.parts.remove(&key);
            return None;
        }

        // Dedup on offset.
        if entry.chunks.contains_key(&offset_u32) {
            return None;
        }
        entry.chunks.insert(offset_u32, chunk.to_vec());
        entry.received = entry.received.saturating_add(chunk.len());

        if entry.received < entry.total_len {
            return None;
        }

        // Reassemble.
        let mut out = vec![0u8; entry.total_len];
        let mut written: usize = 0;
        for (off, data) in entry.chunks.iter() {
            let off = *off as usize;
            if off >= out.len() {
                continue;
            }
            let n = (out.len() - off).min(data.len());
            out[off..off + n].copy_from_slice(&data[..n]);
            written = written.saturating_add(n);
        }
        self.parts.remove(&key);
        if written < out.len() {
            return None;
        }
        Some(out)
    }
}

async fn send_bytes_chunked(
    dc: &webrtc::data_channel::RTCDataChannel,
    payload: &[u8],
) -> Result<()> {
    if payload.len() > MAX_WIRE_BYTES {
        return Err(anyhow!(
            "payload too large for webrtc: {} bytes",
            payload.len()
        ));
    }
    let msg_id = random_msg_id_16();
    let total_len = payload.len() as u32;
    let mut offset: usize = 0;
    while offset < payload.len() {
        let end = (offset + CHUNK_PAYLOAD_MAX).min(payload.len());
        let frame = build_frame(&msg_id, total_len, offset as u32, &payload[offset..end]);
        dc.send(&frame)
            .await
            .map_err(|e| anyhow!("dc send: {e:#}"))?;
        offset = end;
    }
    Ok(())
}

fn relay_base_from_actor(actor_url: &str) -> Option<String> {
    let uri: Uri = actor_url.parse().ok()?;
    let scheme = uri.scheme_str()?;
    let auth = uri.authority()?.as_str();
    Some(format!("{scheme}://{auth}"))
}

fn peer_id_from_session_id(session_id: &str) -> Option<String> {
    let (peer_id, _) = session_id.split_once('-')?;
    let peer_id = peer_id.trim();
    if peer_id.is_empty() {
        None
    } else {
        Some(peer_id.to_string())
    }
}

async fn signed_json_post(
    http: &reqwest::Client,
    private_key_pem: &str,
    key_id: &str,
    url: &str,
    body_json: serde_json::Value,
) -> Result<serde_json::Value> {
    let body = serde_json::to_vec(&body_json).unwrap_or_default();
    let uri: Uri = url.parse().context("parse url")?;
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "application/json".parse().expect("static header"),
    );
    sign_request_rsa_sha256(
        private_key_pem,
        key_id,
        &Method::POST,
        &uri,
        &mut headers,
        &body,
        &["(request-target)", "host", "date", "digest", "content-type"],
    )?;
    let build_req = || {
        let mut req = http.post(url);
        for (k, v) in headers.iter() {
            req = req.header(k.as_str(), v.to_str().unwrap_or_default());
        }
        req.body(body.clone())
    };
    let resp = send_with_retry(build_req, 3).await?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        warn!("webrtc signal post failed url={url} status={status} body={text}");
        return Err(anyhow!("signal post failed: {} {}", status, text));
    }
    serde_json::from_str(&text).context("parse json")
}

async fn signed_get_json(
    http: &reqwest::Client,
    private_key_pem: &str,
    key_id: &str,
    url: &str,
) -> Result<serde_json::Value> {
    let uri: Uri = url.parse().context("parse url")?;
    let mut headers = HeaderMap::new();
    headers.insert("accept", "application/json".parse().expect("static header"));
    sign_request_rsa_sha256(
        private_key_pem,
        key_id,
        &Method::GET,
        &uri,
        &mut headers,
        &[],
        &["(request-target)", "host", "date"],
    )?;
    let build_req = || {
        let mut req = http.get(url);
        for (k, v) in headers.iter() {
            req = req.header(k.as_str(), v.to_str().unwrap_or_default());
        }
        req
    };
    let resp = send_with_retry(build_req, 3).await?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        warn!("webrtc signal get failed url={url} status={status} body={text}");
        return Err(anyhow!("signal get failed: {} {}", status, text));
    }
    serde_json::from_str(&text).context("parse json")
}

async fn send_signal(
    http: &reqwest::Client,
    cfg: &ManagerCfg,
    remote_relay_base: &str,
    to_peer_id: &str,
    session_id: &str,
    kind: &str,
    payload: serde_json::Value,
) -> Result<()> {
    let url = format!(
        "{}/_fedi3/webrtc/send",
        remote_relay_base.trim_end_matches('/')
    );
    let body = serde_json::json!({
      "to_peer_id": to_peer_id,
      "session_id": session_id,
      "kind": kind,
      "payload": payload,
    });
    match signed_json_post(http, &cfg.private_key_pem, &cfg.key_id, &url, body).await {
        Ok(_) => {
            info!(
                "webrtc signal sent relay={} to_peer_id={} session_id={} kind={}",
                remote_relay_base,
                to_peer_id,
                session_id,
                kind
            );
        }
        Err(e) => {
            warn!(
                "webrtc signal send failed relay={} to_peer_id={} session_id={} kind={} err={e:#}",
                remote_relay_base,
                to_peer_id,
                session_id,
                kind
            );
            return Err(e);
        }
    }
    Ok(())
}

async fn ack_signals(http: &reqwest::Client, cfg: &ManagerCfg, ids: Vec<String>) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let url = format!(
        "{}/_fedi3/webrtc/ack",
        cfg.self_relay_base.trim_end_matches('/')
    );
    let body = serde_json::json!({
      "to_peer_id": cfg.self_peer_id,
      "ids": ids,
    });
    let _ = signed_json_post(http, &cfg.private_key_pem, &cfg.key_id, &url, body).await?;
    Ok(())
}

fn build_ice_servers(cfg: &ManagerCfg) -> Vec<RTCIceServer> {
    if cfg.ice_urls.is_empty() {
        return Vec::new();
    }
    vec![RTCIceServer {
        urls: cfg.ice_urls.clone(),
        username: cfg.ice_username.clone().unwrap_or_default(),
        credential: cfg.ice_credential.clone().unwrap_or_default(),
        ..Default::default()
    }]
}

fn normalize_ice_urls(ice_urls: Vec<String>, ipv4_only: bool) -> Vec<String> {
    if !ipv4_only {
        return ice_urls;
    }
    ice_urls
        .into_iter()
        .filter_map(|url| map_ice_url_ipv4(&url))
        .collect()
}

fn map_ice_url_ipv4(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }
    let Some((scheme, rest)) = trimmed.split_once(':') else {
        return Some(trimmed.to_string());
    };
    let scheme_lower = scheme.to_ascii_lowercase();
    if !matches!(scheme_lower.as_str(), "stun" | "stuns" | "turn" | "turns") {
        return Some(trimmed.to_string());
    }
    let rest = rest.trim_start_matches("//");
    let (hostport, query) = rest
        .split_once('?')
        .map(|(a, b)| (a, Some(b)))
        .unwrap_or((rest, None));
    if hostport.starts_with('[') {
        // IPv6 literal; drop in IPv4-only mode.
        return None;
    }
    let (host, port_opt) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (hostport, None),
    };
    let host = host.trim();
    if host.is_empty() {
        return None;
    }
    if host.parse::<std::net::Ipv4Addr>().is_ok() {
        let mut out = format!("{scheme}:{host}");
        if let Some(port) = port_opt {
            out.push(':');
            out.push_str(port);
        }
        if let Some(q) = query {
            out.push('?');
            out.push_str(q);
        }
        return Some(out);
    }
    let port = port_opt
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(3478);
    let mut addrs = (host, port).to_socket_addrs().ok()?;
    let v4 = addrs.find(|a| a.is_ipv4())?.ip();
    let std::net::IpAddr::V4(v4) = v4 else {
        return None;
    };
    let mut out = format!("{scheme}:{v4}:{port}");
    if let Some(q) = query {
        out.push('?');
        out.push_str(q);
    }
    Some(out)
}

async fn new_peer_connection(cfg: &ManagerCfg) -> Result<Arc<RTCPeerConnection>> {
    let mut setting_engine = SettingEngine::default();
    if cfg.ipv4_only {
        setting_engine.set_network_types(vec![NetworkType::Udp4, NetworkType::Tcp4]);
    }
    let api = APIBuilder::new()
        .with_setting_engine(setting_engine)
        .build();
    let pc = api
        .new_peer_connection(RTCConfiguration {
            ice_servers: build_ice_servers(cfg),
            ..Default::default()
        })
        .await
        .context("new peer connection")?;
    Ok(Arc::new(pc))
}

async fn resolve_peer_id_from_actor(
    http: &reqwest::Client,
    actor_url: &str,
) -> Result<Option<String>> {
    let resp = send_with_retry(
        || {
            http.get(actor_url).header(
                "Accept",
                "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"",
            )
        },
        3,
    )
    .await?;
    let text = resp.text().await.unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    let pid = v
        .get("endpoints")
        .and_then(|e| e.get("fedi3PeerId"))
        .and_then(|s| s.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    Ok(pid)
}

type Handler = Arc<
    Mutex<BoxCloneService<http::Request<Body>, http::Response<Body>, std::convert::Infallible>>,
>;

async fn handle_incoming_wire_message(
    cfg: &ManagerCfg,
    handler: Handler,
    msg: WireMsg,
    dc: Arc<webrtc::data_channel::RTCDataChannel>,
    metrics: Arc<NetMetrics>,
    remote_peer_id: &str,
) {
    match msg {
        WireMsg::Req { id, req } => {
            let req = decrypt_relay_http_request_body(&cfg.private_key_pem, req);
            let mut svc = handler.lock().await;
            let resp = handle_relay_http_request(&mut *svc, req).await;
            let out = WireMsg::Resp { id, resp };
            if let Ok(bytes) = serde_json::to_vec(&out) {
                metrics.webrtc_tx_add(bytes.len() as u64);
                metrics.webrtc_peer_seen(remote_peer_id);
                let _ = send_bytes_chunked(&dc, &bytes).await;
            }
        }
        WireMsg::Resp { .. } => {}
    }
}

pub fn start_webrtc(
    p2p_cfg: P2pConfig,
    self_peer_id: String,
    self_relay_base: String,
    private_key_pem: String,
    key_id: String,
    handler: Handler,
    http: reqwest::Client,
    shutdown: watch::Receiver<bool>,
    metrics: Arc<NetMetrics>,
) -> Result<Option<WebrtcHandle>>
where
    Handler: Clone + Send + 'static,
{
    let enabled = p2p_cfg.webrtc_enable.unwrap_or(false);
    let ice_urls = normalize_ice_urls(
        p2p_cfg.webrtc_ice_urls.clone().unwrap_or_default(),
        p2p_cfg.ipv4_only.unwrap_or(true),
    );
    if !enabled || ice_urls.is_empty() {
        return Ok(None);
    }

    let cfg = ManagerCfg {
        self_peer_id,
        self_relay_base,
        private_key_pem,
        key_id,
        poll_secs: p2p_cfg.webrtc_poll_secs.unwrap_or(2).max(1).min(30),
        connect_timeout_secs: p2p_cfg
            .webrtc_connect_timeout_secs
            .unwrap_or(20)
            .max(5)
            .min(120),
        idle_ttl_secs: p2p_cfg
            .webrtc_idle_ttl_secs
            .unwrap_or(300)
            .max(30)
            .min(3600),
        ice_urls,
        ice_username: p2p_cfg
            .webrtc_ice_username
            .clone()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        ice_credential: p2p_cfg
            .webrtc_ice_credential
            .clone()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        ipv4_only: p2p_cfg.ipv4_only.unwrap_or(true),
    };

    let (tx, mut rx) = mpsc::channel::<OutboundMsg>(128);
    let handle = WebrtcHandle { tx };

    let sessions: Arc<tokio::sync::Mutex<HashMap<String, Session>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let pending_candidates: Arc<tokio::sync::Mutex<HashMap<String, Vec<RTCIceCandidateInit>>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let handler_for_poll = handler.clone();
    let handler_for_outbound = handler.clone();

    let shutdown_poll = shutdown.clone();
    // Poller + dispatcher.
    {
        let cfg = cfg.clone();
        let http = http.clone();
        let sessions = sessions.clone();
        let pending_candidates = pending_candidates.clone();
        let handler = handler_for_poll.clone();
        let metrics = metrics.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(cfg.poll_secs));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut shutdown = shutdown_poll;
            loop {
                tokio::select! {
                    _ = shutdown.changed() => {
                        if *shutdown.borrow() { break; }
                    }
                    _ = tick.tick() => {}
                }
                if *shutdown.borrow() {
                    break;
                }

                // Cleanup idle sessions.
                let mut to_close = Vec::new();
                {
                    let cutoff = now_ms().saturating_sub((cfg.idle_ttl_secs as i64) * 1000);
                    let mut guard = sessions.lock().await;
                    guard.retain(|_, s| {
                        if s.last_used_ms < cutoff {
                            to_close.push(s.pc.clone());
                            false
                        } else {
                            true
                        }
                    });
                    metrics.webrtc_sessions_set(guard.len() as u64);
                }
                for pc in to_close {
                    let _ = pc.close().await;
                }

                let url = format!(
                    "{}/_fedi3/webrtc/poll?to_peer_id={}&limit=200",
                    cfg.self_relay_base.trim_end_matches('/'),
                    urlencoding::encode(&cfg.self_peer_id)
                );
                let json =
                    match signed_get_json(&http, &cfg.private_key_pem, &cfg.key_id, &url).await {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("webrtc poll failed: {e:#}");
                            continue;
                        }
                    };
                let poll: PollResp = match serde_json::from_value(json) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("webrtc poll bad json: {e:#}");
                        continue;
                    }
                };
                if !poll.ok || poll.messages.is_empty() {
                    continue;
                }

                let mut ack_ids = Vec::new();
                for m in poll.messages {
                    let kind = m.kind.trim().to_ascii_lowercase();
                    let session_id = m.session_id.trim().to_string();
                    if session_id.is_empty() {
                        continue;
                    }

                    // Ensure we know the remote peer id to respond.
                    let session_peer_id = {
                        let guard = sessions.lock().await;
                        guard.get(&session_id).map(|s| s.remote_peer_id.clone())
                    };
                    let remote_peer_id = match session_peer_id
                        .or_else(|| peer_id_from_session_id(&session_id))
                    {
                        Some(pid) => pid,
                        None => match resolve_peer_id_from_actor(&http, &m.from_actor).await {
                            Ok(Some(pid)) => pid,
                            _ => {
                                warn!(from_actor=%m.from_actor, "webrtc signal ignored: cannot resolve remote peer id");
                                continue;
                            }
                        },
                    };
                    let remote_relay_base = match relay_base_from_actor(&m.from_actor) {
                        Some(v) => v,
                        None => {
                            ack_ids.push(m.id);
                            continue;
                        }
                    };

                    if kind == "offer" {
                        // Create answerer session if absent.
                        let guard = sessions.lock().await;
                        if guard.contains_key(&session_id) {
                            ack_ids.push(m.id);
                            continue;
                        }
                        drop(guard);

                        let offer = match serde_json::from_value::<RTCSessionDescription>(
                            m.payload.clone(),
                        ) {
                            Ok(v) => v,
                            Err(_) => {
                                ack_ids.push(m.id);
                                continue;
                            }
                        };

                        let pc = match new_peer_connection(&cfg).await {
                            Ok(v) => v,
                            Err(e) => {
                                warn!("webrtc pc create failed: {e:#}");
                                ack_ids.push(m.id);
                                continue;
                            }
                        };

                        // Candidate trickle from us -> remote.
                        {
                            let http2 = http.clone();
                            let cfg2 = cfg.clone();
                            let remote_peer_id2 = remote_peer_id.clone();
                            let remote_relay_base2 = remote_relay_base.clone();
                            let session_id2 = session_id.clone();
                            let from_actor2 = m.from_actor.clone();
                            pc.on_ice_candidate(Box::new(move |cand| {
                                let http2 = http2.clone();
                                let cfg2 = cfg2.clone();
                                let remote_peer_id2 = remote_peer_id2.clone();
                                let remote_relay_base2 = remote_relay_base2.clone();
                                let session_id2 = session_id2.clone();
                                let from_actor2 = from_actor2.clone();
                                Box::pin(async move {
                                    let Some(cand) = cand else { return };
                            if let Ok(init) = cand.to_json() {
                                let payload = serde_json::to_value(&init).unwrap_or(serde_json::Value::Null);
                                if let Err(e) = send_signal(&http2, &cfg2, &remote_relay_base2, &remote_peer_id2, &session_id2, "candidate", payload).await {
                                    warn!(to=%from_actor2, "webrtc send candidate failed: {e:#}");
                                }
                            }
                        })
                    }));
                        }

                        // Incoming data channel: respond to requests.
                        {
                            let handler2 = handler.clone();
                            let cfg2 = cfg.clone();
                            let metrics2 = metrics.clone();
                            let remote_peer_id_in = remote_peer_id.clone();
                            let sessions_for_dc = sessions.clone();
                            let session_id_for_dc = session_id.clone();
                            pc.on_data_channel(Box::new(move |dc| {
                                let handler2 = handler2.clone();
                                let cfg2 = cfg2.clone();
                                let metrics2 = metrics2.clone();
                                let remote_peer_id_in = remote_peer_id_in.clone();
                                let sessions_for_dc = sessions_for_dc.clone();
                                let session_id_for_dc = session_id_for_dc.clone();
                                Box::pin({
                                    let sessions_for_dc = sessions_for_dc.clone();
                                    let session_id_for_dc = session_id_for_dc.clone();
                                    async move {
                                    let dc2 = dc.clone();
                                    let assembler =
                                        Arc::new(tokio::sync::Mutex::new(Assembler::default()));
                                    let handler3 = handler2.clone();
                                    let cfg3 = cfg2.clone();
                                    let metrics3 = metrics2.clone();
                                    let remote_peer_id3 = remote_peer_id_in.clone();
                                    let sessions4 = sessions_for_dc.clone();
                                    let session_id4 = session_id_for_dc.clone();
                                    dc.on_message(Box::new(move |m: DataChannelMessage| {
                                        let dc2 = dc2.clone();
                                        let assembler = assembler.clone();
                                        let handler3 = handler3.clone();
                                        let cfg3 = cfg3.clone();
                                        let metrics3 = metrics3.clone();
                                        let remote_peer_id3 = remote_peer_id3.clone();
                                        let sessions4 = sessions4.clone();
                                        let session_id4 = session_id4.clone();
                                        Box::pin(async move {
                                            let frame_bytes = m.data;
                                            metrics3.webrtc_rx_add(frame_bytes.len() as u64);
                                            metrics3.webrtc_peer_seen(&remote_peer_id3);
                                            {
                                                let mut guard = sessions4.lock().await;
                                                if let Some(s) = guard.get_mut(&session_id4) {
                                                    s.last_used_ms = now_ms();
                                                }
                                            }
                                            let payload = {
                                                let mut a = assembler.lock().await;
                                                a.ingest(&frame_bytes)
                                            };
                                            let Some(payload) = payload else { return };
                                            if let Ok(w) =
                                                serde_json::from_slice::<WireMsg>(&payload)
                                            {
                                                handle_incoming_wire_message(
                                                    &cfg3,
                                                    handler3,
                                                    w,
                                                    dc2.clone(),
                                                    metrics3,
                                                    &remote_peer_id3,
                                                )
                                                .await;
                                            }
                                        })
                                    }));
                                }
                                })
                            }));
                        }

                        if let Err(e) = pc.set_remote_description(offer).await {
                            warn!("webrtc set_remote_description failed: {e:#}");
                            ack_ids.push(m.id);
                            continue;
                        }
                        let answer = match pc.create_answer(None).await {
                            Ok(a) => a,
                            Err(e) => {
                                warn!("webrtc create_answer failed: {e:#}");
                                ack_ids.push(m.id);
                                continue;
                            }
                        };
                        if let Err(e) = pc.set_local_description(answer.clone()).await {
                            warn!("webrtc set_local_description failed: {e:#}");
                            ack_ids.push(m.id);
                            continue;
                        }
                        let payload =
                            serde_json::to_value(&answer).unwrap_or(serde_json::Value::Null);
                        if let Err(e) = send_signal(
                            &http,
                            &cfg,
                            &remote_relay_base,
                            &remote_peer_id,
                            &session_id,
                            "answer",
                            payload,
                        )
                        .await
                        {
                            warn!("webrtc send answer failed: {e:#}");
                        }

                        // Apply buffered candidates (if any).
                        let buffered = {
                            let mut pcand = pending_candidates.lock().await;
                            pcand.remove(&session_id).unwrap_or_default()
                        };
                        for c in buffered {
                            let _ = pc.add_ice_candidate(c).await;
                        }

                        let mut guard = sessions.lock().await;
                        guard.insert(
                            session_id.clone(),
                            Session {
                                session_id: session_id.clone(),
                                remote_actor: m.from_actor.clone(),
                                remote_peer_id,
                                remote_relay_base,
                                pc,
                                last_used_ms: now_ms(),
                                pending_resp: None,
                                pending_req_id: None,
                                pending_since_ms: None,
                            },
                        );
                        metrics.webrtc_sessions_set(guard.len() as u64);
                        ack_ids.push(m.id);
                        continue;
                    }

                    if kind == "answer" {
                        let answer = match serde_json::from_value::<RTCSessionDescription>(
                            m.payload.clone(),
                        ) {
                            Ok(v) => v,
                            Err(_) => {
                                ack_ids.push(m.id);
                                continue;
                            }
                        };
                        let mut guard = sessions.lock().await;
                        if let Some(s) = guard.get_mut(&session_id) {
                            let _ = s.pc.set_remote_description(answer).await;
                            s.last_used_ms = now_ms();
                        }
                        ack_ids.push(m.id);
                        continue;
                    }

                    if kind == "candidate" {
                        let cand = match serde_json::from_value::<RTCIceCandidateInit>(
                            m.payload.clone(),
                        ) {
                            Ok(v) => v,
                            Err(_) => {
                                ack_ids.push(m.id);
                                continue;
                            }
                        };
                        let mut guard = sessions.lock().await;
                        if let Some(s) = guard.get_mut(&session_id) {
                            let _ = s.pc.add_ice_candidate(cand).await;
                            s.last_used_ms = now_ms();
                        } else {
                            drop(guard);
                            let mut pcand = pending_candidates.lock().await;
                            pcand.entry(session_id.clone()).or_default().push(cand);
                        }
                        ack_ids.push(m.id);
                        continue;
                    }

                    ack_ids.push(m.id);
                }

                let _ = ack_signals(&http, &cfg, ack_ids).await;
            }
        });
    }

    // Outbound request loop.
    {
        let cfg = cfg.clone();
        let sessions = sessions.clone();
        let pending_candidates = pending_candidates.clone();
        let metrics = metrics.clone();
        let http = http.clone();
        let handler = handler_for_outbound.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                let OutboundMsg::Request {
                    peer_actor_url,
                    peer_id,
                    req,
                    resp_tx,
                } = msg;

                let remote_relay_base = match relay_base_from_actor(&peer_actor_url) {
                    Some(v) => v,
                    None => {
                        let _ = resp_tx.send(Err(anyhow!("invalid peer actor url")));
                        continue;
                    }
                };

                let session_id = format!("{}-{}", &cfg.self_peer_id, random_id());
                let pc = match new_peer_connection(&cfg).await {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = resp_tx.send(Err(e));
                        continue;
                    }
                };

                // Trickle ICE from us -> remote.
                {
                    let http2 = http.clone();
                    let cfg2 = cfg.clone();
                    let peer_id2 = peer_id.clone();
                    let remote_relay_base2 = remote_relay_base.clone();
                    let session_id2 = session_id.clone();
                    let peer_actor_url2 = peer_actor_url.clone();
                    pc.on_ice_candidate(Box::new(move |cand| {
                        let http2 = http2.clone();
                        let cfg2 = cfg2.clone();
                        let peer_id2 = peer_id2.clone();
                        let remote_relay_base2 = remote_relay_base2.clone();
                        let session_id2 = session_id2.clone();
                        let peer_actor_url2 = peer_actor_url2.clone();
                        Box::pin(async move {
                            let Some(cand) = cand else { return };
                                    if let Ok(init) = cand.to_json() {
                                        let payload = serde_json::to_value(&init).unwrap_or(serde_json::Value::Null);
                                        if let Err(e) = send_signal(&http2, &cfg2, &remote_relay_base2, &peer_id2, &session_id2, "candidate", payload).await {
                                            warn!(to=%peer_actor_url2, "webrtc send candidate failed: {e:#}");
                                        }
                                    }
                                })
                    }));
                }

                // Create data channel and wait for open.
                let dc = match pc.create_data_channel("fedi3", None).await {
                    Ok(v) => Arc::new(v),
                    Err(e) => {
                        warn!(session_id=%session_id, peer_id=%peer_id, "failed to create data channel: {e:#}");
                        let _ = resp_tx.send(Err(anyhow!("create datachannel failed: {e:#}")));
                        continue;
                    }
                };

                // Add data channel state change monitoring
                {
                    let session_id2 = session_id.clone();
                    let peer_id2 = peer_id.clone();
                    let sessions2 = sessions.clone();
                    let dc2 = dc.clone();
                    dc.on_open(Box::new(move || {
                        let _session_id2 = session_id2.clone();
                        let _peer_id2 = peer_id2.clone();
                        let _sessions2 = sessions2.clone();
                        let _dc2 = dc2.clone();
                        Box::pin(async move {
                            // Monitor for data channel closure by checking if it becomes closed
                            // This is a simplified approach since on_state_change doesn't exist
                            // We'll rely on the peer connection state change handler for reconnection
                        })
                    }));
                }

                let (open_tx, open_rx) = oneshot::channel::<()>();
                dc.on_open(Box::new(move || {
                    let _ = open_tx.send(());
                    Box::pin(async {})
                }));

                // Handle response.
                {
                    let sessions2 = sessions.clone();
                    let session_id2 = session_id.clone();
                    let assembler = Arc::new(tokio::sync::Mutex::new(Assembler::default()));
                    let metrics2 = metrics.clone();
                    let peer_id2 = peer_id.clone();
                        dc.on_message(Box::new(move |m: DataChannelMessage| {
                            let sessions2 = sessions2.clone();
                            let session_id2 = session_id2.clone();
                            let assembler = assembler.clone();
                            let metrics2 = metrics2.clone();
                            let peer_id2 = peer_id2.clone();
                            Box::pin({
                                let sessions4 = sessions2.clone();
                                let session_id4 = session_id2.clone();
                                async move {
                                let frame_bytes = m.data;
                                metrics2.webrtc_rx_add(frame_bytes.len() as u64);
                                metrics2.webrtc_peer_seen(&peer_id2);
                                {
                                    let mut guard = sessions4.lock().await;
                                    if let Some(s) = guard.get_mut(&session_id4) {
                                        s.last_used_ms = now_ms();
                                    }
                                }
                                let payload = {
                                    let mut a = assembler.lock().await;
                                    a.ingest(&frame_bytes)
                                };
                            let Some(payload) = payload else { return };
                            let Ok(w) = serde_json::from_slice::<WireMsg>(&payload) else {
                                return;
                            };
                            match w {
                                WireMsg::Resp { id: _, resp } => {
                                    let mut guard = sessions2.lock().await;
                                    if let Some(s) = guard.remove(&session_id2) {
                                        if let Some(tx) = s.pending_resp {
                                            let _ = tx.send(Ok(resp));
                                        }
                                        let _ = s.pc.close().await;
                                    }
                                    metrics2.webrtc_sessions_set(guard.len() as u64);
                                }
                                _ => {}
                            }
                            }
                            })
                        }));
                }

                // Offer / local description.
                let offer = match pc.create_offer(None).await {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = resp_tx.send(Err(anyhow!("create_offer failed: {e:#}")));
                        continue;
                    }
                };
                if let Err(e) = pc.set_local_description(offer.clone()).await {
                    let _ = resp_tx.send(Err(anyhow!("set_local_description failed: {e:#}")));
                    continue;
                }
                let payload = serde_json::to_value(&offer).unwrap_or(serde_json::Value::Null);
                if let Err(e) = send_signal(
                    &http,
                    &cfg,
                    &remote_relay_base,
                    &peer_id,
                    &session_id,
                    "offer",
                    payload,
                )
                .await
                {
                    let _ = resp_tx.send(Err(e));
                    continue;
                }

                // Apply any buffered candidates we may have already polled (rare).
                let buffered = {
                    let mut pcand = pending_candidates.lock().await;
                    pcand.remove(&session_id).unwrap_or_default()
                };
                for c in buffered {
                    let _ = pc.add_ice_candidate(c).await;
                }

                // Register session before waiting for answer/candidates.
                {
                    let mut guard = sessions.lock().await;
                    guard.insert(
                        session_id.clone(),
                        Session {
                            session_id: session_id.clone(),
                            remote_actor: peer_actor_url.clone(),
                            remote_peer_id: peer_id.clone(),
                            remote_relay_base: remote_relay_base.clone(),
                            pc: pc.clone(),
                            last_used_ms: now_ms(),
                            pending_resp: Some(resp_tx),
                            pending_req_id: Some(req.id.clone()),
                            pending_since_ms: Some(now_ms()),
                        },
                    );
                    metrics.webrtc_sessions_set(guard.len() as u64);
                }

                // Wait for open, then send the request.
                let open_ok =
                    tokio::time::timeout(Duration::from_secs(cfg.connect_timeout_secs), open_rx)
                        .await
                        .is_ok();
                if !open_ok {
                    let mut guard = sessions.lock().await;
                    if let Some(s) = guard.remove(&session_id) {
                        if let Some(tx) = s.pending_resp {
                            let _ = tx.send(Err(anyhow!("webrtc connect timeout")));
                        }
                        let _ = s.pc.close().await;
                    }
                    continue;
                }

                let wire = WireMsg::Req {
                    id: req.id.clone(),
                    req,
                };
                if let Ok(bytes) = serde_json::to_vec(&wire) {
                    metrics.webrtc_tx_add(bytes.len() as u64);
                    metrics.webrtc_peer_seen(&peer_id);
                    if let Err(e) = send_bytes_chunked(&dc, &bytes).await {
                        let mut guard = sessions.lock().await;
                        if let Some(s) = guard.remove(&session_id) {
                            if let Some(tx) = s.pending_resp {
                                let _ = tx.send(Err(anyhow!("webrtc send failed: {e:#}")));
                            }
                            let _ = s.pc.close().await;
                        }
                        metrics.webrtc_sessions_set(guard.len() as u64);
                        continue;
                    }
                }

                // Best-effort: if peer connection fails, release session.
                {
                    let sessions3 = sessions.clone();
                    let session_id3 = session_id.clone();
                    let metrics3 = metrics.clone();
                    let cfg3 = cfg.clone();
                    let http3 = http.clone();
                    let remote_relay_base3 = remote_relay_base.clone();
                    let peer_id3 = peer_id.clone();
                    let assembler3 = Arc::new(tokio::sync::Mutex::new(Assembler::default()));
                    let handler3 = handler.clone();

                    pc.on_peer_connection_state_change(Box::new(
                        move |st: RTCPeerConnectionState| {
                            let sessions3 = sessions3.clone();
                            let session_id3 = session_id3.clone();
                            let metrics3 = metrics3.clone();
                            let cfg3 = cfg3.clone();
                            let http3 = http3.clone();
                            let remote_relay_base3 = remote_relay_base3.clone();
                            let peer_id3 = peer_id3.clone();
                            let assembler3 = assembler3.clone();
                            let handler3 = handler3.clone();
                            Box::pin(async move {
                                match st {
                                    RTCPeerConnectionState::Failed
                                    | RTCPeerConnectionState::Closed
                                    | RTCPeerConnectionState::Disconnected => {
                                        let mut guard = sessions3.lock().await;
                                        if let Some(_session) = guard.remove(&session_id3) {
                                            // Attempt reconnection for Failed/Disconnected states
                                            if st == RTCPeerConnectionState::Failed
                                                || st == RTCPeerConnectionState::Disconnected
                                            {
                                                tokio::spawn(reconnect_session(
                                                    cfg3,
                                                    http3,
                                                    remote_relay_base3,
                                                    peer_id3,
                                                    session_id3,
                                                    sessions3.clone(),
                                                    metrics3.clone(),
                                                    assembler3,
                                                    handler3,
                                                ));
                                            }
                                        }
                                        metrics3.webrtc_sessions_set(guard.len() as u64);
                                    }
                                    _ => {}
                                }
                            })
                        },
                    ));
                }
            }
        });
    }

    info!(peer=%cfg.self_peer_id, "webrtc enabled");

    // Start connection health monitoring
    {
        let sessions_clone = sessions.clone();
        let metrics_clone = metrics.clone();
        let cfg_clone = cfg.clone();
        let http_clone = http.clone();
        let assembler_clone = Arc::new(tokio::sync::Mutex::new(Assembler::default()));
        let handler_clone = handler.clone();
        tokio::spawn(async move {
            monitor_connection_health(
                sessions_clone,
                metrics_clone,
                cfg_clone,
                http_clone,
                assembler_clone,
                handler_clone,
                shutdown,
            )
            .await;
        });
    }

    Ok(Some(handle))
}

// Connection health monitoring and proactive reconnection
async fn monitor_connection_health(
    sessions: Arc<tokio::sync::Mutex<HashMap<String, Session>>>,
    metrics: Arc<NetMetrics>,
    cfg: ManagerCfg,
    http: reqwest::Client,
    assembler: Arc<tokio::sync::Mutex<Assembler>>,
    handler: Handler,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(30)); // Check every 30 seconds

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() { break; }
            }
            _ = interval.tick() => {}
        }

        if *shutdown.borrow() {
            break;
        }

        let mut guard = sessions.lock().await;
        let now = now_ms();
        let mut sessions_to_reconnect = Vec::new();
        let mut sessions_to_timeout = Vec::new();

        // Check for stale connections
        for (session_id, session) in guard.iter() {
            let last_used = session.last_used_ms;
            let idle_threshold = (cfg.idle_ttl_secs as i64) * 1000;

            if now - last_used > idle_threshold / 2 {
                // Connection is getting stale, mark for reconnection
                sessions_to_reconnect.push(session_id.clone());
            }
            if session.pending_resp.is_some() {
                let since = session.pending_since_ms.unwrap_or(session.last_used_ms);
                if now.saturating_sub(since) > (REQUEST_TIMEOUT_SECS as i64) * 1000 {
                    sessions_to_timeout.push(session_id.clone());
                }
            }
        }

        // Attempt to refresh stale connections
        for session_id in sessions_to_reconnect {
            if let Some(session) = guard.remove(&session_id) {
                warn!(session_id=%session_id, "proactively reconnecting stale webrtc session");

                // Spawn reconnection task
                let sessions_clone = sessions.clone();
                let metrics_clone = metrics.clone();
                let cfg_clone = cfg.clone();
                let http_clone = http.clone();
                let assembler_clone = assembler.clone();
                let handler_clone = handler.clone();

                tokio::spawn(async move {
                    reconnect_session(
                        cfg_clone,
                        http_clone,
                        session.remote_relay_base,
                        session.remote_peer_id,
                        session_id,
                        sessions_clone,
                        metrics_clone,
                        assembler_clone,
                        handler_clone,
                    )
                    .await;
                });
            }
        }

        let mut to_close = Vec::new();
        let mut to_fail = Vec::new();
        for session_id in sessions_to_timeout {
            if let Some(session) = guard.remove(&session_id) {
                to_close.push(session.pc.clone());
                if let Some(tx) = session.pending_resp {
                    to_fail.push(tx);
                }
            }
        }

        metrics.webrtc_sessions_set(guard.len() as u64);
        drop(guard);
        for tx in to_fail {
            let _ = tx.send(Err(anyhow!("webrtc request timeout")));
        }
        for pc in to_close {
            let _ = pc.close().await;
        }
    }
}

async fn reconnect_session(
    cfg: ManagerCfg,
    http: reqwest::Client,
    remote_relay_base: String,
    peer_id: String,
    session_id: String,
    sessions: Arc<tokio::sync::Mutex<HashMap<String, Session>>>,
    metrics: Arc<NetMetrics>,
    assembler: Arc<tokio::sync::Mutex<Assembler>>,
    handler: Handler,
) {
    // Wait a short delay before attempting reconnection
    tokio::time::sleep(Duration::from_secs(2)).await;

    warn!(session_id=%session_id, peer_id=%peer_id, "attempting webrtc reconnection");

    let pc = match new_peer_connection(&cfg).await {
        Ok(v) => v,
        Err(e) => {
            warn!(session_id=%session_id, peer_id=%peer_id, "reconnection failed to create peer connection: {e:#}");
            return;
        }
    };

    // Trickle ICE from us -> remote.
    {
        let http2 = http.clone();
        let cfg2 = cfg.clone();
        let peer_id2 = peer_id.clone();
        let remote_relay_base2 = remote_relay_base.clone();
        let session_id2 = session_id.clone();
        pc.on_ice_candidate(Box::new(move |cand| {
            let http2 = http2.clone();
            let cfg2 = cfg2.clone();
            let peer_id2 = peer_id2.clone();
            let remote_relay_base2 = remote_relay_base2.clone();
            let session_id2 = session_id2.clone();
            Box::pin(async move {
                let Some(cand) = cand else { return };
                if let Ok(init) = cand.to_json() {
                    let payload = serde_json::to_value(&init).unwrap_or(serde_json::Value::Null);
                    if let Err(e) = send_signal(&http2, &cfg2, &remote_relay_base2, &peer_id2, &session_id2, "candidate", payload).await {
                        warn!(session_id=%session_id2, peer_id=%peer_id2, "webrtc send candidate failed during reconnection: {e:#}");
                    }
                }
            })
        }));
    }

    // Create data channel and wait for open.
    let dc = match pc.create_data_channel("fedi3", None).await {
        Ok(v) => Arc::new(v),
        Err(e) => {
            warn!(session_id=%session_id, peer_id=%peer_id, "reconnection failed to create data channel: {e:#}");
            return;
        }
    };

    let (open_tx, open_rx) = oneshot::channel::<()>();
    dc.on_open(Box::new(move || {
        let _ = open_tx.send(());
        Box::pin(async {})
    }));

    // Handle incoming messages on the reconnected channel.
    {
        let sessions2 = sessions.clone();
        let session_id2 = session_id.clone();
        let assembler2 = assembler.clone();
        let metrics2 = metrics.clone();
        let peer_id2 = peer_id.clone();
        let handler2 = handler.clone();
        let private_key_pem = cfg.private_key_pem.clone();
        let dc2 = dc.clone();
        dc.on_message(Box::new(move |m: DataChannelMessage| {
            let sessions2 = sessions2.clone();
            let session_id2 = session_id2.clone();
            let assembler2 = assembler2.clone();
            let metrics2 = metrics2.clone();
            let peer_id2 = peer_id2.clone();
            let handler2 = handler2.clone();
            let private_key_pem = private_key_pem.clone();
            let dc2 = dc2.clone();
            Box::pin({
                let sessions4 = sessions2.clone();
                let session_id4 = session_id2.clone();
                async move {
                let frame_bytes = m.data;
                metrics2.webrtc_rx_add(frame_bytes.len() as u64);
                metrics2.webrtc_peer_seen(&peer_id2);
                {
                    let mut guard = sessions4.lock().await;
                    if let Some(s) = guard.get_mut(&session_id4) {
                        s.last_used_ms = now_ms();
                    }
                }
                let payload = {
                    let mut a = assembler2.lock().await;
                    a.ingest(&frame_bytes)
                };
                let Some(payload) = payload else { return };
                let Ok(w) = serde_json::from_slice::<WireMsg>(&payload) else {
                    return;
                };
                match w {
                    WireMsg::Resp { id: _, resp } => {
                        let mut guard = sessions2.lock().await;
                        if let Some(s) = guard.get_mut(&session_id2) {
                            if let Some(tx) = s.pending_resp.take() {
                                let _ = tx.send(Ok(resp));
                            }
                            s.pending_since_ms = None;
                        }
                        metrics2.webrtc_sessions_set(guard.len() as u64);
                    }
                    WireMsg::Req { id, req } => {
                        let req = decrypt_relay_http_request_body(&private_key_pem, req);
                        let mut svc = handler2.lock().await;
                        let resp = handle_relay_http_request(&mut *svc, req).await;
                        let out = WireMsg::Resp { id, resp };
                        if let Ok(bytes) = serde_json::to_vec(&out) {
                            metrics2.webrtc_tx_add(bytes.len() as u64);
                            metrics2.webrtc_peer_seen(&peer_id2);
                            let _ = send_bytes_chunked(&dc2, &bytes).await;
                        }
                    }
                }
                }
            })
        }));
    }

    // Create offer for reconnection.
    let offer = match pc.create_offer(None).await {
        Ok(v) => v,
        Err(e) => {
            warn!(session_id=%session_id, peer_id=%peer_id, "reconnection failed to create offer: {e:#}");
            return;
        }
    };
    if let Err(e) = pc.set_local_description(offer.clone()).await {
        warn!(session_id=%session_id, peer_id=%peer_id, "reconnection failed to set local description: {e:#}");
        return;
    }
    let payload = serde_json::to_value(&offer).unwrap_or(serde_json::Value::Null);
    if let Err(e) = send_signal(
        &http,
        &cfg,
        &remote_relay_base,
        &peer_id,
        &session_id,
        "offer",
        payload,
    )
    .await
    {
        warn!(session_id=%session_id, peer_id=%peer_id, "reconnection failed to send offer: {e:#}");
        return;
    }

    // Wait for open.
    let open_ok = tokio::time::timeout(Duration::from_secs(cfg.connect_timeout_secs), open_rx)
        .await
        .is_ok();
    if !open_ok {
        warn!(session_id=%session_id, peer_id=%peer_id, "reconnection failed: data channel did not open");
        return;
    }

    // Register the reconnected session.
    {
        let mut guard = sessions.lock().await;
        guard.insert(
            session_id.clone(),
            Session {
                session_id: session_id.clone(),
                remote_actor: format!(
                    "{}/users/{}",
                    cfg.self_relay_base.trim_end_matches('/'),
                    cfg.self_peer_id
                ),
                remote_peer_id: peer_id.clone(),
                remote_relay_base: remote_relay_base.clone(),
                pc,
                last_used_ms: now_ms(),
                pending_resp: None,
                pending_req_id: None,
                pending_since_ms: None,
            },
        );
        metrics.webrtc_sessions_set(guard.len() as u64);
    }

    info!(session_id=%session_id, peer_id=%peer_id, "webrtc reconnection successful");
}
