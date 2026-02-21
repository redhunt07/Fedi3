#![recursion_limit = "256"]
/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use anyhow::Result;
use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, Path, Query, RawQuery, State,
    },
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    middleware::{from_fn, from_fn_with_state, Next},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{any, delete, get, post},
    Router,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use deadpool::managed::QueueMode;
use deadpool_postgres::{ManagerConfig, Pool, PoolConfig, RecyclingMethod, Runtime, Timeouts};
use fedi3_protocol::{RelayHttpRequest, RelayHttpResponse};
use futures_util::{stream, SinkExt, StreamExt};
use http::{header, Request, Uri};
use httpdate::parse_http_date;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::convert::Infallible;
use std::future::Future;
use std::net::IpAddr;
use std::sync::OnceLock;
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::Path as FsPath,
    path::PathBuf,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;
use tokio::sync::RwLock as TokioRwLock;
use tokio::sync::{broadcast, mpsc, oneshot, RwLock, Semaphore};
use tokio_postgres::types::ToSql;
use tokio_postgres::{NoTls, Row};
use tokio_util::sync::CancellationToken;
use tower_http::trace::TraceLayer;
use tracing::{error, info, info_span, warn};

use ed25519_dalek::{Signer as _, Verifier as _};
use rusqlite::{params, Connection, OptionalExtension};

mod media_store;
mod relay_mesh;
mod relay_notes;

use relay_notes::{
    actor_to_index_from_note, extract_media_from_note, extract_notes_from_value, note_to_index,
    RelayActorIndex, RelayMediaIndex, RelayNoteIndex, RelaySyncNoteItem, RelaySyncNotesResponse,
};

static REQ_ID: AtomicU64 = AtomicU64::new(1);
const DB_BATCH_DELETE_MAX: usize = 500;
const WEBRTC_SIGNAL_TTL_SECS: i64 = 300;
const WEBRTC_SIGNAL_MAX_PER_PEER: usize = 200;
const WEBRTC_KEY_CACHE_TTL_SECS: i64 = 3600;

fn next_request_id() -> String {
    let id = REQ_ID.fetch_add(1, Ordering::Relaxed);
    format!("req-{id}")
}

#[derive(Debug, Clone, serde::Deserialize)]
struct PeerHello {
    username: String,
    actor: String,
}

#[derive(Debug, Clone, Serialize)]
struct PresenceItem {
    username: String,
    actor_url: String,
    online: bool,
}

#[derive(Debug, Clone, Serialize)]
struct PresenceSnapshot {
    ts_ms: i64,
    items: Vec<PresenceItem>,
}

#[derive(Debug, Clone)]
enum PresenceEvent {
    Update(PresenceItem),
}

#[derive(Debug, Clone)]
struct RelayReputation {
    score: i32,
    last_ms: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RelayTelemetry {
    relay_url: String,
    timestamp_ms: i64,
    online_users: u64,
    online_peers: u64,
    total_users: u64,
    total_peers_seen: u64,
    peers_seen_window_ms: i64,
    peers_seen_cutoff_ms: i64,
    base_domain: Option<String>,
    relays: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_indexed_users: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_total_users: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_last_index_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_window_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_relays_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_relays_synced: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_relays_last_sync_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_relay_sync_window_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    p2p_upnp_port_start: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    p2p_upnp_port_end: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    relay_p2p_peer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sign_pubkey_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signature_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    users: Vec<RelayUserEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    peers: Vec<RelayPeerEntry>,
}

#[derive(Debug, Deserialize)]
struct ClientTelemetryInput {
    username: String,
    #[serde(rename = "type")]
    event_type: String,
    message: String,
    stack: Option<String>,
    mode: Option<String>,
    ts: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WebrtcSendReq {
    to_peer_id: String,
    session_id: String,
    kind: String,
    payload: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct WebrtcAckReq {
    to_peer_id: String,
    ids: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RelayUserEntry {
    username: String,
    actor_url: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RelayPeerEntry {
    peer_id: String,
    username: String,
    actor_url: String,
}

#[derive(Clone)]
struct MeiliSearch {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    notes_index: String,
    users_index: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct MeiliNoteDoc {
    id: String,
    note_json: String,
    content_text: String,
    content_html: String,
    tags: Vec<String>,
    created_at_ms: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct MeiliUserDoc {
    id: String,
    username: String,
    actor_url: String,
    actor_json: Option<String>,
    updated_at_ms: i64,
}

#[derive(Debug, serde::Deserialize)]
struct MeiliSearchResponse<T> {
    hits: Vec<T>,
    #[serde(default)]
    #[serde(rename = "estimatedTotalHits")]
    estimated_total_hits: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RelayMoveNotice {
    username: String,
    moved_to_actor: String,
    #[serde(default)]
    old_actor: Option<String>,
    ts_ms: i64,
    nonce: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RelayListEntry {
    relay_url: String,
    #[serde(default)]
    sign_pubkey_b64: Option<String>,
    #[serde(default)]
    relay_p2p_peer_id: Option<String>,
    #[serde(default)]
    base_domain: Option<String>,
    #[serde(default)]
    last_seen_ms: i64,
}

#[derive(Debug, serde::Deserialize)]
struct GithubContentResponse {
    content: Option<String>,
    sha: Option<String>,
    encoding: Option<String>,
}

#[derive(Debug, Clone)]
struct SpoolItem {
    id: i64,
    method: String,
    path: String,
    query: String,
    headers_json: String,
    body_b64: String,
}

#[derive(Debug, Clone)]
struct MediaItem {
    id: String,
    username: String,
    backend: String,
    storage_key: String,
    media_type: String,
    size: i64,
    created_at_ms: i64,
}

#[derive(Debug, Clone)]
struct UserBackupItem {
    username: String,
    storage_key: String,
    content_type: String,
    size_bytes: i64,
    updated_at_ms: i64,
    meta_json: Option<String>,
}

#[derive(Debug, Clone)]
struct ActorCacheMeta {
    actor_json: String,
    updated_at_ms: i64,
    actor_id: Option<String>,
    actor_url: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct WebrtcSignal {
    id: String,
    from_actor: String,
    session_id: String,
    kind: String,
    payload: serde_json::Value,
    created_at_ms: i64,
}

#[derive(Debug, Clone)]
struct CollectionPage<T> {
    total: u64,
    items: Vec<T>,
    next: Option<String>,
}

#[derive(Clone)]
struct AppState {
    tunnels: Arc<RwLock<HashMap<String, TunnelHandle>>>,
    inflight_per_user: Arc<RwLock<HashMap<String, Arc<Semaphore>>>>,
    peer_hello: Arc<RwLock<HashMap<String, PeerHello>>>,
    relay_mesh_peer_id: Arc<RwLock<Option<String>>>,
    presence_tx: broadcast::Sender<PresenceEvent>,
    presence_last_seen: Arc<Mutex<HashMap<String, i64>>>,
    github_issues: Option<Arc<GithubIssueReporter>>,
    telemetry_dedupe: Arc<Mutex<HashMap<String, i64>>>,
    webrtc_signals: Arc<Mutex<HashMap<String, Vec<WebrtcSignal>>>>,
    webrtc_key_cache: Arc<Mutex<HashMap<String, (String, i64)>>>,
    relay_reputation: Arc<Mutex<HashMap<String, RelayReputation>>>,
    cfg: RelayConfig,
    db: Arc<Mutex<Db>>,
    limiter: Arc<RateLimiter>,
    http: reqwest::Client,
    search: Option<Arc<MeiliSearch>>,
    meili_indexer: Option<Arc<MeiliIndexer>>,
    search_cache: Option<Arc<SearchCache>>,
    media_cfg: media_store::MediaConfig,
    media_backend: Arc<dyn media_store::MediaBackend>,
}

#[derive(Clone)]
struct TunnelHandle {
    tx: mpsc::Sender<TunnelRequest>,
}

struct TunnelRequest {
    id: String,
    req: RelayHttpRequest,
    resp_tx: oneshot::Sender<RelayHttpResponse>,
}

enum MeiliItem {
    User(MeiliUserDoc),
    Note(MeiliNoteDoc),
}

struct MeiliIndexer {
    tx: mpsc::Sender<MeiliItem>,
}

struct GithubIssueReporter {
    labels: Vec<String>,
    assignee: Option<String>,
    tx: mpsc::Sender<GithubIssueRequest>,
}

struct GithubIssueRequest {
    title: String,
    body: String,
    labels: Vec<String>,
    assignee: Option<String>,
}

impl MeiliIndexer {
    fn new(search: Arc<MeiliSearch>, batch_max: usize, flush_ms: u64, queue_max: usize) -> Self {
        let (tx, mut rx) = mpsc::channel(queue_max.max(16));
        let batch_max = batch_max.max(1).min(500);
        let flush_ms = flush_ms.max(50).min(5_000);
        tokio::spawn(async move {
            let mut users: Vec<MeiliUserDoc> = Vec::with_capacity(batch_max);
            let mut notes: Vec<MeiliNoteDoc> = Vec::with_capacity(batch_max);
            let mut ticker = tokio::time::interval(Duration::from_millis(flush_ms));
            loop {
                tokio::select! {
                    Some(item) = rx.recv() => {
                        match item {
                            MeiliItem::User(doc) => {
                                users.push(doc);
                                if users.len() >= batch_max {
                                    let _ = search.upsert_users(&users).await;
                                    users.clear();
                                }
                            }
                            MeiliItem::Note(doc) => {
                                notes.push(doc);
                                if notes.len() >= batch_max {
                                    let _ = search.upsert_notes(&notes).await;
                                    notes.clear();
                                }
                            }
                        }
                    }
                    _ = ticker.tick() => {
                        if !users.is_empty() {
                            let _ = search.upsert_users(&users).await;
                            users.clear();
                        }
                        if !notes.is_empty() {
                            let _ = search.upsert_notes(&notes).await;
                            notes.clear();
                        }
                    }
                }
            }
        });
        Self { tx }
    }

    fn enqueue_user(&self, doc: MeiliUserDoc) {
        let _ = self.tx.try_send(MeiliItem::User(doc));
    }

    fn enqueue_note(&self, doc: MeiliNoteDoc) {
        let _ = self.tx.try_send(MeiliItem::Note(doc));
    }
}

fn spawn_github_issues(cfg: &RelayConfig, http: reqwest::Client) -> Option<Arc<GithubIssueReporter>> {
    let repo = cfg.github_repo.as_ref()?.trim().to_string();
    let token = cfg.github_token.as_ref()?.trim().to_string();
    if repo.is_empty() || token.is_empty() {
        return None;
    }
    let (tx, mut rx) = mpsc::channel::<GithubIssueRequest>(200);
    let labels = cfg.github_issue_labels.clone();
    let assignee = cfg.github_issue_assignee.clone();
    let reporter = GithubIssueReporter {
        labels,
        assignee,
        tx,
    };
    tokio::spawn(async move {
        while let Some(req) = rx.recv().await {
            let url = format!("https://api.github.com/repos/{repo}/issues");
            let mut payload = serde_json::json!({
                "title": req.title,
                "body": req.body,
                "labels": req.labels,
            });
            if let Some(a) = req.assignee.as_ref().filter(|v| !v.is_empty()) {
                payload["assignees"] = serde_json::json!([a]);
            }
            let resp = http
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .header("Accept", "application/vnd.github+json")
                .header("User-Agent", "fedi3-relay")
                .json(&payload)
                .send()
                .await;
            match resp {
                Ok(r) if r.status().is_success() => {}
                Ok(r) if r.status().as_u16() == 422 => {
                    let payload = serde_json::json!({
                        "title": req.title,
                        "body": req.body,
                    });
                    let _ = http
                        .post(&url)
                        .header("Authorization", format!("Bearer {token}"))
                        .header("Accept", "application/vnd.github+json")
                        .header("User-Agent", "fedi3-relay")
                        .json(&payload)
                        .send()
                        .await;
                }
                Ok(r) => {
                    let status = r.status();
                    let body = r.text().await.unwrap_or_default();
                    warn!("github issue failed: {status} {body}");
                }
                Err(e) => warn!("github issue send failed: {e}"),
            }
        }
    });
    Some(Arc::new(reporter))
}

async fn sync_relay_list_once(state: &AppState) -> Result<()> {
    let Some(repo) = state.cfg.relay_list_repo.as_deref().filter(|v| !v.is_empty()) else {
        return Ok(());
    };
    let path = state.cfg.relay_list_path.trim().to_string();
    let branch = state.cfg.relay_list_branch.trim().to_string();
    let (mut entries, sha) =
        fetch_relay_list_from_github(state, repo, &path, &branch, state.cfg.relay_list_token.as_deref())
            .await?;

    if !entries.is_empty() {
        let mut db = state.db.lock().await;
        for entry in &entries {
            if entry.relay_url.trim().is_empty() {
                continue;
            }
            let _ = db.upsert_relay(
                &entry.relay_url,
                entry.base_domain.clone(),
                None,
                entry.sign_pubkey_b64.clone(),
            );
        }
    }

    let mut changed = false;
    if let Some(self_entry) = build_self_relay_list_entry(state).await? {
        if let Some(existing) = entries
            .iter_mut()
            .find(|e| e.relay_url.trim_end_matches('/') == self_entry.relay_url)
        {
            if existing.sign_pubkey_b64 != self_entry.sign_pubkey_b64
                || existing.relay_p2p_peer_id != self_entry.relay_p2p_peer_id
                || existing.base_domain != self_entry.base_domain
            {
                *existing = self_entry;
                changed = true;
            }
        } else {
            entries.push(self_entry);
            changed = true;
        }
    }

    if changed {
        if let Some(token) = state.cfg.relay_list_token.as_deref() {
            entries.sort_by(|a, b| a.relay_url.cmp(&b.relay_url));
            update_relay_list_on_github(state, repo, &path, &branch, token, &entries, sha).await?;
        }
    }

    Ok(())
}

async fn build_self_relay_list_entry(state: &AppState) -> Result<Option<RelayListEntry>> {
    let Some(relay_url) = state.cfg.public_url.clone() else {
        return Ok(None);
    };
    let relay_url = relay_url.trim_end_matches('/').to_string();
    if relay_url.is_empty() {
        return Ok(None);
    }
    let peer_id = state.relay_mesh_peer_id.read().await.clone();
    let db = state.db.lock().await;
    let (pk_b64, _) = db.load_or_create_signing_keypair_b64()?;
    Ok(Some(RelayListEntry {
        relay_url,
        sign_pubkey_b64: Some(pk_b64),
        relay_p2p_peer_id: peer_id,
        base_domain: state.cfg.base_domain.clone(),
        last_seen_ms: now_ms(),
    }))
}

async fn fetch_relay_list_from_github(
    state: &AppState,
    repo: &str,
    path: &str,
    branch: &str,
    token: Option<&str>,
) -> Result<(Vec<RelayListEntry>, Option<String>)> {
    let url = format!(
        "https://api.github.com/repos/{repo}/contents/{path}?ref={branch}"
    );
    let mut req = state
        .http
        .get(url)
        .header("User-Agent", "fedi3-relay");
    if let Some(tok) = token {
        req = req.header("Authorization", format!("Bearer {tok}"));
    }
    let resp = req.send().await?;
    if resp.status().as_u16() == 404 {
        return Ok((Vec::new(), None));
    }
    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("relay list fetch failed: {}", resp.status()));
    }
    let data: GithubContentResponse = resp.json().await?;
    let Some(content) = data.content else {
        return Ok((Vec::new(), data.sha));
    };
    let mut raw = content.replace('\n', "");
    if data.encoding.as_deref() == Some("base64") {
        let bytes = B64.decode(raw.as_bytes()).unwrap_or_default();
        raw = String::from_utf8(bytes).unwrap_or_default();
    }
    let entries = parse_relay_list_entries(&raw);
    Ok((entries, data.sha))
}

fn parse_relay_list_entries(raw: &str) -> Vec<RelayListEntry> {
    let value: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let items = if let Some(arr) = value.as_array() {
        arr.clone()
    } else if let Some(arr) = value.get("entries").and_then(|v| v.as_array()) {
        arr.clone()
    } else {
        Vec::new()
    };
    let mut out = Vec::new();
    for item in items {
        if let Ok(entry) = serde_json::from_value::<RelayListEntry>(item) {
            if entry.relay_url.trim().is_empty() {
                continue;
            }
            out.push(entry);
        }
    }
    out
}

async fn update_relay_list_on_github(
    state: &AppState,
    repo: &str,
    path: &str,
    branch: &str,
    token: &str,
    entries: &[RelayListEntry],
    sha: Option<String>,
) -> Result<()> {
    let url = format!("https://api.github.com/repos/{repo}/contents/{path}");
    let payload_json = serde_json::to_vec(entries)?;
    let content = B64.encode(payload_json);
    let mut payload = serde_json::json!({
        "message": "fedi3 relay list update",
        "content": content,
        "branch": branch,
    });
    if let Some(sha) = sha {
        payload["sha"] = serde_json::Value::String(sha);
    }
    let resp = state
        .http
        .put(url)
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "fedi3-relay")
        .json(&payload)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(anyhow::anyhow!(
            "relay list update failed: {}",
            resp.status()
        ));
    }
    Ok(())
}

#[derive(Clone)]
struct SearchCache {
    ttl_secs: u64,
    max_entries: usize,
    users: Arc<TokioRwLock<HashMap<String, (i64, serde_json::Value)>>>,
    notes: Arc<TokioRwLock<HashMap<String, (i64, serde_json::Value)>>>,
}

impl SearchCache {
    fn new(ttl_secs: u64, max_entries: usize) -> Self {
        Self {
            ttl_secs: ttl_secs.max(5).min(300),
            max_entries: max_entries.max(64).min(10_000),
            users: Arc::new(TokioRwLock::new(HashMap::new())),
            notes: Arc::new(TokioRwLock::new(HashMap::new())),
        }
    }

    async fn get_users(&self, key: &str) -> Option<serde_json::Value> {
        let now = now_ms();
        let map = self.users.read().await;
        let (ts, value) = map.get(key)?.clone();
        if now.saturating_sub(ts) <= (self.ttl_secs as i64 * 1000) {
            Some(value)
        } else {
            None
        }
    }

    async fn set_users(&self, key: String, value: serde_json::Value) {
        let now = now_ms();
        let mut map = self.users.write().await;
        if map.len() >= self.max_entries {
            map.clear();
        }
        map.insert(key, (now, value));
    }

    async fn get_notes(&self, key: &str) -> Option<serde_json::Value> {
        let now = now_ms();
        let map = self.notes.read().await;
        let (ts, value) = map.get(key)?.clone();
        if now.saturating_sub(ts) <= (self.ttl_secs as i64 * 1000) {
            Some(value)
        } else {
            None
        }
    }

    async fn set_notes(&self, key: String, value: serde_json::Value) {
        let now = now_ms();
        let mut map = self.notes.write().await;
        if map.len() >= self.max_entries {
            map.clear();
        }
        map.insert(key, (now, value));
    }
}

#[derive(Clone, Debug, Default)]
struct AuditMeta {
    request_id: Option<String>,
    correlation_id: Option<String>,
    user_agent: Option<String>,
}

#[derive(Clone, Debug)]
struct AdminAuditContext {
    ip: String,
    meta: AuditMeta,
}

#[derive(Clone)]
struct RelayConfig {
    bind: SocketAddr,
    base_domain: Option<String>,
    trust_proxy_headers: bool,
    allow_self_register: bool,
    admin_token: Option<String>,
    public_url: Option<String>,
    telemetry_token: Option<String>,
    github_token: Option<String>,
    github_repo: Option<String>,
    github_issue_labels: Vec<String>,
    github_issue_assignee: Option<String>,
    relay_list_repo: Option<String>,
    relay_list_path: String,
    relay_list_branch: String,
    relay_list_token: Option<String>,
    relay_list_refresh_secs: u64,
    seed_relays: Vec<String>,
    p2p_infra_peer_id: Option<String>,
    p2p_infra_multiaddrs: Vec<String>,
    p2p_infra_host: Option<String>,
    p2p_infra_port: u16,
    relay_mesh_enable: bool,
    relay_mesh_listen: Vec<String>,
    relay_mesh_bootstrap: Vec<String>,
    relay_mesh_key_path: PathBuf,
    p2p_upnp_port_start: Option<u16>,
    p2p_upnp_port_end: Option<u16>,
    telemetry_interval_secs: u64,
    max_body_bytes: usize,
    http_timeout_secs: u64,
    http_connect_timeout_secs: u64,
    http_pool_idle_timeout_secs: u64,
    http_pool_max_idle_per_host: usize,
    hsts_max_age_secs: u64,
    csp: Option<String>,
    tunnel_timeout_secs: u64,
    rate_limit_register_per_min: u32,
    rate_limit_tunnel_per_min: u32,
    rate_limit_inbox_per_min: u32,
    rate_limit_forward_per_min: u32,
    rate_limit_admin_per_min: u32,
    rate_limit_client_telemetry_per_min: u32,
    search_backend: String,
    search_total_mode: SearchTotalMode,
    search_cache_ttl_secs: u64,
    search_cache_max_entries: usize,
    meili_url: Option<String>,
    meili_api_key: Option<String>,
    meili_timeout_secs: u64,
    meili_notes_index: String,
    meili_users_index: String,
    meili_batch_max: usize,
    meili_flush_ms: u64,
    meili_queue_max: usize,
    db_driver: DbDriver,
    db_url: Option<String>,
    db_synchronous: String,
    db_cache_kb: i64,
    db_busy_timeout_ms: u64,
    pg_pool_max_size: usize,
    pg_pool_wait_ms: Option<u64>,
    pg_pool_create_timeout_ms: Option<u64>,
    pg_pool_recycle_timeout_ms: Option<u64>,
    pg_pool_queue_mode: QueueMode,
    pg_init_retries: usize,
    pg_init_backoff_ms: u64,
    redis_url: Option<String>,
    redis_prefix: String,
    redis_pool_size: usize,
    ip_allowlist: Vec<IpRule>,
    ip_denylist: Vec<IpRule>,
    noisy_backoff_base_secs: u64,
    noisy_backoff_max_secs: u64,
    max_inbox_fanout: usize,
    max_inflight_per_user: usize,
    spool_ttl_secs: u64,
    move_notice_ttl_secs: u64,
    move_notice_fanout_interval_secs: u64,
    spool_max_rows_per_user: usize,
    spool_flush_batch: usize,
    peer_directory_ttl_days: u32,
    media_backend: String,
    media_dir: PathBuf,
    media_prefix: String,
    media_webdav_base_url: Option<String>,
    media_webdav_username: Option<String>,
    media_webdav_password: Option<String>,
    media_webdav_bearer_token: Option<String>,
    media_s3_region: Option<String>,
    media_s3_bucket: Option<String>,
    media_s3_endpoint: Option<String>,
    media_s3_access_key: Option<String>,
    media_s3_secret_key: Option<String>,
    media_s3_path_style: bool,
    backup_max_bytes: usize,
    backup_retention_count: usize,
    backup_rate_limit_per_hour: u32,
    outbox_index_interval_secs: u64,
    outbox_index_pages: u32,
    outbox_index_page_limit: u32,
    telemetry_users_limit: u32,
    telemetry_peers_limit: u32,
    relay_sync_interval_secs: u64,
    relay_sync_limit: u32,
    relay_media_ttl_secs: u64,
    relay_actor_ttl_secs: u64,
    relay_reputation_ttl_secs: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchTotalMode {
    Exact,
    Approx,
    None,
}

#[derive(Clone)]
struct Db {
    driver: DbDriver,
    path: PathBuf,
    db_url: Option<String>,
    db_synchronous: String,
    db_cache_kb: i64,
    db_busy_timeout_ms: u64,
    pg_pool_max_size: usize,
    pg_pool_wait_ms: Option<u64>,
    pg_pool_create_timeout_ms: Option<u64>,
    pg_pool_recycle_timeout_ms: Option<u64>,
    pg_pool_queue_mode: QueueMode,
    pg_init_retries: usize,
    pg_init_backoff_ms: u64,
    pg_pool: OnceLock<Pool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DbDriver {
    Sqlite,
    Postgres,
}

struct PgConn {
    client: deadpool_postgres::Object,
}

struct PgTx<'a> {
    tx: deadpool_postgres::Transaction<'a>,
}

impl PgConn {
    fn execute(&mut self, stmt: &str, params: &[&(dyn ToSql + Sync)]) -> Result<u64> {
        block_on_result(self.client.execute(stmt, params)).map_err(Into::into)
    }

    fn transaction(&mut self) -> Result<PgTx<'_>> {
        let tx = block_on_result(self.client.transaction())?;
        Ok(PgTx { tx })
    }

    fn batch_execute(&mut self, stmt: &str) -> Result<()> {
        block_on_result(self.client.batch_execute(stmt)).map_err(Into::into)
    }

    fn query(&mut self, stmt: &str, params: &[&(dyn ToSql + Sync)]) -> Result<Vec<Row>> {
        block_on_result(self.client.query(stmt, params)).map_err(Into::into)
    }

    fn query_one(&mut self, stmt: &str, params: &[&(dyn ToSql + Sync)]) -> Result<Row> {
        block_on_result(self.client.query_one(stmt, params)).map_err(Into::into)
    }

    fn query_opt(&mut self, stmt: &str, params: &[&(dyn ToSql + Sync)]) -> Result<Option<Row>> {
        block_on_result(self.client.query_opt(stmt, params)).map_err(Into::into)
    }
}

impl<'a> PgTx<'a> {
    fn execute(&mut self, stmt: &str, params: &[&(dyn ToSql + Sync)]) -> Result<u64> {
        block_on_result(self.tx.execute(stmt, params)).map_err(Into::into)
    }

    fn commit(self) -> Result<()> {
        block_on_result(self.tx.commit()).map_err(Into::into)
    }
}

fn block_on_result<F, T, E>(fut: F) -> Result<T>
where
    F: Future<Output = std::result::Result<T, E>>,
    E: std::error::Error + Send + Sync + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(fut)).map_err(Into::into)
    } else {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(fut).map_err(Into::into)
    }
}

#[derive(Debug, Deserialize)]
struct WebfingerQuery {
    resource: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct RelayMeQuery {
    username: String,
}

#[derive(Debug, Deserialize)]
struct TunnelQuery {
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RelayTelemetryQuery {
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct RelayPeersQuery {
    q: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct RelaySyncNotesQuery {
    limit: Option<u32>,
    since: Option<i64>,
    cursor: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    username: String,
    token: String,
}

#[derive(Debug, serde::Serialize)]
struct AdminRotateResponse {
    token: String,
}

#[derive(serde::Serialize)]
struct NodeInfoLinks {
    links: Vec<NodeInfoLink>,
}

#[derive(serde::Serialize)]
struct NodeInfoLink {
    rel: String,
    href: String,
}

#[allow(non_snake_case)]
#[derive(serde::Serialize)]
struct NodeInfo2 {
    version: String,
    software: NodeInfoSoftware,
    protocols: Vec<String>,
    services: NodeInfoServices,
    openRegistrations: bool,
    usage: NodeInfoUsage,
    metadata: serde_json::Value,
}

#[derive(serde::Serialize)]
struct NodeInfoSoftware {
    name: String,
    version: String,
}

#[derive(serde::Serialize)]
struct NodeInfoServices {
    inbound: Vec<String>,
    outbound: Vec<String>,
}

#[derive(serde::Serialize)]
struct NodeInfoUsage {
    users: NodeInfoUsers,
}

#[derive(serde::Serialize)]
struct NodeInfoUsers {
    total: u64,
}

impl MeiliSearch {
    fn req(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        let mut builder = self.client.request(method, url);
        if let Some(key) = self.api_key.as_ref() {
            let bearer = format!("Bearer {key}");
            builder = builder
                .header("Authorization", bearer)
                .header("X-Meili-API-Key", key);
        }
        builder
    }

    async fn ensure_indexes(&self) -> Result<()> {
        let notes_body = serde_json::json!({
            "uid": self.notes_index,
            "primaryKey": "id"
        });
        let users_body = serde_json::json!({
            "uid": self.users_index,
            "primaryKey": "id"
        });
        let resp = self
            .req(reqwest::Method::POST, "/indexes")
            .json(&notes_body)
            .send()
            .await?;
        if !resp.status().is_success() && resp.status().as_u16() != 409 {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            warn!("meili create notes index failed: {status} {body}");
        }
        let resp = self
            .req(reqwest::Method::POST, "/indexes")
            .json(&users_body)
            .send()
            .await?;
        if !resp.status().is_success() && resp.status().as_u16() != 409 {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            warn!("meili create users index failed: {status} {body}");
        }

        let notes_settings = serde_json::json!({
            "filterableAttributes": ["tags", "created_at_ms"],
            "sortableAttributes": ["created_at_ms"],
        });
        let users_settings = serde_json::json!({
            "filterableAttributes": ["username", "updated_at_ms"],
            "sortableAttributes": ["updated_at_ms"],
        });
        let resp = self
            .req(
                reqwest::Method::PATCH,
                &format!("/indexes/{}/settings", self.notes_index),
            )
            .json(&notes_settings)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            warn!("meili notes settings failed: {status} {body}");
        }
        let resp = self
            .req(
                reqwest::Method::PATCH,
                &format!("/indexes/{}/settings", self.users_index),
            )
            .json(&users_settings)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            warn!("meili users settings failed: {status} {body}");
        }
        Ok(())
    }

    async fn upsert_notes(&self, docs: &[MeiliNoteDoc]) -> Result<()> {
        if docs.is_empty() {
            return Ok(());
        }
        let resp = self
            .req(
                reqwest::Method::POST,
                &format!("/indexes/{}/documents", self.notes_index),
            )
            .json(docs)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("meili upsert notes failed: {status} {body}");
        }
        Ok(())
    }

    async fn upsert_users(&self, docs: &[MeiliUserDoc]) -> Result<()> {
        if docs.is_empty() {
            return Ok(());
        }
        let resp = self
            .req(
                reqwest::Method::POST,
                &format!("/indexes/{}/documents", self.users_index),
            )
            .json(docs)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("meili upsert users failed: {status} {body}");
        }
        Ok(())
    }

    async fn search_notes(
        &self,
        q: &str,
        tag: &str,
        limit: u32,
        cursor: Option<i64>,
        since: Option<i64>,
    ) -> Result<CollectionPage<String>> {
        let mut filters: Vec<String> = Vec::new();
        let tag_norm = tag.trim().trim_start_matches('#');
        if !tag_norm.is_empty() {
            filters.push(format!("tags = \"{}\"", escape_meili_filter(tag_norm)));
        }
        if let Some(since) = since {
            filters.push(format!("created_at_ms > {}", since));
        }
        if let Some(cur) = cursor {
            filters.push(format!("created_at_ms < {}", cur));
        }
        let filter = if filters.is_empty() {
            None
        } else {
            Some(filters.join(" AND "))
        };
        let body = serde_json::json!({
            "q": q,
            "limit": limit.min(200).max(1),
            "filter": filter,
            "sort": ["created_at_ms:desc"]
        });
        let resp = self
            .req(
                reqwest::Method::POST,
                &format!("/indexes/{}/search", self.notes_index),
            )
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("meili search notes failed: {status} {body}");
        }
        let out: MeiliSearchResponse<MeiliNoteDoc> = resp.json().await?;
        let mut items = Vec::new();
        let mut last_created = None;
        for hit in out.hits {
            last_created = Some(hit.created_at_ms);
            items.push(hit.note_json);
        }
        let next = if items.len() as u32 == limit.min(200).max(1) {
            last_created.map(|v| v.to_string())
        } else {
            None
        };
        Ok(CollectionPage {
            total: out.estimated_total_hits.unwrap_or(items.len() as u64),
            items,
            next,
        })
    }

    async fn search_users(
        &self,
        q: &str,
        limit: u32,
        cursor: Option<i64>,
        base_template: &str,
    ) -> Result<CollectionPage<String>> {
        let mut filters: Vec<String> = Vec::new();
        if let Some(cur) = cursor {
            filters.push(format!("updated_at_ms < {}", cur));
        }
        let filter = if filters.is_empty() {
            None
        } else {
            Some(filters.join(" AND "))
        };
        let body = serde_json::json!({
            "q": q,
            "limit": limit.min(200).max(1),
            "filter": filter,
            "sort": ["updated_at_ms:desc"]
        });
        let resp = self
            .req(
                reqwest::Method::POST,
                &format!("/indexes/{}/search", self.users_index),
            )
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("meili search users failed: {status} {body}");
        }
        let out: MeiliSearchResponse<MeiliUserDoc> = resp.json().await?;
        let mut items = Vec::new();
        let mut last_updated = None;
        for hit in out.hits {
            last_updated = Some(hit.updated_at_ms);
            if let Some(actor_json) = hit.actor_json {
                items.push(actor_json);
            } else {
                let stub = actor_stub_from_actor_url(&hit.username, &hit.actor_url, base_template);
                items.push(serde_json::to_string(&stub).unwrap_or_default());
            }
        }
        let next = if items.len() as u32 == limit.min(200).max(1) {
            last_updated.map(|v| v.to_string())
        } else {
            None
        };
        Ok(CollectionPage {
            total: out.estimated_total_hits.unwrap_or(items.len() as u64),
            items,
            next,
        })
    }
}

impl AppState {
    fn meili_index_user(&self, doc: MeiliUserDoc) {
        if let Some(indexer) = self.meili_indexer.as_ref() {
            indexer.enqueue_user(doc);
        }
    }

    fn meili_index_note(&self, doc: MeiliNoteDoc) {
        if let Some(indexer) = self.meili_indexer.as_ref() {
            indexer.enqueue_note(doc);
        }
    }
}

fn escape_meili_filter(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

async fn build_meili(cfg: &RelayConfig, _http: &reqwest::Client) -> Option<Arc<MeiliSearch>> {
    if cfg.search_backend != "meili" {
        return None;
    }
    let Some(base_url) = cfg.meili_url.as_ref() else {
        return None;
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(cfg.meili_timeout_secs))
        .build()
        .ok()?;
    let search = MeiliSearch {
        client,
        base_url: base_url.trim_end_matches('/').to_string(),
        api_key: cfg.meili_api_key.clone(),
        notes_index: cfg.meili_notes_index.clone(),
        users_index: cfg.meili_users_index.clone(),
    };
    if let Err(e) = search.ensure_indexes().await {
        error!("meili init failed: {e:#}");
        return None;
    }
    Some(Arc::new(search))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("info".parse().unwrap()),
        )
        .init();

    let cfg = load_config();
    let db_path = std::env::var("FEDI3_RELAY_DB").unwrap_or_else(|_| "fedi3_relay.db".to_string());
    let db = Db {
        driver: cfg.db_driver,
        path: PathBuf::from(db_path),
        db_url: cfg.db_url.clone(),
        db_synchronous: cfg.db_synchronous.clone(),
        db_cache_kb: cfg.db_cache_kb,
        db_busy_timeout_ms: cfg.db_busy_timeout_ms,
        pg_pool_max_size: cfg.pg_pool_max_size,
        pg_pool_wait_ms: cfg.pg_pool_wait_ms,
        pg_pool_create_timeout_ms: cfg.pg_pool_create_timeout_ms,
        pg_pool_recycle_timeout_ms: cfg.pg_pool_recycle_timeout_ms,
        pg_pool_queue_mode: cfg.pg_pool_queue_mode,
        pg_init_retries: cfg.pg_init_retries,
        pg_init_backoff_ms: cfg.pg_init_backoff_ms,
        pg_pool: OnceLock::new(),
    };
    db.init().expect("db init");
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(cfg.http_timeout_secs))
        .connect_timeout(Duration::from_secs(cfg.http_connect_timeout_secs))
        .pool_idle_timeout(Duration::from_secs(cfg.http_pool_idle_timeout_secs))
        .pool_max_idle_per_host(cfg.http_pool_max_idle_per_host)
        .build()
        .expect("http client init");
    let media_cfg = media_store::MediaConfig {
        backend: cfg.media_backend.clone(),
        local_dir: cfg.media_dir.clone(),
        webdav_base_url: cfg.media_webdav_base_url.clone(),
        webdav_username: cfg.media_webdav_username.clone(),
        webdav_password: cfg.media_webdav_password.clone(),
        webdav_bearer_token: cfg.media_webdav_bearer_token.clone(),
        s3_region: cfg.media_s3_region.clone(),
        s3_bucket: cfg.media_s3_bucket.clone(),
        s3_endpoint: cfg.media_s3_endpoint.clone(),
        s3_access_key: cfg.media_s3_access_key.clone(),
        s3_secret_key: cfg.media_s3_secret_key.clone(),
        s3_path_style: cfg.media_s3_path_style,
    };
    let media_backend = media_store::build_media_backend(&media_cfg, http.clone())
        .await
        .expect("media backend init");
    let search = build_meili(&cfg, &http).await;
    let meili_indexer = search.as_ref().map(|search| {
        Arc::new(MeiliIndexer::new(
            search.clone(),
            cfg.meili_batch_max,
            cfg.meili_flush_ms,
            cfg.meili_queue_max,
        ))
    });
    let search_cache = if cfg.search_cache_ttl_secs == 0 || cfg.search_cache_max_entries == 0 {
        None
    } else {
        Some(Arc::new(SearchCache::new(
            cfg.search_cache_ttl_secs,
            cfg.search_cache_max_entries,
        )))
    };

    let limiter = Arc::new(
        RateLimiter::new(
            cfg.noisy_backoff_base_secs,
            cfg.noisy_backoff_max_secs,
            cfg.redis_url.clone(),
            cfg.redis_prefix.clone(),
            cfg.redis_pool_size,
        )
        .await,
    );

    let state = AppState {
        tunnels: Arc::new(RwLock::new(HashMap::new())),
        inflight_per_user: Arc::new(RwLock::new(HashMap::new())),
        peer_hello: Arc::new(RwLock::new(HashMap::new())),
        relay_mesh_peer_id: Arc::new(RwLock::new(None)),
        presence_tx: broadcast::channel(256).0,
        presence_last_seen: Arc::new(Mutex::new(HashMap::new())),
        github_issues: spawn_github_issues(&cfg, http.clone()),
        telemetry_dedupe: Arc::new(Mutex::new(HashMap::new())),
        webrtc_signals: Arc::new(Mutex::new(HashMap::new())),
        webrtc_key_cache: Arc::new(Mutex::new(HashMap::new())),
        relay_reputation: Arc::new(Mutex::new(HashMap::new())),
        cfg,
        db: Arc::new(Mutex::new(db)),
        limiter,
        http,
        search,
        meili_indexer,
        search_cache,
        media_cfg,
        media_backend: Arc::from(media_backend),
    };

    let addr = state.cfg.bind;
    let base_domain = state.cfg.base_domain.clone();
    let max_body = state.cfg.max_body_bytes;

    let reputation_ttl_ms = (state.cfg.relay_reputation_ttl_secs as i64) * 1000;
    if let Ok(entries) = {
        let db = state.db.lock().await;
        db.list_relay_reputation()
    } {
        let now = now_ms();
        let mut rep = state.relay_reputation.lock().await;
        for (relay_url, score, updated_at_ms) in entries {
            if reputation_ttl_ms == 0 || now.saturating_sub(updated_at_ms) <= reputation_ttl_ms {
                rep.insert(
                    relay_url,
                    RelayReputation {
                        score,
                        last_ms: updated_at_ms,
                    },
                );
            }
        }
    }

    relay_mesh::spawn_relay_mesh(state.clone());

    let relay_list_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = sync_relay_list_once(&relay_list_state).await {
            warn!("relay list sync failed: {e:#}");
        }
        let mut interval =
            tokio::time::interval(Duration::from_secs(relay_list_state.cfg.relay_list_refresh_secs));
        loop {
            interval.tick().await;
            if let Err(e) = sync_relay_list_once(&relay_list_state).await {
                warn!("relay list sync failed: {e:#}");
            }
        }
    });

    let cleanup_state = state.clone();
    let spool_ttl_secs = cleanup_state.cfg.spool_ttl_secs;
    let peer_directory_ttl_days = cleanup_state.cfg.peer_directory_ttl_days;
    let relay_media_ttl_secs = cleanup_state.cfg.relay_media_ttl_secs;
    let relay_actor_ttl_secs = cleanup_state.cfg.relay_actor_ttl_secs;
    let relay_reputation_ttl_secs = cleanup_state.cfg.relay_reputation_ttl_secs;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let db = cleanup_state.db.lock().await;
            if let Err(e) = db.cleanup_spool(spool_ttl_secs) {
                error!("spool cleanup failed: {e}");
            }
            if let Err(e) = db.cleanup_move_notices(cleanup_state.cfg.move_notice_ttl_secs) {
                error!("move_notices cleanup failed: {e}");
            }
            if let Err(e) = db.cleanup_relay_media(relay_media_ttl_secs) {
                error!("relay_media cleanup failed: {e}");
            }
            if let Err(e) = db.cleanup_relay_actors(relay_actor_ttl_secs) {
                error!("relay_actors cleanup failed: {e}");
            }
            if let Err(e) = db.cleanup_relay_reputation(relay_reputation_ttl_secs) {
                error!("relay_reputation cleanup failed: {e}");
            }
            if peer_directory_ttl_days > 0 {
                if let Err(e) = db.cleanup_peer_directory(peer_directory_ttl_days) {
                    error!("peer_directory cleanup failed: {e}");
                }
            }
            if peer_directory_ttl_days > 0 {
                if let Err(e) = db.cleanup_peer_registry(peer_directory_ttl_days) {
                    error!("peer_registry cleanup failed: {e}");
                }
            }
        }
    });

    // Periodic move_notice fanout with retry/backoff.
    let fanout_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(
            fanout_state.cfg.move_notice_fanout_interval_secs.max(10),
        ));
        loop {
            interval.tick().await;
            if let Err(e) = fanout_pending_move_notices(&fanout_state).await {
                error!("move_notice fanout worker failed: {e:#}");
            }
        }
    });

    let index_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(
            index_state.cfg.outbox_index_interval_secs.max(30),
        ));
        loop {
            interval.tick().await;
            if let Err(e) = run_outbox_index_once(&index_state).await {
                error!("outbox indexer failed: {e:#}");
            }
        }
    });

    let relay_sync_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(
            relay_sync_state.cfg.relay_sync_interval_secs.max(30),
        ));
        loop {
            interval.tick().await;
            if let Err(e) = sync_relays_once(&relay_sync_state).await {
                error!("relay sync failed: {e:#}");
            }
        }
    });

    let app = Router::new()
        .route("/tunnel/:user", get(tunnel_ws))
        .route("/register", post(register))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/.well-known/host-meta", get(host_meta))
        .route("/.well-known/nodeinfo", get(nodeinfo_links))
        .route("/nodeinfo/2.0", get(nodeinfo_2))
        .route("/.well-known/webfinger", get(webfinger))
        .route("/inbox", post(shared_inbox))
        .route("/admin/users", get(admin_list_users))
        .route(
            "/admin/users/:user",
            get(admin_get_user).delete(admin_delete_user),
        )
        .route("/admin/users/:user/disable", post(admin_disable_user))
        .route("/admin/users/:user/enable", post(admin_enable_user))
        .route("/admin/users/:user/rotate_token", post(admin_rotate_token))
        .route("/admin/peers/:peer_id", delete(admin_delete_peer))
        .route("/admin/audit", get(admin_audit_list))
        .route("/_fedi3/relay/stats", get(relay_stats))
        .route("/_fedi3/relay/me", get(relay_me))
        .route("/_fedi3/relay/relays", get(relay_list))
        .route("/_fedi3/relay/peers", get(relay_peers))
        .route("/_fedi3/relay/presence/stream", get(relay_presence_stream))
        .route("/_fedi3/relay/p2p_infra", get(relay_p2p_infra))
        .route("/_fedi3/relay/metrics", get(relay_metrics_json))
        .route("/_fedi3/relay/metrics.prom", get(relay_metrics_prom))
        .route("/_fedi3/relay/search/notes", get(relay_search_notes))
        .route("/_fedi3/relay/search/users", get(relay_search_users))
        .route("/_fedi3/relay/search/hashtags", get(relay_search_hashtags))
        .route("/_fedi3/relay/search/coverage", get(relay_search_coverage))
        .route("/_fedi3/relay/sync/notes", get(relay_sync_notes))
        .route("/_fedi3/relay/reindex", post(relay_reindex))
        .route("/_fedi3/relay/telemetry", post(relay_telemetry_post))
        .route(
            "/_fedi3/relay/telemetry/client",
            post(relay_client_telemetry_post),
        )
        .route("/_fedi3/webrtc/send", post(webrtc_send))
        .route("/_fedi3/webrtc/poll", get(webrtc_poll))
        .route("/_fedi3/webrtc/ack", post(webrtc_ack))
        .route("/_fedi3/relay/move", post(relay_move_post))
        .route(
            "/_fedi3/relay/move/:user",
            axum::routing::delete(relay_move_delete),
        )
        .route("/_fedi3/relay/move_notice", post(relay_move_notice_post))
        .route("/_fedi3/backup", get(relay_backup_meta).put(relay_backup_put))
        .route("/_fedi3/backup/blob", get(relay_backup_blob))
        .route("/api/users/show", post(api_user_show).get(api_user_show_get))
        .route("/users/:user/media", post(media_upload))
        .route("/users/:user/media/:id", get(media_get))
        .route("/users/:user", any(forward_user_root))
        .route("/users/:user/*rest", any(forward_user_rest))
        .route("/*rest", any(forward_host_any))
        .layer(axum::extract::DefaultBodyLimit::max(max_body))
        .layer(
            TraceLayer::new_for_http().make_span_with(|req: &axum::http::Request<_>| {
                let request_id = req
                    .headers()
                    .get("x-request-id")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("req");
                let correlation_id = req
                    .headers()
                    .get("x-correlation-id")
                    .and_then(|v| v.to_str().ok());
                info_span!(
                    "http",
                    method = %req.method(),
                    uri = %req.uri(),
                    request_id = %request_id,
                    correlation_id = ?correlation_id
                )
            }),
        )
        .layer(from_fn_with_state(state.clone(), enforce_ip_policy))
        .layer(from_fn_with_state(state.clone(), add_security_headers))
        .layer(from_fn(ensure_request_ids))
        .with_state(state.clone());

    // Seed relays + periodic telemetry.
    if let Some(self_url) = state.cfg.public_url.clone() {
        let mut db = state.db.lock().await;
        let _ = db.upsert_relay(&self_url, state.cfg.base_domain.clone(), None, None);
        for r in &state.cfg.seed_relays {
            let _ = db.upsert_relay(r, None, None, None);
        }
    }
    let sync_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(
            sync_state.cfg.telemetry_interval_secs.max(10),
        ));
        loop {
            interval.tick().await;
            if let Err(e) = push_telemetry_once(&sync_state).await {
                error!("telemetry push failed: {e:#}");
            }
        }
    });

    info!("fedi3_relay listening on http://{addr}");
    if let Some(d) = base_domain {
        info!("host routing enabled for base domain: {d}");
    }
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

fn load_config() -> RelayConfig {
    let bind = std::env::var("FEDI3_RELAY_BIND").unwrap_or_else(|_| "0.0.0.0:8787".to_string());
    let bind: SocketAddr = bind.parse().expect("FEDI3_RELAY_BIND invalid");
    let base_domain = std::env::var("FEDI3_RELAY_BASE_DOMAIN")
        .ok()
        .map(normalize_host);
    let trust_proxy_headers = std::env::var("FEDI3_RELAY_TRUST_PROXY_HEADERS")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let allow_self_register = std::env::var("FEDI3_RELAY_ALLOW_SELF_REGISTER")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let admin_token = std::env::var("FEDI3_RELAY_ADMIN_TOKEN").ok();
    let public_url = std::env::var("FEDI3_RELAY_PUBLIC_URL")
        .ok()
        .map(|s| s.trim_end_matches('/').to_string());
    let telemetry_token = std::env::var("FEDI3_RELAY_TELEMETRY_TOKEN").ok();
    let github_token = std::env::var("FEDI3_GITHUB_TOKEN")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let github_repo = std::env::var("FEDI3_GITHUB_REPO")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let github_issue_labels = std::env::var("FEDI3_GITHUB_ISSUE_LABELS")
        .ok()
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec!["telemetry".to_string(), "auto-generated".to_string()]);
    let github_issue_assignee = std::env::var("FEDI3_GITHUB_ISSUE_ASSIGNEE")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let relay_list_repo = std::env::var("FEDI3_RELAY_LIST_REPO")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let relay_list_path = std::env::var("FEDI3_RELAY_LIST_PATH")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "relay_list.json".to_string());
    let relay_list_branch = std::env::var("FEDI3_RELAY_LIST_BRANCH")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "main".to_string());
    let relay_list_token = std::env::var("FEDI3_RELAY_LIST_TOKEN")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let relay_list_refresh_secs = std::env::var("FEDI3_RELAY_LIST_REFRESH_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(600)
        .max(30);
    let seed_relays = std::env::var("FEDI3_RELAY_SEED_RELAYS")
        .ok()
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().trim_end_matches('/').to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let p2p_infra_peer_id = std::env::var("FEDI3_P2P_INFRA_PEER_ID")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let path = std::env::var("FEDI3_P2P_INFRA_PEER_ID_FILE")
                .unwrap_or_else(|_| "/p2p_infra/fedi3_p2p_peer_id".to_string());
            match std::fs::read_to_string(&path) {
                Ok(v) => {
                    let t = v.trim().to_string();
                    if t.is_empty() {
                        None
                    } else {
                        Some(t)
                    }
                }
                Err(e) => {
                    warn!(path=%path, "read p2p_infra peer_id file failed: {e}");
                    None
                }
            }
        });
    let p2p_infra_multiaddrs = std::env::var("FEDI3_P2P_INFRA_MULTIADDRS")
        .ok()
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let p2p_infra_host = std::env::var("FEDI3_P2P_INFRA_HOST")
        .ok()
        .map(normalize_host)
        .filter(|s| !s.is_empty());
    let p2p_infra_port = std::env::var("FEDI3_P2P_INFRA_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(4001);
    let relay_mesh_enable = std::env::var("FEDI3_RELAY_MESH_ENABLE")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(true);
    let relay_mesh_listen = std::env::var("FEDI3_RELAY_MESH_LISTEN")
        .ok()
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            vec![
                "/ip4/0.0.0.0/tcp/0".to_string(),
                "/ip4/0.0.0.0/udp/0/quic-v1".to_string(),
            ]
        });
    let relay_mesh_bootstrap = std::env::var("FEDI3_RELAY_MESH_BOOTSTRAP")
        .ok()
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty())
        .unwrap_or_default();
    let relay_mesh_key_path = std::env::var("FEDI3_RELAY_MESH_KEY")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("fedi3_relay_mesh_keypair.pb"));
    let relay_mesh_bootstrap = if relay_mesh_bootstrap.is_empty() {
        p2p_infra_multiaddrs.clone()
    } else {
        relay_mesh_bootstrap
    };
    let p2p_upnp_port_start = std::env::var("FEDI3_RELAY_P2P_UPNP_PORT_START")
        .ok()
        .and_then(|v| v.parse::<u16>().ok());
    let p2p_upnp_port_end = std::env::var("FEDI3_RELAY_P2P_UPNP_PORT_END")
        .ok()
        .and_then(|v| v.parse::<u16>().ok());
    let telemetry_interval_secs = std::env::var("FEDI3_RELAY_TELEMETRY_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30);
    let max_body_bytes = std::env::var("FEDI3_RELAY_MAX_BODY_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(64 * 1024 * 1024);
    let backup_max_bytes = std::env::var("FEDI3_RELAY_BACKUP_MAX_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(200 * 1024 * 1024)
        .clamp(1 * 1024 * 1024, 2 * 1024 * 1024 * 1024)
        .min(max_body_bytes);
    let backup_retention_count = std::env::var("FEDI3_RELAY_BACKUP_RETENTION")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(3)
        .clamp(1, 10);
    let backup_rate_limit_per_hour = std::env::var("FEDI3_RELAY_BACKUP_RL_PER_HOUR")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(1)
        .clamp(1, 100);
    let hsts_max_age_secs = std::env::var("FEDI3_RELAY_HSTS_MAX_AGE_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let csp = std::env::var("FEDI3_RELAY_CSP")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let http_timeout_secs = std::env::var("FEDI3_RELAY_HTTP_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30)
        .clamp(5, 120);
    let http_connect_timeout_secs = std::env::var("FEDI3_RELAY_HTTP_CONNECT_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10)
        .clamp(1, 60);
    let http_pool_idle_timeout_secs = std::env::var("FEDI3_RELAY_HTTP_POOL_IDLE_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(90)
        .clamp(10, 600);
    let http_pool_max_idle_per_host = std::env::var("FEDI3_RELAY_HTTP_POOL_MAX_IDLE_PER_HOST")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(8)
        .clamp(1, 128);
    let tunnel_timeout_secs = std::env::var("FEDI3_RELAY_TUNNEL_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(15);
    let rate_limit_register_per_min = std::env::var("FEDI3_RELAY_RL_REGISTER_PER_MIN")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(200);
    let rate_limit_tunnel_per_min = std::env::var("FEDI3_RELAY_RL_TUNNEL_PER_MIN")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(600);
    let rate_limit_inbox_per_min = std::env::var("FEDI3_RELAY_RL_INBOX_PER_MIN")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(600);
    let rate_limit_forward_per_min = std::env::var("FEDI3_RELAY_RL_FORWARD_PER_MIN")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(1200);
    let rate_limit_admin_per_min = std::env::var("FEDI3_RELAY_RL_ADMIN_PER_MIN")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(120);
    let rate_limit_client_telemetry_per_min =
        std::env::var("FEDI3_RELAY_RL_CLIENT_TELEMETRY_PER_MIN")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(30);
    let search_backend = std::env::var("FEDI3_RELAY_SEARCH_BACKEND")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "db".to_string());
    let search_total_mode = std::env::var("FEDI3_RELAY_SEARCH_TOTAL_MODE")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .and_then(|v| match v.as_str() {
            "exact" => Some(SearchTotalMode::Exact),
            "none" => Some(SearchTotalMode::None),
            "approx" | "approximate" => Some(SearchTotalMode::Approx),
            _ => None,
        })
        .unwrap_or(SearchTotalMode::Approx);
    let search_cache_ttl_secs = std::env::var("FEDI3_RELAY_SEARCH_CACHE_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10)
        .min(300);
    let search_cache_max_entries = std::env::var("FEDI3_RELAY_SEARCH_CACHE_MAX")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(512)
        .min(10_000);
    let meili_url = std::env::var("FEDI3_RELAY_MEILI_URL")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let meili_api_key = std::env::var("FEDI3_RELAY_MEILI_API_KEY")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let meili_timeout_secs = std::env::var("FEDI3_RELAY_MEILI_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10)
        .clamp(2, 60);
    let meili_notes_index = std::env::var("FEDI3_RELAY_MEILI_NOTES_INDEX")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "relay_notes".to_string());
    let meili_users_index = std::env::var("FEDI3_RELAY_MEILI_USERS_INDEX")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "relay_users".to_string());
    let meili_batch_max = std::env::var("FEDI3_RELAY_MEILI_BATCH_MAX")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(64)
        .min(500);
    let meili_flush_ms = std::env::var("FEDI3_RELAY_MEILI_FLUSH_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(250)
        .min(5_000);
    let meili_queue_max = std::env::var("FEDI3_RELAY_MEILI_QUEUE_MAX")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(2_000)
        .min(50_000);
    let db_driver = std::env::var("FEDI3_RELAY_DB_DRIVER")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .and_then(|v| match v.as_str() {
            "postgres" | "postgresql" | "pg" => Some(DbDriver::Postgres),
            "sqlite" | "sqlite3" => Some(DbDriver::Sqlite),
            _ => None,
        })
        .unwrap_or(DbDriver::Sqlite);
    let db_url = std::env::var("FEDI3_RELAY_DB_URL")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let db_synchronous = std::env::var("FEDI3_RELAY_DB_SYNC")
        .ok()
        .map(|v| v.trim().to_ascii_uppercase())
        .filter(|v| matches!(v.as_str(), "OFF" | "NORMAL" | "FULL" | "EXTRA"))
        .unwrap_or_else(|| "NORMAL".to_string());
    let mut db_cache_kb = std::env::var("FEDI3_RELAY_DB_CACHE_KB")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(64 * 1024);
    if db_cache_kb == 0 {
        db_cache_kb = 64 * 1024;
    }
    db_cache_kb = -db_cache_kb.abs();
    let db_busy_timeout_ms = std::env::var("FEDI3_RELAY_DB_BUSY_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(2000)
        .min(60_000);
    let pg_pool_max_size = std::env::var("FEDI3_RELAY_PG_POOL_MAX_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(16)
        .max(1)
        .min(256);
    let pg_pool_wait_ms = std::env::var("FEDI3_RELAY_PG_POOL_WAIT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0);
    let pg_pool_create_timeout_ms = std::env::var("FEDI3_RELAY_PG_POOL_CREATE_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0);
    let pg_pool_recycle_timeout_ms = std::env::var("FEDI3_RELAY_PG_POOL_RECYCLE_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0);
    let pg_pool_queue_mode = std::env::var("FEDI3_RELAY_PG_POOL_QUEUE_MODE")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .and_then(|v| match v.as_str() {
            "lifo" => Some(QueueMode::Lifo),
            "fifo" => Some(QueueMode::Fifo),
            _ => None,
        })
        .unwrap_or(QueueMode::Fifo);
    let pg_init_retries = std::env::var("FEDI3_RELAY_PG_INIT_RETRIES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(30)
        .max(1)
        .min(300);
    let pg_init_backoff_ms = std::env::var("FEDI3_RELAY_PG_INIT_BACKOFF_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(500)
        .max(50)
        .min(30_000);
    let redis_url = std::env::var("FEDI3_RELAY_REDIS_URL")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let redis_prefix = std::env::var("FEDI3_RELAY_REDIS_PREFIX")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "fedi3".to_string());
    let redis_pool_size = std::env::var("FEDI3_RELAY_REDIS_POOL_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4)
        .max(1)
        .min(64);
    let ip_allowlist = parse_ip_rules(std::env::var("FEDI3_RELAY_IP_ALLOWLIST").ok());
    let ip_denylist = parse_ip_rules(std::env::var("FEDI3_RELAY_IP_DENYLIST").ok());
    let noisy_backoff_base_secs = std::env::var("FEDI3_RELAY_NOISY_BACKOFF_BASE_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
        .min(3600);
    let noisy_backoff_max_secs = std::env::var("FEDI3_RELAY_NOISY_BACKOFF_MAX_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(600)
        .max(noisy_backoff_base_secs);
    let max_inbox_fanout = std::env::var("FEDI3_RELAY_MAX_INBOX_FANOUT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(25);
    let max_inflight_per_user = std::env::var("FEDI3_RELAY_MAX_INFLIGHT_PER_USER")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(32);
    let spool_ttl_secs = std::env::var("FEDI3_RELAY_SPOOL_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30 * 24 * 60 * 60);
    let move_notice_ttl_secs = std::env::var("FEDI3_RELAY_MOVE_NOTICE_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(7 * 24 * 60 * 60);
    let move_notice_fanout_interval_secs =
        std::env::var("FEDI3_RELAY_MOVE_NOTICE_FANOUT_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60);
    let peer_directory_ttl_days = std::env::var("FEDI3_RELAY_PEER_DIRECTORY_TTL_DAYS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(30)
        .min(3650);

    let spool_max_rows_per_user = std::env::var("FEDI3_RELAY_SPOOL_MAX_ROWS_PER_USER")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(5_000);
    let spool_flush_batch = std::env::var("FEDI3_RELAY_SPOOL_FLUSH_BATCH")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(50);
    let media_backend =
        std::env::var("FEDI3_RELAY_MEDIA_BACKEND").unwrap_or_else(|_| "local".to_string());
    let media_dir =
        std::env::var("FEDI3_RELAY_MEDIA_DIR").unwrap_or_else(|_| "fedi3_relay_media".to_string());
    let media_prefix = std::env::var("FEDI3_RELAY_MEDIA_PREFIX").unwrap_or_default();
    let media_webdav_base_url = std::env::var("FEDI3_RELAY_MEDIA_WEBDAV_BASE_URL").ok();
    let media_webdav_username = std::env::var("FEDI3_RELAY_MEDIA_WEBDAV_USERNAME").ok();
    let media_webdav_password = std::env::var("FEDI3_RELAY_MEDIA_WEBDAV_PASSWORD").ok();
    let media_webdav_bearer_token = std::env::var("FEDI3_RELAY_MEDIA_WEBDAV_BEARER_TOKEN").ok();
    let media_s3_region = std::env::var("FEDI3_RELAY_MEDIA_S3_REGION").ok();
    let media_s3_bucket = std::env::var("FEDI3_RELAY_MEDIA_S3_BUCKET").ok();
    let media_s3_endpoint = std::env::var("FEDI3_RELAY_MEDIA_S3_ENDPOINT").ok();
    let media_s3_access_key = std::env::var("FEDI3_RELAY_MEDIA_S3_ACCESS_KEY").ok();
    let media_s3_secret_key = std::env::var("FEDI3_RELAY_MEDIA_S3_SECRET_KEY").ok();
    let media_s3_path_style = std::env::var("FEDI3_RELAY_MEDIA_S3_PATH_STYLE")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let outbox_index_interval_secs = std::env::var("FEDI3_RELAY_OUTBOX_INDEX_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(300);
    let outbox_index_pages = std::env::var("FEDI3_RELAY_OUTBOX_INDEX_PAGES")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(5);
    let outbox_index_page_limit = std::env::var("FEDI3_RELAY_OUTBOX_INDEX_PAGE_LIMIT")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(40);
    let telemetry_users_limit = std::env::var("FEDI3_RELAY_TELEMETRY_USERS_LIMIT")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(200);
    let telemetry_peers_limit = std::env::var("FEDI3_RELAY_TELEMETRY_PEERS_LIMIT")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(200);
    let relay_sync_interval_secs = std::env::var("FEDI3_RELAY_SYNC_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(120);
    let relay_sync_limit = std::env::var("FEDI3_RELAY_SYNC_LIMIT")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(200);
    let relay_media_ttl_secs = std::env::var("FEDI3_RELAY_MEDIA_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(14 * 24 * 60 * 60);
    let relay_actor_ttl_secs = std::env::var("FEDI3_RELAY_ACTOR_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30 * 24 * 60 * 60);
    let relay_reputation_ttl_secs = std::env::var("FEDI3_RELAY_REPUTATION_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30 * 24 * 60 * 60);
    RelayConfig {
        bind,
        base_domain,
        trust_proxy_headers,
        allow_self_register,
        admin_token,
        public_url,
        telemetry_token,
        github_token,
        github_repo,
        github_issue_labels,
        github_issue_assignee,
        relay_list_repo,
        relay_list_path,
        relay_list_branch,
        relay_list_token,
        relay_list_refresh_secs,
        seed_relays,
        p2p_infra_peer_id,
        p2p_infra_multiaddrs,
        p2p_infra_host,
        p2p_infra_port,
        relay_mesh_enable,
        relay_mesh_listen,
        relay_mesh_bootstrap,
        relay_mesh_key_path,
        p2p_upnp_port_start,
        p2p_upnp_port_end,
        telemetry_interval_secs,
        max_body_bytes,
        http_timeout_secs,
        http_connect_timeout_secs,
        http_pool_idle_timeout_secs,
        http_pool_max_idle_per_host,
        hsts_max_age_secs,
        csp,
        tunnel_timeout_secs,
        rate_limit_register_per_min,
        rate_limit_tunnel_per_min,
        rate_limit_inbox_per_min,
        rate_limit_forward_per_min,
        rate_limit_admin_per_min,
        rate_limit_client_telemetry_per_min,
        search_backend,
        search_total_mode,
        search_cache_ttl_secs,
        search_cache_max_entries,
        meili_url,
        meili_api_key,
        meili_timeout_secs,
        meili_notes_index,
        meili_users_index,
        meili_batch_max,
        meili_flush_ms,
        meili_queue_max,
        db_driver,
        db_url,
        db_synchronous,
        db_cache_kb,
        db_busy_timeout_ms,
        pg_pool_max_size,
        pg_pool_wait_ms,
        pg_pool_create_timeout_ms,
        pg_pool_recycle_timeout_ms,
        pg_pool_queue_mode,
        pg_init_retries,
        pg_init_backoff_ms,
        redis_url,
        redis_prefix,
        redis_pool_size,
        ip_allowlist,
        ip_denylist,
        noisy_backoff_base_secs,
        noisy_backoff_max_secs,
        max_inbox_fanout,
        max_inflight_per_user,
        spool_ttl_secs,
        move_notice_ttl_secs,
        move_notice_fanout_interval_secs,
        spool_max_rows_per_user,
        spool_flush_batch,
        peer_directory_ttl_days,
        media_backend,
        media_dir: PathBuf::from(media_dir),
        media_prefix,
        media_webdav_base_url,
        media_webdav_username,
        media_webdav_password,
        media_webdav_bearer_token,
        media_s3_region,
        media_s3_bucket,
        media_s3_endpoint,
        media_s3_access_key,
        media_s3_secret_key,
        media_s3_path_style,
        backup_max_bytes,
        backup_retention_count,
        backup_rate_limit_per_hour,
        outbox_index_interval_secs,
        outbox_index_pages,
        outbox_index_page_limit,
        telemetry_users_limit,
        telemetry_peers_limit,
        relay_sync_interval_secs,
        relay_sync_limit,
        relay_media_ttl_secs,
        relay_actor_ttl_secs,
        relay_reputation_ttl_secs,
    }
}

async fn get_user_semaphore(state: &AppState, user: &str) -> Arc<Semaphore> {
    if let Some(sem) = state.inflight_per_user.read().await.get(user).cloned() {
        return sem;
    }
    let mut map = state.inflight_per_user.write().await;
    map.entry(user.to_string())
        .or_insert_with(|| Arc::new(Semaphore::new(state.cfg.max_inflight_per_user)))
        .clone()
}

async fn tunnel_ws(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(user): Path<String>,
    Query(q): Query<TunnelQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| handle_tunnel(state, peer, user, q.token, socket))
}

async fn handle_tunnel(
    state: AppState,
    peer: SocketAddr,
    user: String,
    token: Option<String>,
    socket: WebSocket,
) {
    let token = match token {
        Some(t) if !t.is_empty() => t,
        _ => {
            error!(%user, "tunnel rejected: missing token");
            return;
        }
    };
    if token.len() < 16 {
        error!(%user, "tunnel rejected: token too short");
        return;
    }

    if !state
        .limiter
        .check(
            peer_ip(&peer),
            "tunnel",
            state.cfg.rate_limit_tunnel_per_min,
        )
        .await
    {
        error!(%user, "tunnel rejected: rate limited");
        return;
    }

    // Auth / registration
    let mut db = state.db.lock().await;
    match db.verify_or_register(&state.cfg, &user, &token) {
        Ok(()) => {}
        Err(e) => {
            error!(%user, "tunnel rejected: {e}");
            return;
        }
    }
    drop(db);

    info!(%user, "tunnel connected");

    let (mut ws_tx, mut ws_rx) = socket.split();
    let (tx, mut rx) = mpsc::channel::<TunnelRequest>(64);
    let tx_for_hello = tx.clone();

    state
        .tunnels
        .write()
        .await
        .insert(user.clone(), TunnelHandle { tx });

    {
        let stub_peer_id = format!("user:{user}");
        let actor_url = format!("{}/users/{}", user_base_url(&state.cfg, &user), user);
        let db = state.db.lock().await;
        let _ = db.upsert_peer_directory(&stub_peer_id, &user, &actor_url);
        emit_presence_update(&state, &user, &actor_url, true);
    }

    // Fetch peer hello (best-effort) and store it for directory/telemetry.
    let hello_state = state.clone();
    let hello_user = user.clone();
    tokio::spawn(async move {
        if let Ok(Some(hello)) = fetch_peer_hello(&hello_state, &hello_user, tx_for_hello).await {
            let actor_url = if hello.actor.trim().is_empty() {
                format!(
                    "{}/users/{}",
                    user_base_url(&hello_state.cfg, &hello_user),
                    hello_user
                )
            } else {
                hello.actor.trim().to_string()
            };
            let db = hello_state.db.lock().await;
            let _ = db.upsert_peer_directory(&format!("user:{hello_user}"), &hello.username, &actor_url);
            let stub = actor_stub_from_actor_url(
                &hello.username,
                &actor_url,
                &user_base_template(&hello_state.cfg),
            );
            let doc = MeiliUserDoc {
                id: meili_doc_id(&actor_url),
                username: hello.username.clone(),
                actor_url: actor_url.clone(),
                actor_json: Some(serde_json::to_string(&stub).unwrap_or_default()),
                updated_at_ms: now_ms(),
            };
            hello_state.meili_index_user(doc);
            hello_state
                .peer_hello
                .write()
                .await
                .insert(hello_user.clone(), hello);
            emit_presence_update(&hello_state, &hello_user, &actor_url, true);
        }
    });

    let cache_state = state.clone();
    let cache_user = user.clone();
    tokio::spawn(async move {
        let _ = ensure_user_cached(&cache_state, &cache_user).await;
        let _ = index_outbox_for_user(&cache_state, &cache_user).await;
    });

    tokio::spawn(flush_spool_for_user(state.clone(), user.clone()));

    let inflight: Arc<RwLock<HashMap<String, oneshot::Sender<RelayHttpResponse>>>> =
        Arc::new(RwLock::new(HashMap::new()));

    let inflight_writer = inflight.clone();
    let user_writer = user.clone();
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let id = msg.id.clone();
            inflight_writer
                .write()
                .await
                .insert(id.clone(), msg.resp_tx);
            let json = match serde_json::to_string(&msg.req) {
                Ok(v) => v,
                Err(e) => {
                    error!(%user_writer, "serialize request failed: {e}");
                    continue;
                }
            };
            if ws_tx.send(Message::Text(json)).await.is_err() {
                break;
            }
        }
    });

    let inflight_reader = inflight.clone();
    let user_reader = user.clone();
    let cancel = CancellationToken::new();
    let cancel_reader = cancel.clone();
    let cancel_writer = cancel.clone();
    let reader = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            let Message::Text(text) = msg else { continue };
            let resp: RelayHttpResponse = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(e) => {
                    error!(%user_reader, "deserialize response failed: {e}");
                    continue;
                }
            };
            let tx = inflight_reader.write().await.remove(&resp.id);
            if let Some(tx) = tx {
                let _ = tx.send(resp);
            }
        }
        cancel_reader.cancel();
    });

    // Stop writer when socket closes.
    let writer2 = tokio::spawn(async move {
        tokio::select! {
          _ = cancel_writer.cancelled() => {}
          _ = writer => {}
        }
    });

    let _ = tokio::join!(writer2, reader);

    state.tunnels.write().await.remove(&user);
    state.peer_hello.write().await.remove(&user);
    let actor_url = format!("{}/users/{}", user_base_url(&state.cfg, &user), user);
    emit_presence_update(&state, &user, &actor_url, false);
    info!(%user, "tunnel disconnected");
}

async fn webfinger(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(q): Query<WebfingerQuery>,
) -> impl IntoResponse {
    if let Some(resp) = maybe_redirect_canonical(
        &state.cfg,
        &headers,
        &Method::GET,
        "/.well-known/webfinger",
        None,
    ) {
        return resp.into_response();
    }
    let Some(resource) = q.resource else {
        return (StatusCode::BAD_REQUEST, "missing resource").into_response();
    };

    let acct = resource.strip_prefix("acct:").unwrap_or(&resource);
    let user = acct.split('@').next().unwrap_or("").to_string();
    if user.is_empty() {
        return (StatusCode::BAD_REQUEST, "invalid resource").into_response();
    }

    if !state
        .limiter
        .check(
            client_ip(&state.cfg, &peer, &headers),
            "forward",
            state.cfg.rate_limit_forward_per_min,
        )
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }

    let db = state.db.lock().await;
    let enabled = db.is_user_enabled(&user).unwrap_or(false);
    let moved = db.get_user_move(&user).ok().flatten().is_some();
    drop(db);
    if !enabled && !moved {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }

    let (scheme, host) = origin_for_links_with_cfg(&state.cfg, &headers);
    let actor_url = format!("{scheme}://{host}/users/{user}");
    let body = serde_json::json!({
      "subject": format!("acct:{user}@{host}"),
      "links": [{
        "rel": "self",
        "type": "application/activity+json",
        "href": actor_url
      }]
    });

    (
        StatusCode::OK,
        [("Content-Type", "application/jrd+json; charset=utf-8")],
        body.to_string(),
    )
        .into_response()
}

async fn register(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    axum::Json(req): axum::Json<RegisterRequest>,
) -> impl IntoResponse {
    if !state
        .limiter
        .check(
            client_ip(&state.cfg, &peer, &headers),
            "register",
            state.cfg.rate_limit_register_per_min,
        )
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }

    if !is_valid_username(&req.username) {
        return (StatusCode::BAD_REQUEST, "invalid username").into_response();
    }
    if req.token.len() < 16 {
        return (StatusCode::BAD_REQUEST, "token too short").into_response();
    }

    if !state.cfg.allow_self_register {
        if !is_authorized_admin(&state.cfg, &headers) {
            return (StatusCode::UNAUTHORIZED, "admin token required").into_response();
        }
    }

    let mut db = state.db.lock().await;
    let result = db.upsert_user_token(&state.cfg, &headers, &req.username, &req.token);
    drop(db);
    if matches!(
        result,
        Ok(UpsertUserResult::Created | UpsertUserResult::Updated)
    ) {
        let actor_url = format!("{}/users/{}", relay_self_base(&state.cfg), req.username);
        let stub =
            actor_stub_from_actor_url(&req.username, &actor_url, &user_base_template(&state.cfg));
        let doc = MeiliUserDoc {
            id: meili_doc_id(&actor_url),
            username: req.username.clone(),
            actor_url: actor_url.clone(),
            actor_json: Some(serde_json::to_string(&stub).unwrap_or_default()),
            updated_at_ms: now_ms(),
        };
        state.meili_index_user(doc);
    }
    match result {
        Ok(UpsertUserResult::Created) => (StatusCode::CREATED, "created").into_response(),
        Ok(UpsertUserResult::Exists) => (StatusCode::OK, "exists").into_response(),
        Ok(UpsertUserResult::Updated) => (StatusCode::OK, "updated").into_response(),
        Ok(UpsertUserResult::Unauthorized) => {
            (StatusCode::UNAUTHORIZED, "invalid token").into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response(),
    }
}

async fn media_upload(
    State(state): State<AppState>,
    Path(user): Path<String>,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    body: Bytes,
) -> Response {
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid user").into_response();
    }
    let token = match bearer_token(&headers) {
        Some(v) => v,
        None => return (StatusCode::UNAUTHORIZED, "missing token").into_response(),
    };
    let db = state.db.lock().await;
    let ok = db.verify_user_token(&user, &token).unwrap_or(false);
    let enabled = db.is_user_enabled(&user).unwrap_or(false);
    drop(db);
    if !ok || !enabled {
        return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
    }
    if !state
        .limiter
        .check_weighted(
            client_ip(&state.cfg, &peer, &headers),
            "media_upload",
            state.cfg.rate_limit_inbox_per_min,
            1,
        )
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }
    if body.is_empty() {
        return (StatusCode::BAD_REQUEST, "empty body").into_response();
    }
    let bytes = body.to_vec();
    let filename = headers
        .get("X-Filename")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("upload.bin");
    let ext = FsPath::new(filename)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("bin");
    let media_type = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");
    let id = generate_media_id(ext);
    let prefix = state.cfg.media_prefix.trim().trim_matches('/').to_string();
    let prefix = if prefix.is_empty() {
        String::new()
    } else {
        format!("{}/", prefix)
    };
    let storage_key = media_store::sanitize_key(&format!("{prefix}{user}/{id}"));
    let saved = match state
        .media_backend
        .save_upload(&storage_key, media_type, &bytes)
        .await
    {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("store failed: {e:#}")).into_response(),
    };
    let item = MediaItem {
        id: id.clone(),
        username: user.clone(),
        backend: state.media_cfg.backend.clone(),
        storage_key: saved.storage_key.clone(),
        media_type: saved.media_type.clone(),
        size: saved.size as i64,
        created_at_ms: now_ms(),
    };
    let db = state.db.lock().await;
    if db.upsert_media_item(&item).is_err() {
        return (StatusCode::BAD_GATEWAY, "db error").into_response();
    }
    let (scheme, host) = origin_for_links_with_cfg(&state.cfg, &headers);
    let url = format!("{scheme}://{host}/users/{user}/media/{id}");
    let body = serde_json::json!({
      "id": id,
      "url": url,
      "mediaType": saved.media_type,
      "size": saved.size
    });
    (
        StatusCode::CREATED,
        [(
            http::header::CONTENT_TYPE,
            "application/json; charset=utf-8",
        )],
        body.to_string(),
    )
        .into_response()
}

async fn media_get(
    State(state): State<AppState>,
    Path((user, id)): Path<(String, String)>,
) -> Response {
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid user").into_response();
    }
    if id.is_empty() || id.contains("..") || id.contains('/') || id.contains('\\') {
        return (StatusCode::BAD_REQUEST, "invalid media id").into_response();
    }
    let db = state.db.lock().await;
    let item = match db.get_media_item(&user, &id) {
        Ok(Some(v)) => v,
        Ok(None) => {
            drop(db);
            let is_online = { state.tunnels.read().await.contains_key(&user) };
            if is_online {
                let media_path = format!("/users/{user}/media/{id}");
                return forward_to_user(
                    state,
                    user.clone(),
                    Method::GET,
                    &media_path,
                    String::new(),
                    HeaderMap::new(),
                    Bytes::new(),
                )
                .await;
            }
            return (StatusCode::NOT_FOUND, "not found").into_response();
        }
        Err(_) => return (StatusCode::BAD_GATEWAY, "db error").into_response(),
    };
    drop(db);
    match state.media_backend.load(&item.storage_key).await {
        Ok(bytes) => {
            let mut headers_out = HeaderMap::new();
            headers_out.insert(
                http::header::CONTENT_TYPE,
                HeaderValue::from_str(&item.media_type)
                    .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
            );
            headers_out.insert(
                http::header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000, immutable"),
            );
            (StatusCode::OK, headers_out, bytes).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn healthz(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let audit = match admin_guard(&state, &peer, &headers, "admin_healthz", None).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let _ = state.db.lock().await.insert_admin_audit(
        "admin_healthz",
        None,
        None,
        Some(&audit.ip),
        true,
        None,
        &audit.meta,
    );
    (StatusCode::OK, "ok").into_response()
}

async fn readyz(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let audit = match admin_guard(&state, &peer, &headers, "admin_readyz", None).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let db = state.db.lock().await;
    if db.health_check().is_err() {
        let _ = db.insert_admin_audit(
            "admin_readyz",
            None,
            None,
            Some(&audit.ip),
            false,
            Some("db not ready"),
            &audit.meta,
        );
        return (StatusCode::SERVICE_UNAVAILABLE, "db not ready").into_response();
    }
    drop(db);
    if let Err(e) = state.media_backend.health_check().await {
        let _ = state.db.lock().await.insert_admin_audit(
            "admin_readyz",
            None,
            None,
            Some(&audit.ip),
            false,
            Some("media not ready"),
            &audit.meta,
        );
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            format!("media not ready: {e}"),
        )
            .into_response();
    }
    let relay_sync_window_ms: i64 = 24 * 3600 * 1000;
    let relay_sync_cutoff_ms = now_ms().saturating_sub(relay_sync_window_ms);
    let db = state.db.lock().await;
    let sync_rows = db.list_relay_sync_state().unwrap_or_default();
    let mut last_sync_ms = None;
    for (_relay, last_ms) in sync_rows {
        if last_sync_ms.map(|v| last_ms > v).unwrap_or(true) {
            last_sync_ms = Some(last_ms);
        }
    }
    if let Some(last_ms) = last_sync_ms {
        if last_ms < relay_sync_cutoff_ms {
            let _ = db.insert_admin_audit(
                "admin_readyz",
                None,
                None,
                Some(&audit.ip),
                false,
                Some("relay sync stale"),
                &audit.meta,
            );
            return (StatusCode::SERVICE_UNAVAILABLE, "relay sync stale").into_response();
        }
    }
    let _ = db.insert_admin_audit(
        "admin_readyz",
        None,
        None,
        Some(&audit.ip),
        true,
        None,
        &audit.meta,
    );
    (StatusCode::OK, "ready").into_response()
}

async fn add_security_headers(
    State(state): State<AppState>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Response {
    let req_headers = req.headers();
    let request_id = req_headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(next_request_id);
    let correlation = req_headers
        .get("x-correlation-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let mut resp = next.run(req).await;
    let headers = resp.headers_mut();
    headers.insert(
        "X-Request-Id",
        HeaderValue::from_str(&request_id).unwrap_or_else(|_| HeaderValue::from_static("req")),
    );
    if let Some(correlation) = correlation {
        headers.insert(
            "X-Correlation-Id",
            HeaderValue::from_str(&correlation)
                .unwrap_or_else(|_| HeaderValue::from_static("corr")),
        );
    }
    headers
        .entry("X-Content-Type-Options")
        .or_insert(HeaderValue::from_static("nosniff"));
    headers
        .entry("X-Frame-Options")
        .or_insert(HeaderValue::from_static("DENY"));
    headers
        .entry("Referrer-Policy")
        .or_insert(HeaderValue::from_static("no-referrer"));
    headers
        .entry("Permissions-Policy")
        .or_insert(HeaderValue::from_static(
            "geolocation=(), microphone=(), camera=()",
        ));
    if state.cfg.hsts_max_age_secs > 0 {
        let value = format!(
            "max-age={}; includeSubDomains; preload",
            state.cfg.hsts_max_age_secs
        );
        headers.insert(
            "Strict-Transport-Security",
            HeaderValue::from_str(&value).unwrap_or_else(|_| HeaderValue::from_static("max-age=0")),
        );
    }
    if let Some(csp) = &state.cfg.csp {
        headers.insert(
            "Content-Security-Policy",
            HeaderValue::from_str(csp)
                .unwrap_or_else(|_| HeaderValue::from_static("default-src 'none'")),
        );
    }
    resp
}

async fn ensure_request_ids(
    mut req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Response {
    let headers = req.headers_mut();
    if headers.get("x-request-id").is_none() {
        let request_id = next_request_id();
        headers.insert(
            "x-request-id",
            HeaderValue::from_str(&request_id).unwrap_or_else(|_| HeaderValue::from_static("req")),
        );
    }
    if headers.get("x-correlation-id").is_none() {
        if let Some(req_id) = headers.get("x-request-id").and_then(|v| v.to_str().ok()) {
            headers.insert(
                "x-correlation-id",
                HeaderValue::from_str(req_id).unwrap_or_else(|_| HeaderValue::from_static("corr")),
            );
        }
    }
    next.run(req).await
}

async fn enforce_ip_policy(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Response {
    let ip = client_ip_addr(&state.cfg, &peer, req.headers());
    if !is_ip_allowed(&state.cfg, ip) {
        return (StatusCode::FORBIDDEN, "ip blocked").into_response();
    }
    if let Some(retry_secs) = state.limiter.noisy_block_remaining(&ip.to_string()).await {
        let mut resp = (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
        resp.headers_mut().insert(
            "Retry-After",
            HeaderValue::from_str(&retry_secs.to_string())
                .unwrap_or_else(|_| HeaderValue::from_static("60")),
        );
        return resp;
    }
    next.run(req).await
}

async fn relay_metrics_json(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(q): Query<RelayTelemetryQuery>,
) -> impl IntoResponse {
    let _ = q;
    let audit = match admin_guard(&state, &peer, &headers, "admin_metrics_json", None).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let telemetry = match build_self_telemetry(&state).await {
        Ok(t) => t,
        Err(e) => {
            return (StatusCode::BAD_GATEWAY, format!("telemetry error: {e}")).into_response()
        }
    };
    let _ = state.db.lock().await.insert_admin_audit(
        "admin_metrics_json",
        None,
        None,
        Some(&audit.ip),
        true,
        None,
        &audit.meta,
    );
    axum::Json(telemetry).into_response()
}

async fn relay_metrics_prom(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let audit = match admin_guard(&state, &peer, &headers, "admin_metrics_prom", None).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let telemetry = match build_self_telemetry(&state).await {
        Ok(t) => t,
        Err(e) => {
            return (StatusCode::BAD_GATEWAY, format!("telemetry error: {e}")).into_response()
        }
    };
    let mut out = String::new();
    out.push_str("# TYPE fedi3_relay_online_users gauge\n");
    out.push_str(&format!(
        "fedi3_relay_online_users {}\n",
        telemetry.online_users
    ));
    out.push_str("# TYPE fedi3_relay_online_peers gauge\n");
    out.push_str(&format!(
        "fedi3_relay_online_peers {}\n",
        telemetry.online_peers
    ));
    out.push_str("# TYPE fedi3_relay_total_users gauge\n");
    out.push_str(&format!(
        "fedi3_relay_total_users {}\n",
        telemetry.total_users
    ));
    out.push_str("# TYPE fedi3_relay_total_peers_seen gauge\n");
    out.push_str(&format!(
        "fedi3_relay_total_peers_seen {}\n",
        telemetry.total_peers_seen
    ));
    out.push_str("# TYPE fedi3_relay_relays_total gauge\n");
    out.push_str(&format!(
        "fedi3_relay_relays_total {}\n",
        telemetry.relays.len()
    ));
    if let Some(v) = telemetry.search_indexed_users {
        out.push_str("# TYPE fedi3_relay_search_indexed_users gauge\n");
        out.push_str(&format!("fedi3_relay_search_indexed_users {v}\n"));
    }
    let resp = (
        StatusCode::OK,
        [("Content-Type", "text/plain; version=0.0.4")],
        out,
    )
        .into_response();
    let _ = state.db.lock().await.insert_admin_audit(
        "admin_metrics_prom",
        None,
        None,
        Some(&audit.ip),
        true,
        None,
        &audit.meta,
    );
    resp
}

async fn host_meta(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some(resp) = maybe_redirect_canonical(
        &state.cfg,
        &headers,
        &Method::GET,
        "/.well-known/host-meta",
        None,
    ) {
        return resp.into_response();
    }
    let (scheme, host) = origin_for_links_with_cfg(&state.cfg, &headers);
    let template = format!("{scheme}://{host}/.well-known/webfinger?resource={{uri}}");
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<XRD xmlns="http://docs.oasis-open.org/ns/xri/xrd-1.0">
  <Link rel="lrdd" type="application/xrd+xml" template="{template}"/>
</XRD>
"#
    );
    (
        StatusCode::OK,
        [("Content-Type", "application/xrd+xml; charset=utf-8")],
        body,
    )
        .into_response()
}

async fn nodeinfo_links(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some(resp) = maybe_redirect_canonical(
        &state.cfg,
        &headers,
        &Method::GET,
        "/.well-known/nodeinfo",
        None,
    ) {
        return resp.into_response();
    }
    let (scheme, host) = origin_for_links_with_cfg(&state.cfg, &headers);
    let href = format!("{scheme}://{host}/nodeinfo/2.0");
    axum::Json(NodeInfoLinks {
        links: vec![NodeInfoLink {
            rel: "http://nodeinfo.diaspora.software/ns/schema/2.0".to_string(),
            href,
        }],
    })
    .into_response()
}

async fn nodeinfo_2(State(state): State<AppState>) -> impl IntoResponse {
    let total_users = {
        let db = state.db.lock().await;
        db.count_users().unwrap_or(0)
    };

    axum::Json(NodeInfo2 {
        version: "2.0".to_string(),
        software: NodeInfoSoftware {
            name: "fedi3-relay".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        protocols: vec!["activitypub".to_string()],
        services: NodeInfoServices {
            inbound: vec![],
            outbound: vec![],
        },
        openRegistrations: state.cfg.allow_self_register,
        usage: NodeInfoUsage {
            users: NodeInfoUsers { total: total_users },
        },
        metadata: serde_json::json!({}),
    })
}

async fn forward_host_any(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    method: Method,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    Path(rest): Path<String>,
    body: Bytes,
) -> impl IntoResponse {
    if let Some(resp) = maybe_redirect_canonical(
        &state.cfg,
        &headers,
        &method,
        &format!("/{rest}"),
        raw_query.as_deref(),
    ) {
        return resp.into_response();
    }

    // Don't shadow reserved endpoints.
    if rest.starts_with("tunnel/")
        || rest == "register"
        || rest == "healthz"
        || rest == "readyz"
        || rest.starts_with(".well-known/")
    {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }

    if !state
        .limiter
        .check(
            client_ip(&state.cfg, &peer, &headers),
            "forward",
            state.cfg.rate_limit_forward_per_min,
        )
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }

    let Some(user) = user_from_host(&state.cfg, &headers) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };

    let path = format!("/{rest}");
    let query = raw_query.map(|q| format!("?{q}")).unwrap_or_default();
    forward_to_user(state, user, method, &path, query, headers, body).await
}

async fn forward_user_root(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(user): Path<String>,
    RawQuery(raw_query): RawQuery,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if let Some(resp) = maybe_redirect_canonical(
        &state.cfg,
        &headers,
        &method,
        &format!("/users/{user}"),
        raw_query.as_deref(),
    ) {
        return resp.into_response();
    }

    if !state
        .limiter
        .check(
            client_ip(&state.cfg, &peer, &headers),
            "forward",
            state.cfg.rate_limit_forward_per_min,
        )
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }
    let path = format!("/users/{user}");
    let query = raw_query.map(|q| format!("?{q}")).unwrap_or_default();
    forward_to_user(state, user, method, &path, query, headers, body).await
}

async fn forward_user_rest(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((user, rest)): Path<(String, String)>,
    RawQuery(raw_query): RawQuery,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if let Some(resp) = maybe_redirect_canonical(
        &state.cfg,
        &headers,
        &method,
        &format!("/users/{user}/{rest}"),
        raw_query.as_deref(),
    ) {
        return resp.into_response();
    }

    if !state
        .limiter
        .check(
            client_ip(&state.cfg, &peer, &headers),
            "forward",
            state.cfg.rate_limit_forward_per_min,
        )
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }
    // Allow internal UI/core endpoints to be accessed via the relay when the client
    // chooses to talk "through" the relay (e.g. UI-only device). We rewrite:
    //   /users/<user>/_fedi3/...  ->  /_fedi3/...
    // so the core's internal router matches.
    let path = if rest == "_fedi3" || rest.starts_with("_fedi3/") {
        format!("/{rest}")
    } else {
        format!("/users/{user}/{rest}")
    };
    let query = raw_query.map(|q| format!("?{q}")).unwrap_or_default();
    forward_to_user(state, user, method, &path, query, headers, body).await
}

async fn forward_to_user(
    state: AppState,
    user: String,
    method: Method,
    path: &str,
    query: String,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid user").into_response();
    }
    {
        let db = state.db.lock().await;
        if !db.user_exists(&user).unwrap_or(false) {
            return (StatusCode::NOT_FOUND, "not found").into_response();
        }
    }

    // If the user is offline, still serve cached profile/collections to improve
    // legacy compatibility (profile/key discovery + collection fetches).
    if method == Method::GET
        && (path == format!("/users/{user}") || is_cached_collection_path(&user, path))
    {
        let is_online = { state.tunnels.read().await.contains_key(&user) };
        if !is_online {
            let db = state.db.lock().await;
            if let Ok(Some((moved_to, _moved_at_ms))) = db.get_user_move(&user) {
                if path == format!("/users/{user}") {
                    if wants_activity_json(&headers) {
                        // Prefer serving a movedTo stub actor so legacy servers can pick up the migration.
                        if let Ok(Some(actor_json)) = db.get_actor_cache(&user) {
                            if let Some(patched) = patch_actor_with_moved_to(&actor_json, &moved_to)
                            {
                                return (
                                    StatusCode::OK,
                                    [("Content-Type", "application/activity+json; charset=utf-8")],
                                    patched,
                                )
                                    .into_response();
                            }
                        }
                        let stub = moved_actor_stub_json(&state.cfg, &headers, &user, &moved_to);
                        return (
                            StatusCode::OK,
                            [("Content-Type", "application/activity+json; charset=utf-8")],
                            stub,
                        )
                            .into_response();
                    }
                    return (StatusCode::PERMANENT_REDIRECT, [("Location", moved_to)], "")
                        .into_response();
                }

                // For collections, redirect to the new actor URL + same suffix.
                let suffix = path.strip_prefix(&format!("/users/{user}")).unwrap_or("");
                let location = format!("{}{}", moved_to.trim_end_matches('/'), suffix);
                return (StatusCode::PERMANENT_REDIRECT, [("Location", location)], "")
                    .into_response();
            }
            if path == format!("/users/{user}") {
                if let Ok(Some(actor_json)) = db.get_actor_cache(&user) {
                    return (
                        StatusCode::OK,
                        [("Content-Type", "application/activity+json; charset=utf-8")],
                        actor_json,
                    )
                        .into_response();
                }
            } else if let Some(kind) = collection_kind_from_path(&user, path) {
                if let Ok(Some(json)) = db.get_collection_cache(&user, kind) {
                    return (
                        StatusCode::OK,
                        [("Content-Type", "application/activity+json; charset=utf-8")],
                        json,
                    )
                        .into_response();
                }
                let stub = collection_stub_json(&user, kind, &headers);
                return (
                    StatusCode::OK,
                    [("Content-Type", "application/activity+json; charset=utf-8")],
                    stub,
                )
                    .into_response();
            }
            return (StatusCode::SERVICE_UNAVAILABLE, "user offline").into_response();
        }
    }

    let sem = get_user_semaphore(&state, &user).await;
    let Ok(_permit) = sem.try_acquire_owned() else {
        return (StatusCode::TOO_MANY_REQUESTS, "user inflight limit").into_response();
    };

    let tunnels = state.tunnels.read().await;
    let Some(tunnel) = tunnels.get(&user) else {
        return (StatusCode::SERVICE_UNAVAILABLE, "user offline").into_response();
    };

    let headers_vec = headers_to_vec(&headers);
    let id = format!("{user}-{}", REQ_ID.fetch_add(1, Ordering::Relaxed));
    let req = RelayHttpRequest {
        id: id.clone(),
        method: method.to_string(),
        path: path.to_string(),
        query,
        headers: headers_vec,
        body_b64: B64.encode(&body),
    };
    let (resp_tx, resp_rx) = oneshot::channel();
    let msg = TunnelRequest {
        id: id.clone(),
        req,
        resp_tx,
    };

    if tunnel.tx.send(msg).await.is_err() {
        return (StatusCode::BAD_GATEWAY, "tunnel send failed").into_response();
    }

    let Ok(resp) =
        tokio::time::timeout(Duration::from_secs(state.cfg.tunnel_timeout_secs), resp_rx).await
    else {
        return (StatusCode::GATEWAY_TIMEOUT, "tunnel timeout").into_response();
    };
    let Ok(resp) = resp else {
        return (StatusCode::BAD_GATEWAY, "tunnel response dropped").into_response();
    };

    if method == Method::GET && resp.status == 200 {
        if let Ok(bytes) = B64.decode(resp.body_b64.as_bytes()) {
            if let Ok(actor_json) = String::from_utf8(bytes) {
                let db = state.db.lock().await;
                if path == format!("/users/{user}") {
                    let _ = db.upsert_actor_cache(&user, &actor_json);
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                        let actor_url = v
                            .get("id")
                            .and_then(|id| id.as_str())
                            .unwrap_or("")
                            .to_string();
                        let meili_raw_id = if actor_url.is_empty() {
                            format!("user:{user}")
                        } else {
                            actor_url.clone()
                        };
                        let doc = MeiliUserDoc {
                            id: meili_doc_id(&meili_raw_id),
                            username: user.clone(),
                            actor_url,
                            actor_json: Some(actor_json.clone()),
                            updated_at_ms: now_ms(),
                        };
                        state.meili_index_user(doc);
                    }
                } else if let Some(kind) = collection_kind_from_path(&user, path) {
                    let _ = db.upsert_collection_cache(&user, kind, &actor_json);
                    if kind == "outbox" {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                            for note in extract_notes_from_value(&v) {
                                if let Some(idx) = note_to_index(&note) {
                                    let _ = db.upsert_relay_note(&idx);
                                }
                                for media in extract_media_from_note(&note) {
                                    let _ = db.upsert_relay_media(&media);
                                }
                                if let Some(actor_idx) = actor_to_index_from_note(&note) {
                                    let _ = db.upsert_relay_actor(&actor_idx);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    build_response(resp)
}

fn is_cached_collection_path(user: &str, path: &str) -> bool {
    collection_kind_from_path(user, path).is_some()
}

fn collection_kind_from_path<'a>(user: &str, path: &'a str) -> Option<&'a str> {
    if path == format!("/users/{user}/outbox") {
        return Some("outbox");
    }
    if path == format!("/users/{user}/followers") {
        return Some("followers");
    }
    if path == format!("/users/{user}/following") {
        return Some("following");
    }
    None
}

fn collection_stub_json(user: &str, kind: &str, headers: &HeaderMap) -> String {
    let host = host_only(headers);
    let scheme = scheme_from_headers(headers);
    let id = format!("{scheme}://{host}/users/{user}/{kind}");
    serde_json::json!({
      "@context": "https://www.w3.org/ns/activitystreams",
      "id": id,
      "type": "OrderedCollection",
      "totalItems": 0,
      "first": format!("{id}?page=true")
    })
    .to_string()
}

fn wants_activity_json(headers: &HeaderMap) -> bool {
    let accept = headers
        .get("Accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let accept = accept.to_ascii_lowercase();
    accept.contains("application/activity+json")
        || accept.contains("application/ld+json")
        || accept.contains("application/json")
        || accept.contains("*/*")
        || accept.is_empty()
}

fn moved_actor_stub_json(
    cfg: &RelayConfig,
    headers: &HeaderMap,
    user: &str,
    moved_to_actor: &str,
) -> String {
    let (scheme, host) = origin_for_links_with_cfg(cfg, headers);
    let id = format!("{scheme}://{host}/users/{user}");
    let inbox = format!("{scheme}://{host}/inbox");
    serde_json::json!({
      "@context": [
        "https://www.w3.org/ns/activitystreams",
        "https://w3id.org/security/v1"
      ],
      "id": id,
      "type": "Person",
      "preferredUsername": user,
      "inbox": inbox,
      "movedTo": moved_to_actor,
      "alsoKnownAs": [moved_to_actor],
    })
    .to_string()
}

fn patch_actor_with_moved_to(actor_json: &str, moved_to_actor: &str) -> Option<String> {
    let mut v: serde_json::Value = serde_json::from_str(actor_json).ok()?;
    if !v.is_object() {
        return None;
    }
    v["movedTo"] = serde_json::Value::String(moved_to_actor.to_string());
    let aka = v.get_mut("alsoKnownAs");
    match aka {
        Some(serde_json::Value::Array(arr)) => {
            if !arr.iter().any(|x| x.as_str() == Some(moved_to_actor)) {
                arr.push(serde_json::Value::String(moved_to_actor.to_string()));
            }
        }
        Some(_) => {
            v["alsoKnownAs"] = serde_json::Value::Array(vec![serde_json::Value::String(
                moved_to_actor.to_string(),
            )]);
        }
        None => {
            v["alsoKnownAs"] = serde_json::Value::Array(vec![serde_json::Value::String(
                moved_to_actor.to_string(),
            )]);
        }
    }
    serde_json::to_string(&v).ok()
}

async fn shared_inbox(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // sharedInbox POST fan-out: route to user tunnels based on recipients.
    if method != Method::POST {
        return (StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response();
    }

    let users = match extract_users_from_activity(&body) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("bad json: {e}")).into_response(),
    };
    if users.is_empty() {
        return (StatusCode::BAD_REQUEST, "no local recipients").into_response();
    }

    if users.len() > state.cfg.max_inbox_fanout {
        return (StatusCode::PAYLOAD_TOO_LARGE, "too many recipients").into_response();
    }

    let ip = client_ip(&state.cfg, &peer, &headers);
    if !state
        .limiter
        .check_weighted(
            ip,
            "inbox",
            state.cfg.rate_limit_inbox_per_min,
            users.len().max(1) as u32,
        )
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }

    let mut delivered = 0u32;
    let mut spooled = 0u32;
    let headers_vec = headers_to_vec(&headers);
    let body_b64 = B64.encode(&body);

    if let Err(e) = index_activity_bytes_for_search(&state, &body).await {
        error!("relay search index failed: {e}");
    }

    for user in users {
        let is_online = { state.tunnels.read().await.contains_key(&user) };
        if is_online {
            let resp = forward_to_user(
                state.clone(),
                user,
                Method::POST,
                "/inbox",
                String::new(),
                headers.clone(),
                body.clone(),
            )
            .await;
            if resp.status().is_success() || resp.status().as_u16() == 202 {
                delivered += 1;
            }
            continue;
        }

        let db = state.db.lock().await;
        match db.is_user_enabled(&user) {
            Ok(true) => {
                if db
                    .enqueue_spool(
                        &state.cfg,
                        &user,
                        "POST",
                        "/inbox",
                        "",
                        &headers_vec,
                        &body_b64,
                        body.len() as i64,
                    )
                    .is_ok()
                {
                    spooled += 1;
                }
            }
            Ok(false) => {}
            Err(e) => error!(%user, "db error: {e}"),
        }
    }
    if delivered == 0 && spooled == 0 {
        (StatusCode::SERVICE_UNAVAILABLE, "no recipients online").into_response()
    } else {
        (StatusCode::ACCEPTED, "accepted").into_response()
    }
}

async fn index_activity_bytes_for_search(state: &AppState, body: &Bytes) -> Result<()> {
    let v: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let notes = extract_notes_from_value(&v);
    if notes.is_empty() {
        return Ok(());
    }
    let mut meili_docs = Vec::new();
    let db = state.db.lock().await;
    for note in notes {
        if let Some(idx) = note_to_index(&note) {
            let _ = db.upsert_relay_note(&idx);
            meili_docs.push(MeiliNoteDoc {
                id: meili_doc_id(&idx.note_id),
                note_json: idx.note_json.clone(),
                content_text: idx.content_text.clone(),
                content_html: idx.content_html.clone(),
                tags: idx.tags.clone(),
                created_at_ms: idx.created_at_ms,
            });
        }
        for media in extract_media_from_note(&note) {
            let _ = db.upsert_relay_media(&media);
        }
        if let Some(actor_idx) = actor_to_index_from_note(&note) {
            let _ = db.upsert_relay_actor(&actor_idx);
        }
    }
    drop(db);
    for doc in meili_docs {
        state.meili_index_note(doc);
    }
    Ok(())
}

async fn run_outbox_index_once(state: &AppState) -> Result<()> {
    let mut offset = 0u32;
    let batch = 200u32;
    loop {
        let users = {
            let db = state.db.lock().await;
            db.list_users(batch, offset).unwrap_or_default()
        };
        if users.is_empty() {
            break;
        }
        for (user, _created_at_ms, disabled) in users {
            if disabled != 0 {
                continue;
            }
            if let Err(e) = index_outbox_for_user(state, &user).await {
                error!(%user, "outbox index error: {e:#}");
                let db = state.db.lock().await;
                let _ = db.upsert_outbox_index_state(&user, false);
            }
        }
        offset = offset.saturating_add(batch);
    }
    let db = state.db.lock().await;
    let _ = db.relay_meta_set("search_index_last_ms", &now_ms().to_string());
    Ok(())
}

async fn index_outbox_for_user(state: &AppState, user: &str) -> Result<()> {
    let mut next_url: Option<String> = Some(outbox_first_page_url(state, user));
    let mut pages = 0u32;
    while let Some(url) = next_url.take() {
        if pages >= state.cfg.outbox_index_pages.max(1) {
            break;
        }
        pages += 1;
        let Some(value) = fetch_json_url(state, &url).await else {
            break;
        };
        let mut meili_docs = Vec::new();
        let db = state.db.lock().await;
        for note in extract_notes_from_value(&value) {
            if let Some(idx) = note_to_index(&note) {
                let _ = db.upsert_relay_note(&idx);
                meili_docs.push(MeiliNoteDoc {
                    id: meili_doc_id(&idx.note_id),
                    note_json: idx.note_json.clone(),
                    content_text: idx.content_text.clone(),
                    content_html: idx.content_html.clone(),
                    tags: idx.tags.clone(),
                    created_at_ms: idx.created_at_ms,
                });
            }
            for media in extract_media_from_note(&note) {
                let _ = db.upsert_relay_media(&media);
            }
            if let Some(actor_idx) = actor_to_index_from_note(&note) {
                let _ = db.upsert_relay_actor(&actor_idx);
            }
        }
        drop(db);
        for doc in meili_docs {
            state.meili_index_note(doc);
        }
        next_url = next_url_from_collection(state, user, &value);
        if next_url.is_none() {
            break;
        }
    }
    let db = state.db.lock().await;
    let _ = db.upsert_outbox_index_state(user, true);
    Ok(())
}

async fn ensure_user_cached(state: &AppState, user: &str) -> Result<()> {
    let url = format!("{}/users/{user}", user_base_url(&state.cfg, user));
    let _ = fetch_json_url(state, &url).await;
    Ok(())
}

fn outbox_first_page_url(state: &AppState, user: &str) -> String {
    let base = user_base_url(&state.cfg, user);
    format!(
        "{base}/users/{user}/outbox?page=true&limit={}",
        state.cfg.outbox_index_page_limit.max(1)
    )
}

fn next_url_from_collection(
    state: &AppState,
    user: &str,
    value: &serde_json::Value,
) -> Option<String> {
    let next = value.get("next")?;
    let raw = if let Some(s) = next.as_str() {
        s.to_string()
    } else if let Some(obj) = next.as_object() {
        obj.get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        String::new()
    };
    if raw.trim().is_empty() {
        return None;
    }
    if raw.starts_with("http://") || raw.starts_with("https://") {
        return Some(raw);
    }
    let base = user_base_url(&state.cfg, user);
    if raw.starts_with('/') {
        return Some(format!("{base}{raw}"));
    }
    Some(format!("{base}/users/{user}/outbox?{raw}"))
}

async fn fetch_json_url(state: &AppState, url: &str) -> Option<serde_json::Value> {
    let resp = state
        .http
        .get(url)
        .header(header::ACCEPT, "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\", application/json")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<serde_json::Value>().await.ok()
}

fn relay_self_base(cfg: &RelayConfig) -> String {
    if let Some(public_url) = cfg.public_url.as_ref() {
        return public_url.trim_end_matches('/').to_string();
    }
    let ip = cfg.bind.ip();
    let host = if ip.is_unspecified() {
        "127.0.0.1".to_string()
    } else {
        ip.to_string()
    };
    format!("http://{}:{}", host, cfg.bind.port())
}

fn relay_host_name(cfg: &RelayConfig) -> Option<String> {
    let base = relay_self_base(cfg);
    let uri: Uri = base.parse().ok()?;
    uri.host().map(|h| h.to_string())
}

fn host_matches_relay(state: &AppState, host: Option<&str>) -> bool {
    let host = host.unwrap_or("").trim();
    if host.is_empty() {
        return true;
    }
    let host_norm = normalize_host(host.to_string());
    if let Some(base) = relay_host_name(&state.cfg) {
        if normalize_host(base) == host_norm {
            return true;
        }
    }
    if let Some(base_domain) = state.cfg.base_domain.as_deref() {
        if normalize_host(base_domain.to_string()) == host_norm {
            return true;
        }
    }
    false
}

fn rfc3339_from_ms(ms: i64) -> Option<String> {
    if ms <= 0 {
        return None;
    }
    let dt = Utc.timestamp_millis_opt(ms).single()?;
    Some(dt.to_rfc3339())
}

fn collection_total_items(json: &str) -> Option<u64> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    v.get("totalItems").and_then(|v| v.as_u64())
}

fn user_base_url(cfg: &RelayConfig, user: &str) -> String {
    if let Some(base_domain) = cfg.base_domain.as_ref() {
        let scheme = cfg
            .public_url
            .as_ref()
            .and_then(|u| u.parse::<http::Uri>().ok())
            .and_then(|u| u.scheme_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "https".to_string());
        return format!("{scheme}://{}.{}", user, base_domain);
    }
    relay_self_base(cfg)
}

fn user_base_template(cfg: &RelayConfig) -> String {
    if let Some(base_domain) = cfg.base_domain.as_ref() {
        let scheme = cfg
            .public_url
            .as_ref()
            .and_then(|u| u.parse::<http::Uri>().ok())
            .and_then(|u| u.scheme_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "https".to_string());
        return format!("{scheme}://{{user}}.{base_domain}");
    }
    relay_self_base(cfg)
}

async fn flush_spool_for_user(state: AppState, user: String) {
    if !is_valid_username(&user) {
        return;
    }
    let batch = state.cfg.spool_flush_batch.max(1).min(500);
    loop {
        if !state.tunnels.read().await.contains_key(&user) {
            break;
        }

        let items = {
            let db = state.db.lock().await;
            match db.list_spool(&user, batch) {
                Ok(v) => v,
                Err(e) => {
                    error!(%user, "spool list failed: {e}");
                    break;
                }
            }
        };

        if items.is_empty() {
            break;
        }

        let mut delivered_ids: Vec<i64> = Vec::new();
        for item in &items {
            let headers_vec: Vec<(String, String)> =
                serde_json::from_str(&item.headers_json).unwrap_or_default();
            let headers = vec_to_headers(&headers_vec);
            let body_bytes = B64.decode(item.body_b64.as_bytes()).unwrap_or_default();
            let method = item.method.parse::<Method>().unwrap_or(Method::POST);

            let resp = forward_to_user(
                state.clone(),
                user.clone(),
                method,
                &item.path,
                item.query.clone(),
                headers,
                Bytes::from(body_bytes),
            )
            .await;

            if resp.status().is_success() || resp.status().as_u16() == 202 {
                delivered_ids.push(item.id);
                continue;
            }
            if resp.status() == StatusCode::SERVICE_UNAVAILABLE {
                // User went offline mid-flush.
                break;
            }
            // Any other error: keep remaining items for later to avoid a hot loop.
            break;
        }

        if !delivered_ids.is_empty() {
            let db = state.db.lock().await;
            if let Err(e) = db.delete_spool_ids(&delivered_ids) {
                error!(%user, "spool delete failed: {e}");
                break;
            }
        }

        // If we couldn't deliver a full batch, stop.
        if delivered_ids.len() < items.len() {
            break;
        }
    }
}

fn extract_users_from_activity(body: &Bytes) -> anyhow::Result<Vec<String>> {
    let v: serde_json::Value = serde_json::from_slice(body)?;
    let mut out: Vec<String> = Vec::new();
    collect_users(&v, "to", &mut out);
    collect_users(&v, "cc", &mut out);
    collect_users(&v, "bcc", &mut out);
    collect_users(&v, "audience", &mut out);
    out.sort();
    out.dedup();
    Ok(out)
}

fn collect_users(v: &serde_json::Value, field: &str, out: &mut Vec<String>) {
    let Some(val) = v.get(field) else { return };
    match val {
        serde_json::Value::String(s) => out.extend(extract_user_from_string(s)),
        serde_json::Value::Array(arr) => {
            for item in arr {
                if let serde_json::Value::String(s) = item {
                    out.extend(extract_user_from_string(s));
                }
            }
        }
        _ => {}
    }
}

fn extract_user_from_string(s: &str) -> Vec<String> {
    // Match ".../users/<user>" or ".../users/<user>/inbox"
    let mut out = Vec::new();
    let needle = "/users/";
    let mut start = 0usize;
    while let Some(idx) = s[start..].find(needle) {
        let ustart = start + idx + needle.len();
        let rest = &s[ustart..];
        let uname: String = rest
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_' || *c == '-')
            .collect();
        if !uname.is_empty() && is_valid_username(&uname) {
            out.push(uname);
        }
        start = ustart;
    }
    out
}

fn extract_actor_ids_from_json(actor_json: &str) -> (Option<String>, Option<String>) {
    let v: serde_json::Value = match serde_json::from_str(actor_json) {
        Ok(v) => v,
        Err(_) => return (None, None),
    };
    let actor_id = v
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string());
    let actor_url = extract_actor_url_from_value(&v).or_else(|| actor_id.clone());
    (actor_id, actor_url)
}

fn extract_actor_url_from_value(v: &serde_json::Value) -> Option<String> {
    let url_val = v.get("url")?;
    if let Some(s) = url_val.as_str() {
        let s = s.trim();
        return if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        };
    }
    if let Some(obj) = url_val.as_object() {
        if let Some(s) = obj.get("href").and_then(|v| v.as_str()) {
            let s = s.trim();
            return if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            };
        }
    }
    if let Some(arr) = url_val.as_array() {
        for item in arr {
            if let Some(s) = item.as_str() {
                let s = s.trim();
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
            if let Some(obj) = item.as_object() {
                if let Some(s) = obj.get("href").and_then(|v| v.as_str()) {
                    let s = s.trim();
                    if !s.is_empty() {
                        return Some(s.to_string());
                    }
                }
            }
        }
    }
    None
}

fn audit_meta_from_headers(headers: &HeaderMap) -> AuditMeta {
    AuditMeta {
        request_id: headers
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        correlation_id: headers
            .get("x-correlation-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        user_agent: headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
    }
}

fn build_response(resp: RelayHttpResponse) -> Response {
    let status = StatusCode::from_u16(resp.status).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut headers = HeaderMap::new();
    for (k, v) in resp.headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(&v),
        ) {
            headers.append(name, value);
        }
    }
    let body = match B64.decode(resp.body_b64.as_bytes()) {
        Ok(b) => b,
        Err(_) => Vec::new(),
    };
    (status, headers, body).into_response()
}

fn headers_to_vec(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|vs| (k.to_string(), vs.to_string())))
        .collect()
}

fn vec_to_headers(v: &[(String, String)]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (k, val) in v {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(val),
        ) {
            headers.append(name, value);
        }
    }
    headers
}

// (intentionally left out; was used for forward-webfinger)

fn is_authorized_admin(cfg: &RelayConfig, headers: &HeaderMap) -> bool {
    let Some(expected) = &cfg.admin_token else {
        return false;
    };
    let auth = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let Some(token) = auth.strip_prefix("Bearer ") else {
        return false;
    };
    token == expected
}

fn is_valid_username(user: &str) -> bool {
    if user.is_empty() || user.len() > 64 {
        return false;
    }
    user.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn normalize_host(host: String) -> String {
    host.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn simple(status: StatusCode, msg: &str) -> Response<Body> {
    let mut resp = Response::new(Body::from(msg.to_string()));
    *resp.status_mut() = status;
    resp
}

fn relay_host_for_request(cfg: &RelayConfig, headers: &HeaderMap) -> String {
    if let Some(base) = cfg.base_domain.as_ref() {
        return base.clone();
    }
    if let Some(url) = cfg.public_url.as_ref() {
        if let Ok(parsed) = url.parse::<http::Uri>() {
            if let Some(host) = parsed.host() {
                return host.to_string();
            }
        }
    }
    headers
        .get("Host")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(':').next().unwrap_or(v).to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn short_text(mut text: String, max_len: usize) -> String {
    if text.len() <= max_len {
        return text;
    }
    text.truncate(max_len.saturating_sub(3));
    text.push_str("...");
    text
}

fn redact_secrets(text: &str) -> String {
    let mut out = text.replace("Bearer ", "Bearer <redacted>");
    for key in ["token=", "secret=", "password=", "apikey=", "api_key="] {
        loop {
            let Some(pos) = out.to_lowercase().find(key) else { break };
            let start = pos + key.len();
            let end = out[start..]
                .find(|c: char| c.is_whitespace())
                .map(|o| start + o)
                .unwrap_or(out.len());
            out.replace_range(start..end, "<redacted>");
        }
    }
    out
}

fn scrub_tokens(text: &str) -> String {
    text.split_whitespace()
        .map(|t| {
            if t.contains("http://") || t.contains("https://") || t.contains("file://") {
                "<url>"
            } else if t.contains("\\") || t.contains("/home/") || t.contains("/Users/") {
                "<path>"
            } else {
                t
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn sanitize_message(text: &str) -> String {
    let text = redact_secrets(text);
    let text = scrub_tokens(&text);
    short_text(text.trim().to_string(), 500)
}

fn sanitize_stack(stack: &str) -> String {
    let mut lines = Vec::new();
    for raw in stack.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.contains("package:") || line.contains("dart:") {
            lines.push(line.to_string());
        } else if let Some(idx) = line.find('(') {
            lines.push(line[..idx].trim().to_string());
        } else {
            lines.push(line.to_string());
        }
        if lines.len() >= 12 {
            break;
        }
    }
    if lines.is_empty() {
        return String::new();
    }
    lines.join("\n")
}

fn classify_telemetry_level(event_type: &str, message: &str) -> &'static str {
    let t = event_type.to_ascii_lowercase();
    let m = message.to_ascii_lowercase();
    if t.contains("crash") || t.contains("panic") || m.contains("panic") {
        return "crash";
    }
    if t.contains("warn") || m.contains("warn") {
        return "warn";
    }
    if t.contains("error")
        || t.contains("exception")
        || m.contains("error")
        || m.contains("exception")
    {
        return "error";
    }
    "telemetry"
}

async fn dedupe_telemetry(state: &AppState, fingerprint: &str, window_secs: i64) -> bool {
    let mut map = state.telemetry_dedupe.lock().await;
    let now = now_ms();
    map.retain(|_, ts| now.saturating_sub(*ts) <= window_secs * 1000);
    if map.contains_key(fingerprint) {
        return true;
    }
    map.insert(fingerprint.to_string(), now);
    false
}

async fn require_user_or_admin(
    state: &AppState,
    headers: &HeaderMap,
    username: &str,
) -> Result<(), Response<Body>> {
    let Some(tok) = bearer_token(headers) else {
        return Err(simple(StatusCode::UNAUTHORIZED, "missing bearer token"));
    };
    let db = state.db.lock().await;
    let authorized = if is_authorized_admin(&state.cfg, headers) {
        true
    } else {
        db.verify_token(username, &tok).unwrap_or(false)
    };
    drop(db);
    if !authorized {
        return Err(simple(
            StatusCode::UNAUTHORIZED,
            "admin or user token required",
        ));
    }
    Ok(())
}

fn user_from_host(cfg: &RelayConfig, headers: &HeaderMap) -> Option<String> {
    let base = cfg.base_domain.as_ref()?;
    let host = headers.get("Host")?.to_str().ok()?;
    let host = normalize_host(host.split(':').next().unwrap_or(host).to_string());

    // Expect: <user>.<base_domain>
    let suffix = format!(".{base}");
    if !host.ends_with(&suffix) {
        return None;
    }
    let prefix = host.strip_suffix(&suffix)?;
    if prefix.is_empty() || prefix.contains('.') {
        return None;
    }
    let user = prefix.to_string();
    if is_valid_username(&user) {
        Some(user)
    } else {
        None
    }
}

impl Db {
    fn open_sqlite_conn(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path)?;
        self.apply_pragmas(&conn)?;
        Ok(conn)
    }

    fn open_pg_conn(&self) -> Result<PgConn> {
        let pool = self
            .pg_pool
            .get()
            .ok_or_else(|| anyhow::anyhow!("postgres pool not initialized"))?;
        let client = block_on_result(pool.get())?;
        Ok(PgConn { client })
    }

    fn apply_pragmas(&self, conn: &Connection) -> rusqlite::Result<()> {
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", self.db_synchronous.as_str());
        let _ = conn.pragma_update(None, "temp_store", "MEMORY");
        let _ = conn.pragma_update(None, "cache_size", &self.db_cache_kb);
        let _ = conn.busy_timeout(Duration::from_millis(self.db_busy_timeout_ms));
        Ok(())
    }

    fn init(&self) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute_batch(
                    r#"
            PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS users (
              username TEXT PRIMARY KEY,
              token_sha256 TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL,
              disabled INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_users_username_lower ON users(lower(username));
            CREATE TABLE IF NOT EXISTS user_cache (
              username TEXT PRIMARY KEY,
              actor_json TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              actor_id TEXT NULL,
              actor_url TEXT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_user_cache_updated ON user_cache(updated_at_ms DESC);
            CREATE INDEX IF NOT EXISTS idx_user_cache_username_lower ON user_cache(lower(username));
            CREATE INDEX IF NOT EXISTS idx_user_cache_actor_id_lower ON user_cache(lower(actor_id));
            CREATE INDEX IF NOT EXISTS idx_user_cache_actor_url_lower ON user_cache(lower(actor_url));
            CREATE TABLE IF NOT EXISTS user_collection_cache (
              username TEXT NOT NULL,
              kind TEXT NOT NULL,
              json TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              PRIMARY KEY(username, kind)
            );
            CREATE TABLE IF NOT EXISTS inbox_spool (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              username TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL,
              method TEXT NOT NULL,
              path TEXT NOT NULL,
              query TEXT NOT NULL,
              headers_json TEXT NOT NULL,
              body_b64 TEXT NOT NULL,
              body_len INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS inbox_spool_user_created ON inbox_spool(username, created_at_ms);

            CREATE TABLE IF NOT EXISTS relay_registry (
              relay_url TEXT PRIMARY KEY,
              base_domain TEXT NULL,
              last_seen_ms INTEGER NOT NULL,
              last_telemetry_json TEXT NULL,
              sign_pubkey_b64 TEXT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_relay_registry_seen ON relay_registry(last_seen_ms DESC);

            CREATE TABLE IF NOT EXISTS relay_meta (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS peer_registry (
              peer_id TEXT PRIMARY KEY,
              last_seen_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS peer_directory (
              peer_id TEXT PRIMARY KEY,
              username TEXT NOT NULL,
              actor_url TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_peer_directory_user ON peer_directory(username);
            CREATE INDEX IF NOT EXISTS idx_peer_directory_actor ON peer_directory(actor_url);
            CREATE INDEX IF NOT EXISTS idx_peer_directory_user_lower ON peer_directory(lower(username));
            CREATE INDEX IF NOT EXISTS idx_peer_directory_actor_lower ON peer_directory(lower(actor_url));
            CREATE INDEX IF NOT EXISTS idx_peer_directory_updated ON peer_directory(updated_at_ms DESC);

            CREATE TABLE IF NOT EXISTS relay_user_directory (
              actor_url TEXT PRIMARY KEY,
              username TEXT NOT NULL,
              relay_url TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_relay_user_dir_name ON relay_user_directory(username);
            CREATE INDEX IF NOT EXISTS idx_relay_user_dir_relay ON relay_user_directory(relay_url);
            CREATE INDEX IF NOT EXISTS idx_relay_user_dir_name_lower ON relay_user_directory(lower(username));
            CREATE INDEX IF NOT EXISTS idx_relay_user_dir_actor_lower ON relay_user_directory(lower(actor_url));
            CREATE INDEX IF NOT EXISTS idx_relay_user_dir_updated ON relay_user_directory(updated_at_ms DESC);

            CREATE TABLE IF NOT EXISTS user_moves (
              username TEXT PRIMARY KEY,
              moved_to_actor TEXT NOT NULL,
              moved_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS move_notices (
              notice_id TEXT PRIMARY KEY,
              notice_json TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_move_notices_created ON move_notices(created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS move_notice_fanout (
              notice_id TEXT NOT NULL,
              relay_url TEXT NOT NULL,
              tries INTEGER NOT NULL,
              last_try_ms INTEGER NOT NULL,
              sent_ok INTEGER NOT NULL,
              PRIMARY KEY(notice_id, relay_url)
            );

            CREATE TABLE IF NOT EXISTS media_items (
              id TEXT PRIMARY KEY,
              username TEXT NOT NULL,
              backend TEXT NOT NULL,
              storage_key TEXT NOT NULL,
              media_type TEXT NOT NULL,
              size INTEGER NOT NULL,
              created_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_media_user_created ON media_items(username, created_at_ms DESC);
            CREATE TABLE IF NOT EXISTS user_backups (
              username TEXT PRIMARY KEY,
              storage_key TEXT NOT NULL,
              content_type TEXT NOT NULL,
              size_bytes INTEGER NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              meta_json TEXT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_user_backups_updated ON user_backups(updated_at_ms DESC);
            CREATE TABLE IF NOT EXISTS user_backups_history (
              storage_key TEXT PRIMARY KEY,
              username TEXT NOT NULL,
              content_type TEXT NOT NULL,
              size_bytes INTEGER NOT NULL,
              created_at_ms INTEGER NOT NULL,
              meta_json TEXT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_user_backups_hist_user_created ON user_backups_history(username, created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS relay_notes (
              note_id TEXT PRIMARY KEY,
              actor_id TEXT NULL,
              published_ms INTEGER NULL,
              content_text TEXT NOT NULL,
              content_html TEXT NOT NULL,
              note_json TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_relay_notes_created ON relay_notes(created_at_ms DESC);
            CREATE INDEX IF NOT EXISTS idx_relay_notes_actor ON relay_notes(actor_id);
            CREATE INDEX IF NOT EXISTS idx_relay_notes_published ON relay_notes(published_ms DESC);

            CREATE TABLE IF NOT EXISTS relay_note_tags (
              note_id TEXT NOT NULL,
              tag TEXT NOT NULL,
              PRIMARY KEY(note_id, tag)
            );
            CREATE INDEX IF NOT EXISTS idx_relay_note_tags_tag ON relay_note_tags(tag);
            CREATE INDEX IF NOT EXISTS idx_relay_note_tags_tag_lower ON relay_note_tags(lower(tag));

            CREATE TABLE IF NOT EXISTS relay_tag_counts (
              tag TEXT PRIMARY KEY,
              count INTEGER NOT NULL
            );
            INSERT OR IGNORE INTO relay_tag_counts(tag, count)
              SELECT tag, COUNT(*) FROM relay_note_tags GROUP BY tag;
            CREATE TRIGGER IF NOT EXISTS relay_tag_counts_insert
              AFTER INSERT ON relay_note_tags
            BEGIN
              INSERT INTO relay_tag_counts(tag, count) VALUES (NEW.tag, 1)
              ON CONFLICT(tag) DO UPDATE SET count = count + 1;
            END;
            CREATE TRIGGER IF NOT EXISTS relay_tag_counts_delete
              AFTER DELETE ON relay_note_tags
            BEGIN
              UPDATE relay_tag_counts SET count = count - 1 WHERE tag = OLD.tag;
              DELETE FROM relay_tag_counts WHERE tag = OLD.tag AND count <= 0;
            END;

            CREATE TABLE IF NOT EXISTS relay_notes_count (
              id INTEGER PRIMARY KEY CHECK(id = 1),
              count INTEGER NOT NULL
            );
            INSERT OR IGNORE INTO relay_notes_count(id, count)
              VALUES (1, (SELECT COUNT(*) FROM relay_notes));
            CREATE TRIGGER IF NOT EXISTS relay_notes_count_insert
              AFTER INSERT ON relay_notes
            BEGIN
              UPDATE relay_notes_count SET count = count + 1 WHERE id = 1;
            END;
            CREATE TRIGGER IF NOT EXISTS relay_notes_count_delete
              AFTER DELETE ON relay_notes
            BEGIN
              UPDATE relay_notes_count SET count = count - 1 WHERE id = 1;
            END;

            CREATE TABLE IF NOT EXISTS relay_media (
              media_url TEXT PRIMARY KEY,
              media_type TEXT NULL,
              name TEXT NULL,
              width INTEGER NULL,
              height INTEGER NULL,
              blurhash TEXT NULL,
              created_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_relay_media_created ON relay_media(created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS relay_actors (
              actor_url TEXT PRIMARY KEY,
              username TEXT NULL,
              actor_json TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_relay_actors_updated ON relay_actors(updated_at_ms DESC);
            CREATE INDEX IF NOT EXISTS idx_relay_actors_username_lower ON relay_actors(lower(username));
            CREATE INDEX IF NOT EXISTS idx_relay_actors_url_lower ON relay_actors(lower(actor_url));

            CREATE TABLE IF NOT EXISTS relay_reputation (
              relay_url TEXT PRIMARY KEY,
              score INTEGER NOT NULL,
              updated_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_relay_reputation_updated ON relay_reputation(updated_at_ms DESC);

            CREATE TABLE IF NOT EXISTS relay_outbox_index (
              username TEXT PRIMARY KEY,
              last_index_ms INTEGER NOT NULL,
              last_ok INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS admin_audit (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              action TEXT NOT NULL,
              username TEXT NULL,
              actor TEXT NULL,
              ip TEXT NULL,
              ok INTEGER NOT NULL,
              detail TEXT NULL,
              created_at_ms INTEGER NOT NULL,
              request_id TEXT NULL,
              correlation_id TEXT NULL,
              user_agent TEXT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_admin_audit_created ON admin_audit(created_at_ms DESC);
            "#,
                )?;
                // Migrate existing dbs.
                let _ = conn.execute(
                    "ALTER TABLE users ADD COLUMN disabled INTEGER NOT NULL DEFAULT 0",
                    [],
                );
                let _ = conn.execute("ALTER TABLE user_cache ADD COLUMN actor_id TEXT NULL", []);
                let _ = conn.execute("ALTER TABLE user_cache ADD COLUMN actor_url TEXT NULL", []);
                let _ = conn.execute(
                    "ALTER TABLE relay_registry ADD COLUMN sign_pubkey_b64 TEXT NULL",
                    [],
                );
                let _ = conn.execute(
                    "ALTER TABLE admin_audit ADD COLUMN request_id TEXT NULL",
                    [],
                );
                let _ = conn.execute(
                    "ALTER TABLE admin_audit ADD COLUMN correlation_id TEXT NULL",
                    [],
                );
                let _ = conn.execute(
                    "ALTER TABLE admin_audit ADD COLUMN user_agent TEXT NULL",
                    [],
                );
                Ok(())
            }
            DbDriver::Postgres => {
                let url = self.db_url.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("FEDI3_RELAY_DB_URL is required for postgres")
                })?;
                let mut cfg = deadpool_postgres::Config::new();
                cfg.url = Some(url.to_string());
                cfg.manager = Some(ManagerConfig {
                    recycling_method: RecyclingMethod::Fast,
                });
                let mut pool_cfg = PoolConfig::new(self.pg_pool_max_size);
                pool_cfg.queue_mode = self.pg_pool_queue_mode;
                pool_cfg.timeouts = Timeouts {
                    wait: self.pg_pool_wait_ms.map(Duration::from_millis),
                    create: self.pg_pool_create_timeout_ms.map(Duration::from_millis),
                    recycle: self.pg_pool_recycle_timeout_ms.map(Duration::from_millis),
                };
                cfg.pool = Some(pool_cfg);
                let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;
                let _ = self.pg_pool.set(pool);
                let max_retries = self.pg_init_retries;
                let mut last_err: Option<anyhow::Error> = None;
                for attempt in 1..=max_retries {
                    match self.open_pg_conn() {
                        Ok(mut conn) => {
                            conn.batch_execute(include_str!("../sql/postgres_schema.sql"))?;
                            return Ok(());
                        }
                        Err(err) => {
                            last_err = Some(err);
                            let backoff_ms = (attempt as u64 * self.pg_init_backoff_ms).min(30_000);
                            warn!(
                                "postgres not ready (attempt {attempt}/{max_retries}); retrying in {backoff_ms}ms"
                            );
                            std::thread::sleep(Duration::from_millis(backoff_ms));
                        }
                    }
                }
                Err(anyhow::anyhow!(
                    "db init: Error occurred while creating a new object: {:#}",
                    last_err.unwrap()
                ))
            }
        }
    }

    fn health_check(&self) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row("SELECT 1", [], |_| Ok(()))?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_one("SELECT 1", &[])?;
                let _: i32 = row.get(0);
                Ok(())
            }
        }
    }

    fn insert_admin_audit(
        &self,
        action: &str,
        username: Option<&str>,
        actor: Option<&str>,
        ip: Option<&str>,
        ok: bool,
        detail: Option<&str>,
        meta: &AuditMeta,
    ) -> Result<()> {
        let ts = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO admin_audit(action, username, actor, ip, ok, detail, created_at_ms, request_id, correlation_id, user_agent) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        action,
                        username,
                        actor,
                        ip,
                        if ok { 1 } else { 0 },
                        detail,
                        ts,
                        meta.request_id,
                        meta.correlation_id,
                        meta.user_agent
                    ],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO admin_audit(action, username, actor, ip, ok, detail, created_at_ms, request_id, correlation_id, user_agent) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                    &[
                        &action,
                        &username,
                        &actor,
                        &ip,
                        &ok,
                        &detail,
                        &ts,
                        &meta.request_id,
                        &meta.correlation_id,
                        &meta.user_agent,
                    ],
                )?;
                Ok(())
            }
        }
    }

    fn list_admin_audit(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<
        Vec<(
            i64,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            bool,
            Option<String>,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
        )>,
    > {
        let limit = limit.min(500).max(1) as i64;
        let offset = offset as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT id, action, username, actor, ip, ok, detail, created_at_ms, request_id, correlation_id, user_agent FROM admin_audit ORDER BY created_at_ms DESC LIMIT ?1 OFFSET ?2",
                )?;
                let mut rows = stmt.query(params![limit, offset])?;
                let mut out = Vec::new();
                while let Some(r) = rows.next()? {
                    let ok_int: i64 = r.get(5)?;
                    out.push((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        ok_int != 0,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                        r.get(9)?,
                        r.get(10)?,
                    ));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT id, action, username, actor, ip, ok, detail, created_at_ms, request_id, correlation_id, user_agent FROM admin_audit ORDER BY created_at_ms DESC LIMIT $1 OFFSET $2",
                    &[&limit, &offset],
                )?;
                let mut out = Vec::new();
                for r in rows {
                    out.push((
                        r.get(0),
                        r.get(1),
                        r.get(2),
                        r.get(3),
                        r.get(4),
                        r.get(5),
                        r.get(6),
                        r.get(7),
                        r.get(8),
                        r.get(9),
                        r.get(10),
                    ));
                }
                Ok(out)
            }
        }
    }

    fn upsert_relay(
        &mut self,
        relay_url: &str,
        base_domain: Option<String>,
        telemetry_json: Option<String>,
        sign_pubkey_b64: Option<String>,
    ) -> Result<()> {
        let relay_url = relay_url.trim_end_matches('/');
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    r#"
            INSERT INTO relay_registry(relay_url, base_domain, last_seen_ms, last_telemetry_json, sign_pubkey_b64)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(relay_url) DO UPDATE SET
              base_domain=COALESCE(excluded.base_domain, relay_registry.base_domain),
              last_seen_ms=excluded.last_seen_ms,
              last_telemetry_json=COALESCE(excluded.last_telemetry_json, relay_registry.last_telemetry_json),
              sign_pubkey_b64=COALESCE(relay_registry.sign_pubkey_b64, excluded.sign_pubkey_b64)
            "#,
                    params![relay_url, base_domain, now, telemetry_json, sign_pubkey_b64],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    r#"
            INSERT INTO relay_registry(relay_url, base_domain, last_seen_ms, last_telemetry_json, sign_pubkey_b64)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT(relay_url) DO UPDATE SET
              base_domain=COALESCE(EXCLUDED.base_domain, relay_registry.base_domain),
              last_seen_ms=EXCLUDED.last_seen_ms,
              last_telemetry_json=COALESCE(EXCLUDED.last_telemetry_json, relay_registry.last_telemetry_json),
              sign_pubkey_b64=COALESCE(relay_registry.sign_pubkey_b64, EXCLUDED.sign_pubkey_b64)
            "#,
                    &[&relay_url, &base_domain, &now, &telemetry_json, &sign_pubkey_b64],
                )?;
                Ok(())
            }
        }
    }

    fn list_relays(
        &self,
        limit: u32,
    ) -> Result<Vec<(String, Option<String>, i64, Option<String>, Option<String>)>> {
        let limit = limit.min(500) as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT relay_url, base_domain, last_seen_ms, last_telemetry_json, sign_pubkey_b64 FROM relay_registry ORDER BY last_seen_ms DESC LIMIT ?1",
                )?;
                let mut rows = stmt.query(params![limit])?;
                let mut out = Vec::new();
                while let Some(r) = rows.next()? {
                    out.push((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT relay_url, base_domain, last_seen_ms, last_telemetry_json, sign_pubkey_b64 FROM relay_registry ORDER BY last_seen_ms DESC LIMIT $1",
                    &[&limit],
                )?;
                let mut out = Vec::new();
                for r in rows {
                    out.push((r.get(0), r.get(1), r.get(2), r.get(3), r.get(4)));
                }
                Ok(out)
            }
        }
    }

    fn upsert_relay_reputation(&self, relay_url: &str, score: i32, updated_at_ms: i64) -> Result<()> {
        let relay_url = relay_url.trim_end_matches('/');
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO relay_reputation(relay_url, score, updated_at_ms) VALUES (?1, ?2, ?3)\n             ON CONFLICT(relay_url) DO UPDATE SET score=excluded.score, updated_at_ms=excluded.updated_at_ms",
                    params![relay_url, score, updated_at_ms],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO relay_reputation(relay_url, score, updated_at_ms) VALUES ($1, $2, $3)\n             ON CONFLICT(relay_url) DO UPDATE SET score=EXCLUDED.score, updated_at_ms=EXCLUDED.updated_at_ms",
                    &[&relay_url, &score, &updated_at_ms],
                )?;
                Ok(())
            }
        }
    }

    fn list_relay_reputation(&self) -> Result<Vec<(String, i32, i64)>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT relay_url, score, updated_at_ms FROM relay_reputation ORDER BY updated_at_ms DESC",
                )?;
                let mut rows = stmt.query([])?;
                let mut out = Vec::new();
                while let Some(r) = rows.next()? {
                    out.push((r.get(0)?, r.get(1)?, r.get(2)?));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT relay_url, score, updated_at_ms FROM relay_reputation ORDER BY updated_at_ms DESC",
                    &[],
                )?;
                Ok(rows.into_iter().map(|r| (r.get(0), r.get(1), r.get(2))).collect())
            }
        }
    }

    fn cleanup_relay_reputation(&self, ttl_secs: u64) -> Result<u64> {
        if ttl_secs == 0 {
            return Ok(0);
        }
        let cutoff = now_ms() - (ttl_secs as i64 * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM relay_reputation WHERE updated_at_ms < ?1",
                    params![cutoff],
                )?;
                Ok(deleted as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM relay_reputation WHERE updated_at_ms < $1",
                    &[&cutoff],
                )?;
                Ok(deleted as u64)
            }
        }
    }

    fn get_relay_pubkey_b64(&self, relay_url: &str) -> Result<Option<String>> {
        let relay_url = relay_url.trim_end_matches('/');
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT sign_pubkey_b64 FROM relay_registry WHERE relay_url=?1",
                    params![relay_url],
                    |r| r.get(0),
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT sign_pubkey_b64 FROM relay_registry WHERE relay_url=$1",
                    &[&relay_url],
                )?;
                Ok(row.map(|r| r.get(0)))
            }
        }
    }

    fn relay_meta_get(&self, key: &str) -> Result<Option<String>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT value FROM relay_meta WHERE key=?1",
                    params![key],
                    |r| r.get(0),
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt("SELECT value FROM relay_meta WHERE key=$1", &[&key])?;
                Ok(row.map(|r| r.get(0)))
            }
        }
    }

    fn relay_meta_set(&self, key: &str, value: &str) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT OR REPLACE INTO relay_meta(key,value) VALUES (?1,?2)",
                    params![key, value],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO relay_meta(key, value) VALUES ($1, $2) ON CONFLICT(key) DO UPDATE SET value=EXCLUDED.value",
                    &[&key, &value],
                )?;
                Ok(())
            }
        }
    }

    fn list_relay_sync_state(&self) -> Result<Vec<(String, i64)>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT key, value FROM relay_meta WHERE key LIKE 'relay_sync_last_ms:%'",
                )?;
                let mut rows = stmt.query([])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    let key: String = row.get(0)?;
                    let value: String = row.get(1)?;
                    let relay_url = key.trim_start_matches("relay_sync_last_ms:").to_string();
                    let last_ms = value.parse::<i64>().unwrap_or(0);
                    out.push((relay_url, last_ms));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT key, value FROM relay_meta WHERE key LIKE 'relay_sync_last_ms:%'",
                    &[],
                )?;
                let mut out = Vec::new();
                for row in rows {
                    let key: String = row.get(0);
                    let value: String = row.get(1);
                    let relay_url = key.trim_start_matches("relay_sync_last_ms:").to_string();
                    let last_ms = value.parse::<i64>().unwrap_or(0);
                    out.push((relay_url, last_ms));
                }
                Ok(out)
            }
        }
    }

    fn load_or_create_signing_keypair_b64(&self) -> Result<(String, String)> {
        if let (Some(pk), Some(sk)) = (
            self.relay_meta_get("sign_pk_b64")?,
            self.relay_meta_get("sign_sk_b64")?,
        ) {
            return Ok((pk, sk));
        }

        let signing = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let verifying = signing.verifying_key();
        let pk_b64 = B64.encode(verifying.to_bytes());
        let sk_b64 = B64.encode(signing.to_bytes());
        self.relay_meta_set("sign_pk_b64", &pk_b64)?;
        self.relay_meta_set("sign_sk_b64", &sk_b64)?;
        Ok((pk_b64, sk_b64))
    }

    fn count_peers_seen_since(&self, cutoff_ms: i64) -> Result<u64> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let n: u64 = conn.query_row(
                    "SELECT COUNT(*) FROM peer_registry WHERE last_seen_ms >= ?1",
                    params![cutoff_ms],
                    |r| r.get(0),
                )?;
                Ok(n)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_one(
                    "SELECT COUNT(*) FROM peer_registry WHERE last_seen_ms >= $1",
                    &[&cutoff_ms],
                )?;
                let n: i64 = row.get(0);
                Ok(n.max(0) as u64)
            }
        }
    }

    fn count_users_total(&self) -> Result<u64> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let n: u64 =
                    conn.query_row("SELECT COUNT(*) FROM users WHERE disabled=0", [], |r| {
                        r.get(0)
                    })?;
                Ok(n)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_one("SELECT COUNT(*) FROM users WHERE disabled=false", &[])?;
                let n: i64 = row.get(0);
                Ok(n.max(0) as u64)
            }
        }
    }

    fn create_user(&mut self, username: &str, token: &str) -> Result<bool> {
        let hash = token_hash_hex(token);
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let exists: Option<String> = conn
                    .query_row(
                        "SELECT username FROM users WHERE username = ?1",
                        params![username],
                        |r| r.get(0),
                    )
                    .optional()?;
                if exists.is_some() {
                    return Ok(false);
                }
                conn.execute(
                    "INSERT INTO users(username, token_sha256, created_at_ms) VALUES (?1, ?2, ?3)",
                    params![username, hash, now],
                )?;
                Ok(true)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let exists = conn.query_opt(
                    "SELECT username FROM users WHERE username = $1",
                    &[&username],
                )?;
                if exists.is_some() {
                    return Ok(false);
                }
                conn.execute(
                    "INSERT INTO users(username, token_sha256, created_at_ms) VALUES ($1, $2, $3)",
                    &[&username, &hash, &now],
                )?;
                Ok(true)
            }
        }
    }

    fn update_user_token(&mut self, username: &str, token: &str) -> Result<()> {
        let hash = token_hash_hex(token);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "UPDATE users SET token_sha256=?2 WHERE username=?1",
                    params![username, hash],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "UPDATE users SET token_sha256=$2 WHERE username=$1",
                    &[&username, &hash],
                )?;
                Ok(())
            }
        }
    }

    fn verify_user_token(&self, username: &str, token: &str) -> Result<bool> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let row: Option<(String, i64)> = conn
                    .query_row(
                        "SELECT token_sha256, disabled FROM users WHERE username = ?1",
                        params![username],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                match row {
                    Some((stored, disabled)) => {
                        if disabled != 0 {
                            return Ok(false);
                        }
                        Ok(stored == token_hash_hex(token))
                    }
                    None => Ok(false),
                }
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT token_sha256, disabled FROM users WHERE username = $1",
                    &[&username],
                )?;
                match row {
                    Some(r) => {
                        let stored: String = r.get(0);
                        let disabled: bool = r.get(1);
                        if disabled {
                            return Ok(false);
                        }
                        Ok(stored == token_hash_hex(token))
                    }
                    None => Ok(false),
                }
            }
        }
    }

    fn user_exists(&self, username: &str) -> Result<bool> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let exists: Option<String> = conn
                    .query_row(
                        "SELECT username FROM users WHERE username = ?1",
                        params![username],
                        |r| r.get(0),
                    )
                    .optional()?;
                Ok(exists.is_some())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let exists = conn.query_opt(
                    "SELECT username FROM users WHERE username = $1",
                    &[&username],
                )?;
                Ok(exists.is_some())
            }
        }
    }

    fn upsert_user_token(
        &mut self,
        cfg: &RelayConfig,
        headers: &HeaderMap,
        username: &str,
        new_token: &str,
    ) -> Result<UpsertUserResult> {
        if !self.user_exists(username)? {
            let created = self.create_user(username, new_token)?;
            return Ok(if created {
                UpsertUserResult::Created
            } else {
                UpsertUserResult::Exists
            });
        }

        // User exists: only admin can change token unless self-register is enabled
        // and the caller proves ownership by presenting the current token.
        if is_authorized_admin(cfg, headers) {
            self.update_user_token(username, new_token)?;
            return Ok(UpsertUserResult::Updated);
        }
        if cfg.allow_self_register {
            if let Some(old) = bearer_token(headers) {
                if self.verify_user_token(username, &old)? {
                    self.update_user_token(username, new_token)?;
                    return Ok(UpsertUserResult::Updated);
                }
                return Ok(UpsertUserResult::Unauthorized);
            }
            return Ok(UpsertUserResult::Exists);
        }

        Ok(UpsertUserResult::Unauthorized)
    }

    fn verify_or_register(&mut self, cfg: &RelayConfig, username: &str, token: &str) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let row: Option<(String, i64)> = conn
                    .query_row(
                        "SELECT token_sha256, disabled FROM users WHERE username = ?1",
                        params![username],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                match row {
                    Some((stored, disabled)) => {
                        if disabled != 0 {
                            return Err(anyhow::anyhow!("user disabled"));
                        }
                        let got = token_hash_hex(token);
                        if stored == got {
                            Ok(())
                        } else {
                            Err(anyhow::anyhow!("invalid token"))
                        }
                    }
                    None => {
                        if !cfg.allow_self_register {
                            return Err(anyhow::anyhow!("unknown user (registration disabled)"));
                        }
                        drop(conn);
                        let created = self.create_user(username, token)?;
                        if created {
                            Ok(())
                        } else {
                            Err(anyhow::anyhow!("user exists"))
                        }
                    }
                }
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT token_sha256, disabled FROM users WHERE username = $1",
                    &[&username],
                )?;
                match row {
                    Some(r) => {
                        let stored: String = r.get(0);
                        let disabled: bool = r.get(1);
                        if disabled {
                            return Err(anyhow::anyhow!("user disabled"));
                        }
                        let got = token_hash_hex(token);
                        if stored == got {
                            Ok(())
                        } else {
                            Err(anyhow::anyhow!("invalid token"))
                        }
                    }
                    None => {
                        if !cfg.allow_self_register {
                            return Err(anyhow::anyhow!("unknown user (registration disabled)"));
                        }
                        drop(conn);
                        let created = self.create_user(username, token)?;
                        if created {
                            Ok(())
                        } else {
                            Err(anyhow::anyhow!("user exists"))
                        }
                    }
                }
            }
        }
    }

    fn verify_token(&self, username: &str, token: &str) -> Result<bool> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let row: Option<(String, i64)> = conn
                    .query_row(
                        "SELECT token_sha256, disabled FROM users WHERE username = ?1",
                        params![username],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                let Some((stored, disabled)) = row else {
                    return Ok(false);
                };
                if disabled != 0 {
                    return Ok(false);
                }
                Ok(stored == token_hash_hex(token))
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT token_sha256, disabled FROM users WHERE username = $1",
                    &[&username],
                )?;
                let Some(r) = row else { return Ok(false) };
                let stored: String = r.get(0);
                let disabled: bool = r.get(1);
                if disabled {
                    return Ok(false);
                }
                Ok(stored == token_hash_hex(token))
            }
        }
    }

    fn set_user_move(&self, username: &str, moved_to_actor: &str) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO user_moves(username, moved_to_actor, moved_at_ms) VALUES (?1, ?2, ?3)\n             ON CONFLICT(username) DO UPDATE SET moved_to_actor=excluded.moved_to_actor, moved_at_ms=excluded.moved_at_ms",
                    params![username, moved_to_actor, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO user_moves(username, moved_to_actor, moved_at_ms) VALUES ($1, $2, $3)\n             ON CONFLICT(username) DO UPDATE SET moved_to_actor=EXCLUDED.moved_to_actor, moved_at_ms=EXCLUDED.moved_at_ms",
                    &[&username, &moved_to_actor, &now],
                )?;
                Ok(())
            }
        }
    }

    fn clear_user_move(&self, username: &str) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let _ = conn.execute(
                    "DELETE FROM user_moves WHERE username=?1",
                    params![username],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let _ = conn.execute("DELETE FROM user_moves WHERE username=$1", &[&username])?;
                Ok(())
            }
        }
    }

    fn get_user_move(&self, username: &str) -> Result<Option<(String, i64)>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT moved_to_actor, moved_at_ms FROM user_moves WHERE username=?1",
                    params![username],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT moved_to_actor, moved_at_ms FROM user_moves WHERE username=$1",
                    &[&username],
                )?;
                Ok(row.map(|r| (r.get(0), r.get(1))))
            }
        }
    }

    fn has_move_notice(&self, notice_id: &str) -> Result<bool> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let v: Option<String> = conn
                    .query_row(
                        "SELECT notice_id FROM move_notices WHERE notice_id=?1",
                        params![notice_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                Ok(v.is_some())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT notice_id FROM move_notices WHERE notice_id=$1",
                    &[&notice_id],
                )?;
                Ok(row.is_some())
            }
        }
    }

    fn upsert_move_notice(&self, notice_id: &str, notice_json: &str) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO move_notices(notice_id, notice_json, created_at_ms) VALUES (?1, ?2, ?3)\n             ON CONFLICT(notice_id) DO UPDATE SET notice_json=excluded.notice_json, created_at_ms=excluded.created_at_ms",
                    params![notice_id, notice_json, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO move_notices(notice_id, notice_json, created_at_ms) VALUES ($1, $2, $3)\n             ON CONFLICT(notice_id) DO UPDATE SET notice_json=EXCLUDED.notice_json, created_at_ms=EXCLUDED.created_at_ms",
                    &[&notice_id, &notice_json, &now],
                )?;
                Ok(())
            }
        }
    }

    fn list_recent_move_notices(
        &self,
        cutoff_ms: i64,
        limit: u32,
    ) -> Result<Vec<(String, String, i64)>> {
        let limit = limit.max(1).min(1000) as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT notice_id, notice_json, created_at_ms FROM move_notices WHERE created_at_ms >= ?1 ORDER BY created_at_ms DESC LIMIT ?2",
                )?;
                let mut rows = stmt.query(params![cutoff_ms, limit])?;
                let mut out = Vec::new();
                while let Some(r) = rows.next()? {
                    out.push((r.get(0)?, r.get(1)?, r.get(2)?));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT notice_id, notice_json, created_at_ms FROM move_notices WHERE created_at_ms >= $1 ORDER BY created_at_ms DESC LIMIT $2",
                    &[&cutoff_ms, &limit],
                )?;
                let mut out = Vec::new();
                for r in rows {
                    out.push((r.get(0), r.get(1), r.get(2)));
                }
                Ok(out)
            }
        }
    }

    fn get_fanout_status(
        &self,
        notice_id: &str,
        relay_url: &str,
    ) -> Result<Option<(i64, i64, i64)>> {
        // returns (tries, last_try_ms, sent_ok)
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT tries, last_try_ms, sent_ok FROM move_notice_fanout WHERE notice_id=?1 AND relay_url=?2",
                    params![notice_id, relay_url],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT tries, last_try_ms, sent_ok FROM move_notice_fanout WHERE notice_id=$1 AND relay_url=$2",
                    &[&notice_id, &relay_url],
                )?;
                Ok(row.map(|r| (r.get(0), r.get(1), r.get(2))))
            }
        }
    }

    fn record_fanout_attempt(&self, notice_id: &str, relay_url: &str, sent_ok: bool) -> Result<()> {
        let relay_url = relay_url.trim_end_matches('/');
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO move_notice_fanout(notice_id, relay_url, tries, last_try_ms, sent_ok) VALUES (?1, ?2, 1, ?3, ?4)\n             ON CONFLICT(notice_id, relay_url) DO UPDATE SET tries=move_notice_fanout.tries+1, last_try_ms=excluded.last_try_ms, sent_ok=excluded.sent_ok",
                    params![notice_id, relay_url, now, if sent_ok { 1 } else { 0 }],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO move_notice_fanout(notice_id, relay_url, tries, last_try_ms, sent_ok) VALUES ($1, $2, 1, $3, $4)\n             ON CONFLICT(notice_id, relay_url) DO UPDATE SET tries=move_notice_fanout.tries+1, last_try_ms=EXCLUDED.last_try_ms, sent_ok=EXCLUDED.sent_ok",
                    &[&notice_id, &relay_url, &now, &sent_ok],
                )?;
                Ok(())
            }
        }
    }

    fn count_users(&self) -> Result<u64> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let n: u64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
                Ok(n)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_one("SELECT COUNT(*) FROM users", &[])?;
                let n: i64 = row.get(0);
                Ok(n.max(0) as u64)
            }
        }
    }

    fn list_users(&self, limit: u32, offset: u32) -> Result<Vec<(String, i64, i64)>> {
        let limit = limit.min(500).max(1) as i64;
        let offset = offset as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT username, created_at_ms, disabled FROM users ORDER BY created_at_ms DESC LIMIT ?1 OFFSET ?2",
                )?;
                let mut rows = stmt.query(params![limit, offset])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    out.push((row.get(0)?, row.get(1)?, row.get(2)?));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT username, created_at_ms, disabled FROM users ORDER BY created_at_ms DESC LIMIT $1 OFFSET $2",
                    &[&limit, &offset],
                )?;
                let mut out = Vec::new();
                for row in rows {
                    let disabled: bool = row.get(2);
                    out.push((row.get(0), row.get(1), if disabled { 1 } else { 0 }));
                }
                Ok(out)
            }
        }
    }

    fn set_disabled(&self, username: &str, disabled: bool) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "UPDATE users SET disabled=?2 WHERE username=?1",
                    params![username, if disabled { 1 } else { 0 }],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "UPDATE users SET disabled=$2 WHERE username=$1",
                    &[&username, &disabled],
                )?;
                Ok(())
            }
        }
    }

    fn rotate_token(&self, username: &str, new_token: &str) -> Result<()> {
        let hash = token_hash_hex(new_token);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "UPDATE users SET token_sha256=?2 WHERE username=?1",
                    params![username, hash],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "UPDATE users SET token_sha256=$2 WHERE username=$1",
                    &[&username, &hash],
                )?;
                Ok(())
            }
        }
    }

    fn get_user(&self, username: &str) -> Result<Option<(i64, i64)>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT created_at_ms, disabled FROM users WHERE username=?1",
                    params![username],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT created_at_ms, disabled FROM users WHERE username=$1",
                    &[&username],
                )?;
                Ok(row.map(|r| {
                    let disabled: bool = r.get(1);
                    (r.get(0), if disabled { 1 } else { 0 })
                }))
            }
        }
    }

    fn delete_user(&self, username: &str) -> Result<bool> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let _ = conn.execute(
                    "DELETE FROM inbox_spool WHERE username=?1",
                    params![username],
                )?;
                let _ = conn.execute(
                    "DELETE FROM user_cache WHERE username=?1",
                    params![username],
                )?;
                let _ = conn.execute(
                    "DELETE FROM user_collection_cache WHERE username=?1",
                    params![username],
                )?;
                let _ = conn.execute(
                    "DELETE FROM media_items WHERE username=?1",
                    params![username],
                )?;
                let _ = conn.execute(
                    "DELETE FROM peer_directory WHERE username=?1",
                    params![username],
                )?;
                let changed =
                    conn.execute("DELETE FROM users WHERE username=?1", params![username])?;
                Ok(changed > 0)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let _ = conn.execute("DELETE FROM inbox_spool WHERE username=$1", &[&username])?;
                let _ = conn.execute("DELETE FROM user_cache WHERE username=$1", &[&username])?;
                let _ = conn.execute(
                    "DELETE FROM user_collection_cache WHERE username=$1",
                    &[&username],
                )?;
                let _ = conn.execute("DELETE FROM media_items WHERE username=$1", &[&username])?;
                let _ =
                    conn.execute("DELETE FROM peer_directory WHERE username=$1", &[&username])?;
                let changed = conn.execute("DELETE FROM users WHERE username=$1", &[&username])?;
                Ok(changed > 0)
            }
        }
    }

    fn is_user_enabled(&self, username: &str) -> Result<bool> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let row: Option<i64> = conn
                    .query_row(
                        "SELECT disabled FROM users WHERE username=?1",
                        params![username],
                        |r| r.get(0),
                    )
                    .optional()?;
                Ok(matches!(row, Some(v) if v == 0))
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row =
                    conn.query_opt("SELECT disabled FROM users WHERE username=$1", &[&username])?;
                Ok(row.map(|r| !r.get::<_, bool>(0)).unwrap_or(false))
            }
        }
    }

    fn enqueue_spool(
        &self,
        cfg: &RelayConfig,
        username: &str,
        method: &str,
        path: &str,
        query: &str,
        headers: &[(String, String)],
        body_b64: &str,
        body_len: i64,
    ) -> Result<()> {
        let headers_json = serde_json::to_string(headers).unwrap_or_else(|_| "[]".to_string());
        let now = now_ms();
        let cap = cfg.spool_max_rows_per_user as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO inbox_spool(username, created_at_ms, method, path, query, headers_json, body_b64, body_len) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![username, now, method, path, query, headers_json, body_b64, body_len],
                )?;

                let count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM inbox_spool WHERE username=?1",
                    params![username],
                    |r| r.get(0),
                )?;
                if cap > 0 && count > cap {
                    let excess = count - cap;
                    let _ = conn.execute(
                        "DELETE FROM inbox_spool WHERE id IN (SELECT id FROM inbox_spool WHERE username=?1 ORDER BY created_at_ms ASC LIMIT ?2)",
                        params![username, excess],
                    )?;
                }
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO inbox_spool(username, created_at_ms, method, path, query, headers_json, body_b64, body_len) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                    &[&username, &now, &method, &path, &query, &headers_json, &body_b64, &body_len],
                )?;
                let row = conn.query_one(
                    "SELECT COUNT(*) FROM inbox_spool WHERE username=$1",
                    &[&username],
                )?;
                let count: i64 = row.get(0);
                if cap > 0 && count > cap {
                    let excess = count - cap;
                    let _ = conn.execute(
                        "DELETE FROM inbox_spool WHERE id IN (SELECT id FROM inbox_spool WHERE username=$1 ORDER BY created_at_ms ASC LIMIT $2)",
                        &[&username, &excess],
                    )?;
                }
                Ok(())
            }
        }
    }

    fn list_spool(&self, username: &str, limit: usize) -> Result<Vec<SpoolItem>> {
        let limit = limit.min(1000) as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT id, method, path, query, headers_json, body_b64 FROM inbox_spool WHERE username=?1 ORDER BY created_at_ms ASC LIMIT ?2",
                )?;
                let mut rows = stmt.query(params![username, limit])?;
                let mut out = Vec::new();
                while let Some(r) = rows.next()? {
                    out.push(SpoolItem {
                        id: r.get(0)?,
                        method: r.get(1)?,
                        path: r.get(2)?,
                        query: r.get(3)?,
                        headers_json: r.get(4)?,
                        body_b64: r.get(5)?,
                    });
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT id, method, path, query, headers_json, body_b64 FROM inbox_spool WHERE username=$1 ORDER BY created_at_ms ASC LIMIT $2",
                    &[&username, &limit],
                )?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(SpoolItem {
                        id: r.get(0),
                        method: r.get(1),
                        path: r.get(2),
                        query: r.get(3),
                        headers_json: r.get(4),
                        body_b64: r.get(5),
                    });
                }
                Ok(out)
            }
        }
    }

    fn delete_spool_ids(&self, ids: &[i64]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        match self.driver {
            DbDriver::Sqlite => {
                let mut conn = self.open_sqlite_conn()?;
                let tx = conn.transaction()?;
                for chunk in ids.chunks(DB_BATCH_DELETE_MAX) {
                    let placeholders = std::iter::repeat("?")
                        .take(chunk.len())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let sql = format!("DELETE FROM inbox_spool WHERE id IN ({placeholders})");
                    let params: Vec<&dyn rusqlite::ToSql> =
                        chunk.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
                    let _ = tx.execute(&sql, rusqlite::params_from_iter(params))?;
                }
                tx.commit()?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                for chunk in ids.chunks(DB_BATCH_DELETE_MAX) {
                    let _ =
                        conn.execute("DELETE FROM inbox_spool WHERE id = ANY($1)", &[&chunk])?;
                }
                Ok(())
            }
        }
    }

    fn cleanup_spool(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms() - (ttl_secs as i64 * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM inbox_spool WHERE created_at_ms < ?1",
                    params![cutoff],
                )?;
                Ok(deleted as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM inbox_spool WHERE created_at_ms < $1",
                    &[&cutoff],
                )?;
                Ok(deleted as u64)
            }
        }
    }

    fn cleanup_move_notices(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms() - (ttl_secs as i64 * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM move_notices WHERE created_at_ms < ?1",
                    params![cutoff],
                )?;
                let _ = conn.execute(
                    "DELETE FROM move_notice_fanout WHERE notice_id NOT IN (SELECT notice_id FROM move_notices)",
                    [],
                )?;
                Ok(deleted as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM move_notices WHERE created_at_ms < $1",
                    &[&cutoff],
                )?;
                let _ = conn.execute(
                    "DELETE FROM move_notice_fanout WHERE notice_id NOT IN (SELECT notice_id FROM move_notices)",
                    &[],
                )?;
                Ok(deleted as u64)
            }
        }
    }

    fn cleanup_relay_media(&self, ttl_secs: u64) -> Result<u64> {
        if ttl_secs == 0 {
            return Ok(0);
        }
        let cutoff = now_ms() - (ttl_secs as i64 * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM relay_media WHERE created_at_ms < ?1",
                    params![cutoff],
                )?;
                Ok(deleted as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM relay_media WHERE created_at_ms < $1",
                    &[&cutoff],
                )?;
                Ok(deleted as u64)
            }
        }
    }

    fn cleanup_relay_actors(&self, ttl_secs: u64) -> Result<u64> {
        if ttl_secs == 0 {
            return Ok(0);
        }
        let cutoff = now_ms() - (ttl_secs as i64 * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM relay_actors WHERE updated_at_ms < ?1",
                    params![cutoff],
                )?;
                Ok(deleted as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM relay_actors WHERE updated_at_ms < $1",
                    &[&cutoff],
                )?;
                Ok(deleted as u64)
            }
        }
    }

    fn spool_stats(&self, username: &str) -> Result<(u64, u64)> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let (count, bytes): (i64, i64) = conn.query_row(
                    "SELECT COUNT(*), COALESCE(SUM(body_len), 0) FROM inbox_spool WHERE username=?1",
                    params![username],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                Ok((count.max(0) as u64, bytes.max(0) as u64))
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_one(
                    "SELECT COUNT(*), COALESCE(SUM(body_len), 0)::BIGINT FROM inbox_spool WHERE username=$1",
                    &[&username],
                )?;
                let count: i64 = row.get(0);
                let bytes: i64 = row.get(1);
                Ok((count.max(0) as u64, bytes.max(0) as u64))
            }
        }
    }

    fn upsert_actor_cache(&self, username: &str, actor_json: &str) -> Result<()> {
        let now = now_ms();
        let (actor_id, actor_url) = extract_actor_ids_from_json(actor_json);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO user_cache(username, actor_json, updated_at_ms, actor_id, actor_url) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(username) DO UPDATE SET actor_json=excluded.actor_json, updated_at_ms=excluded.updated_at_ms, actor_id=excluded.actor_id, actor_url=excluded.actor_url",
                    params![username, actor_json, now, actor_id, actor_url],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO user_cache(username, actor_json, updated_at_ms, actor_id, actor_url) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT(username) DO UPDATE SET actor_json=EXCLUDED.actor_json, updated_at_ms=EXCLUDED.updated_at_ms, actor_id=EXCLUDED.actor_id, actor_url=EXCLUDED.actor_url",
                    &[&username, &actor_json, &now, &actor_id, &actor_url],
                )?;
                Ok(())
            }
        }
    }

    fn get_actor_cache(&self, username: &str) -> Result<Option<String>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT actor_json FROM user_cache WHERE username=?1",
                    params![username],
                    |r| r.get(0),
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT actor_json FROM user_cache WHERE username=$1",
                    &[&username],
                )?;
                Ok(row.map(|r| r.get(0)))
            }
        }
    }

    fn get_actor_cache_with_meta(&self, username: &str) -> Result<Option<ActorCacheMeta>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT actor_json, updated_at_ms, actor_id, actor_url FROM user_cache WHERE username=?1",
                    params![username],
                    |r| {
                        Ok(ActorCacheMeta {
                            actor_json: r.get(0)?,
                            updated_at_ms: r.get(1)?,
                            actor_id: r.get(2)?,
                            actor_url: r.get(3)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT actor_json, updated_at_ms, actor_id, actor_url FROM user_cache WHERE username=$1",
                    &[&username],
                )?;
                Ok(row.map(|r| ActorCacheMeta {
                    actor_json: r.get(0),
                    updated_at_ms: r.get(1),
                    actor_id: r.get(2),
                    actor_url: r.get(3),
                }))
            }
        }
    }

    fn upsert_collection_cache(&self, username: &str, kind: &str, json: &str) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO user_collection_cache(username, kind, json, updated_at_ms) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(username, kind) DO UPDATE SET json=excluded.json, updated_at_ms=excluded.updated_at_ms",
                    params![username, kind, json, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO user_collection_cache(username, kind, json, updated_at_ms) VALUES ($1, $2, $3, $4)
             ON CONFLICT(username, kind) DO UPDATE SET json=EXCLUDED.json, updated_at_ms=EXCLUDED.updated_at_ms",
                    &[&username, &kind, &json, &now],
                )?;
                Ok(())
            }
        }
    }

    fn get_collection_cache(&self, username: &str, kind: &str) -> Result<Option<String>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT json FROM user_collection_cache WHERE username=?1 AND kind=?2",
                    params![username, kind],
                    |r| r.get(0),
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT json FROM user_collection_cache WHERE username=$1 AND kind=$2",
                    &[&username, &kind],
                )?;
                Ok(row.map(|r| r.get(0)))
            }
        }
    }

    fn cleanup_peer_directory(&self, ttl_days: u32) -> Result<u64> {
        if ttl_days == 0 {
            return Ok(0);
        }
        let cutoff = now_ms().saturating_sub((ttl_days as i64) * 24 * 60 * 60 * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM peer_directory WHERE updated_at_ms < ?1",
                    params![cutoff],
                )?;
                Ok(deleted as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM peer_directory WHERE updated_at_ms < $1",
                    &[&cutoff],
                )?;
                Ok(deleted)
            }
        }
    }

    fn cleanup_peer_registry(&self, ttl_days: u32) -> Result<u64> {
        if ttl_days == 0 {
            return Ok(0);
        }
        let cutoff = now_ms().saturating_sub((ttl_days as i64) * 24 * 60 * 60 * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM peer_registry WHERE last_seen_ms < ?1",
                    params![cutoff],
                )?;
                Ok(deleted as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM peer_registry WHERE last_seen_ms < $1",
                    &[&cutoff],
                )?;
                Ok(deleted)
            }
        }
    }

    fn upsert_media_item(&self, item: &MediaItem) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO media_items(id, username, backend, storage_key, media_type, size, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)\n             ON CONFLICT(id) DO UPDATE SET backend=excluded.backend, storage_key=excluded.storage_key, media_type=excluded.media_type, size=excluded.size",
                    params![
                        item.id,
                        item.username,
                        item.backend,
                        item.storage_key,
                        item.media_type,
                        item.size,
                        item.created_at_ms
                    ],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO media_items(id, username, backend, storage_key, media_type, size, created_at_ms) VALUES ($1, $2, $3, $4, $5, $6, $7)\n             ON CONFLICT(id) DO UPDATE SET backend=EXCLUDED.backend, storage_key=EXCLUDED.storage_key, media_type=EXCLUDED.media_type, size=EXCLUDED.size",
                    &[
                        &item.id,
                        &item.username,
                        &item.backend,
                        &item.storage_key,
                        &item.media_type,
                        &item.size,
                        &item.created_at_ms,
                    ],
                )?;
                Ok(())
            }
        }
    }

    fn get_media_item(&self, username: &str, id: &str) -> Result<Option<MediaItem>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT id, username, backend, storage_key, media_type, size, created_at_ms FROM media_items WHERE username=?1 AND id=?2",
                    params![username, id],
                    |r| {
                        Ok(MediaItem {
                            id: r.get(0)?,
                            username: r.get(1)?,
                            backend: r.get(2)?,
                            storage_key: r.get(3)?,
                            media_type: r.get(4)?,
                            size: r.get(5)?,
                            created_at_ms: r.get(6)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT id, username, backend, storage_key, media_type, size, created_at_ms FROM media_items WHERE username=$1 AND id=$2",
                    &[&username, &id],
                )?;
                Ok(row.map(|r| MediaItem {
                    id: r.get(0),
                    username: r.get(1),
                    backend: r.get(2),
                    storage_key: r.get(3),
                    media_type: r.get(4),
                    size: r.get(5),
                    created_at_ms: r.get(6),
                }))
            }
        }
    }

    fn upsert_user_backup(&self, item: &UserBackupItem) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO user_backups(username, storage_key, content_type, size_bytes, updated_at_ms, meta_json)\n             VALUES (?1, ?2, ?3, ?4, ?5, ?6)\n             ON CONFLICT(username) DO UPDATE SET\n               storage_key=excluded.storage_key,\n               content_type=excluded.content_type,\n               size_bytes=excluded.size_bytes,\n               updated_at_ms=excluded.updated_at_ms,\n               meta_json=excluded.meta_json",
                    params![
                        item.username,
                        item.storage_key,
                        item.content_type,
                        item.size_bytes,
                        item.updated_at_ms,
                        item.meta_json
                    ],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO user_backups(username, storage_key, content_type, size_bytes, updated_at_ms, meta_json)\n             VALUES ($1, $2, $3, $4, $5, $6)\n             ON CONFLICT(username) DO UPDATE SET\n               storage_key=EXCLUDED.storage_key,\n               content_type=EXCLUDED.content_type,\n               size_bytes=EXCLUDED.size_bytes,\n               updated_at_ms=EXCLUDED.updated_at_ms,\n               meta_json=EXCLUDED.meta_json",
                    &[
                        &item.username,
                        &item.storage_key,
                        &item.content_type,
                        &item.size_bytes,
                        &item.updated_at_ms,
                        &item.meta_json,
                    ],
                )?;
                Ok(())
            }
        }
    }

    fn insert_user_backup_history(&self, item: &UserBackupItem) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT OR IGNORE INTO user_backups_history(storage_key, username, content_type, size_bytes, created_at_ms, meta_json)\n             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        item.storage_key,
                        item.username,
                        item.content_type,
                        item.size_bytes,
                        item.updated_at_ms,
                        item.meta_json
                    ],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO user_backups_history(storage_key, username, content_type, size_bytes, created_at_ms, meta_json)\n             VALUES ($1, $2, $3, $4, $5, $6)\n             ON CONFLICT(storage_key) DO NOTHING",
                    &[
                        &item.storage_key,
                        &item.username,
                        &item.content_type,
                        &item.size_bytes,
                        &item.updated_at_ms,
                        &item.meta_json,
                    ],
                )?;
                Ok(())
            }
        }
    }

    fn get_user_backup(&self, username: &str) -> Result<Option<UserBackupItem>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT username, storage_key, content_type, size_bytes, updated_at_ms, meta_json\n             FROM user_backups WHERE username=?1",
                    params![username],
                    |r| {
                        Ok(UserBackupItem {
                            username: r.get(0)?,
                            storage_key: r.get(1)?,
                            content_type: r.get(2)?,
                            size_bytes: r.get(3)?,
                            updated_at_ms: r.get(4)?,
                            meta_json: r.get(5)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT username, storage_key, content_type, size_bytes, updated_at_ms, meta_json\n             FROM user_backups WHERE username=$1",
                    &[&username],
                )?;
                Ok(row.map(|r| UserBackupItem {
                    username: r.get(0),
                    storage_key: r.get(1),
                    content_type: r.get(2),
                    size_bytes: r.get(3),
                    updated_at_ms: r.get(4),
                    meta_json: r.get(5),
                }))
            }
        }
    }

    fn list_user_backup_keys(&self, username: &str) -> Result<Vec<String>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT storage_key FROM user_backups_history WHERE username=?1 ORDER BY created_at_ms DESC",
                )?;
                let rows = stmt
                    .query_map(params![username], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT storage_key FROM user_backups_history WHERE username=$1 ORDER BY created_at_ms DESC",
                    &[&username],
                )?;
                Ok(rows.into_iter().map(|r| r.get(0)).collect())
            }
        }
    }

    fn delete_user_backup_history(&self, username: &str, storage_key: &str) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "DELETE FROM user_backups_history WHERE username=?1 AND storage_key=?2",
                    params![username, storage_key],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "DELETE FROM user_backups_history WHERE username=$1 AND storage_key=$2",
                    &[&username, &storage_key],
                )?;
                Ok(())
            }
        }
    }

    fn count_user_backups_since(&self, username: &str, since_ms: i64) -> Result<u64> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM user_backups_history WHERE username=?1 AND created_at_ms >= ?2",
                    params![username, since_ms],
                    |r| r.get(0),
                )?;
                Ok(count.max(0) as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_one(
                    "SELECT COUNT(*) FROM user_backups_history WHERE username=$1 AND created_at_ms >= $2",
                    &[&username, &since_ms],
                )?;
                let count: i64 = row.get(0);
                Ok(count.max(0) as u64)
            }
        }
    }

    fn upsert_relay_note(&self, note: &RelayNoteIndex) -> Result<()> {
        let published_ms = note.published_ms.unwrap_or(note.created_at_ms);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO relay_notes(note_id, actor_id, published_ms, content_text, content_html, note_json, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)\n             ON CONFLICT(note_id) DO UPDATE SET\n               actor_id=excluded.actor_id,\n               published_ms=excluded.published_ms,\n               content_text=excluded.content_text,\n               content_html=excluded.content_html,\n               note_json=excluded.note_json",
                    params![
                        note.note_id,
                        note.actor_id,
                        published_ms,
                        note.content_text,
                        note.content_html,
                        note.note_json,
                        note.created_at_ms
                    ],
                )?;
                let tx = conn.unchecked_transaction()?;
                tx.execute(
                    "DELETE FROM relay_note_tags WHERE note_id=?1",
                    params![note.note_id],
                )?;
                for tag in &note.tags {
                    tx.execute(
                        "INSERT OR IGNORE INTO relay_note_tags(note_id, tag) VALUES (?1, ?2)",
                        params![note.note_id, tag],
                    )?;
                }
                tx.commit()?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let mut tx = conn.transaction()?;
                let params: &[&(dyn ToSql + Sync)] = &[
                    &note.note_id,
                    &note.actor_id,
                    &published_ms,
                    &note.content_text,
                    &note.content_html,
                    &note.note_json,
                    &note.created_at_ms,
                ];
                tx.execute(
                    "INSERT INTO relay_notes(note_id, actor_id, published_ms, content_text, content_html, note_json, created_at_ms) VALUES ($1, $2, $3, $4, $5, $6, $7)\n             ON CONFLICT(note_id) DO UPDATE SET\n               actor_id=EXCLUDED.actor_id,\n               published_ms=EXCLUDED.published_ms,\n               content_text=EXCLUDED.content_text,\n               content_html=EXCLUDED.content_html,\n               note_json=EXCLUDED.note_json",
                    params,
                )?;
                tx.execute(
                    "DELETE FROM relay_note_tags WHERE note_id=$1",
                    &[&note.note_id],
                )?;
                for tag in &note.tags {
                    tx.execute(
                        "INSERT INTO relay_note_tags(note_id, tag) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                        &[&note.note_id, &tag],
                    )?;
                }
                tx.commit()?;
                Ok(())
            }
        }
    }

    fn upsert_relay_media(&self, media: &RelayMediaIndex) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO relay_media(media_url, media_type, name, width, height, blurhash, created_at_ms)\n             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)\n             ON CONFLICT(media_url) DO UPDATE SET\n               media_type=excluded.media_type,\n               name=excluded.name,\n               width=excluded.width,\n               height=excluded.height,\n               blurhash=excluded.blurhash",
                    params![
                        media.url,
                        media.media_type,
                        media.name,
                        media.width,
                        media.height,
                        media.blurhash,
                        media.created_at_ms
                    ],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO relay_media(media_url, media_type, name, width, height, blurhash, created_at_ms)\n             VALUES ($1, $2, $3, $4, $5, $6, $7)\n             ON CONFLICT(media_url) DO UPDATE SET\n               media_type=EXCLUDED.media_type,\n               name=EXCLUDED.name,\n               width=EXCLUDED.width,\n               height=EXCLUDED.height,\n               blurhash=EXCLUDED.blurhash",
                    &[
                        &media.url,
                        &media.media_type,
                        &media.name,
                        &media.width,
                        &media.height,
                        &media.blurhash,
                        &media.created_at_ms,
                    ],
                )?;
                Ok(())
            }
        }
    }

    fn upsert_relay_actor(&self, actor: &RelayActorIndex) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO relay_actors(actor_url, username, actor_json, updated_at_ms)\n             VALUES (?1, ?2, ?3, ?4)\n             ON CONFLICT(actor_url) DO UPDATE SET\n               username=excluded.username,\n               actor_json=excluded.actor_json,\n               updated_at_ms=excluded.updated_at_ms",
                    params![
                        actor.actor_url,
                        actor.username,
                        actor.actor_json,
                        actor.updated_at_ms
                    ],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO relay_actors(actor_url, username, actor_json, updated_at_ms)\n             VALUES ($1, $2, $3, $4)\n             ON CONFLICT(actor_url) DO UPDATE SET\n               username=EXCLUDED.username,\n               actor_json=EXCLUDED.actor_json,\n               updated_at_ms=EXCLUDED.updated_at_ms",
                    &[
                        &actor.actor_url,
                        &actor.username,
                        &actor.actor_json,
                        &actor.updated_at_ms,
                    ],
                )?;
                Ok(())
            }
        }
    }

    fn search_relay_notes(
        &self,
        q: &str,
        tag: &str,
        limit: u32,
        cursor: Option<i64>,
        since: Option<i64>,
        total_mode: SearchTotalMode,
    ) -> Result<CollectionPage<String>> {
        let limit = limit.min(200).max(1) as i64;
        let q_norm = q.trim().to_lowercase();
        let tag_norm = tag.trim().trim_start_matches('#').to_lowercase();
        let q_like = if q_norm.is_empty() {
            "%".to_string()
        } else {
            format!("%{}%", escape_like(&q_norm))
        };
        let tag_like = if tag_norm.is_empty() {
            "%".to_string()
        } else {
            format!("%{}%", escape_like(&tag_norm))
        };

        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let total_exact: u64 = if total_mode == SearchTotalMode::Exact {
                    if !tag_norm.is_empty() {
                        conn.query_row(
                            r#"
                SELECT COUNT(*)
                FROM relay_note_tags t
                JOIN relay_notes n ON n.note_id = t.note_id
                WHERE lower(t.tag) LIKE ?1
                "#,
                            params![tag_like],
                            |r| r.get(0),
                        )
                        .unwrap_or(0)
                    } else {
                        conn.query_row(
                            "SELECT COUNT(*) FROM relay_notes WHERE lower(content_text) LIKE ?1 OR lower(content_html) LIKE ?1",
                            params![q_like],
                            |r| r.get(0),
                        )
                        .unwrap_or(0)
                    }
                } else {
                    0
                };

                let mut stmt;
                let mut rows;
                if !tag_norm.is_empty() {
                    if let Some(since) = since {
                        stmt = conn.prepare(
                            r#"
                    SELECT n.note_json, n.created_at_ms
                    FROM relay_note_tags t
                    JOIN relay_notes n ON n.note_id = t.note_id
                    WHERE lower(t.tag) LIKE ?1 AND n.created_at_ms > ?2
                    ORDER BY n.created_at_ms DESC
                    LIMIT ?3
                    "#,
                        )?;
                        rows = stmt.query(params![tag_like, since, limit])?;
                    } else if let Some(cur) = cursor {
                        stmt = conn.prepare(
                            r#"
                    SELECT n.note_json, n.created_at_ms
                    FROM relay_note_tags t
                    JOIN relay_notes n ON n.note_id = t.note_id
                    WHERE lower(t.tag) LIKE ?1 AND n.created_at_ms < ?2
                    ORDER BY n.created_at_ms DESC
                    LIMIT ?3
                    "#,
                        )?;
                        rows = stmt.query(params![tag_like, cur, limit])?;
                    } else {
                        stmt = conn.prepare(
                            r#"
                    SELECT n.note_json, n.created_at_ms
                    FROM relay_note_tags t
                    JOIN relay_notes n ON n.note_id = t.note_id
                    WHERE lower(t.tag) LIKE ?1
                    ORDER BY n.created_at_ms DESC
                    LIMIT ?2
                    "#,
                        )?;
                        rows = stmt.query(params![tag_like, limit])?;
                    }
                } else if let Some(since) = since {
                    stmt = conn.prepare(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE (lower(content_text) LIKE ?1 OR lower(content_html) LIKE ?1) AND created_at_ms > ?2 ORDER BY created_at_ms DESC LIMIT ?3",
                    )?;
                    rows = stmt.query(params![q_like, since, limit])?;
                } else if let Some(cur) = cursor {
                    stmt = conn.prepare(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE (lower(content_text) LIKE ?1 OR lower(content_html) LIKE ?1) AND created_at_ms < ?2 ORDER BY created_at_ms DESC LIMIT ?3",
                    )?;
                    rows = stmt.query(params![q_like, cur, limit])?;
                } else {
                    stmt = conn.prepare(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE lower(content_text) LIKE ?1 OR lower(content_html) LIKE ?1 ORDER BY created_at_ms DESC LIMIT ?2",
                    )?;
                    rows = stmt.query(params![q_like, limit])?;
                }
                let mut items = Vec::<String>::new();
                let mut last_created = None;
                while let Some(row) = rows.next()? {
                    let note_json: String = row.get(0)?;
                    let created_at_ms: i64 = row.get(1)?;
                    last_created = Some(created_at_ms);
                    items.push(note_json);
                }
                let next = if items.len() as i64 == limit {
                    last_created.map(|v| v.to_string())
                } else {
                    None
                };
                let total = match total_mode {
                    SearchTotalMode::Exact => total_exact,
                    SearchTotalMode::Approx => {
                        if !tag_norm.is_empty() {
                            let n: i64 = conn
                                .query_row(
                                    "SELECT COALESCE(SUM(count), 0) FROM relay_tag_counts WHERE lower(tag) LIKE ?1",
                                    params![tag_like],
                                    |r| r.get(0),
                                )
                                .unwrap_or(0);
                            n.max(0) as u64
                        } else if q_norm.is_empty() {
                            let n: i64 = conn
                                .query_row(
                                    "SELECT count FROM relay_notes_count WHERE id = 1",
                                    [],
                                    |r| r.get(0),
                                )
                                .unwrap_or(0);
                            n.max(0) as u64
                        } else {
                            items.len() as u64
                        }
                    }
                    SearchTotalMode::None => 0,
                };
                Ok(CollectionPage { total, items, next })
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let total_exact: u64 = if total_mode == SearchTotalMode::Exact {
                    if !tag_norm.is_empty() {
                        let row = conn.query_one(
                            r#"
                SELECT COUNT(*)
                FROM relay_note_tags t
                JOIN relay_notes n ON n.note_id = t.note_id
                WHERE t.tag_tsv @@ plainto_tsquery('simple', $1)
                "#,
                            &[&tag_norm],
                        )?;
                        let n: i64 = row.get(0);
                        n.max(0) as u64
                    } else if !q_norm.is_empty() {
                        let row = conn.query_one(
                            "SELECT COUNT(*) FROM relay_notes WHERE search_tsv @@ plainto_tsquery('simple', $1)",
                            &[&q_norm],
                        )?;
                        let n: i64 = row.get(0);
                        n.max(0) as u64
                    } else {
                        let row = conn.query_one("SELECT COUNT(*) FROM relay_notes", &[])?;
                        let n: i64 = row.get(0);
                        n.max(0) as u64
                    }
                } else {
                    0
                };

                let rows = if !tag_norm.is_empty() {
                    if let Some(since) = since {
                        conn.query(
                            r#"
                    SELECT n.note_json, n.created_at_ms
                    FROM relay_note_tags t
                    JOIN relay_notes n ON n.note_id = t.note_id
                    WHERE t.tag_tsv @@ plainto_tsquery('simple', $1) AND n.created_at_ms > $2
                    ORDER BY n.created_at_ms DESC
                    LIMIT $3
                    "#,
                            &[&tag_norm, &since, &limit],
                        )?
                    } else if let Some(cur) = cursor {
                        conn.query(
                            r#"
                    SELECT n.note_json, n.created_at_ms
                    FROM relay_note_tags t
                    JOIN relay_notes n ON n.note_id = t.note_id
                    WHERE t.tag_tsv @@ plainto_tsquery('simple', $1) AND n.created_at_ms < $2
                    ORDER BY n.created_at_ms DESC
                    LIMIT $3
                    "#,
                            &[&tag_norm, &cur, &limit],
                        )?
                    } else {
                        conn.query(
                            r#"
                    SELECT n.note_json, n.created_at_ms
                    FROM relay_note_tags t
                    JOIN relay_notes n ON n.note_id = t.note_id
                    WHERE t.tag_tsv @@ plainto_tsquery('simple', $1)
                    ORDER BY n.created_at_ms DESC
                    LIMIT $2
                    "#,
                            &[&tag_norm, &limit],
                        )?
                    }
                } else if !q_norm.is_empty() && since.is_some() {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE search_tsv @@ plainto_tsquery('simple', $1) AND created_at_ms > $2 ORDER BY created_at_ms DESC LIMIT $3",
                        &[&q_norm, &since.unwrap(), &limit],
                    )?
                } else if !q_norm.is_empty() && cursor.is_some() {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE search_tsv @@ plainto_tsquery('simple', $1) AND created_at_ms < $2 ORDER BY created_at_ms DESC LIMIT $3",
                        &[&q_norm, &cursor.unwrap(), &limit],
                    )?
                } else if !q_norm.is_empty() {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE search_tsv @@ plainto_tsquery('simple', $1) ORDER BY created_at_ms DESC LIMIT $2",
                        &[&q_norm, &limit],
                    )?
                } else if let Some(since) = since {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE created_at_ms > $1 ORDER BY created_at_ms DESC LIMIT $2",
                        &[&since, &limit],
                    )?
                } else if let Some(cur) = cursor {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE created_at_ms < $1 ORDER BY created_at_ms DESC LIMIT $2",
                        &[&cur, &limit],
                    )?
                } else {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes ORDER BY created_at_ms DESC LIMIT $1",
                        &[&limit],
                    )?
                };

                let mut items = Vec::<String>::new();
                let mut last_created = None;
                for row in rows {
                    let note_json: String = row.get(0);
                    let created_at_ms: i64 = row.get(1);
                    last_created = Some(created_at_ms);
                    items.push(note_json);
                }
                let next = if items.len() as i64 == limit {
                    last_created.map(|v| v.to_string())
                } else {
                    None
                };
                let total = match total_mode {
                    SearchTotalMode::Exact => total_exact,
                    SearchTotalMode::Approx => {
                        if !tag_norm.is_empty() {
                            let row = conn.query_one(
                                "SELECT COALESCE(SUM(count), 0) FROM relay_tag_counts WHERE lower(tag) LIKE $1",
                                &[&tag_like],
                            )?;
                            let n: i64 = row.get(0);
                            n.max(0) as u64
                        } else if q_norm.is_empty() {
                            let row = conn.query_one(
                                "SELECT count FROM relay_notes_count WHERE id = 1",
                                &[],
                            )?;
                            let n: i64 = row.get(0);
                            n.max(0) as u64
                        } else {
                            items.len() as u64
                        }
                    }
                    SearchTotalMode::None => 0,
                };
                Ok(CollectionPage { total, items, next })
            }
        }
    }

    fn list_relay_notes_sync(
        &self,
        limit: u32,
        since: Option<i64>,
        cursor: Option<i64>,
    ) -> Result<CollectionPage<(String, i64)>> {
        let limit = limit.min(200).max(1) as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt;
                let mut rows;
                if let Some(since) = since {
                    stmt = conn.prepare(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE created_at_ms > ?1 ORDER BY created_at_ms DESC LIMIT ?2",
                    )?;
                    rows = stmt.query(params![since, limit])?;
                } else if let Some(cur) = cursor {
                    stmt = conn.prepare(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE created_at_ms < ?1 ORDER BY created_at_ms DESC LIMIT ?2",
                    )?;
                    rows = stmt.query(params![cur, limit])?;
                } else {
                    stmt = conn.prepare(
                        "SELECT note_json, created_at_ms FROM relay_notes ORDER BY created_at_ms DESC LIMIT ?1",
                    )?;
                    rows = stmt.query(params![limit])?;
                }
                let mut items = Vec::<(String, i64)>::new();
                let mut last_created = None;
                while let Some(row) = rows.next()? {
                    let note_json: String = row.get(0)?;
                    let created_at_ms: i64 = row.get(1)?;
                    last_created = Some(created_at_ms);
                    items.push((note_json, created_at_ms));
                }
                let next = if items.len() as i64 == limit {
                    last_created.map(|v| v.to_string())
                } else {
                    None
                };
                Ok(CollectionPage {
                    total: items.len() as u64,
                    items,
                    next,
                })
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = if let Some(since) = since {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE created_at_ms > $1 ORDER BY created_at_ms DESC LIMIT $2",
                        &[&since, &limit],
                    )?
                } else if let Some(cur) = cursor {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE created_at_ms < $1 ORDER BY created_at_ms DESC LIMIT $2",
                        &[&cur, &limit],
                    )?
                } else {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes ORDER BY created_at_ms DESC LIMIT $1",
                        &[&limit],
                    )?
                };
                let mut items = Vec::<(String, i64)>::new();
                let mut last_created = None;
                for row in rows {
                    let note_json: String = row.get(0);
                    let created_at_ms: i64 = row.get(1);
                    last_created = Some(created_at_ms);
                    items.push((note_json, created_at_ms));
                }
                let next = if items.len() as i64 == limit {
                    last_created.map(|v| v.to_string())
                } else {
                    None
                };
                Ok(CollectionPage {
                    total: items.len() as u64,
                    items,
                    next,
                })
            }
        }
    }

    fn list_relay_media_sync(
        &self,
        limit: u32,
        since: Option<i64>,
        cursor: Option<i64>,
    ) -> Result<CollectionPage<RelayMediaIndex>> {
        let limit = limit.min(200).max(1) as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt;
                let mut rows;
                if let Some(since) = since {
                    stmt = conn.prepare(
                        "SELECT media_url, media_type, name, width, height, blurhash, created_at_ms FROM relay_media WHERE created_at_ms > ?1 ORDER BY created_at_ms DESC LIMIT ?2",
                    )?;
                    rows = stmt.query(params![since, limit])?;
                } else if let Some(cur) = cursor {
                    stmt = conn.prepare(
                        "SELECT media_url, media_type, name, width, height, blurhash, created_at_ms FROM relay_media WHERE created_at_ms < ?1 ORDER BY created_at_ms DESC LIMIT ?2",
                    )?;
                    rows = stmt.query(params![cur, limit])?;
                } else {
                    stmt = conn.prepare(
                        "SELECT media_url, media_type, name, width, height, blurhash, created_at_ms FROM relay_media ORDER BY created_at_ms DESC LIMIT ?1",
                    )?;
                    rows = stmt.query(params![limit])?;
                }
                let mut items = Vec::new();
                let mut last_created = None;
                while let Some(row) = rows.next()? {
                    let created_at_ms: i64 = row.get(6)?;
                    last_created = Some(created_at_ms);
                    items.push(RelayMediaIndex {
                        url: row.get(0)?,
                        media_type: row.get(1)?,
                        name: row.get(2)?,
                        width: row.get(3)?,
                        height: row.get(4)?,
                        blurhash: row.get(5)?,
                        created_at_ms,
                    });
                }
                let next = if items.len() as i64 == limit {
                    last_created.map(|v| v.to_string())
                } else {
                    None
                };
                Ok(CollectionPage {
                    total: items.len() as u64,
                    items,
                    next,
                })
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = if let Some(since) = since {
                    conn.query(
                        "SELECT media_url, media_type, name, width, height, blurhash, created_at_ms FROM relay_media WHERE created_at_ms > $1 ORDER BY created_at_ms DESC LIMIT $2",
                        &[&since, &limit],
                    )?
                } else if let Some(cur) = cursor {
                    conn.query(
                        "SELECT media_url, media_type, name, width, height, blurhash, created_at_ms FROM relay_media WHERE created_at_ms < $1 ORDER BY created_at_ms DESC LIMIT $2",
                        &[&cur, &limit],
                    )?
                } else {
                    conn.query(
                        "SELECT media_url, media_type, name, width, height, blurhash, created_at_ms FROM relay_media ORDER BY created_at_ms DESC LIMIT $1",
                        &[&limit],
                    )?
                };
                let mut items = Vec::new();
                let mut last_created = None;
                for row in rows {
                    let created_at_ms: i64 = row.get(6);
                    last_created = Some(created_at_ms);
                    items.push(RelayMediaIndex {
                        url: row.get(0),
                        media_type: row.get(1),
                        name: row.get(2),
                        width: row.get(3),
                        height: row.get(4),
                        blurhash: row.get(5),
                        created_at_ms,
                    });
                }
                let next = if items.len() as i64 == limit {
                    last_created.map(|v| v.to_string())
                } else {
                    None
                };
                Ok(CollectionPage {
                    total: items.len() as u64,
                    items,
                    next,
                })
            }
        }
    }

    fn list_relay_actor_sync(
        &self,
        limit: u32,
        since: Option<i64>,
        cursor: Option<i64>,
    ) -> Result<CollectionPage<RelayActorIndex>> {
        let limit = limit.min(200).max(1) as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt;
                let mut rows;
                if let Some(since) = since {
                    stmt = conn.prepare(
                        "SELECT actor_url, username, actor_json, updated_at_ms FROM relay_actors WHERE updated_at_ms > ?1 ORDER BY updated_at_ms DESC LIMIT ?2",
                    )?;
                    rows = stmt.query(params![since, limit])?;
                } else if let Some(cur) = cursor {
                    stmt = conn.prepare(
                        "SELECT actor_url, username, actor_json, updated_at_ms FROM relay_actors WHERE updated_at_ms < ?1 ORDER BY updated_at_ms DESC LIMIT ?2",
                    )?;
                    rows = stmt.query(params![cur, limit])?;
                } else {
                    stmt = conn.prepare(
                        "SELECT actor_url, username, actor_json, updated_at_ms FROM relay_actors ORDER BY updated_at_ms DESC LIMIT ?1",
                    )?;
                    rows = stmt.query(params![limit])?;
                }
                let mut items = Vec::new();
                let mut last_updated = None;
                while let Some(row) = rows.next()? {
                    let updated_at_ms: i64 = row.get(3)?;
                    last_updated = Some(updated_at_ms);
                    items.push(RelayActorIndex {
                        actor_url: row.get(0)?,
                        username: row.get(1)?,
                        actor_json: row.get(2)?,
                        updated_at_ms,
                    });
                }
                let next = if items.len() as i64 == limit {
                    last_updated.map(|v| v.to_string())
                } else {
                    None
                };
                Ok(CollectionPage {
                    total: items.len() as u64,
                    items,
                    next,
                })
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = if let Some(since) = since {
                    conn.query(
                        "SELECT actor_url, username, actor_json, updated_at_ms FROM relay_actors WHERE updated_at_ms > $1 ORDER BY updated_at_ms DESC LIMIT $2",
                        &[&since, &limit],
                    )?
                } else if let Some(cur) = cursor {
                    conn.query(
                        "SELECT actor_url, username, actor_json, updated_at_ms FROM relay_actors WHERE updated_at_ms < $1 ORDER BY updated_at_ms DESC LIMIT $2",
                        &[&cur, &limit],
                    )?
                } else {
                    conn.query(
                        "SELECT actor_url, username, actor_json, updated_at_ms FROM relay_actors ORDER BY updated_at_ms DESC LIMIT $1",
                        &[&limit],
                    )?
                };
                let mut items = Vec::new();
                let mut last_updated = None;
                for row in rows {
                    let updated_at_ms: i64 = row.get(3);
                    last_updated = Some(updated_at_ms);
                    items.push(RelayActorIndex {
                        actor_url: row.get(0),
                        username: row.get(1),
                        actor_json: row.get(2),
                        updated_at_ms,
                    });
                }
                let next = if items.len() as i64 == limit {
                    last_updated.map(|v| v.to_string())
                } else {
                    None
                };
                Ok(CollectionPage {
                    total: items.len() as u64,
                    items,
                    next,
                })
            }
        }
    }

    fn search_relay_users(
        &self,
        q: &str,
        limit: u32,
        cursor: Option<i64>,
        base_template: &str,
        total_mode: SearchTotalMode,
    ) -> Result<CollectionPage<String>> {
        let limit = limit.min(200).max(1) as i64;
        let q_norm = q.trim().to_lowercase();
        let q_like = if q_norm.is_empty() {
            "%".to_string()
        } else {
            format!("%{}%", escape_like(&q_norm))
        };
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let total_exact: u64 = if total_mode == SearchTotalMode::Exact {
                    let total_cache: u64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM user_cache WHERE lower(username) LIKE ?1 OR lower(actor_id) LIKE ?1 OR lower(actor_url) LIKE ?1",
                            params![q_like],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    let total_users: u64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM users WHERE disabled=0 AND lower(username) LIKE ?1",
                            params![q_like],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    let total_peers: u64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM peer_directory WHERE lower(username) LIKE ?1 OR lower(actor_url) LIKE ?1",
                            params![q_like],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    let total_relay_users: u64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM relay_user_directory WHERE lower(username) LIKE ?1 OR lower(actor_url) LIKE ?1",
                            params![q_like],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    total_cache
                        .saturating_add(total_users)
                        .saturating_add(total_peers)
                        .saturating_add(total_relay_users)
                } else {
                    0
                };

                let mut stmt = if cursor.is_some() {
                    conn.prepare(
                        "SELECT actor_json, updated_at_ms FROM user_cache WHERE (lower(username) LIKE ?1 OR lower(actor_id) LIKE ?1 OR lower(actor_url) LIKE ?1) AND updated_at_ms < ?2 ORDER BY updated_at_ms DESC LIMIT ?3",
                    )?
                } else {
                    conn.prepare(
                        "SELECT actor_json, updated_at_ms FROM user_cache WHERE lower(username) LIKE ?1 OR lower(actor_id) LIKE ?1 OR lower(actor_url) LIKE ?1 ORDER BY updated_at_ms DESC LIMIT ?2",
                    )?
                };
                let mut rows = if let Some(cur) = cursor {
                    stmt.query(params![q_like, cur, limit])?
                } else {
                    stmt.query(params![q_like, limit])?
                };
                let mut items = Vec::<String>::new();
                let mut seen_ids = std::collections::HashSet::new();
                let mut last_updated = None;
                while let Some(row) = rows.next()? {
                    let actor_json: String = row.get(0)?;
                    let updated_at_ms: i64 = row.get(1)?;
                    last_updated = Some(updated_at_ms);
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                        if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                            if seen_ids.insert(id.to_string()) {
                                items.push(actor_json);
                            }
                        } else {
                            items.push(actor_json);
                        }
                    }
                }

                let mut stmt_users = conn.prepare(
                    "SELECT username, created_at_ms FROM users WHERE disabled=0 AND lower(username) LIKE ?1 ORDER BY created_at_ms DESC LIMIT ?2",
                )?;
                let mut user_rows = stmt_users.query(params![q_like, limit])?;
                while let Some(row) = user_rows.next()? {
                    let username: String = row.get(0)?;
                    if let Some(actor_json) = self.get_actor_cache(&username).unwrap_or(None) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                            if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                                if seen_ids.insert(id.to_string()) {
                                    items.push(actor_json);
                                }
                                continue;
                            }
                        }
                    }
                    let stub = actor_stub_json(&username, base_template);
                    let actor_json = serde_json::to_string(&stub).unwrap_or_default();
                    if let Some(id) = stub.get("id").and_then(|i| i.as_str()) {
                        if seen_ids.insert(id.to_string()) {
                            items.push(actor_json);
                        }
                    }
                }

                let mut stmt_peers = conn.prepare(
                    "SELECT username, actor_url FROM peer_directory WHERE lower(username) LIKE ?1 OR lower(actor_url) LIKE ?1 ORDER BY updated_at_ms DESC LIMIT ?2",
                )?;
                let mut peer_rows = stmt_peers.query(params![q_like, limit])?;
                while let Some(row) = peer_rows.next()? {
                    let username: String = row.get(0)?;
                    let actor_url: String = row.get(1)?;
                    if let Some(actor_json) = self.get_actor_cache(&username).unwrap_or(None) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                            if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                                if seen_ids.insert(id.to_string()) {
                                    items.push(actor_json);
                                }
                                continue;
                            }
                        }
                    }
                    let stub = actor_stub_from_actor_url(&username, &actor_url, base_template);
                    let actor_json = serde_json::to_string(&stub).unwrap_or_default();
                    if let Some(id) = stub.get("id").and_then(|i| i.as_str()) {
                        if seen_ids.insert(id.to_string()) {
                            items.push(actor_json);
                        }
                    }
                }

                let mut stmt_remote = conn.prepare(
                    "SELECT username, actor_url FROM relay_user_directory WHERE lower(username) LIKE ?1 OR lower(actor_url) LIKE ?1 ORDER BY updated_at_ms DESC LIMIT ?2",
                )?;
                let mut remote_rows = stmt_remote.query(params![q_like, limit])?;
                while let Some(row) = remote_rows.next()? {
                    let username: String = row.get(0)?;
                    let actor_url: String = row.get(1)?;
                    if let Some(actor_json) = self.get_actor_cache(&username).unwrap_or(None) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                            if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                                if seen_ids.insert(id.to_string()) {
                                    items.push(actor_json);
                                }
                                continue;
                            }
                        }
                    }
                    let stub = actor_stub_from_actor_url(&username, &actor_url, base_template);
                    let actor_json = serde_json::to_string(&stub).unwrap_or_default();
                    if let Some(id) = stub.get("id").and_then(|i| i.as_str()) {
                        if seen_ids.insert(id.to_string()) {
                            items.push(actor_json);
                        }
                    }
                }

                let next = if items.len() as i64 == limit {
                    last_updated.map(|v| v.to_string())
                } else {
                    None
                };
                let total = match total_mode {
                    SearchTotalMode::Exact => total_exact,
                    SearchTotalMode::Approx => items.len() as u64,
                    SearchTotalMode::None => 0,
                };
                Ok(CollectionPage { total, items, next })
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let total_exact: u64 = if total_mode == SearchTotalMode::Exact {
                    let total_cache: u64 = {
                        let row = conn.query_one(
                            "SELECT COUNT(*) FROM user_cache WHERE lower(username) LIKE $1 OR lower(actor_id) LIKE $1 OR lower(actor_url) LIKE $1",
                            &[&q_like],
                        )?;
                        let n: i64 = row.get(0);
                        n.max(0) as u64
                    };
                    let total_users: u64 = {
                        let row = conn.query_one(
                            "SELECT COUNT(*) FROM users WHERE disabled=false AND lower(username) LIKE $1",
                            &[&q_like],
                        )?;
                        let n: i64 = row.get(0);
                        n.max(0) as u64
                    };
                    let total_peers: u64 = {
                        let row = conn.query_one(
                            "SELECT COUNT(*) FROM peer_directory WHERE lower(username) LIKE $1 OR lower(actor_url) LIKE $1",
                            &[&q_like],
                        )?;
                        let n: i64 = row.get(0);
                        n.max(0) as u64
                    };
                    let total_relay_users: u64 = {
                        let row = conn.query_one(
                            "SELECT COUNT(*) FROM relay_user_directory WHERE lower(username) LIKE $1 OR lower(actor_url) LIKE $1",
                            &[&q_like],
                        )?;
                        let n: i64 = row.get(0);
                        n.max(0) as u64
                    };
                    total_cache
                        .saturating_add(total_users)
                        .saturating_add(total_peers)
                        .saturating_add(total_relay_users)
                } else {
                    0
                };

                let rows = if let Some(cur) = cursor {
                    conn.query(
                        "SELECT actor_json, updated_at_ms FROM user_cache WHERE (lower(username) LIKE $1 OR lower(actor_id) LIKE $1 OR lower(actor_url) LIKE $1) AND updated_at_ms < $2 ORDER BY updated_at_ms DESC LIMIT $3",
                        &[&q_like, &cur, &limit],
                    )?
                } else {
                    conn.query(
                        "SELECT actor_json, updated_at_ms FROM user_cache WHERE lower(username) LIKE $1 OR lower(actor_id) LIKE $1 OR lower(actor_url) LIKE $1 ORDER BY updated_at_ms DESC LIMIT $2",
                        &[&q_like, &limit],
                    )?
                };
                let mut items = Vec::<String>::new();
                let mut seen_ids = std::collections::HashSet::new();
                let mut last_updated = None;
                for row in rows {
                    let actor_json: String = row.get(0);
                    let updated_at_ms: i64 = row.get(1);
                    last_updated = Some(updated_at_ms);
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                        if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                            if seen_ids.insert(id.to_string()) {
                                items.push(actor_json);
                            }
                        } else {
                            items.push(actor_json);
                        }
                    }
                }

                let user_rows = conn.query(
                    "SELECT username, created_at_ms FROM users WHERE disabled=false AND lower(username) LIKE $1 ORDER BY created_at_ms DESC LIMIT $2",
                    &[&q_like, &limit],
                )?;
                for row in user_rows {
                    let username: String = row.get(0);
                    if let Some(actor_json) = self.get_actor_cache(&username).unwrap_or(None) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                            if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                                if seen_ids.insert(id.to_string()) {
                                    items.push(actor_json);
                                }
                                continue;
                            }
                        }
                    }
                    let stub = actor_stub_json(&username, base_template);
                    let actor_json = serde_json::to_string(&stub).unwrap_or_default();
                    if let Some(id) = stub.get("id").and_then(|i| i.as_str()) {
                        if seen_ids.insert(id.to_string()) {
                            items.push(actor_json);
                        }
                    }
                }

                let peer_rows = conn.query(
                    "SELECT username, actor_url FROM peer_directory WHERE lower(username) LIKE $1 OR lower(actor_url) LIKE $1 ORDER BY updated_at_ms DESC LIMIT $2",
                    &[&q_like, &limit],
                )?;
                for row in peer_rows {
                    let username: String = row.get(0);
                    let actor_url: String = row.get(1);
                    if let Some(actor_json) = self.get_actor_cache(&username).unwrap_or(None) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                            if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                                if seen_ids.insert(id.to_string()) {
                                    items.push(actor_json);
                                }
                                continue;
                            }
                        }
                    }
                    let stub = actor_stub_from_actor_url(&username, &actor_url, base_template);
                    let actor_json = serde_json::to_string(&stub).unwrap_or_default();
                    if let Some(id) = stub.get("id").and_then(|i| i.as_str()) {
                        if seen_ids.insert(id.to_string()) {
                            items.push(actor_json);
                        }
                    }
                }

                let remote_rows = conn.query(
                    "SELECT username, actor_url FROM relay_user_directory WHERE lower(username) LIKE $1 OR lower(actor_url) LIKE $1 ORDER BY updated_at_ms DESC LIMIT $2",
                    &[&q_like, &limit],
                )?;
                for row in remote_rows {
                    let username: String = row.get(0);
                    let actor_url: String = row.get(1);
                    if let Some(actor_json) = self.get_actor_cache(&username).unwrap_or(None) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                            if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                                if seen_ids.insert(id.to_string()) {
                                    items.push(actor_json);
                                }
                                continue;
                            }
                        }
                    }
                    let stub = actor_stub_from_actor_url(&username, &actor_url, base_template);
                    let actor_json = serde_json::to_string(&stub).unwrap_or_default();
                    if let Some(id) = stub.get("id").and_then(|i| i.as_str()) {
                        if seen_ids.insert(id.to_string()) {
                            items.push(actor_json);
                        }
                    }
                }

                let next = if items.len() as i64 == limit {
                    last_updated.map(|v| v.to_string())
                } else {
                    None
                };
                let total = match total_mode {
                    SearchTotalMode::Exact => total_exact,
                    SearchTotalMode::Approx => items.len() as u64,
                    SearchTotalMode::None => 0,
                };
                Ok(CollectionPage { total, items, next })
            }
        }
    }

    fn search_relay_tags(&self, q: &str, limit: u32) -> Result<Vec<(String, u64)>> {
        let limit = limit.min(200).max(1) as i64;
        let q_norm = q.trim().trim_start_matches('#').to_lowercase();
        let q_like = if q_norm.is_empty() {
            "%".to_string()
        } else {
            format!("%{}%", escape_like(&q_norm))
        };
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    r#"
            SELECT tag, count
            FROM relay_tag_counts
            WHERE lower(tag) LIKE ?1
            ORDER BY count DESC
            LIMIT ?2
            "#,
                )?;
                let mut rows = stmt.query(params![q_like, limit])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    let tag: String = row.get(0)?;
                    let count: u64 = row.get(1)?;
                    out.push((tag, count));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    r#"
            SELECT tag, count
            FROM relay_tag_counts
            WHERE lower(tag) LIKE $1
            ORDER BY count DESC
            LIMIT $2
            "#,
                    &[&q_like, &limit],
                )?;
                let mut out = Vec::new();
                for row in rows {
                    let tag: String = row.get(0);
                    let count: i64 = row.get(1);
                    out.push((tag, count.max(0) as u64));
                }
                Ok(out)
            }
        }
    }

    fn upsert_peer_directory(&self, peer_id: &str, username: &str, actor_url: &str) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO peer_directory(peer_id, username, actor_url, updated_at_ms) VALUES (?1, ?2, ?3, ?4)\n             ON CONFLICT(peer_id) DO UPDATE SET username=excluded.username, actor_url=excluded.actor_url, updated_at_ms=excluded.updated_at_ms",
                    params![peer_id, username, actor_url, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO peer_directory(peer_id, username, actor_url, updated_at_ms) VALUES ($1, $2, $3, $4)\n             ON CONFLICT(peer_id) DO UPDATE SET username=EXCLUDED.username, actor_url=EXCLUDED.actor_url, updated_at_ms=EXCLUDED.updated_at_ms",
                    &[&peer_id, &username, &actor_url, &now],
                )?;
                Ok(())
            }
        }
    }

    fn list_peer_directory(
        &self,
        q: &str,
        limit: u32,
        cutoff_ms: Option<i64>,
    ) -> Result<Vec<(String, String, String)>> {
        let limit = limit.min(200).max(1) as i64;
        let q_norm = q.trim().to_lowercase();
        let q_like = if q_norm.is_empty() {
            "%".to_string()
        } else {
            format!("%{}%", escape_like(&q_norm))
        };
        let cutoff_ms = cutoff_ms.unwrap_or(0);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt;
                let mut rows;
                if cutoff_ms > 0 {
                    stmt = conn.prepare(
                        "SELECT peer_id, username, actor_url FROM peer_directory WHERE (lower(username) LIKE ?1 OR lower(actor_url) LIKE ?1) AND updated_at_ms >= ?3 ORDER BY updated_at_ms DESC LIMIT ?2",
                    )?;
                    rows = stmt.query(params![q_like, limit, cutoff_ms])?;
                } else {
                    stmt = conn.prepare(
                        "SELECT peer_id, username, actor_url FROM peer_directory WHERE lower(username) LIKE ?1 OR lower(actor_url) LIKE ?1 ORDER BY updated_at_ms DESC LIMIT ?2",
                    )?;
                    rows = stmt.query(params![q_like, limit])?;
                }
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    out.push((row.get(0)?, row.get(1)?, row.get(2)?));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows;
                if cutoff_ms > 0 {
                    rows = conn.query(
                        "SELECT peer_id, username, actor_url FROM peer_directory WHERE (lower(username) LIKE $1 OR lower(actor_url) LIKE $1) AND updated_at_ms >= $3 ORDER BY updated_at_ms DESC LIMIT $2",
                        &[&q_like, &limit, &cutoff_ms],
                    )?;
                } else {
                    rows = conn.query(
                        "SELECT peer_id, username, actor_url FROM peer_directory WHERE lower(username) LIKE $1 OR lower(actor_url) LIKE $1 ORDER BY updated_at_ms DESC LIMIT $2",
                        &[&q_like, &limit],
                    )?;
                }
                let mut out = Vec::new();
                for row in rows {
                    out.push((row.get(0), row.get(1), row.get(2)));
                }
                Ok(out)
            }
        }
    }

    fn delete_peer_directory_entry(&self, peer_id: &str) -> Result<u64> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM peer_directory WHERE peer_id = ?1",
                    params![peer_id],
                )?;
                Ok(deleted as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let deleted =
                    conn.execute("DELETE FROM peer_directory WHERE peer_id = $1", &[&peer_id])?;
                Ok(deleted)
            }
        }
    }

    fn upsert_relay_user_directory(
        &self,
        username: &str,
        actor_url: &str,
        relay_url: &str,
    ) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO relay_user_directory(actor_url, username, relay_url, updated_at_ms) VALUES (?1, ?2, ?3, ?4)\n             ON CONFLICT(actor_url) DO UPDATE SET username=excluded.username, relay_url=excluded.relay_url, updated_at_ms=excluded.updated_at_ms",
                    params![actor_url, username, relay_url, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO relay_user_directory(actor_url, username, relay_url, updated_at_ms) VALUES ($1, $2, $3, $4)\n             ON CONFLICT(actor_url) DO UPDATE SET username=EXCLUDED.username, relay_url=EXCLUDED.relay_url, updated_at_ms=EXCLUDED.updated_at_ms",
                    &[&actor_url, &username, &relay_url, &now],
                )?;
                Ok(())
            }
        }
    }

    fn upsert_outbox_index_state(&self, username: &str, ok: bool) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO relay_outbox_index(username, last_index_ms, last_ok) VALUES (?1, ?2, ?3)\n             ON CONFLICT(username) DO UPDATE SET last_index_ms=excluded.last_index_ms, last_ok=excluded.last_ok",
                    params![username, now, if ok { 1 } else { 0 }],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO relay_outbox_index(username, last_index_ms, last_ok) VALUES ($1, $2, $3)\n             ON CONFLICT(username) DO UPDATE SET last_index_ms=EXCLUDED.last_index_ms, last_ok=EXCLUDED.last_ok",
                    &[&username, &now, &ok],
                )?;
                Ok(())
            }
        }
    }

    fn count_outbox_indexed_since(&self, cutoff_ms: i64) -> Result<u64> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT COUNT(*) FROM relay_outbox_index WHERE last_index_ms >= ?1 AND last_ok=1",
                    params![cutoff_ms],
                    |r| r.get(0),
                )
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_one(
                    "SELECT COUNT(*) FROM relay_outbox_index WHERE last_index_ms >= $1 AND last_ok=true",
                    &[&cutoff_ms],
                )?;
                let n: i64 = row.get(0);
                Ok(n.max(0) as u64)
            }
        }
    }

    fn get_outbox_index_state(&self, username: &str) -> Result<Option<(i64, bool)>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT last_index_ms, last_ok FROM relay_outbox_index WHERE username=?1",
                    params![username],
                    |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0)),
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT last_index_ms, last_ok FROM relay_outbox_index WHERE username=$1",
                    &[&username],
                )?;
                Ok(row.map(|r| (r.get(0), r.get(1))))
            }
        }
    }

}

enum UpsertUserResult {
    Created,
    Exists,
    Updated,
    Unauthorized,
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let v = headers
        .get("Authorization")?
        .to_str()
        .ok()?
        .trim()
        .to_string();
    let v = v
        .strip_prefix("Bearer ")
        .or_else(|| v.strip_prefix("bearer "))?;
    let v = v.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

struct RateLimiter {
    inner: Mutex<HashMap<String, WindowCounter>>,
    noisy: Mutex<HashMap<String, NoisyState>>,
    noisy_backoff_base_secs: u64,
    noisy_backoff_max_secs: u64,
    redis: Option<Vec<Mutex<ConnectionManager>>>,
    redis_index: AtomicUsize,
    redis_prefix: String,
}

#[derive(Clone, Copy)]
struct WindowCounter {
    window_start_ms: i64,
    count: u32,
}

#[derive(Clone, Copy)]
struct NoisyState {
    strikes: u32,
    blocked_until_ms: i64,
    last_hit_ms: i64,
}

impl RateLimiter {
    fn redis_handle(&self) -> Option<&Mutex<ConnectionManager>> {
        let redis = self.redis.as_ref()?;
        let idx = self.redis_index.fetch_add(1, Ordering::Relaxed) % redis.len();
        redis.get(idx)
    }

    async fn new(
        noisy_backoff_base_secs: u64,
        noisy_backoff_max_secs: u64,
        redis_url: Option<String>,
        redis_prefix: String,
        redis_pool_size: usize,
    ) -> Self {
        let redis = match redis_url {
            Some(url) => {
                let client = match redis::Client::open(url.as_str()) {
                    Ok(client) => client,
                    Err(e) => {
                        error!("redis init failed: {e}");
                        return Self {
                            inner: Mutex::new(HashMap::new()),
                            noisy: Mutex::new(HashMap::new()),
                            noisy_backoff_base_secs,
                            noisy_backoff_max_secs,
                            redis: None,
                            redis_index: AtomicUsize::new(0),
                            redis_prefix,
                        };
                    }
                };
                let mut conns = Vec::with_capacity(redis_pool_size.max(1));
                for _ in 0..redis_pool_size.max(1) {
                    match ConnectionManager::new(client.clone()).await {
                        Ok(conn) => conns.push(Mutex::new(conn)),
                        Err(e) => error!("redis init failed: {e}"),
                    }
                }
                if conns.is_empty() {
                    None
                } else {
                    Some(conns)
                }
            }
            None => None,
        };
        Self {
            inner: Mutex::new(HashMap::new()),
            noisy: Mutex::new(HashMap::new()),
            noisy_backoff_base_secs,
            noisy_backoff_max_secs,
            redis,
            redis_index: AtomicUsize::new(0),
            redis_prefix,
        }
    }

    async fn check(&self, ip: String, bucket: &str, per_minute: u32) -> bool {
        self.check_weighted(ip, bucket, per_minute, 1).await
    }

    async fn check_weighted(&self, ip: String, bucket: &str, per_minute: u32, weight: u32) -> bool {
        if let Some(_) = self.noisy_block_remaining(&ip).await {
            return false;
        }
        if let Some(ok) = self
            .redis_check_weighted(&ip, bucket, per_minute, weight)
            .await
        {
            if !ok {
                let now = now_ms();
                self.register_noisy(&ip, now).await;
            }
            return ok;
        }
        let key = format!("{bucket}:{ip}");
        let mut map = self.inner.lock().await;
        let now = now_ms();

        // Opportunistic cleanup to bound memory: prune entries inactive for >2 minutes.
        if map.len() > 10_000 {
            let cutoff = now - 120_000;
            map.retain(|_, v| v.window_start_ms >= cutoff);
        }

        let win = map.entry(key).or_insert(WindowCounter {
            window_start_ms: now,
            count: 0,
        });
        if now - win.window_start_ms > 60_000 {
            win.window_start_ms = now;
            win.count = 0;
        }
        let weight = weight.max(1);
        if win.count.saturating_add(weight) > per_minute {
            drop(map);
            self.register_noisy(&ip, now).await;
            return false;
        }
        win.count = win.count.saturating_add(weight);
        true
    }

    async fn noisy_block_remaining(&self, ip: &str) -> Option<u64> {
        if self.noisy_backoff_base_secs == 0 {
            return None;
        }
        if let Some(ttl) = self.redis_noisy_remaining(ip).await {
            return Some(ttl);
        }
        let now = now_ms();
        let mut noisy = self.noisy.lock().await;
        if noisy.len() > 10_000 {
            let cutoff = now - 24 * 3600 * 1000;
            noisy.retain(|_, v| v.last_hit_ms >= cutoff);
        }
        let state = noisy.get(ip)?;
        if state.blocked_until_ms <= now {
            return None;
        }
        let remaining_ms = state.blocked_until_ms.saturating_sub(now);
        Some(((remaining_ms as u64) + 999) / 1000)
    }

    async fn register_noisy(&self, ip: &str, now: i64) {
        if self.noisy_backoff_base_secs == 0 {
            return;
        }
        if self.redis.is_some() {
            let _ = self.redis_register_noisy(ip).await;
        }
        let mut noisy = self.noisy.lock().await;
        let entry = noisy.entry(ip.to_string()).or_insert(NoisyState {
            strikes: 0,
            blocked_until_ms: 0,
            last_hit_ms: now,
        });
        if now - entry.last_hit_ms > 10 * 60 * 1000 {
            entry.strikes = 0;
        }
        entry.strikes = entry.strikes.saturating_add(1);
        let shift = entry.strikes.saturating_sub(1).min(10);
        let base = self.noisy_backoff_base_secs.saturating_mul(1u64 << shift);
        let backoff = base.min(
            self.noisy_backoff_max_secs
                .max(self.noisy_backoff_base_secs),
        );
        entry.blocked_until_ms = now + (backoff as i64).saturating_mul(1000);
        entry.last_hit_ms = now;
    }

    async fn redis_check_weighted(
        &self,
        ip: &str,
        bucket: &str,
        per_minute: u32,
        weight: u32,
    ) -> Option<bool> {
        let Some(redis) = self.redis_handle() else {
            return None;
        };
        let key = format!(
            "{}:rl:{}:{}:{}",
            self.redis_prefix,
            bucket,
            ip,
            now_ms() / 60_000
        );
        let script = r#"
            local v = redis.call("INCRBY", KEYS[1], ARGV[1])
            if v == tonumber(ARGV[1]) then
              redis.call("EXPIRE", KEYS[1], 60)
            end
            return v
        "#;
        let mut conn = redis.lock().await;
        let res: redis::RedisResult<i64> = redis::Script::new(script)
            .key(key)
            .arg(weight.max(1) as i64)
            .invoke_async(&mut *conn)
            .await;
        match res {
            Ok(v) => Some(v <= per_minute as i64),
            Err(e) => {
                error!("redis rate limit error: {e}");
                None
            }
        }
    }

    async fn redis_noisy_remaining(&self, ip: &str) -> Option<u64> {
        let Some(redis) = self.redis_handle() else {
            return None;
        };
        let key = format!("{}:noisy:block:{}", self.redis_prefix, ip);
        let mut conn = redis.lock().await;
        let ttl: redis::RedisResult<i64> = conn.ttl(key).await;
        match ttl {
            Ok(v) if v > 0 => Some(v as u64),
            _ => None,
        }
    }

    async fn redis_register_noisy(&self, ip: &str) -> Option<()> {
        let Some(redis) = self.redis_handle() else {
            return None;
        };
        let base = self.noisy_backoff_base_secs.max(1);
        let max = self.noisy_backoff_max_secs.max(base);
        let mut conn = redis.lock().await;
        let strikes_key = format!("{}:noisy:strikes:{}", self.redis_prefix, ip);
        let block_key = format!("{}:noisy:block:{}", self.redis_prefix, ip);
        let strikes: i64 = conn.incr(&strikes_key, 1).await.ok()?;
        let _: redis::RedisResult<i64> = conn.expire(&strikes_key, 600).await;
        let shift = (strikes - 1).clamp(0, 10) as u32;
        let backoff = base.saturating_mul(1u64 << shift).min(max);
        let _: redis::RedisResult<()> = conn.set_ex(&block_key, 1, backoff as u64).await;
        Some(())
    }
}

fn peer_ip(peer: &SocketAddr) -> String {
    peer.ip().to_string()
}

fn client_ip(cfg: &RelayConfig, peer: &SocketAddr, headers: &HeaderMap) -> String {
    if !cfg.trust_proxy_headers {
        return peer_ip(peer);
    }

    // Only safe when a trusted reverse proxy is deployed in front and overwrites these headers.
    if let Some(v) = headers.get("X-Real-IP").and_then(|v| v.to_str().ok()) {
        if let Some(ip) = parse_ip_str(v) {
            return ip;
        }
    }
    if let Some(v) = headers.get("Forwarded").and_then(|v| v.to_str().ok()) {
        if let Some(ip) = parse_forwarded_for_ip(v) {
            return ip;
        }
    }
    if let Some(v) = headers.get("X-Forwarded-For").and_then(|v| v.to_str().ok()) {
        if let Some(ip) = v.split(',').map(|s| s.trim()).find_map(parse_ip_str) {
            return ip;
        }
    }

    peer_ip(peer)
}

fn client_ip_addr(cfg: &RelayConfig, peer: &SocketAddr, headers: &HeaderMap) -> IpAddr {
    client_ip(cfg, peer, headers)
        .parse()
        .unwrap_or_else(|_| peer.ip())
}

fn is_ip_allowed(cfg: &RelayConfig, ip: IpAddr) -> bool {
    if ip_in_rules(&cfg.ip_denylist, ip) {
        return false;
    }
    if !cfg.ip_allowlist.is_empty() && !ip_in_rules(&cfg.ip_allowlist, ip) {
        return false;
    }
    true
}

fn parse_ip_str(s: &str) -> Option<String> {
    let s = s.trim().trim_matches('"');
    let s = s.trim_start_matches('[').trim_end_matches(']');
    let ip_part = s.split(':').next().unwrap_or("").trim();
    let ip: std::net::IpAddr = ip_part.parse().ok()?;
    Some(ip.to_string())
}

#[derive(Debug, Clone)]
enum IpRule {
    Single(IpAddr),
    Cidr(IpAddr, u8),
}

fn parse_ip_rules(env: Option<String>) -> Vec<IpRule> {
    let Some(raw) = env else {
        return Vec::new();
    };
    raw.split(|c| c == ',' || c == ' ' || c == '\n' || c == '\t')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .filter_map(parse_ip_rule)
        .collect()
}

fn parse_ip_rule(s: &str) -> Option<IpRule> {
    let s = s.trim();
    if let Some((addr, prefix)) = s.split_once('/') {
        let ip: IpAddr = addr.trim().parse().ok()?;
        let prefix: u8 = prefix.trim().parse().ok()?;
        return Some(IpRule::Cidr(ip, prefix));
    }
    let ip: IpAddr = s.parse().ok()?;
    Some(IpRule::Single(ip))
}

fn ip_in_rules(rules: &[IpRule], ip: IpAddr) -> bool {
    rules.iter().any(|rule| match rule {
        IpRule::Single(addr) => *addr == ip,
        IpRule::Cidr(addr, prefix) => ip_in_cidr(ip, *addr, *prefix),
    })
}

fn ip_in_cidr(ip: IpAddr, base: IpAddr, prefix: u8) -> bool {
    match (ip, base) {
        (IpAddr::V4(ip), IpAddr::V4(base)) => {
            let prefix = prefix.min(32);
            let mask = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - prefix)
            };
            (u32::from(ip) & mask) == (u32::from(base) & mask)
        }
        (IpAddr::V6(ip), IpAddr::V6(base)) => {
            let prefix = prefix.min(128);
            let mask = if prefix == 0 {
                0
            } else {
                u128::MAX << (128 - prefix)
            };
            (u128::from(ip) & mask) == (u128::from(base) & mask)
        }
        _ => false,
    }
}

fn parse_forwarded_for_ip(forwarded: &str) -> Option<String> {
    // Forwarded: for=...;proto=https;host=...
    // Values may be quoted and may include IPv6 in [].
    for part in forwarded.split(';') {
        let part = part.trim();
        let lower = part.to_ascii_lowercase();
        if !lower.starts_with("for=") {
            continue;
        }
        let raw = part.splitn(2, '=').nth(1)?.trim();
        return parse_ip_str(raw);
    }
    None
}

async fn admin_guard(
    state: &AppState,
    peer: &SocketAddr,
    headers: &HeaderMap,
    action: &str,
    username: Option<&str>,
) -> std::result::Result<AdminAuditContext, Response> {
    let ip = client_ip(&state.cfg, peer, headers);
    let meta = audit_meta_from_headers(headers);
    if !state
        .limiter
        .check(ip.clone(), "admin", state.cfg.rate_limit_admin_per_min)
        .await
    {
        let _ = state.db.lock().await.insert_admin_audit(
            action,
            username,
            None,
            Some(&ip),
            false,
            Some("rate limited"),
            &meta,
        );
        return Err((StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response());
    }
    if !is_authorized_admin(&state.cfg, headers) {
        let _ = state.db.lock().await.insert_admin_audit(
            action,
            username,
            None,
            Some(&ip),
            false,
            Some("unauthorized"),
            &meta,
        );
        return Err((StatusCode::UNAUTHORIZED, "admin token required").into_response());
    }
    Ok(AdminAuditContext { ip, meta })
}

async fn admin_list_users(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let audit = match admin_guard(&state, &peer, &headers, "admin_list_users", None).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let limit = q
        .get("limit")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(100)
        .min(500);
    let offset = q
        .get("offset")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let db = state.db.lock().await;
    match db.list_users(limit, offset) {
        Ok(users) => {
            let _ = db.insert_admin_audit(
                "admin_list_users",
                None,
                None,
                Some(&audit.ip),
                true,
                None,
                &audit.meta,
            );
            axum::Json(
                users
                    .into_iter()
                    .map(|(u, created_at_ms, disabled)| {
                        serde_json::json!({
                          "username": u,
                          "created_at_ms": created_at_ms,
                          "disabled": disabled != 0
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .into_response()
        }
        Err(e) => {
            let _ = db.insert_admin_audit(
                "admin_list_users",
                None,
                None,
                Some(&audit.ip),
                false,
                Some("db error"),
                &audit.meta,
            );
            (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response()
        }
    }
}

async fn admin_get_user(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(user): Path<String>,
) -> impl IntoResponse {
    let audit = match admin_guard(&state, &peer, &headers, "admin_get_user", Some(&user)).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid user").into_response();
    }

    let online = state.tunnels.read().await.contains_key(&user);
    let db = state.db.lock().await;
    let row = match db.get_user(&user) {
        Ok(v) => v,
        Err(e) => {
            let _ = db.insert_admin_audit(
                "admin_get_user",
                Some(&user),
                None,
                Some(&audit.ip),
                false,
                Some("db error"),
                &audit.meta,
            );
            return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response();
        }
    };
    let Some((created_at_ms, disabled)) = row else {
        let _ = db.insert_admin_audit(
            "admin_get_user",
            Some(&user),
            None,
            Some(&audit.ip),
            false,
            Some("not found"),
            &audit.meta,
        );
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    let (spool_count, spool_bytes) = db.spool_stats(&user).unwrap_or((0, 0));
    let _ = db.insert_admin_audit(
        "admin_get_user",
        Some(&user),
        None,
        Some(&audit.ip),
        true,
        None,
        &audit.meta,
    );

    axum::Json(serde_json::json!({
      "username": user,
      "created_at_ms": created_at_ms,
      "disabled": disabled != 0,
      "online": online,
      "spool": { "count": spool_count, "bytes": spool_bytes }
    }))
    .into_response()
}

async fn admin_disable_user(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(user): Path<String>,
) -> impl IntoResponse {
    let audit = match admin_guard(&state, &peer, &headers, "admin_disable_user", Some(&user)).await
    {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid user").into_response();
    }
    let db = state.db.lock().await;
    match db.set_disabled(&user, true) {
        Ok(()) => {
            let _ = db.insert_admin_audit(
                "admin_disable_user",
                Some(&user),
                None,
                Some(&audit.ip),
                true,
                None,
                &audit.meta,
            );
            (StatusCode::OK, "disabled").into_response()
        }
        Err(e) => {
            let _ = db.insert_admin_audit(
                "admin_disable_user",
                Some(&user),
                None,
                Some(&audit.ip),
                false,
                Some("db error"),
                &audit.meta,
            );
            (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response()
        }
    }
}

async fn admin_enable_user(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(user): Path<String>,
) -> impl IntoResponse {
    let audit = match admin_guard(&state, &peer, &headers, "admin_enable_user", Some(&user)).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid user").into_response();
    }
    let db = state.db.lock().await;
    match db.set_disabled(&user, false) {
        Ok(()) => {
            let _ = db.insert_admin_audit(
                "admin_enable_user",
                Some(&user),
                None,
                Some(&audit.ip),
                true,
                None,
                &audit.meta,
            );
            (StatusCode::OK, "enabled").into_response()
        }
        Err(e) => {
            let _ = db.insert_admin_audit(
                "admin_enable_user",
                Some(&user),
                None,
                Some(&audit.ip),
                false,
                Some("db error"),
                &audit.meta,
            );
            (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response()
        }
    }
}

async fn admin_rotate_token(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(user): Path<String>,
) -> impl IntoResponse {
    let audit = match admin_guard(&state, &peer, &headers, "admin_rotate_token", Some(&user)).await
    {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid user").into_response();
    }
    let token = generate_token();
    let db = state.db.lock().await;
    match db.rotate_token(&user, &token) {
        Ok(()) => {
            let _ = db.insert_admin_audit(
                "admin_rotate_token",
                Some(&user),
                None,
                Some(&audit.ip),
                true,
                None,
                &audit.meta,
            );
            axum::Json(AdminRotateResponse { token }).into_response()
        }
        Err(e) => {
            let _ = db.insert_admin_audit(
                "admin_rotate_token",
                Some(&user),
                None,
                Some(&audit.ip),
                false,
                Some("db error"),
                &audit.meta,
            );
            (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response()
        }
    }
}

async fn relay_stats(
    State(state): State<AppState>,
    Query(q): Query<RelayTelemetryQuery>,
) -> impl IntoResponse {
    let _ = q;
    let telemetry = match build_self_telemetry(&state).await {
        Ok(t) => t,
        Err(e) => {
            return (StatusCode::BAD_GATEWAY, format!("telemetry error: {e}")).into_response()
        }
    };
    axum::Json(telemetry).into_response()
}

async fn relay_me(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RelayMeQuery>,
) -> impl IntoResponse {
    if !is_valid_username(&q.username) {
        return (StatusCode::BAD_REQUEST, "invalid username").into_response();
    }
    let Some(tok) = bearer_token(&headers) else {
        return (StatusCode::UNAUTHORIZED, "missing bearer token").into_response();
    };

    let online = { state.tunnels.read().await.contains_key(&q.username) };
    let db = state.db.lock().await;
    let known = db.user_exists(&q.username).unwrap_or(false);
    let enabled = db.is_user_enabled(&q.username).unwrap_or(false);
    let token_ok = db.verify_token(&q.username, &tok).unwrap_or(false);

    axum::Json(serde_json::json!({
      "username": q.username,
      "known": known,
      "enabled": enabled,
      "online": online,
      "token_ok": token_ok
    }))
    .into_response()
}

#[derive(Debug, serde::Deserialize)]
struct RelayBackupQuery {
    username: String,
}

#[derive(Debug, serde::Serialize)]
struct RelayBackupMeta {
    username: String,
    updated_at_ms: i64,
    size_bytes: i64,
    content_type: String,
    meta_json: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ApiUserShowRequest {
    #[serde(alias = "userId", alias = "user_id")]
    user_id: Option<String>,
    username: Option<String>,
    host: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ApiUserShowQuery {
    #[serde(alias = "userId", alias = "user_id")]
    user_id: Option<String>,
    username: Option<String>,
    host: Option<String>,
}

async fn relay_backup_meta(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RelayBackupQuery>,
) -> impl IntoResponse {
    let user = q.username.trim().to_string();
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid username").into_response();
    }
    if let Err(resp) = require_user_or_admin(&state, &headers, &user).await {
        return resp;
    }
    let db = state.db.lock().await;
    let item = match db.get_user_backup(&user) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response(),
    };
    let Some(item) = item else {
        return (StatusCode::NOT_FOUND, "backup not found").into_response();
    };
    axum::Json(RelayBackupMeta {
        username: item.username,
        updated_at_ms: item.updated_at_ms,
        size_bytes: item.size_bytes,
        content_type: item.content_type,
        meta_json: item.meta_json,
    })
    .into_response()
}

async fn relay_backup_put(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RelayBackupQuery>,
    body: Body,
) -> impl IntoResponse {
    let user = q.username.trim().to_string();
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid username").into_response();
    }
    if let Err(resp) = require_user_or_admin(&state, &headers, &user).await {
        return resp;
    }
    let since_ms = now_ms().saturating_sub(60 * 60 * 1000);
    {
        let db = state.db.lock().await;
        match db.count_user_backups_since(&user, since_ms) {
            Ok(count) if count >= state.cfg.backup_rate_limit_per_hour as u64 => {
                return (StatusCode::TOO_MANY_REQUESTS, "backup rate limited").into_response();
            }
            Ok(_) => {}
            Err(e) => return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response(),
        }
    }
    let content_type = headers
        .get("Content-Type")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let meta_json = headers
        .get("X-Fedi3-Backup-Meta")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let bytes = match axum::body::to_bytes(body, state.cfg.backup_max_bytes).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid body").into_response(),
    };
    if bytes.is_empty() {
        return (StatusCode::BAD_REQUEST, "empty backup").into_response();
    }
    let backup_id = generate_token();
    let raw_key = format!("backups/{user}/{backup_id}.enc");
    let storage_key = media_store::sanitize_key(&raw_key);
    let saved = match state
        .media_backend
        .save_upload(&storage_key, &content_type, &bytes)
        .await
    {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("storage error: {e}")).into_response(),
    };
    let now = now_ms();
    let item = UserBackupItem {
        username: user.clone(),
        storage_key: saved.storage_key,
        content_type: saved.media_type,
        size_bytes: saved.size as i64,
        updated_at_ms: now,
        meta_json,
    };
    let saved_key = item.storage_key.clone();
    let keys_to_delete = {
        let db = state.db.lock().await;
        if let Err(e) = db.insert_user_backup_history(&item) {
            drop(db);
            let _ = state.media_backend.delete(&saved_key).await;
            return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response();
        }
        if let Err(e) = db.upsert_user_backup(&item) {
            drop(db);
            let _ = state.media_backend.delete(&saved_key).await;
            return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response();
        }
        match db.list_user_backup_keys(&user) {
            Ok(keys) => {
                if keys.len() > state.cfg.backup_retention_count {
                    keys[state.cfg.backup_retention_count..].to_vec()
                } else {
                    Vec::new()
                }
            }
            Err(e) => {
                return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response();
            }
        }
    };
    for key in keys_to_delete {
        if let Err(e) = state.media_backend.delete(&key).await {
            warn!("backup delete failed key={key} err={e}");
            continue;
        }
        let db = state.db.lock().await;
        if let Err(e) = db.delete_user_backup_history(&user, &key) {
            warn!("backup history delete failed key={key} err={e}");
        }
    }
    axum::Json(serde_json::json!({
      "ok": true,
      "username": user,
      "updated_at_ms": now,
      "size_bytes": item.size_bytes
    }))
    .into_response()
}

async fn relay_backup_blob(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RelayBackupQuery>,
) -> impl IntoResponse {
    let user = q.username.trim().to_string();
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid username").into_response();
    }
    if let Err(resp) = require_user_or_admin(&state, &headers, &user).await {
        return resp;
    }
    let db = state.db.lock().await;
    let item = match db.get_user_backup(&user) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response(),
    };
    let Some(item) = item else {
        return (StatusCode::NOT_FOUND, "backup not found").into_response();
    };
    let bytes = match state.media_backend.load(&item.storage_key).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("storage error: {e}")).into_response(),
    };
    let mut resp = Response::new(Body::from(bytes));
    let headers = resp.headers_mut();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_str(&item.content_type).unwrap_or_else(|_| {
            HeaderValue::from_static("application/octet-stream")
        }),
    );
    headers.insert(
        http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    resp
}

async fn api_user_show(
    State(state): State<AppState>,
    body: Bytes,
) -> impl IntoResponse {
    let input: ApiUserShowRequest = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid json").into_response(),
    };
    user_show_response(&state, input.username, input.host, input.user_id).await
}

async fn api_user_show_get(
    State(state): State<AppState>,
    Query(q): Query<ApiUserShowQuery>,
) -> impl IntoResponse {
    user_show_response(&state, q.username, q.host, q.user_id).await
}

async fn user_show_response(
    state: &AppState,
    username: Option<String>,
    host: Option<String>,
    user_id: Option<String>,
) -> Response {
    if user_id.is_some() && username.is_none() {
        return (StatusCode::BAD_REQUEST, "userId unsupported").into_response();
    }
    let username = username.unwrap_or_default().trim().to_string();
    if !is_valid_username(&username) {
        return (StatusCode::BAD_REQUEST, "invalid username").into_response();
    }

    if !host_matches_relay(state, host.as_deref()) {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }

    let db = state.db.lock().await;
    if !db.user_exists(&username).unwrap_or(false) {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let enabled = db.is_user_enabled(&username).unwrap_or(false);
    let user_created_ms = db.get_user(&username).ok().flatten().map(|v| v.0);

    let actor_cache = db.get_actor_cache_with_meta(&username).ok().flatten();
    let followers_json = db.get_collection_cache(&username, "followers").ok().flatten();
    let following_json = db.get_collection_cache(&username, "following").ok().flatten();
    let outbox_json = db.get_collection_cache(&username, "outbox").ok().flatten();
    drop(db);

    let actor_value = actor_cache
        .as_ref()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c.actor_json).ok())
        .unwrap_or_else(|| {
            let base = user_base_template(&state.cfg);
            actor_stub_json(&username, &base)
        });

    let mut actor_id = actor_value.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if actor_id.is_empty() {
        if let Some(cached_actor_id) = actor_cache.as_ref().and_then(|c| c.actor_id.clone()) {
            actor_id = cached_actor_id;
        } else if let Some(actor_url) = actor_cache.as_ref().and_then(|c| c.actor_url.clone())
        {
            actor_id = actor_url;
        } else {
            actor_id = format!("{}/users/{username}", relay_self_base(&state.cfg));
        }
    }
    let preferred_username = actor_value
        .get("preferredUsername")
        .and_then(|v| v.as_str())
        .unwrap_or(&username);
    let name = actor_value.get("name").and_then(|v| v.as_str()).unwrap_or(preferred_username);
    let summary = actor_value.get("summary").and_then(|v| v.as_str()).unwrap_or("");
    let is_bot = actor_value
        .get("type")
        .and_then(|v| v.as_str())
        .map(|t| matches!(t, "Service" | "Application"))
        .unwrap_or(false);
    let is_locked = actor_value
        .get("manuallyApprovesFollowers")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let moved_to = actor_value.get("movedTo").and_then(|v| v.as_str());
    let also_known_as = actor_value
        .get("alsoKnownAs")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let avatar_url = actor_value
        .get("icon")
        .and_then(|v| {
            if let Some(s) = v.as_str() {
                return Some(s);
            }
            v.get("url")
                .or_else(|| v.get("href"))
                .and_then(|u| u.as_str())
        })
        .unwrap_or("");
    let banner_url = actor_value
        .get("image")
        .and_then(|v| {
            if let Some(s) = v.as_str() {
                return Some(s);
            }
            v.get("url")
                .or_else(|| v.get("href"))
                .and_then(|u| u.as_str())
        })
        .unwrap_or("");

    let fields = actor_value
        .get("attachment")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let name = item.get("name")?.as_str()?.to_string();
                    let value = item.get("value")?.as_str()?.to_string();
                    Some(serde_json::json!({ "name": name, "value": value }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let followers_count = followers_json
        .as_deref()
        .and_then(collection_total_items)
        .unwrap_or(0);
    let following_count = following_json
        .as_deref()
        .and_then(collection_total_items)
        .unwrap_or(0);
    let notes_count = outbox_json
        .as_deref()
        .and_then(collection_total_items)
        .unwrap_or(0);

    let online_status = if state.tunnels.read().await.contains_key(&username) {
        "online"
    } else {
        let last_seen = {
            let seen = state.presence_last_seen.lock().await;
            seen.get(&username).copied().unwrap_or(0)
        };
        if last_seen > 0 {
            let age_ms = now_ms().saturating_sub(last_seen);
            if age_ms >= 60_000 && age_ms <= 7 * 24 * 60 * 60 * 1000 {
                "active"
            } else {
                "offline"
            }
        } else {
            "offline"
        }
    };
    let host = relay_host_name(&state.cfg).unwrap_or_default();

    let created_at = actor_value
        .get("published")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| user_created_ms.and_then(rfc3339_from_ms))
        .unwrap_or_else(|| Utc::now().to_rfc3339());

    let updated_at = actor_cache
        .as_ref()
        .and_then(|c| rfc3339_from_ms(c.updated_at_ms));

    let instance = serde_json::json!({
        "name": null,
        "softwareName": "fedi3-relay",
        "softwareVersion": env!("CARGO_PKG_VERSION"),
        "iconUrl": null,
        "faviconUrl": null,
        "themeColor": null,
        "isSilenced": false
    });

    let out = serde_json::json!({
        "id": meili_doc_id(&actor_id),
        "name": name,
        "username": preferred_username,
        "host": if host.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(host) },
        "avatarUrl": if avatar_url.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(avatar_url.to_string()) },
        "avatarBlurhash": null,
        "description": summary,
        "listenbrainz": null,
        "listenbrainzBadgeEnabled": false,
        "createdAt": created_at,
        "avatarDecorations": [],
        "isBot": is_bot,
        "isCat": false,
        "noindex": false,
        "enableRss": false,
        "mandatoryCW": null,
        "rejectQuotes": false,
        "attributionDomains": [],
        "isSilenced": false,
        "speakAsCat": false,
        "approved": enabled,
        "instance": instance,
        "followersCount": followers_count,
        "followingCount": following_count,
        "notesCount": notes_count,
        "emojis": {},
        "onlineStatus": online_status,
        "url": actor_id,
        "uri": actor_id,
        "movedTo": moved_to,
        "alsoKnownAs": also_known_as,
        "updatedAt": updated_at,
        "lastFetchedAt": updated_at,
        "bannerUrl": if banner_url.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(banner_url.to_string()) },
        "bannerBlurhash": null,
        "backgroundUrl": null,
        "backgroundBlurhash": null,
        "isLocked": is_locked,
        "isSuspended": !enabled,
        "location": null,
        "birthday": null,
        "lang": null,
        "fields": fields,
        "verifiedLinks": [],
        "pinnedNoteIds": [],
        "pinnedNotes": [],
        "pinnedPageId": null,
        "pinnedPage": null,
        "publicReactions": false,
        "followersVisibility": "public",
        "followingVisibility": "public",
        "chatScope": "mutual",
        "canChat": true,
        "roles": [],
        "memo": null,
        "moderationNote": "",
        "twoFactorEnabled": false,
        "usePasswordLessLogin": false,
        "securityKeys": false,
        "isFollowing": false,
        "isFollowed": false,
        "hasPendingFollowRequestFromYou": false,
        "hasPendingFollowRequestToYou": false,
        "isBlocking": false,
        "isBlocked": false,
        "isMuted": false,
        "isRenoteMuted": false,
        "notify": "none",
        "withReplies": true,
        "followedMessage": null
    });

    axum::Json(out).into_response()
}

#[derive(Debug, serde::Serialize)]
struct RelayP2pInfraResponse {
    peer_id: Option<String>,
    multiaddrs: Vec<String>,
}

fn relay_p2p_infra_multiaddrs(cfg: &RelayConfig) -> Vec<String> {
    if !cfg.p2p_infra_multiaddrs.is_empty() {
        return cfg.p2p_infra_multiaddrs.clone();
    }
    let Some(peer_id) = cfg.p2p_infra_peer_id.as_ref() else {
        return Vec::new();
    };
    let host = if let Some(host) = cfg.p2p_infra_host.as_ref() {
        host.to_string()
    } else if let Some((_, host)) = canonical_origin(cfg) {
        normalize_host(host.split(':').next().unwrap_or(&host).to_string())
    } else {
        return Vec::new();
    };
    if host.is_empty() {
        return Vec::new();
    }
    let port = cfg.p2p_infra_port;
    vec![
        format!("/dns4/{host}/tcp/{port}/p2p/{peer_id}"),
        format!("/dns4/{host}/udp/{port}/quic-v1/p2p/{peer_id}"),
    ]
}

async fn relay_p2p_infra(State(state): State<AppState>) -> impl IntoResponse {
    let multiaddrs = relay_p2p_infra_multiaddrs(&state.cfg);
    axum::Json(RelayP2pInfraResponse {
        peer_id: state.cfg.p2p_infra_peer_id.clone(),
        multiaddrs,
    })
    .into_response()
}

#[derive(Debug, serde::Deserialize)]
struct RelaySearchQuery {
    username: String,
    q: Option<String>,
    tag: Option<String>,
    limit: Option<u32>,
    cursor: Option<i64>,
    since: Option<i64>,
}

async fn relay_search_notes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RelaySearchQuery>,
) -> impl IntoResponse {
    let user = q.username.trim().to_string();
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid username").into_response();
    }
    let Some(tok) = bearer_token(&headers) else {
        return (StatusCode::UNAUTHORIZED, "missing bearer token").into_response();
    };
    let db = state.db.lock().await;
    let authorized = if is_authorized_admin(&state.cfg, &headers) {
        true
    } else {
        db.verify_token(&user, &tok).unwrap_or(false)
    };
    drop(db);
    if !authorized {
        return (StatusCode::UNAUTHORIZED, "admin or user token required").into_response();
    }
    let limit = q.limit.unwrap_or(30).min(200);
    let query = q.q.unwrap_or_default();
    let tag = q.tag.unwrap_or_default();
    let cursor = q.cursor;
    let since = q.since;
    let cache_key = format!(
        "notes|u={}|q={}|tag={}|limit={}|cursor={:?}|since={:?}|total={:?}|backend={}",
        user,
        query.trim().to_lowercase(),
        tag.trim().to_lowercase(),
        limit,
        cursor,
        since,
        state.cfg.search_total_mode,
        state.cfg.search_backend
    );
    if let Some(cache) = state.search_cache.as_ref() {
        if let Some(cached) = cache.get_notes(&cache_key).await {
            return axum::Json(cached).into_response();
        }
    }
    let page = if let Some(search) = state.search.as_ref() {
        match search
            .search_notes(&query, &tag, limit, cursor, since)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                return (StatusCode::BAD_GATEWAY, format!("search error: {e}")).into_response()
            }
        }
    } else {
        let db = state.db.lock().await;
        match db.search_relay_notes(
            &query,
            &tag,
            limit,
            cursor,
            since,
            state.cfg.search_total_mode,
        ) {
            Ok(p) => p,
            Err(e) => return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response(),
        }
    };
    let items: Vec<serde_json::Value> = page
        .items
        .into_iter()
        .filter_map(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .collect();
    let total = match state.cfg.search_total_mode {
        SearchTotalMode::Exact => page.total,
        SearchTotalMode::Approx => items.len() as u64,
        SearchTotalMode::None => 0,
    };
    let body = serde_json::json!({
      "total": total,
      "items": items,
      "next": page.next,
    });
    if let Some(cache) = state.search_cache.as_ref() {
        cache.set_notes(cache_key, body.clone()).await;
    }
    axum::Json(body).into_response()
}

async fn relay_sync_notes(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Query(q): Query<RelaySyncNotesQuery>,
) -> impl IntoResponse {
    if !state
        .limiter
        .check(
            peer_ip(&peer),
            "relay_sync",
            state.cfg.rate_limit_forward_per_min,
        )
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }
    let limit = q.limit.unwrap_or(200).min(200);
    let db = state.db.lock().await;
    let page = match db.list_relay_notes_sync(limit, q.since, q.cursor) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response(),
    };
    let items = page
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
    axum::Json(RelaySyncNotesResponse {
        items,
        next: page.next,
    })
    .into_response()
}

async fn relay_search_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RelaySearchQuery>,
) -> impl IntoResponse {
    let user = q.username.trim().to_string();
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid username").into_response();
    }
    let Some(tok) = bearer_token(&headers) else {
        return (StatusCode::UNAUTHORIZED, "missing bearer token").into_response();
    };
    let db = state.db.lock().await;
    let authorized = if is_authorized_admin(&state.cfg, &headers) {
        true
    } else {
        db.verify_token(&user, &tok).unwrap_or(false)
    };
    drop(db);
    if !authorized {
        return (StatusCode::UNAUTHORIZED, "admin or user token required").into_response();
    }
    let limit = q.limit.unwrap_or(30).min(200);
    let query = q.q.unwrap_or_default();
    let cursor = q.cursor;
    let base_template = user_base_template(&state.cfg);
    let cache_key = format!(
        "users|u={}|q={}|limit={}|cursor={:?}|total={:?}|backend={}",
        user,
        query.trim().to_lowercase(),
        limit,
        cursor,
        state.cfg.search_total_mode,
        state.cfg.search_backend
    );
    if let Some(cache) = state.search_cache.as_ref() {
        if let Some(cached) = cache.get_users(&cache_key).await {
            return axum::Json(cached).into_response();
        }
    }
    let page = if let Some(search) = state.search.as_ref() {
        match search
            .search_users(&query, limit, cursor, &base_template)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                return (StatusCode::BAD_GATEWAY, format!("search error: {e}")).into_response()
            }
        }
    } else {
        let db = state.db.lock().await;
        match db.search_relay_users(
            &query,
            limit,
            cursor,
            &base_template,
            state.cfg.search_total_mode,
        ) {
            Ok(p) => p,
            Err(e) => return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response(),
        }
    };
    let items: Vec<serde_json::Value> = page
        .items
        .into_iter()
        .filter_map(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .collect();
    let total = match state.cfg.search_total_mode {
        SearchTotalMode::Exact => page.total,
        SearchTotalMode::Approx => items.len() as u64,
        SearchTotalMode::None => 0,
    };
    let body = serde_json::json!({
      "total": total,
      "items": items,
      "next": page.next,
    });
    if let Some(cache) = state.search_cache.as_ref() {
        cache.set_users(cache_key, body.clone()).await;
    }
    axum::Json(body).into_response()
}

async fn relay_search_hashtags(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RelaySearchQuery>,
) -> impl IntoResponse {
    let user = q.username.trim().to_string();
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid username").into_response();
    }
    let Some(tok) = bearer_token(&headers) else {
        return (StatusCode::UNAUTHORIZED, "missing bearer token").into_response();
    };
    let db = state.db.lock().await;
    let authorized = if is_authorized_admin(&state.cfg, &headers) {
        true
    } else {
        db.verify_token(&user, &tok).unwrap_or(false)
    };
    if !authorized {
        return (StatusCode::UNAUTHORIZED, "admin or user token required").into_response();
    }
    let limit = q.limit.unwrap_or(30).min(200);
    let query = q.q.unwrap_or_default();
    let rows = match db.search_relay_tags(&query, limit) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response(),
    };
    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(name, count)| serde_json::json!({ "name": name, "count": count }))
        .collect();
    axum::Json(serde_json::json!({ "items": items })).into_response()
}

#[derive(Debug, serde::Deserialize)]
struct RelayCoverageQuery {
    username: Option<String>,
}

async fn relay_search_coverage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RelayCoverageQuery>,
) -> impl IntoResponse {
    let Some(tok) = bearer_token(&headers) else {
        return (StatusCode::UNAUTHORIZED, "missing bearer token").into_response();
    };
    let db = state.db.lock().await;
    let authorized = if is_authorized_admin(&state.cfg, &headers) {
        true
    } else if let Some(user) = q.username.as_deref() {
        db.verify_token(user, &tok).unwrap_or(false)
    } else {
        false
    };
    if !authorized {
        return (StatusCode::UNAUTHORIZED, "admin or user token required").into_response();
    }

    let total_users = db.count_users().unwrap_or(0);
    let coverage_window_ms: i64 = 24 * 3600 * 1000;
    let cutoff = now_ms().saturating_sub(coverage_window_ms);
    let indexed_users = db.count_outbox_indexed_since(cutoff).unwrap_or(0);
    let last_index_ms = db
        .relay_meta_get("search_index_last_ms")
        .ok()
        .flatten()
        .and_then(|v| v.parse::<i64>().ok());
    let relay_sync_window_ms: i64 = 24 * 3600 * 1000;
    let relay_sync_cutoff = now_ms().saturating_sub(relay_sync_window_ms);
    let sync_rows = db.list_relay_sync_state().unwrap_or_default();
    let relays_total = sync_rows.len() as u64;
    let mut relays_synced = 0u64;
    let mut relays_last_sync_ms = None;
    for (_relay_url, last_ms) in sync_rows {
        if last_ms >= relay_sync_cutoff {
            relays_synced += 1;
        }
        if relays_last_sync_ms.map(|v| last_ms > v).unwrap_or(true) {
            relays_last_sync_ms = Some(last_ms);
        }
    }

    let mut user_state = None;
    if let Some(user) = q.username.as_deref() {
        if let Ok(state_row) = db.get_outbox_index_state(user) {
            if let Some((ms, ok)) = state_row {
                user_state = Some(serde_json::json!({
                    "username": user,
                    "last_index_ms": ms,
                    "last_ok": ok
                }));
            }
        }
    }

    axum::Json(serde_json::json!({
        "total_users": total_users,
        "indexed_users": indexed_users,
        "coverage_window_ms": coverage_window_ms,
        "last_index_ms": last_index_ms,
        "relays_total": relays_total,
        "relays_synced": relays_synced,
        "relay_sync_window_ms": relay_sync_window_ms,
        "relays_last_sync_ms": relays_last_sync_ms,
        "user": user_state
    }))
    .into_response()
}

async fn relay_reindex(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if !is_authorized_admin(&state.cfg, &headers) {
        return (StatusCode::UNAUTHORIZED, "admin token required").into_response();
    }
    let st = state.clone();
    tokio::spawn(async move {
        if let Err(e) = run_outbox_index_once(&st).await {
            error!("manual reindex failed: {e:#}");
        }
    });
    (StatusCode::ACCEPTED, "reindex started").into_response()
}

async fn relay_list(
    State(state): State<AppState>,
    Query(q): Query<RelayTelemetryQuery>,
) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(200).min(500);
    let db = state.db.lock().await;
    let rows = match db.list_relays(limit) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response(),
    };
    drop(db);

    let mut relays = Vec::new();
    for (url, base_domain, last_seen_ms, last_json, sign_pubkey_b64) in rows {
        let parsed: Option<serde_json::Value> = last_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        relays.push(serde_json::json!({
          "relay_url": url,
          "base_domain": base_domain,
          "last_seen_ms": last_seen_ms,
          "sign_pubkey_b64": sign_pubkey_b64,
          "telemetry": parsed,
        }));
    }
    axum::Json(serde_json::json!({ "relays": relays })).into_response()
}

async fn presence_snapshot(state: &AppState) -> Vec<PresenceItem> {
    let online_users: Vec<String> = state
        .tunnels
        .read()
        .await
        .keys()
        .cloned()
        .collect();
    let hello_map = state.peer_hello.read().await;
    online_users
        .into_iter()
        .map(|user| {
            let actor_url = hello_map
                .get(&user)
                .and_then(|hello| {
                    let v = hello.actor.trim();
                    if v.is_empty() {
                        None
                    } else {
                        Some(v.to_string())
                    }
                })
                .unwrap_or_else(|| format!("{}/users/{}", user_base_url(&state.cfg, &user), user));
            PresenceItem {
                username: user,
                actor_url,
                online: true,
            }
        })
        .collect()
}

fn emit_presence_update(state: &AppState, username: &str, actor_url: &str, online: bool) {
    {
        let mut seen = state.presence_last_seen.blocking_lock();
        seen.insert(username.to_string(), now_ms());
    }
    let item = PresenceItem {
        username: username.to_string(),
        actor_url: actor_url.to_string(),
        online,
    };
    let _ = state.presence_tx.send(PresenceEvent::Update(item));
}

async fn relay_peers(
    State(state): State<AppState>,
    Query(q): Query<RelayPeersQuery>,
) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(200).min(500);
    let query = q.q.unwrap_or_default();
    let online_users = {
        let tunnels = state.tunnels.read().await;
        tunnels
            .keys()
            .map(|u| u.to_lowercase())
            .collect::<std::collections::HashSet<String>>()
    };
    let db = state.db.lock().await;
    let rows = match db.list_peer_directory(&query, limit, None) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response(),
    };
    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(peer_id, username, actor_url)| {
            let uname = username.to_lowercase();
            serde_json::json!({
              "peer_id": peer_id,
              "username": username,
              "actor_url": actor_url,
              "online": online_users.contains(&uname),
            })
        })
        .collect();
    let mut merged = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in items {
        if let Some(u) = item.get("username").and_then(|v| v.as_str()) {
            seen.insert(u.to_lowercase());
        }
        merged.push(item);
    }
    for user in online_users {
        if seen.contains(&user) {
            continue;
        }
        let actor_url = format!("{}/users/{}", user_base_url(&state.cfg, &user), user);
        merged.push(serde_json::json!({
          "peer_id": format!("user:{user}"),
          "username": user,
          "actor_url": actor_url,
          "online": true,
        }));
    }
    axum::Json(serde_json::json!({ "items": merged })).into_response()
}

async fn relay_presence_stream(State(state): State<AppState>) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let snapshot = presence_snapshot(&state).await;
    let snapshot_payload = serde_json::to_string(&PresenceSnapshot {
        ts_ms: now_ms(),
        items: snapshot,
    })
    .unwrap_or_else(|_| "{\"items\":[]}".to_string());
    let snapshot_event = Event::default().event("snapshot").data(snapshot_payload);
    let rx = state.presence_tx.subscribe();
    let updates = stream::unfold((state.clone(), rx), |(state, mut rx)| async move {
        loop {
            match rx.recv().await {
                Ok(PresenceEvent::Update(item)) => {
                    let payload = serde_json::to_string(&item).unwrap_or_else(|_| "{}".to_string());
                    let event = Event::default().event("update").data(payload);
                    return Some((Ok(event), (state, rx)));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let snapshot = presence_snapshot(&state).await;
                    let payload = serde_json::to_string(&PresenceSnapshot {
                        ts_ms: now_ms(),
                        items: snapshot,
                    })
                    .unwrap_or_else(|_| "{\"items\":[]}".to_string());
                    let event = Event::default().event("snapshot").data(payload);
                    return Some((Ok(event), (state, rx)));
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    let stream = stream::once(async move { Ok(snapshot_event) }).chain(updates);
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

async fn relay_client_telemetry_post(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    axum::Json(input): axum::Json<ClientTelemetryInput>,
) -> impl IntoResponse {
    let username = input.username.trim().to_ascii_lowercase();
    if !is_valid_username(&username) {
        return (StatusCode::BAD_REQUEST, "invalid username").into_response();
    }
    if input.event_type.trim().is_empty() || input.message.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "missing telemetry fields").into_response();
    }
    if let Err(resp) = require_user_or_admin(&state, &headers, &username).await {
        return resp;
    }
    if !state
        .limiter
        .check(
            client_ip(&state.cfg, &peer, &headers),
            "client_telemetry",
            state.cfg.rate_limit_client_telemetry_per_min,
        )
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }

    let Some(reporter) = state.github_issues.as_ref() else {
        return (StatusCode::ACCEPTED, "telemetry ok").into_response();
    };

    let relay_host = relay_host_for_request(&state.cfg, &headers);
    let handle = format!("@{username}@{relay_host}");
    let level = classify_telemetry_level(&input.event_type, &input.message);
    let message = sanitize_message(&input.message);
    let stack = input
        .stack
        .as_deref()
        .map(|s| sanitize_stack(&redact_secrets(s)))
        .unwrap_or_default();
    let fingerprint_src = format!(
        "{}|{}|{}|{}",
        handle,
        input.event_type.trim(),
        message,
        stack
    );
    let fingerprint = format!("{:x}", Sha256::digest(fingerprint_src.as_bytes()));
    if dedupe_telemetry(&state, &fingerprint, 3600).await {
        return (StatusCode::ACCEPTED, "duplicate").into_response();
    }

    let title = short_text(
        format!(
            "[telemetry][{level}] {}",
            message.split('\n').next().unwrap_or("").trim()
        ),
        120,
    );
    let ts = input.ts.unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let mode = input.mode.unwrap_or_else(|| "unknown".to_string());
    let body = format!(
        "## Auto-generated telemetry (Fedi3)\n\
This issue was created automatically from anonymous client telemetry.\
\n\n\
- user: `{handle}`\n\
- type: `{}`\n\
- level: `{level}`\n\
- mode: `{mode}`\n\
- ts: `{ts}`\n\
- fingerprint: `{fingerprint}`\n\
\n\
### Message\n\
```\n\
{message}\n\
```\n\
\n\
### Stack (sanitized)\n\
```\n\
{stack}\n\
```\n",
        input.event_type.trim()
    );
    let mut labels = reporter.labels.clone();
    labels.push(level.to_string());

    if reporter
        .tx
        .try_send(GithubIssueRequest {
            title,
            body,
            labels,
            assignee: reporter.assignee.clone(),
        })
        .is_err()
    {
        return (StatusCode::SERVICE_UNAVAILABLE, "telemetry queue full").into_response();
    }

    (StatusCode::ACCEPTED, "telemetry ok").into_response()
}

fn signature_header_value(headers: &HeaderMap) -> Option<String> {
    headers
        .get("Signature")
        .or_else(|| headers.get("signature"))
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string())
}

fn signature_key_id(headers: &HeaderMap) -> Option<String> {
    let sig = signature_header_value(headers)?;
    for part in sig.split(',') {
        let part = part.trim();
        let Some((k, v)) = part.split_once('=') else { continue };
        if k.trim() == "keyId" {
            let key_id = v.trim().trim_matches('"').trim().to_string();
            if !key_id.is_empty() {
                return Some(key_id);
            }
        }
    }
    None
}

fn actor_from_key_id(key_id: &str) -> Option<String> {
    let actor = key_id.split('#').next().unwrap_or(key_id).trim().to_string();
    if actor.is_empty() {
        None
    } else {
        Some(actor)
    }
}

async fn fetch_actor_public_key_pem(state: &AppState, actor_url: &str) -> Result<String> {
    let now = now_ms();
    {
        let cache = state.webrtc_key_cache.lock().await;
        if let Some((pem, ts)) = cache.get(actor_url) {
            if now.saturating_sub(*ts) <= WEBRTC_KEY_CACHE_TTL_SECS * 1000 {
                return Ok(pem.clone());
            }
        }
    }
    let resp = state
        .http
        .get(actor_url)
        .header(
            "Accept",
            "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"",
        )
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("actor fetch failed: {status} {body}"));
    }
    let text = resp.text().await.unwrap_or_default();
    let pem = extract_public_key_pem_from_actor_json(&text)
        .ok_or_else(|| anyhow::anyhow!("actor missing public key"))?;
    let mut cache = state.webrtc_key_cache.lock().await;
    cache.insert(actor_url.to_string(), (pem.clone(), now));
    Ok(pem)
}

async fn verify_webrtc_signature(
    state: &AppState,
    headers: &HeaderMap,
    method: &Method,
    uri: &http::Uri,
    body: &[u8],
) -> Result<String> {
    let sig_header = signature_header_value(headers).ok_or_else(|| anyhow::anyhow!("missing signature"))?;
    let key_id = signature_key_id(headers).ok_or_else(|| anyhow::anyhow!("missing keyId"))?;
    let actor_url = actor_from_key_id(&key_id).ok_or_else(|| anyhow::anyhow!("invalid keyId"))?;

    // Date skew (5 minutes).
    let date = headers
        .get("Date")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if date.is_empty() {
        return Err(anyhow::anyhow!("missing date"));
    }
    let ts = parse_http_date(date)?;
    let now = std::time::SystemTime::now();
    let diff = if now > ts {
        now.duration_since(ts).unwrap_or_default()
    } else {
        ts.duration_since(now).unwrap_or_default()
    };
    if diff > Duration::from_secs(300) {
        return Err(anyhow::anyhow!("date skew"));
    }

    // Digest check if present.
    if let Some(d) = headers.get("Digest").and_then(|v| v.to_str().ok()) {
        let Some((alg, value)) = d.split_once('=') else {
            return Err(anyhow::anyhow!("bad digest"));
        };
        if !alg.trim().eq_ignore_ascii_case("SHA-256") {
            return Err(anyhow::anyhow!("unsupported digest"));
        }
        let expected = B64.decode(value.trim().as_bytes()).unwrap_or_default();
        let actual = Sha256::digest(body);
        if expected.as_slice() != actual.as_slice() {
            return Err(anyhow::anyhow!("digest mismatch"));
        }
    }

    let params = parse_signature_header(&sig_header)?;
    let signing_string = build_signing_string(method, uri, headers, &params.headers)?;
    let pem = fetch_actor_public_key_pem(state, &actor_url).await?;
    if !verify_signature_rsa_sha256(&pem, &signing_string, &params.signature) {
        return Err(anyhow::anyhow!("signature invalid"));
    }
    Ok(actor_url)
}

async fn webrtc_send(
    State(state): State<AppState>,
    req: Request<Body>,
) -> impl IntoResponse {
    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, 256 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid body").into_response(),
    };
    let input: WebrtcSendReq = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid json").into_response(),
    };
    let from_actor = match verify_webrtc_signature(
        &state,
        &parts.headers,
        &parts.method,
        &parts.uri,
        &bytes,
    )
    .await
    {
        Ok(v) => v,
        Err(_) => return (StatusCode::UNAUTHORIZED, "invalid signature").into_response(),
    };

    let to_peer_id = input.to_peer_id.trim().to_string();
    if to_peer_id.is_empty() || to_peer_id.len() > 128 {
        return (StatusCode::BAD_REQUEST, "invalid to_peer_id").into_response();
    }
    let session_id = input.session_id.trim().to_string();
    if session_id.is_empty() || session_id.len() > 256 {
        return (StatusCode::BAD_REQUEST, "invalid session_id").into_response();
    }
    let kind = input.kind.trim().to_string();
    if kind.is_empty() || kind.len() > 64 {
        return (StatusCode::BAD_REQUEST, "invalid kind").into_response();
    }

    let now = now_ms();
    let mut signals = state.webrtc_signals.lock().await;
    let list = signals.entry(to_peer_id).or_insert_with(Vec::new);
    list.retain(|s| now.saturating_sub(s.created_at_ms) <= WEBRTC_SIGNAL_TTL_SECS * 1000);
    if list.len() >= WEBRTC_SIGNAL_MAX_PER_PEER {
        list.sort_by_key(|s| s.created_at_ms);
        let drop_count = list.len().saturating_sub(WEBRTC_SIGNAL_MAX_PER_PEER - 1);
        list.drain(0..drop_count);
    }
    let id = format!("sig-{}", generate_token());
    list.push(WebrtcSignal {
        id: id.clone(),
        from_actor,
        session_id,
        kind,
        payload: input.payload,
        created_at_ms: now,
    });

    axum::Json(serde_json::json!({ "ok": true, "id": id })).into_response()
}

async fn webrtc_poll(
    State(state): State<AppState>,
    req: Request<Body>,
) -> impl IntoResponse {
    let (parts, _body) = req.into_parts();
    if verify_webrtc_signature(&state, &parts.headers, &parts.method, &parts.uri, &[])
        .await
        .is_err()
    {
        return (StatusCode::UNAUTHORIZED, "invalid signature").into_response();
    }
    let query = parts.uri.query().unwrap_or("");
    let to_peer_id = query
        .split('&')
        .find(|p: &&str| p.starts_with("to_peer_id="))
        .and_then(|p: &str| p.split_once('='))
        .map(|(_, v): (&str, &str)| v.to_string())
        .unwrap_or_default();
    let limit = query
        .split('&')
        .find(|p: &&str| p.starts_with("limit="))
        .and_then(|p: &str| p.split_once('='))
        .and_then(|(_, v): (&str, &str)| v.parse::<u32>().ok())
        .unwrap_or(200);
    let to_peer_id = to_peer_id.trim().to_string();
    if to_peer_id.is_empty() || to_peer_id.len() > 128 {
        return (StatusCode::BAD_REQUEST, "invalid to_peer_id").into_response();
    }
    let limit = limit.max(1).min(200) as usize;

    let now = now_ms();
    let mut signals = state.webrtc_signals.lock().await;
    let list = signals.entry(to_peer_id).or_insert_with(Vec::new);
    list.retain(|s| now.saturating_sub(s.created_at_ms) <= WEBRTC_SIGNAL_TTL_SECS * 1000);
    let items = list.iter().take(limit).cloned().collect::<Vec<_>>();
    axum::Json(serde_json::json!({ "ok": true, "messages": items })).into_response()
}

async fn webrtc_ack(
    State(state): State<AppState>,
    req: Request<Body>,
) -> impl IntoResponse {
    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, 128 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid body").into_response(),
    };
    let input: WebrtcAckReq = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid json").into_response(),
    };
    if verify_webrtc_signature(&state, &parts.headers, &parts.method, &parts.uri, &bytes)
        .await
        .is_err()
    {
        return (StatusCode::UNAUTHORIZED, "invalid signature").into_response();
    }
    let to_peer_id = input.to_peer_id.trim().to_string();
    if to_peer_id.is_empty() || to_peer_id.len() > 128 {
        return (StatusCode::BAD_REQUEST, "invalid to_peer_id").into_response();
    }
    if input.ids.is_empty() {
        return axum::Json(serde_json::json!({ "ok": true, "deleted": 0 })).into_response();
    }

    let now = now_ms();
    let mut signals = state.webrtc_signals.lock().await;
    let list = signals.entry(to_peer_id).or_insert_with(Vec::new);
    list.retain(|s| now.saturating_sub(s.created_at_ms) <= WEBRTC_SIGNAL_TTL_SECS * 1000);
    let before = list.len();
    let ids = input.ids;
    list.retain(|s| !ids.contains(&s.id));
    let deleted = before.saturating_sub(list.len());
    axum::Json(serde_json::json!({ "ok": true, "deleted": deleted })).into_response()
}

async fn relay_telemetry_post(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    axum::Json(input): axum::Json<RelayTelemetry>,
) -> impl IntoResponse {
    // Optional auth.
    if let Some(expected) = &state.cfg.telemetry_token {
        let got = headers
            .get("X-Fedi3-Telemetry-Token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if got != expected {
            return (StatusCode::UNAUTHORIZED, "telemetry token required").into_response();
        }
    }

    if !state
        .limiter
        .check(
            peer_ip(&peer),
            "telemetry",
            state.cfg.rate_limit_forward_per_min,
        )
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }

    if !(input.relay_url.starts_with("http://") || input.relay_url.starts_with("https://")) {
        return (StatusCode::BAD_REQUEST, "invalid relay_url").into_response();
    }

    // Verify relay telemetry signature (TOFU pinning per relay_url).
    let provided_pk = match input.sign_pubkey_b64.as_deref() {
        Some(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => return (StatusCode::BAD_REQUEST, "missing sign_pubkey_b64").into_response(),
    };
    if input
        .signature_b64
        .as_deref()
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
    {
        return (StatusCode::BAD_REQUEST, "missing signature_b64").into_response();
    }

    // Store incoming relay + its advertised relays.
    let mut db = state.db.lock().await;

    if let Ok(Some(existing)) = db.get_relay_pubkey_b64(&input.relay_url) {
        if existing.trim() != provided_pk {
            return (StatusCode::UNAUTHORIZED, "relay pubkey mismatch").into_response();
        }
    }
    if let Err(_e) = verify_telemetry_signature(&input) {
        return (StatusCode::UNAUTHORIZED, "bad telemetry signature").into_response();
    }

    let telemetry_json = serde_json::to_string(&input).ok();
    let _ = db.upsert_relay(
        &input.relay_url,
        input.base_domain.clone(),
        telemetry_json,
        Some(provided_pk.clone()),
    );
    for r in &input.relays {
        if r.starts_with("http://") || r.starts_with("https://") {
            let _ = db.upsert_relay(r, None, None, None);
        }
    }
    for u in &input.users {
        let username = u.username.trim();
        let actor_url = u.actor_url.trim();
        if username.is_empty() || actor_url.is_empty() {
            continue;
        }
        let _ = db.upsert_relay_user_directory(username, actor_url, &input.relay_url);
        let stub = actor_stub_from_actor_url(username, actor_url, &user_base_template(&state.cfg));
        let doc = MeiliUserDoc {
            id: meili_doc_id(actor_url),
            username: username.to_string(),
            actor_url: actor_url.to_string(),
            actor_json: Some(serde_json::to_string(&stub).unwrap_or_default()),
            updated_at_ms: now_ms(),
        };
        state.meili_index_user(doc);
    }
    for p in &input.peers {
        let peer_id = p.peer_id.trim();
        let username = p.username.trim();
        let actor_url = p.actor_url.trim();
        if peer_id.is_empty() || username.is_empty() || actor_url.is_empty() {
            continue;
        }
        let _ = db.upsert_peer_directory(peer_id, username, actor_url);
        let stub = actor_stub_from_actor_url(username, actor_url, &user_base_template(&state.cfg));
        let doc = MeiliUserDoc {
            id: meili_doc_id(actor_url),
            username: username.to_string(),
            actor_url: actor_url.to_string(),
            actor_json: Some(serde_json::to_string(&stub).unwrap_or_default()),
            updated_at_ms: now_ms(),
        };
        state.meili_index_user(doc);
    }
    drop(db);

    // Reply with our telemetry snapshot (includes our known relays list).
    match build_self_telemetry(&state).await {
        Ok(t) => axum::Json(t).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, format!("telemetry error: {e}")).into_response(),
    }
}

#[derive(Debug, serde::Deserialize)]
struct RelayMoveReq {
    username: String,
    moved_to_actor: String,
}

async fn relay_move_post(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    axum::Json(req): axum::Json<RelayMoveReq>,
) -> impl IntoResponse {
    if !state
        .limiter
        .check(
            peer_ip(&peer),
            "forward",
            state.cfg.rate_limit_forward_per_min,
        )
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }

    let user = req.username.trim().to_string();
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid username").into_response();
    }
    let moved_to = req.moved_to_actor.trim().to_string();
    if !(moved_to.starts_with("http://") || moved_to.starts_with("https://")) {
        return (StatusCode::BAD_REQUEST, "invalid moved_to_actor").into_response();
    }

    let bearer = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    let db = state.db.lock().await;
    let authorized = if is_authorized_admin(&state.cfg, &headers) {
        true
    } else if let Some(tok) = bearer.as_deref() {
        db.verify_token(&user, tok).unwrap_or(false)
    } else {
        false
    };

    if !authorized {
        return (StatusCode::UNAUTHORIZED, "admin or user token required").into_response();
    }

    if let Err(e) = db.set_user_move(&user, &moved_to) {
        return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response();
    }
    (StatusCode::OK, "ok").into_response()
}

async fn relay_move_delete(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(user): Path<String>,
) -> impl IntoResponse {
    if !state
        .limiter
        .check(
            peer_ip(&peer),
            "forward",
            state.cfg.rate_limit_forward_per_min,
        )
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid username").into_response();
    }

    let bearer = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    let db = state.db.lock().await;
    let authorized = if is_authorized_admin(&state.cfg, &headers) {
        true
    } else if let Some(tok) = bearer.as_deref() {
        db.verify_token(&user, tok).unwrap_or(false)
    } else {
        false
    };
    if !authorized {
        return (StatusCode::UNAUTHORIZED, "admin or user token required").into_response();
    }
    let _ = db.clear_user_move(&user);
    (StatusCode::OK, "ok").into_response()
}

async fn relay_move_notice_post(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if !state
        .limiter
        .check(
            peer_ip(&peer),
            "forward",
            state.cfg.rate_limit_forward_per_min,
        )
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }

    let notice: RelayMoveNotice = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid json").into_response(),
    };
    let user = notice.username.trim().to_string();
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid username").into_response();
    }
    let moved_to = notice.moved_to_actor.trim().to_string();
    if !(moved_to.starts_with("http://") || moved_to.starts_with("https://")) {
        return (StatusCode::BAD_REQUEST, "invalid moved_to_actor").into_response();
    }

    // Hop protection.
    let hop = headers
        .get("X-Fedi3-Notice-Hop")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    if hop > 5 {
        return (StatusCode::OK, "ok").into_response();
    }

    let bearer = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    let sig_ok = match verify_move_notice_signature(&state, &headers, &user, &body).await {
        Ok(v) => v,
        Err(e) => {
            error!(%user, "move_notice signature error: {e}");
            false
        }
    };

    let db = state.db.lock().await;
    let authorized = if sig_ok {
        true
    } else if is_authorized_admin(&state.cfg, &headers) {
        true
    } else if let Some(tok) = bearer.as_deref() {
        db.verify_token(&user, tok).unwrap_or(false)
    } else {
        false
    };
    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            "signature or admin/user token required",
        )
            .into_response();
    }

    let notice_id = notice_id_hex(&notice);
    if db.has_move_notice(&notice_id).unwrap_or(false) {
        // Already seen: ensure mapping exists and stop.
        let _ = db.set_user_move(&user, &moved_to);
        return (StatusCode::OK, "ok").into_response();
    }

    if let Err(e) = db.set_user_move(&user, &moved_to) {
        return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response();
    }
    let _ = db.upsert_move_notice(
        &notice_id,
        &serde_json::to_string(&notice).unwrap_or_default(),
    );
    drop(db);

    // Fan-out the signed notice to other relays (best-effort).
    tokio::spawn(fanout_move_notice(
        state.clone(),
        notice_id,
        body.to_vec(),
        hop + 1,
    ));

    (StatusCode::OK, "ok").into_response()
}

async fn admin_delete_user(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(user): Path<String>,
) -> impl IntoResponse {
    let audit = match admin_guard(&state, &peer, &headers, "admin_delete_user", Some(&user)).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid user").into_response();
    }

    // Drop the tunnel sender (best-effort disconnect).
    state.tunnels.write().await.remove(&user);

    let db = state.db.lock().await;
    match db.delete_user(&user) {
        Ok(true) => {
            let _ = db.insert_admin_audit(
                "admin_delete_user",
                Some(&user),
                None,
                Some(&audit.ip),
                true,
                None,
                &audit.meta,
            );
            (StatusCode::OK, "deleted").into_response()
        }
        Ok(false) => {
            let _ = db.insert_admin_audit(
                "admin_delete_user",
                Some(&user),
                None,
                Some(&audit.ip),
                false,
                Some("not found"),
                &audit.meta,
            );
            (StatusCode::NOT_FOUND, "not found").into_response()
        }
        Err(e) => {
            let _ = db.insert_admin_audit(
                "admin_delete_user",
                Some(&user),
                None,
                Some(&audit.ip),
                false,
                Some("db error"),
                &audit.meta,
            );
            (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response()
        }
    }
}

async fn admin_delete_peer(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(peer_id): Path<String>,
) -> impl IntoResponse {
    info!("admin_delete_peer called: peer_id = {}", peer_id);
    let audit =
        match admin_guard(&state, &peer, &headers, "admin_delete_peer", Some(&peer_id)).await {
            Ok(v) => v,
            Err(resp) => {
                info!("admin_delete_peer: auth failed for peer_id = {}", peer_id);
                return resp;
            }
        };
    let db = state.db.lock().await;
    match db.delete_peer_directory_entry(&peer_id) {
        Ok(_) => {
            let _ = db.insert_admin_audit(
                "admin_delete_peer",
                Some(&peer_id),
                None,
                Some(&audit.ip),
                true,
                None,
                &audit.meta,
            );
            (StatusCode::OK, "deleted").into_response()
        }
        Err(e) => {
            let _ = db.insert_admin_audit(
                "admin_delete_peer",
                Some(&peer_id),
                None,
                Some(&audit.ip),
                false,
                Some("db error"),
                &audit.meta,
            );
            (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response()
        }
    }
}

async fn admin_audit_list(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let audit = match admin_guard(&state, &peer, &headers, "admin_audit_list", None).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let limit = q
        .get("limit")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(200)
        .min(500);
    let offset = q
        .get("offset")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let db = state.db.lock().await;
    match db.list_admin_audit(limit, offset) {
        Ok(rows) => {
            let _ = db.insert_admin_audit(
                "admin_audit_list",
                None,
                None,
                Some(&audit.ip),
                true,
                None,
                &audit.meta,
            );
            axum::Json(
                rows.into_iter()
                    .map(
                        |(
                            id,
                            action,
                            username,
                            actor,
                            ip,
                            ok,
                            detail,
                            created_at_ms,
                            request_id,
                            correlation_id,
                            user_agent,
                        )| {
                            serde_json::json!({
                                "id": id,
                                "action": action,
                                "username": username,
                                "actor": actor,
                                "ip": ip,
                                "ok": ok,
                                "detail": detail,
                                "created_at_ms": created_at_ms,
                                "request_id": request_id,
                                "correlation_id": correlation_id,
                                "user_agent": user_agent
                            })
                        },
                    )
                    .collect::<Vec<_>>(),
            )
            .into_response()
        }
        Err(e) => {
            let _ = db.insert_admin_audit(
                "admin_audit_list",
                None,
                None,
                Some(&audit.ip),
                false,
                Some("db error"),
                &audit.meta,
            );
            (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response()
        }
    }
}

fn generate_token() -> String {
    // 24 random bytes -> 48 hex chars
    let mut b = [0u8; 24];
    use rand::RngCore as _;
    rand::rngs::OsRng.fill_bytes(&mut b);
    b.iter().map(|v| format!("{v:02x}")).collect()
}

fn generate_media_id(ext: &str) -> String {
    let mut b = [0u8; 16];
    use rand::RngCore as _;
    rand::rngs::OsRng.fill_bytes(&mut b);
    let id = b.iter().map(|v| format!("{v:02x}")).collect::<String>();
    let ext = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    if ext.is_empty() {
        id
    } else {
        format!("{id}.{ext}")
    }
}

fn token_hash_hex(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let bytes = hasher.finalize();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn meili_doc_id(raw: &str) -> String {
    token_hash_hex(raw)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

async fn fetch_peer_hello(
    state: &AppState,
    user: &str,
    tunnel_tx: mpsc::Sender<TunnelRequest>,
) -> Result<Option<PeerHello>> {
    let id = format!("{user}-hello-{}", REQ_ID.fetch_add(1, Ordering::Relaxed));
    let req = RelayHttpRequest {
        id: id.clone(),
        method: Method::GET.to_string(),
        path: "/_fedi3/hello".to_string(),
        query: "".to_string(),
        headers: vec![("accept".to_string(), "application/json".to_string())],
        body_b64: "".to_string(),
    };
    let (resp_tx, resp_rx) = oneshot::channel();
    let msg = TunnelRequest {
        id: id.clone(),
        req,
        resp_tx,
    };
    if tunnel_tx.send(msg).await.is_err() {
        return Ok(None);
    }
    let Ok(resp) =
        tokio::time::timeout(Duration::from_secs(state.cfg.tunnel_timeout_secs), resp_rx).await
    else {
        return Ok(None);
    };
    let Ok(resp) = resp else { return Ok(None) };
    if resp.status != 200 {
        return Ok(None);
    }
    let bytes = match B64.decode(resp.body_b64.as_bytes()) {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    let hello: PeerHello = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    Ok(Some(hello))
}

async fn build_self_telemetry(state: &AppState) -> Result<RelayTelemetry> {
    let online_users = state.tunnels.read().await.len() as u64;
    let online_peers = online_users;

    let peers_seen_window_ms: i64 = 30 * 24 * 3600 * 1000;
    let cutoff_ms = now_ms().saturating_sub(peers_seen_window_ms);
    let search_window_ms: i64 = 24 * 3600 * 1000;
    let search_cutoff_ms = now_ms().saturating_sub(search_window_ms);
    let relay_sync_window_ms: i64 = 24 * 3600 * 1000;
    let relay_sync_cutoff_ms = now_ms().saturating_sub(relay_sync_window_ms);
    let relay_p2p_peer_id = state.relay_mesh_peer_id.read().await.clone();
    let (
        total_users,
        total_peers_seen,
        relays,
        users,
        peers,
        search_indexed_users,
        search_last_index_ms,
        search_relays_total,
        search_relays_synced,
        search_relays_last_sync_ms,
    ) = {
        let db = state.db.lock().await;
        let total_users = db.count_users_total().unwrap_or(0);
        let total_peers_seen = db.count_peers_seen_since(cutoff_ms).unwrap_or(0);
        let relays = db
            .list_relays(500)
            .unwrap_or_default()
            .into_iter()
            .map(|(url, _, _, _, _)| url)
            .collect::<Vec<_>>();
        let relay_sync = db.list_relay_sync_state().unwrap_or_default();
        let mut relays_total = 0u64;
        let mut relays_synced = 0u64;
        let mut last_sync_ms = None;
        for (relay_url, last_ms) in relay_sync {
            let _ = relay_url;
            relays_total += 1;
            if last_ms >= relay_sync_cutoff_ms {
                relays_synced += 1;
            }
            if last_sync_ms.map(|v| last_ms > v).unwrap_or(true) {
                last_sync_ms = Some(last_ms);
            }
        }
        let search_indexed_users = db.count_outbox_indexed_since(search_cutoff_ms).unwrap_or(0);
        let search_last_index_ms = db
            .relay_meta_get("search_index_last_ms")
            .ok()
            .flatten()
            .and_then(|v| v.parse::<i64>().ok());
        let users = db
            .list_users(state.cfg.telemetry_users_limit, 0)
            .unwrap_or_default()
            .into_iter()
            .map(|(username, _, _)| RelayUserEntry {
                actor_url: format!(
                    "{}/users/{}",
                    user_base_url(&state.cfg, &username),
                    username
                ),
                username,
            })
            .collect::<Vec<_>>();
        let peers = db
            .list_peer_directory("", state.cfg.telemetry_peers_limit, Some(cutoff_ms))
            .unwrap_or_default()
            .into_iter()
            .map(|(peer_id, username, actor_url)| RelayPeerEntry {
                peer_id,
                username,
                actor_url,
            })
            .collect::<Vec<_>>();
        (
            total_users,
            total_peers_seen,
            relays,
            users,
            peers,
            search_indexed_users,
            search_last_index_ms,
            relays_total,
            relays_synced,
            last_sync_ms,
        )
    };

    let mut telemetry = RelayTelemetry {
        relay_url: state
            .cfg
            .public_url
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        timestamp_ms: now_ms(),
        online_users,
        online_peers,
        total_users,
        total_peers_seen,
        peers_seen_window_ms,
        peers_seen_cutoff_ms: cutoff_ms,
        base_domain: state.cfg.base_domain.clone(),
        relays,
        search_indexed_users: Some(search_indexed_users),
        search_total_users: Some(total_users),
        search_last_index_ms,
        search_window_ms: Some(search_window_ms),
        search_relays_total: Some(search_relays_total),
        search_relays_synced: Some(search_relays_synced),
        search_relays_last_sync_ms: search_relays_last_sync_ms,
        search_relay_sync_window_ms: Some(relay_sync_window_ms),
        p2p_upnp_port_start: state.cfg.p2p_upnp_port_start,
        p2p_upnp_port_end: state.cfg.p2p_upnp_port_end,
        relay_p2p_peer_id,
        sign_pubkey_b64: None,
        signature_b64: None,
        users,
        peers,
    };

    // Sign telemetry with our relay keypair.
    if state.cfg.public_url.is_some() {
        let db = state.db.lock().await;
        let (pk_b64, sk_b64) = db.load_or_create_signing_keypair_b64()?;
        telemetry.sign_pubkey_b64 = Some(pk_b64);
        telemetry.signature_b64 = Some(sign_telemetry_b64(&telemetry, &sk_b64)?);
    }

    Ok(telemetry)
}

fn telemetry_bytes_for_signing(t: &RelayTelemetry) -> Result<Vec<u8>> {
    let mut clone = t.clone();
    clone.signature_b64 = None;
    let bytes = serde_json::to_vec(&clone)?;
    Ok(bytes)
}

fn sign_telemetry_b64(t: &RelayTelemetry, sk_b64: &str) -> Result<String> {
    let sk_bytes = B64.decode(sk_b64.as_bytes())?;
    if sk_bytes.len() != 32 {
        return Err(anyhow::anyhow!("bad signing key length"));
    }
    let mut sk = [0u8; 32];
    sk.copy_from_slice(&sk_bytes);
    let signing = ed25519_dalek::SigningKey::from_bytes(&sk);

    let bytes = telemetry_bytes_for_signing(t)?;
    let sig: ed25519_dalek::Signature = signing.sign(&bytes);
    Ok(B64.encode(sig.to_bytes()))
}

fn verify_telemetry_signature(t: &RelayTelemetry) -> Result<()> {
    let pk_b64 = t
        .sign_pubkey_b64
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing pubkey"))?
        .trim();
    let sig_b64 = t
        .signature_b64
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing signature"))?
        .trim();

    let pk_bytes = B64.decode(pk_b64.as_bytes())?;
    if pk_bytes.len() != 32 {
        return Err(anyhow::anyhow!("bad pubkey length"));
    }
    let mut pk = [0u8; 32];
    pk.copy_from_slice(&pk_bytes);
    let verifying = ed25519_dalek::VerifyingKey::from_bytes(&pk)?;

    let sig_bytes = B64.decode(sig_b64.as_bytes())?;
    if sig_bytes.len() != 64 {
        return Err(anyhow::anyhow!("bad signature length"));
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let sig = ed25519_dalek::Signature::from_bytes(&sig_arr);

    // Basic freshness check (best-effort): reject extremely old/future telemetry to reduce replay abuse.
    let now = now_ms();
    if (t.timestamp_ms - now).abs() > 24 * 3600 * 1000 {
        return Err(anyhow::anyhow!("telemetry timestamp out of range"));
    }

    let bytes = telemetry_bytes_for_signing(t)?;
    verifying.verify(&bytes, &sig)?;
    Ok(())
}

async fn push_telemetry_once(state: &AppState) -> Result<()> {
    let Some(self_url) = state.cfg.public_url.clone() else {
        return Ok(());
    };
    let telemetry = build_self_telemetry(state).await?;

    let targets = {
        let db = state.db.lock().await;
        let mut out = db
            .list_relays(500)
            .unwrap_or_default()
            .into_iter()
            .map(|(url, _, _, _, _)| url)
            .collect::<Vec<_>>();
        // Ensure seeds are included.
        for r in &state.cfg.seed_relays {
            out.push(r.clone());
        }
        out.sort();
        out.dedup();
        out.retain(|u| u != &self_url);
        out
    };

    for relay_url in targets {
        let url = format!("{}/_fedi3/relay/telemetry", relay_url.trim_end_matches('/'));
        let mut req = state.http.post(url).json(&telemetry);
        if let Some(tok) = &state.cfg.telemetry_token {
            req = req.header("X-Fedi3-Telemetry-Token", tok);
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(_) => continue,
        };
        if !resp.status().is_success() {
            continue;
        }
        if let Ok(remote) = resp.json::<RelayTelemetry>().await {
            if verify_telemetry_signature(&remote).is_err() {
                continue;
            }
            let telemetry_json = serde_json::to_string(&remote).ok();
            let mut db = state.db.lock().await;
            let _ = db.upsert_relay(
                &remote.relay_url,
                remote.base_domain.clone(),
                telemetry_json,
                remote.sign_pubkey_b64.clone(),
            );
            for r in remote.relays {
                if r.starts_with("http://") || r.starts_with("https://") {
                    let _ = db.upsert_relay(&r, None, None, None);
                }
            }
            for u in &remote.users {
                let username = u.username.trim();
                let actor_url = u.actor_url.trim();
                if username.is_empty() || actor_url.is_empty() {
                    continue;
                }
                let _ = db.upsert_relay_user_directory(username, actor_url, &remote.relay_url);
                let stub =
                    actor_stub_from_actor_url(username, actor_url, &user_base_template(&state.cfg));
                let doc = MeiliUserDoc {
                    id: meili_doc_id(actor_url),
                    username: username.to_string(),
                    actor_url: actor_url.to_string(),
                    actor_json: Some(serde_json::to_string(&stub).unwrap_or_default()),
                    updated_at_ms: now_ms(),
                };
                state.meili_index_user(doc);
            }
            for p in &remote.peers {
                let peer_id = p.peer_id.trim();
                let username = p.username.trim();
                let actor_url = p.actor_url.trim();
                if peer_id.is_empty() || username.is_empty() || actor_url.is_empty() {
                    continue;
                }
                let _ = db.upsert_peer_directory(peer_id, username, actor_url);
                let stub =
                    actor_stub_from_actor_url(username, actor_url, &user_base_template(&state.cfg));
                let doc = MeiliUserDoc {
                    id: meili_doc_id(actor_url),
                    username: username.to_string(),
                    actor_url: actor_url.to_string(),
                    actor_json: Some(serde_json::to_string(&stub).unwrap_or_default()),
                    updated_at_ms: now_ms(),
                };
                state.meili_index_user(doc);
            }
        }
    }
    Ok(())
}

async fn sync_relays_once(state: &AppState) -> Result<()> {
    let self_url = state.cfg.public_url.clone();
    let relays = {
        let db = state.db.lock().await;
        let mut out = db
            .list_relays(500)
            .unwrap_or_default()
            .into_iter()
            .map(|(url, _, _, telemetry_json, _)| (url, telemetry_json))
            .collect::<Vec<_>>();
        for r in &state.cfg.seed_relays {
            out.push((r.clone(), None));
        }
        out.sort();
        out.dedup();
        if let Some(self_url) = &self_url {
            out.retain(|(u, _)| u != self_url);
        }
        out
    };

    for (relay_url, telemetry_json) in relays {
        if state.cfg.relay_mesh_enable {
            if let Some(json) = telemetry_json.as_deref() {
                if telemetry_has_mesh_peer_id(json) {
                    continue;
                }
            }
        }
        if let Err(e) = sync_relay_notes(state, &relay_url).await {
            error!(relay_url = %relay_url, "relay http sync failed: {e:#}");
        }
    }
    Ok(())
}

async fn sync_relay_notes(state: &AppState, relay_url: &str) -> Result<()> {
    info!(relay_url = %relay_url, "relay http sync start");
    let key = format!("relay_sync_last_ms:{relay_url}");
    let last_seen = {
        let db = state.db.lock().await;
        db.relay_meta_get(&key)
            .ok()
            .flatten()
            .and_then(|v| v.parse::<i64>().ok())
    };
    let limit = state.cfg.relay_sync_limit.min(200).max(1);
    let mut cursor = None;
    let mut since = last_seen;
    let mut max_seen = last_seen.unwrap_or(0);
    let mut pages = 0u32;
    let mut total_items = 0usize;

    while pages < 3 {
        let mut url = format!(
            "{}/_fedi3/relay/sync/notes?limit={}",
            relay_url.trim_end_matches('/'),
            limit
        );
        if let Some(s) = since {
            url.push_str(&format!("&since={s}"));
        }
        if let Some(c) = cursor {
            url.push_str(&format!("&cursor={c}"));
        }
        let resp = match state.http.get(url).send().await {
            Ok(r) => r,
            Err(_) => break,
        };
        if !resp.status().is_success() {
            break;
        }
        let data = match resp.json::<RelaySyncNotesResponse>().await {
            Ok(v) => v,
            Err(_) => break,
        };
        if data.items.is_empty() {
            break;
        }
        total_items += data.items.len();
        let db = state.db.lock().await;
        for item in data.items {
            if item.created_at_ms > max_seen {
                max_seen = item.created_at_ms;
            }
            if let Some(mut indexed) = note_to_index(&item.note) {
                indexed.created_at_ms = item.created_at_ms;
                let _ = db.upsert_relay_note(&indexed);
            }
            for mut media in extract_media_from_note(&item.note) {
                media.created_at_ms = item.created_at_ms;
                let _ = db.upsert_relay_media(&media);
            }
            if let Some(mut actor_idx) = actor_to_index_from_note(&item.note) {
                actor_idx.updated_at_ms = item.created_at_ms;
                let _ = db.upsert_relay_actor(&actor_idx);
            }
        }
        drop(db);
        pages += 1;
        if let Some(next) = data.next.and_then(|v| v.parse::<i64>().ok()) {
            cursor = Some(next);
            since = None;
        } else {
            break;
        }
    }

    if max_seen > last_seen.unwrap_or(0) {
        let db = state.db.lock().await;
        let _ = db.relay_meta_set(&key, &max_seen.to_string());
    }
    if total_items > 0 {
        info!(
            relay_url = %relay_url,
            items = total_items,
            max_seen = max_seen,
            "relay http sync applied"
        );
    }
    Ok(())
}

fn telemetry_has_mesh_peer_id(telemetry_json: &str) -> bool {
    let Ok(t) = serde_json::from_str::<RelayTelemetry>(telemetry_json) else {
        return false;
    };
    t.relay_p2p_peer_id
        .as_ref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn host_only(headers: &HeaderMap) -> &str {
    headers
        .get("Host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost")
        .split(':')
        .next()
        .unwrap_or("localhost")
}

fn actor_stub_json(username: &str, base_template: &str) -> serde_json::Value {
    let base = base_template
        .replace("{user}", username)
        .trim_end_matches('/')
        .to_string();
    let id = if base.contains("/users/") {
        base.clone()
    } else {
        format!("{base}/users/{username}")
    };
    let inbox = format!("{base}/inbox");
    let outbox = format!("{base}/users/{username}/outbox");
    serde_json::json!({
      "@context": "https://www.w3.org/ns/activitystreams",
      "id": id,
      "type": "Person",
      "preferredUsername": username,
      "inbox": inbox,
      "outbox": outbox,
    })
}

fn actor_stub_from_actor_url(
    username: &str,
    actor_url: &str,
    fallback_base: &str,
) -> serde_json::Value {
    if actor_url.trim().is_empty() {
        return actor_stub_json(username, fallback_base);
    }
    serde_json::json!({
      "@context": "https://www.w3.org/ns/activitystreams",
      "id": actor_url.trim(),
      "type": "Person",
      "preferredUsername": username,
    })
}

fn origin_for_links_with_cfg(cfg: &RelayConfig, headers: &HeaderMap) -> (String, String) {
    // For host-based routing, the Host header is the canonical origin (per-user subdomain).
    if cfg.base_domain.is_some() {
        return (
            scheme_from_headers(headers).to_string(),
            host_only(headers).to_string(),
        );
    }
    if let Some((s, h)) = canonical_origin(cfg) {
        return (s, h);
    }
    (
        scheme_from_headers(headers).to_string(),
        host_only(headers).to_string(),
    )
}

fn canonical_origin(cfg: &RelayConfig) -> Option<(String, String)> {
    let public_url = cfg.public_url.as_ref()?;
    let uri: http::Uri = public_url.parse().ok()?;
    let scheme = uri.scheme_str()?.to_string();
    let authority = uri.authority()?.as_str().to_string();
    Some((scheme, authority))
}

fn maybe_redirect_canonical(
    cfg: &RelayConfig,
    headers: &HeaderMap,
    method: &Method,
    path: &str,
    raw_query: Option<&str>,
) -> Option<Response> {
    // Never redirect non-idempotent requests: POST inbox signatures would break.
    if *method != Method::GET && *method != Method::HEAD {
        return None;
    }
    // Only enforce canonical origin in path-based mode.
    if cfg.base_domain.is_some() {
        return None;
    }
    let Some((canon_scheme, canon_host)) = canonical_origin(cfg) else {
        return None;
    };

    let cur_scheme = scheme_from_headers(headers);
    let cur_host = host_only(headers);
    if cur_scheme.eq_ignore_ascii_case(&canon_scheme) && cur_host.eq_ignore_ascii_case(&canon_host)
    {
        return None;
    }

    let qs = raw_query.map(|q| format!("?{q}")).unwrap_or_default();
    let location = format!("{canon_scheme}://{canon_host}{path}{qs}");
    Some((StatusCode::PERMANENT_REDIRECT, [("Location", location)], "").into_response())
}

fn scheme_from_headers(headers: &HeaderMap) -> &str {
    if let Some(v) = headers
        .get("X-Forwarded-Proto")
        .and_then(|v| v.to_str().ok())
    {
        if v.eq_ignore_ascii_case("https") {
            return "https";
        }
        if v.eq_ignore_ascii_case("http") {
            return "http";
        }
    }
    if let Some(v) = headers.get("Forwarded").and_then(|v| v.to_str().ok()) {
        // Forwarded: for=...;proto=https;host=...
        let lower = v.to_ascii_lowercase();
        if lower.contains("proto=https") {
            return "https";
        }
    }
    "http"
}

fn notice_id_hex(notice: &RelayMoveNotice) -> String {
    let json = serde_json::to_vec(notice).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&json);
    let bytes = hasher.finalize();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

async fn fanout_move_notice(state: AppState, notice_id: String, body: Vec<u8>, hop: u32) {
    let relays = {
        let db = state.db.lock().await;
        db.list_relays(200)
            .unwrap_or_default()
            .into_iter()
            .map(|(url, _base, _seen, _t, _pk)| url)
            .collect::<Vec<_>>()
    };
    for relay_url in relays {
        let _ = fanout_move_notice_to_relay(&state, &notice_id, &relay_url, &body, hop).await;
    }
}

async fn fanout_move_notice_to_relay(
    state: &AppState,
    notice_id: &str,
    relay_url: &str,
    body: &[u8],
    hop: u32,
) -> Result<bool> {
    if let Some(self_url) = state.cfg.public_url.as_ref() {
        if relay_url.trim_end_matches('/') == self_url.trim_end_matches('/') {
            return Ok(false);
        }
    }

    // Retry/backoff per (notice_id, relay_url).
    {
        let db = state.db.lock().await;
        if let Ok(Some((tries, last_try_ms, sent_ok))) = db.get_fanout_status(notice_id, relay_url)
        {
            if sent_ok != 0 {
                return Ok(true);
            }
            let now = now_ms();
            let backoff_ms = (2_i64.saturating_pow((tries as u32).min(8))) * 1_000;
            if now.saturating_sub(last_try_ms) < backoff_ms {
                return Ok(false);
            }
        }
    }

    let url = format!(
        "{}/_fedi3/relay/move_notice",
        relay_url.trim_end_matches('/')
    );
    let resp = state
        .http
        .post(url)
        .header("Content-Type", "application/json")
        .header("X-Fedi3-Notice-Id", notice_id.to_string())
        .header("X-Fedi3-Notice-Hop", hop.to_string())
        .body(body.to_vec())
        .send()
        .await;
    let ok = resp
        .as_ref()
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    let db = state.db.lock().await;
    let _ = db.record_fanout_attempt(notice_id, relay_url, ok);
    Ok(ok)
}

async fn fanout_pending_move_notices(state: &AppState) -> Result<()> {
    let cutoff = now_ms().saturating_sub((state.cfg.move_notice_ttl_secs as i64) * 1000);
    let items = {
        let db = state.db.lock().await;
        db.list_recent_move_notices(cutoff, 200).unwrap_or_default()
    };
    if items.is_empty() {
        return Ok(());
    }

    let relays = {
        let db = state.db.lock().await;
        db.list_relays(200)
            .unwrap_or_default()
            .into_iter()
            .map(|(url, _base, _seen, _t, _pk)| url)
            .collect::<Vec<_>>()
    };

    for (notice_id, notice_json, _created_at_ms) in items {
        let body = notice_json.as_bytes();
        for relay_url in &relays {
            let _ = fanout_move_notice_to_relay(state, &notice_id, relay_url, body, 1).await;
        }
    }
    Ok(())
}

#[derive(Debug)]
struct SignatureParams {
    headers: Vec<String>,
    signature: Vec<u8>,
}

fn parse_signature_header(value: &str) -> Result<SignatureParams> {
    // Signature: keyId="...",headers="(request-target) host date",signature="base64..."
    let mut map = std::collections::HashMap::<String, String>::new();
    for part in value.split(',') {
        let part = part.trim();
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let v = v.trim().trim_matches('"');
        map.insert(k.trim().to_string(), v.to_string());
    }

    let headers = map
        .get("headers")
        .cloned()
        .unwrap_or_else(|| "date".to_string());
    let signature_b64 = map
        .get("signature")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Signature missing signature"))?;

    let signature = B64.decode(signature_b64.as_bytes())?;
    Ok(SignatureParams {
        headers: headers
            .split_whitespace()
            .map(|s| s.to_ascii_lowercase())
            .collect(),
        signature,
    })
}

fn build_signing_string(
    method: &Method,
    uri: &http::Uri,
    headers: &HeaderMap,
    signed_headers: &[String],
) -> Result<String> {
    let mut out = String::new();
    for (i, name) in signed_headers.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if name == "(request-target)" {
            let path = uri.path();
            let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
            out.push_str("(request-target): ");
            out.push_str(&method.as_str().to_ascii_lowercase());
            out.push(' ');
            out.push_str(path);
            out.push_str(&query);
            continue;
        }
        let header_name = HeaderName::from_bytes(name.as_bytes())?;
        let value = headers
            .get(&header_name)
            .ok_or_else(|| anyhow::anyhow!("missing signed header: {name}"))?
            .to_str()?;
        out.push_str(name);
        out.push_str(": ");
        out.push_str(value.trim());
    }
    Ok(out)
}

fn verify_signature_rsa_sha256(
    public_key_pem: &str,
    signing_string: &str,
    signature: &[u8],
) -> bool {
    use rsa::{pkcs1v15::VerifyingKey, pkcs8::DecodePublicKey, signature::Verifier, RsaPublicKey};
    let Ok(public_key) = RsaPublicKey::from_public_key_pem(public_key_pem) else {
        return false;
    };
    let verifying_key = VerifyingKey::<Sha256>::new(public_key);
    let Ok(sig) = rsa::pkcs1v15::Signature::try_from(signature) else {
        return false;
    };
    verifying_key
        .verify(signing_string.as_bytes(), &sig)
        .is_ok()
}

fn extract_public_key_pem_from_actor_json(actor_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(actor_json).ok()?;
    let pk = v.get("publicKey")?;
    let pem = pk.get("publicKeyPem")?.as_str()?;
    let pem = pem.trim();
    if pem.is_empty() {
        None
    } else {
        Some(pem.to_string())
    }
}

async fn verify_move_notice_signature(
    state: &AppState,
    headers: &HeaderMap,
    user: &str,
    body: &[u8],
) -> Result<bool> {
    let sig = headers
        .get("Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if sig.trim().is_empty() {
        return Ok(false);
    }

    // Date skew (5 minutes).
    let date = headers
        .get("Date")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if date.is_empty() {
        return Ok(false);
    }
    let ts = parse_http_date(date)?;
    let now = std::time::SystemTime::now();
    let diff = if now > ts {
        now.duration_since(ts).unwrap_or_default()
    } else {
        ts.duration_since(now).unwrap_or_default()
    };
    if diff > Duration::from_secs(300) {
        return Ok(false);
    }

    // Digest check if present.
    if let Some(d) = headers.get("Digest").and_then(|v| v.to_str().ok()) {
        let Some((alg, value)) = d.split_once('=') else {
            return Ok(false);
        };
        if !alg.trim().eq_ignore_ascii_case("SHA-256") {
            return Ok(false);
        }
        let expected = B64.decode(value.trim().as_bytes()).unwrap_or_default();
        let actual = Sha256::digest(body);
        if expected.as_slice() != actual.as_slice() {
            return Ok(false);
        }
    }

    let params = parse_signature_header(sig)?;
    let uri: http::Uri = "/_fedi3/relay/move_notice".parse()?;
    let signing_string = build_signing_string(&Method::POST, &uri, headers, &params.headers)?;

    let mut pem = {
        let db = state.db.lock().await;
        db.get_actor_cache(user)
            .ok()
            .flatten()
            .and_then(|actor_json| extract_public_key_pem_from_actor_json(&actor_json))
            .unwrap_or_default()
    };
    if pem.trim().is_empty() {
        // Fallback: try fetching old actor URL (helps relays that didn't previously cache the user).
        if let Ok(notice) = serde_json::from_slice::<RelayMoveNotice>(body) {
            if let Some(old_actor) = notice
                .old_actor
                .as_deref()
                .map(str::trim)
                .filter(|v| v.starts_with("http://") || v.starts_with("https://"))
            {
                if let Ok(resp) = state
                    .http
                    .get(old_actor)
                    .header("Accept", "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"")
                    .send()
                    .await
                {
                    if let Ok(text) = resp.text().await {
                        if let Some(got) = extract_public_key_pem_from_actor_json(&text) {
                            pem = got;
                        }
                    }
                }
            }
        }
    }
    if pem.trim().is_empty() {
        return Ok(false);
    }
    Ok(verify_signature_rsa_sha256(
        &pem,
        &signing_string,
        &params.signature,
    ))
}
