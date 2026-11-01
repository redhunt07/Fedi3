/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, OriginalUri, Path, Query, RawQuery, State,
    },
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    middleware::{from_fn, from_fn_with_state, Next},
    response::{IntoResponse, Redirect, Response},
    routing::{any, get, post, put},
    Router,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use bytes::Bytes;
use deadpool::managed::QueueMode;
use deadpool_postgres::{ManagerConfig, Pool, PoolConfig, RecyclingMethod, Runtime, Timeouts};
use fedi3_protocol::{RelayHttpRequest, RelayHttpResponse};
use futures_util::{SinkExt, StreamExt};
use http::header;
use httpdate::parse_http_date;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use rsa::pkcs8::DecodePublicKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
use tokio::sync::{mpsc, oneshot, RwLock, Semaphore};
use tokio_postgres::types::ToSql;
use tokio_postgres::{NoTls, Row};
use tokio_util::sync::CancellationToken;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, info_span, warn};

use ed25519_dalek::{Signer as _, Verifier as _};
use rusqlite::{params, Connection, OptionalExtension};
use subtle::ConstantTimeEq;

mod media_store;

static REQ_ID: AtomicU64 = AtomicU64::new(1);
static TUNNEL_CONN_ID: AtomicU64 = AtomicU64::new(1);
const DB_BATCH_DELETE_MAX: usize = 500;
/// Current relay schema version. Bumped by every numbered migration.
const SCHEMA_VERSION: i64 = 2;

fn next_request_id() -> String {
    let id = REQ_ID.fetch_add(1, Ordering::Relaxed);
    format!("req-{id}")
}

#[derive(Debug, Clone, serde::Deserialize)]
struct PeerHello {
    username: String,
    actor: String,
    #[allow(dead_code)]
    core_version: String,
    p2p: Option<PeerHelloP2p>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct PeerHelloP2p {
    peer_id: Option<String>,
    #[allow(dead_code)]
    addrs: Option<Vec<String>>,
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
    sign_pubkey_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signature_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    users: Vec<RelayUserEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    peers: Vec<RelayPeerEntry>,
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

#[derive(Debug, Clone)]
struct RelayBlockRow {
    kind: String,
    value: String,
    reason: Option<String>,
    expires_at_ms: Option<i64>,
}

struct SpoolItem {
    id: i64,
    method: String,
    path: String,
    query: String,
    headers_json: String,
    body_b64: String,
    created_at_ms: i64,
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
struct RelayNoteIndex {
    note_id: String,
    actor_id: Option<String>,
    published_ms: Option<i64>,
    content_text: String,
    content_html: String,
    note_json: String,
    created_at_ms: i64,
    tags: Vec<String>,
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
    conn_id: u64,
    inflight: Arc<RwLock<HashMap<String, oneshot::Sender<RelayHttpResponse>>>>,
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

impl MeiliIndexer {
    fn new(search: Arc<MeiliSearch>, batch_max: usize, flush_ms: u64, queue_max: usize) -> Self {
        let (tx, mut rx) = mpsc::channel(queue_max.max(16));
        let batch_max = batch_max.clamp(1, 500);
        let flush_ms = flush_ms.clamp(50, 5_000);
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
            ttl_secs: ttl_secs.clamp(5, 300),
            max_entries: max_entries.clamp(64, 10_000),
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
    seed_relays: Vec<String>,
    p2p_infra_peer_id: Option<String>,
    p2p_infra_multiaddrs: Vec<String>,
    p2p_infra_host: Option<String>,
    p2p_infra_port: u16,
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
    webrtc_signal_ttl_secs: u64,
    webrtc_signal_cleanup_interval_secs: u64,
    webrtc_signal_max_per_peer: u32,
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
    outbox_index_interval_secs: u64,
    outbox_index_pages: u32,
    outbox_index_page_limit: u32,
    telemetry_users_limit: u32,
    telemetry_peers_limit: u32,
    relay_sync_interval_secs: u64,
    relay_sync_limit: u32,
    allow_private_actor_fetch: bool,
    /// Allow relay-initiated fetches to resolve to loopback/private/link-local
    /// addresses (LAN-only federation). Off by default (SSRF guard, H-35).
    allow_private_remote_fetch: bool,
    forward_send_timeout_ms: u64,
    inbox_dedupe_ttl_secs: u64,
    admin_audit_ttl_secs: u64,
    relay_notes_ttl_secs: u64,
    peer_registry_ttl_secs: u64,
    relay_registry_ttl_secs: u64,
    relay_user_directory_ttl_secs: u64,
    media_max_bytes_per_user: u64,
    spool_max_bytes_per_user: u64,
    /// How often the relay refreshes the user directory of its peers (seconds).
    peer_sync_secs: u64,
    /// Maximum relay-to-relay hops before giving up on routing a request.
    peer_max_hops: u32,
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
        block_on_result(self.client.execute(stmt, params))
    }

    fn transaction(&mut self) -> Result<PgTx<'_>> {
        let tx = block_on_result(self.client.transaction())?;
        Ok(PgTx { tx })
    }

    fn batch_execute(&mut self, stmt: &str) -> Result<()> {
        block_on_result(self.client.batch_execute(stmt))
    }

    fn query(&mut self, stmt: &str, params: &[&(dyn ToSql + Sync)]) -> Result<Vec<Row>> {
        block_on_result(self.client.query(stmt, params))
    }

    fn query_one(&mut self, stmt: &str, params: &[&(dyn ToSql + Sync)]) -> Result<Row> {
        block_on_result(self.client.query_one(stmt, params))
    }

    fn query_opt(&mut self, stmt: &str, params: &[&(dyn ToSql + Sync)]) -> Result<Option<Row>> {
        block_on_result(self.client.query_opt(stmt, params))
    }
}

impl<'a> PgTx<'a> {
    fn execute(&mut self, stmt: &str, params: &[&(dyn ToSql + Sync)]) -> Result<u64> {
        block_on_result(self.tx.execute(stmt, params))
    }

    fn commit(self) -> Result<()> {
        block_on_result(self.tx.commit())
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
struct WebRtcSignalPollQuery {
    to_peer_id: Option<String>,
    limit: Option<u32>,
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

#[derive(Debug, serde::Deserialize)]
struct WebRtcSignalSendReq {
    to_peer_id: String,
    session_id: String,
    kind: String, // offer|answer|candidate
    payload: serde_json::Value,
}

#[derive(Debug, serde::Deserialize)]
struct WebRtcSignalAckReq {
    to_peer_id: String,
    ids: Vec<String>,
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
            "limit": limit.clamp(1, 200),
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
        let next = if items.len() as u32 == limit.clamp(1, 200) {
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
            "limit": limit.clamp(1, 200),
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
        let next = if items.len() as u32 == limit.clamp(1, 200) {
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
    let base_url = cfg.meili_url.as_ref()?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(cfg.meili_timeout_secs))
        .redirect(peer_fetch_redirect_policy(cfg.allow_private_remote_fetch))
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

    let cfg = load_config().unwrap_or_else(|e| {
        eprintln!("fedi3_relay config error: {e:#}");
        std::process::exit(2);
    });
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
        .redirect(peer_fetch_redirect_policy(cfg.allow_private_remote_fetch))
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

    // Relay mesh: periodically learn which users live on peer relays so that requests
    // addressed to a user hosted elsewhere can be routed to their home relay.
    {
        let peer_state = state.clone();
        tokio::spawn(async move {
            let secs = peer_state.cfg.peer_sync_secs.max(5);
            let mut interval = tokio::time::interval(Duration::from_secs(secs));
            loop {
                interval.tick().await;
                let peers = {
                    let db = peer_state.db.lock().await;
                    db.list_peers().unwrap_or_default()
                };
                for (url, _) in peers {
                    let dir_url = format!("{}/_fedi3/relay/users", url.trim_end_matches('/'));
                    let resp = peer_state.http.get(&dir_url).send().await;
                    if let Ok(resp) = resp {
                        if resp.status().is_success() {
                            if let Ok(value) = resp.json::<serde_json::Value>().await {
                                let users: Vec<String> = value
                                    .get("users")
                                    .and_then(|u| u.as_array())
                                    .map(|a| {
                                        a.iter()
                                            .filter_map(|u| u.as_str().map(|s| s.to_string()))
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                let db = peer_state.db.lock().await;
                                let _ = db.mark_peer_seen(&url);
                                for u in users {
                                    let _ = db.upsert_remote_user(&u, &url);
                                }
                            }
                        }
                    }
                }
                let cutoff = now_ms() - (secs as i64 * 1000 * 5);
                let db = peer_state.db.lock().await;
                let _ = db.prune_remote_users(cutoff);
            }
        });
    }

    let cleanup_state = state.clone();
    let spool_ttl_secs = cleanup_state.cfg.spool_ttl_secs;
    let webrtc_ttl_secs = cleanup_state.cfg.webrtc_signal_ttl_secs;
    let peer_directory_ttl_days = cleanup_state.cfg.peer_directory_ttl_days;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(
            cleanup_state
                .cfg
                .webrtc_signal_cleanup_interval_secs
                .max(10),
        ));
        loop {
            interval.tick().await;
            let db = cleanup_state.db.lock().await;
            if let Err(e) = db.cleanup_spool(spool_ttl_secs) {
                error!("spool cleanup failed: {e}");
            }
            if let Err(e) = db.cleanup_move_notices(cleanup_state.cfg.move_notice_ttl_secs) {
                error!("move_notices cleanup failed: {e}");
            }
            if let Err(e) = db.cleanup_webrtc_signals(webrtc_ttl_secs) {
                error!("webrtc signals cleanup failed: {e}");
            }
            if peer_directory_ttl_days > 0 {
                if let Err(e) = db.cleanup_peer_directory(peer_directory_ttl_days) {
                    error!("peer_directory cleanup failed: {e}");
                }
            }
            // Retention for the append-only registry/audit/note tables (M-02 / RD-05).
            for (label, result) in [
                (
                    "admin_audit",
                    db.cleanup_admin_audit(cleanup_state.cfg.admin_audit_ttl_secs),
                ),
                (
                    "relay_notes",
                    db.cleanup_relay_notes(cleanup_state.cfg.relay_notes_ttl_secs),
                ),
                (
                    "peer_registry",
                    db.cleanup_peer_registry(cleanup_state.cfg.peer_registry_ttl_secs),
                ),
                (
                    "relay_registry",
                    db.cleanup_relay_registry(cleanup_state.cfg.relay_registry_ttl_secs),
                ),
                (
                    "relay_user_directory",
                    db.cleanup_relay_user_directory(
                        cleanup_state.cfg.relay_user_directory_ttl_secs,
                    ),
                ),
                (
                    "inbox_dedupe",
                    db.cleanup_inbox_dedupe(cleanup_state.cfg.inbox_dedupe_ttl_secs),
                ),
            ] {
                if let Err(e) = result {
                    error!("{label} cleanup failed: {e}");
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
        .route("/_fedi3/relay/users", get(relay_user_directory))
        .route("/_fedi3/relay/user_blocks/:user", put(user_blocks_put))
        .route(
            "/admin/peers",
            get(admin_list_peers)
                .post(admin_add_peer)
                .delete(admin_delete_peer),
        )
        .route(
            "/admin/blocks",
            get(admin_list_blocks)
                .post(admin_add_block)
                .delete(admin_delete_block),
        )
        .route("/admin/users", get(admin_list_users))
        .route(
            "/admin/users/:user",
            get(admin_get_user).delete(admin_delete_user),
        )
        .route("/admin/users/:user/disable", post(admin_disable_user))
        .route("/admin/users/:user/enable", post(admin_enable_user))
        .route("/admin/users/:user/rotate_token", post(admin_rotate_token))
        .route("/admin/audit", get(admin_audit_list))
        .route("/_fedi3/relay/stats", get(relay_stats))
        .route("/_fedi3/relay/me", get(relay_me))
        .route("/_fedi3/relay/relays", get(relay_list))
        .route("/_fedi3/relay/peers", get(relay_peers))
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
        .route("/_fedi3/relay/move", post(relay_move_post))
        .route(
            "/_fedi3/relay/move/:user",
            axum::routing::delete(relay_move_delete),
        )
        .route("/_fedi3/relay/move_notice", post(relay_move_notice_post))
        .route("/_fedi3/webrtc/send", post(webrtc_signal_send))
        .route("/_fedi3/webrtc/poll", get(webrtc_signal_poll))
        .route("/_fedi3/webrtc/ack", post(webrtc_signal_ack))
        .route("/users/:user/media", post(media_upload))
        .route(
            "/users/:user/media/:id",
            get(media_get).delete(media_delete),
        )
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

fn load_config() -> Result<RelayConfig> {
    validate_env_parsing()?;
    let bind = std::env::var("FEDI3_RELAY_BIND").unwrap_or_else(|_| "0.0.0.0:8787".to_string());
    let bind: SocketAddr = bind
        .trim()
        .parse()
        .map_err(|e| anyhow::anyhow!("FEDI3_RELAY_BIND invalid ({bind:?}): {e}"))?;
    let base_domain = std::env::var("FEDI3_RELAY_BASE_DOMAIN")
        .ok()
        .map(normalize_host)
        .filter(|s| !s.is_empty());
    let trust_proxy_headers = std::env::var("FEDI3_RELAY_TRUST_PROXY_HEADERS")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let allow_self_register = std::env::var("FEDI3_RELAY_ALLOW_SELF_REGISTER")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    // Fail-fast on the admin token: it is the only credential protecting the
    // admin/health/metrics surface, so an empty or weak value must stop startup
    // instead of silently turning every admin endpoint into a 401 (H-14).
    let admin_token = std::env::var("FEDI3_RELAY_ADMIN_TOKEN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let admin_token = match admin_token {
        Some(t) if t.len() >= 32 => Some(t),
        Some(t) => anyhow::bail!(
            "FEDI3_RELAY_ADMIN_TOKEN is too short ({} chars, minimum 32)",
            t.len()
        ),
        None => anyhow::bail!("FEDI3_RELAY_ADMIN_TOKEN is required (minimum 32 chars)"),
    };
    let public_url = std::env::var("FEDI3_RELAY_PUBLIC_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty());
    if let Some(url) = &public_url {
        if http::Uri::try_from(url.as_str()).is_err() {
            anyhow::bail!("FEDI3_RELAY_PUBLIC_URL invalid: {url:?}");
        }
    }
    let telemetry_token = std::env::var("FEDI3_RELAY_TELEMETRY_TOKEN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
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
    let telemetry_interval_secs = std::env::var("FEDI3_RELAY_TELEMETRY_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30);
    let max_body_bytes = std::env::var("FEDI3_RELAY_MAX_BODY_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(64 * 1024 * 1024)
        .clamp(1024, 256 * 1024 * 1024);
    // Safe-by-default HSTS: a deployment that forgets the variable must not
    // serve plain HTTP without enforcement (M-06 / RC-05).
    let hsts_max_age_secs = std::env::var("FEDI3_RELAY_HSTS_MAX_AGE_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(31_536_000)
        .min(63_072_000);
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
    let search_backend = std::env::var("FEDI3_RELAY_SEARCH_BACKEND")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "db".to_string());
    if !matches!(search_backend.as_str(), "db" | "meili") {
        anyhow::bail!(
            "FEDI3_RELAY_SEARCH_BACKEND invalid: {search_backend:?} (expected db or meili)"
        );
    }
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
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "sqlite".to_string());
    let db_driver = match db_driver.as_str() {
        "postgres" | "postgresql" | "pg" => DbDriver::Postgres,
        "sqlite" | "sqlite3" => DbDriver::Sqlite,
        other => {
            anyhow::bail!("FEDI3_RELAY_DB_DRIVER invalid: {other:?} (expected sqlite or postgres)")
        }
    };
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
        .clamp(1, 256);
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
        .clamp(1, 300);
    let pg_init_backoff_ms = std::env::var("FEDI3_RELAY_PG_INIT_BACKOFF_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(500)
        .clamp(50, 30_000);
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
        .clamp(1, 64);
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
        .unwrap_or(25)
        .clamp(1, 1000);
    let max_inflight_per_user = std::env::var("FEDI3_RELAY_MAX_INFLIGHT_PER_USER")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(32)
        .clamp(1, 1024);
    let spool_ttl_secs = std::env::var("FEDI3_RELAY_SPOOL_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(7 * 24 * 60 * 60);
    let move_notice_ttl_secs = std::env::var("FEDI3_RELAY_MOVE_NOTICE_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(7 * 24 * 60 * 60);
    let move_notice_fanout_interval_secs =
        std::env::var("FEDI3_RELAY_MOVE_NOTICE_FANOUT_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60);
    let peer_sync_secs = std::env::var("FEDI3_RELAY_PEER_SYNC_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60)
        .max(5);
    let peer_max_hops = std::env::var("FEDI3_RELAY_PEER_MAX_HOPS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(3)
        .clamp(1, 8);
    let peer_directory_ttl_days = std::env::var("FEDI3_RELAY_PEER_DIRECTORY_TTL_DAYS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(30)
        .min(3650);

    let webrtc_signal_ttl_secs = std::env::var("FEDI3_RELAY_WEBRTC_SIGNAL_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10 * 60);
    let webrtc_signal_cleanup_interval_secs =
        std::env::var("FEDI3_RELAY_WEBRTC_SIGNAL_CLEANUP_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(30);
    let webrtc_signal_max_per_peer = std::env::var("FEDI3_RELAY_WEBRTC_SIGNAL_MAX_PER_PEER")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(200);
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
        .unwrap_or(200)
        .clamp(1, 1000);
    // Test/deployment hook: allow fetching actor documents from loopback/private
    // addresses. Off by default to close the SSRF hole on remote keyId URLs.
    let allow_private_actor_fetch = std::env::var("FEDI3_RELAY_ALLOW_PRIVATE_ACTOR_FETCH")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    // Escape hatch for LAN-only deployments: when false (default), every
    // relay-initiated fetch must resolve exclusively to public addresses.
    let allow_private_remote_fetch = std::env::var("FEDI3_ALLOW_PRIVATE_REMOTE_FETCH")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let forward_send_timeout_ms = std::env::var("FEDI3_RELAY_FORWARD_SEND_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(250)
        .clamp(10, 5_000);
    let inbox_dedupe_ttl_secs = std::env::var("FEDI3_RELAY_INBOX_DEDUPE_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(3600)
        .clamp(60, 86_400);
    let admin_audit_ttl_secs = std::env::var("FEDI3_RELAY_ADMIN_AUDIT_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(90 * 24 * 60 * 60)
        .max(3600);
    let relay_notes_ttl_secs = std::env::var("FEDI3_RELAY_NOTES_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30 * 24 * 60 * 60)
        .max(3600);
    let peer_registry_ttl_secs = std::env::var("FEDI3_RELAY_PEER_REGISTRY_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30 * 24 * 60 * 60)
        .max(3600);
    let relay_registry_ttl_secs = std::env::var("FEDI3_RELAY_REGISTRY_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(90 * 24 * 60 * 60)
        .max(3600);
    let relay_user_directory_ttl_secs = std::env::var("FEDI3_RELAY_USER_DIRECTORY_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30 * 24 * 60 * 60)
        .max(3600);
    // 0 means unlimited.
    let media_max_bytes_per_user = std::env::var("FEDI3_RELAY_MEDIA_MAX_BYTES_PER_USER")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1024 * 1024 * 1024);
    let spool_max_bytes_per_user = std::env::var("FEDI3_RELAY_SPOOL_MAX_BYTES_PER_USER")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(64 * 1024 * 1024);
    Ok(RelayConfig {
        bind,
        base_domain,
        trust_proxy_headers,
        allow_self_register,
        admin_token,
        public_url,
        telemetry_token,
        seed_relays,
        p2p_infra_peer_id,
        p2p_infra_multiaddrs,
        p2p_infra_host,
        p2p_infra_port,
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
        webrtc_signal_ttl_secs,
        webrtc_signal_cleanup_interval_secs,
        webrtc_signal_max_per_peer,
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
        outbox_index_interval_secs,
        outbox_index_pages,
        outbox_index_page_limit,
        telemetry_users_limit,
        telemetry_peers_limit,
        relay_sync_interval_secs,
        relay_sync_limit,
        allow_private_actor_fetch,
        allow_private_remote_fetch,
        forward_send_timeout_ms,
        inbox_dedupe_ttl_secs,
        admin_audit_ttl_secs,
        relay_notes_ttl_secs,
        peer_registry_ttl_secs,
        relay_registry_ttl_secs,
        relay_user_directory_ttl_secs,
        media_max_bytes_per_user,
        spool_max_bytes_per_user,
        peer_sync_secs,
        peer_max_hops,
    })
}

/// Strict parsing for every numeric configuration variable: an invalid value
/// must stop startup with a clear message instead of silently becoming the
/// default (RC-01, RC-02, RC-03, M-07).
fn validate_env_parsing() -> Result<()> {
    const U64_VARS: &[&str] = &[
        "FEDI3_RELAY_TELEMETRY_INTERVAL_SECS",
        "FEDI3_RELAY_HSTS_MAX_AGE_SECS",
        "FEDI3_RELAY_HTTP_TIMEOUT_SECS",
        "FEDI3_RELAY_HTTP_CONNECT_TIMEOUT_SECS",
        "FEDI3_RELAY_HTTP_POOL_IDLE_TIMEOUT_SECS",
        "FEDI3_RELAY_TUNNEL_TIMEOUT_SECS",
        "FEDI3_RELAY_MEILI_TIMEOUT_SECS",
        "FEDI3_RELAY_MEILI_FLUSH_MS",
        "FEDI3_RELAY_SEARCH_CACHE_TTL_SECS",
        "FEDI3_RELAY_DB_BUSY_TIMEOUT_MS",
        "FEDI3_RELAY_DB_CACHE_KB",
        "FEDI3_RELAY_PG_POOL_WAIT_MS",
        "FEDI3_RELAY_PG_POOL_CREATE_TIMEOUT_MS",
        "FEDI3_RELAY_PG_POOL_RECYCLE_TIMEOUT_MS",
        "FEDI3_RELAY_PG_INIT_BACKOFF_MS",
        "FEDI3_RELAY_NOISY_BACKOFF_BASE_SECS",
        "FEDI3_RELAY_NOISY_BACKOFF_MAX_SECS",
        "FEDI3_RELAY_SPOOL_TTL_SECS",
        "FEDI3_RELAY_MOVE_NOTICE_TTL_SECS",
        "FEDI3_RELAY_MOVE_NOTICE_FANOUT_INTERVAL_SECS",
        "FEDI3_RELAY_WEBRTC_SIGNAL_TTL_SECS",
        "FEDI3_RELAY_WEBRTC_SIGNAL_CLEANUP_INTERVAL_SECS",
        "FEDI3_RELAY_OUTBOX_INDEX_INTERVAL_SECS",
        "FEDI3_RELAY_SYNC_INTERVAL_SECS",
        "FEDI3_RELAY_FORWARD_SEND_TIMEOUT_MS",
        "FEDI3_RELAY_INBOX_DEDUPE_TTL_SECS",
        "FEDI3_RELAY_ADMIN_AUDIT_TTL_SECS",
        "FEDI3_RELAY_NOTES_TTL_SECS",
        "FEDI3_RELAY_PEER_REGISTRY_TTL_SECS",
        "FEDI3_RELAY_REGISTRY_TTL_SECS",
        "FEDI3_RELAY_USER_DIRECTORY_TTL_SECS",
        "FEDI3_RELAY_MEDIA_MAX_BYTES_PER_USER",
        "FEDI3_RELAY_SPOOL_MAX_BYTES_PER_USER",
    ];
    const USIZE_VARS: &[&str] = &[
        "FEDI3_RELAY_MAX_BODY_BYTES",
        "FEDI3_RELAY_HTTP_POOL_MAX_IDLE_PER_HOST",
        "FEDI3_RELAY_MEILI_BATCH_MAX",
        "FEDI3_RELAY_MEILI_QUEUE_MAX",
        "FEDI3_RELAY_SEARCH_CACHE_MAX",
        "FEDI3_RELAY_PG_POOL_MAX_SIZE",
        "FEDI3_RELAY_PG_INIT_RETRIES",
        "FEDI3_RELAY_REDIS_POOL_SIZE",
        "FEDI3_RELAY_MAX_INBOX_FANOUT",
        "FEDI3_RELAY_MAX_INFLIGHT_PER_USER",
        "FEDI3_RELAY_SPOOL_MAX_ROWS_PER_USER",
        "FEDI3_RELAY_SPOOL_FLUSH_BATCH",
    ];
    const U32_VARS: &[&str] = &[
        "FEDI3_RELAY_RL_REGISTER_PER_MIN",
        "FEDI3_RELAY_RL_TUNNEL_PER_MIN",
        "FEDI3_RELAY_RL_INBOX_PER_MIN",
        "FEDI3_RELAY_RL_FORWARD_PER_MIN",
        "FEDI3_RELAY_RL_ADMIN_PER_MIN",
        "FEDI3_RELAY_WEBRTC_SIGNAL_MAX_PER_PEER",
        "FEDI3_RELAY_OUTBOX_INDEX_PAGES",
        "FEDI3_RELAY_OUTBOX_INDEX_PAGE_LIMIT",
        "FEDI3_RELAY_TELEMETRY_USERS_LIMIT",
        "FEDI3_RELAY_TELEMETRY_PEERS_LIMIT",
        "FEDI3_RELAY_SYNC_LIMIT",
        "FEDI3_RELAY_PEER_DIRECTORY_TTL_DAYS",
    ];
    const U16_VARS: &[&str] = &["FEDI3_P2P_INFRA_PORT"];

    let present = |name: &str| -> Option<String> {
        std::env::var(name)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };

    for name in U64_VARS {
        if let Some(raw) = present(name) {
            if raw.parse::<u64>().is_err() {
                anyhow::bail!("{name} invalid: {raw:?} (expected an unsigned integer)");
            }
        }
    }
    for name in USIZE_VARS {
        if let Some(raw) = present(name) {
            if raw.parse::<usize>().is_err() {
                anyhow::bail!("{name} invalid: {raw:?} (expected an unsigned integer)");
            }
        }
    }
    for name in U32_VARS {
        if let Some(raw) = present(name) {
            if raw.parse::<u32>().is_err() {
                anyhow::bail!("{name} invalid: {raw:?} (expected an unsigned 32-bit integer)");
            }
        }
    }
    for name in U16_VARS {
        if let Some(raw) = present(name) {
            if raw.parse::<u16>().is_err() {
                anyhow::bail!("{name} invalid: {raw:?} (expected an unsigned 16-bit integer)");
            }
        }
    }
    Ok(())
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
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    // Prefer Authorization: Bearer over the legacy query-string token so the
    // long-lived credential does not end up in proxy access logs (H-16).
    let token = bearer_token(&headers).or(q.token);
    ws.on_upgrade(move |socket| handle_tunnel(state, peer, user, token, headers, socket))
}

async fn handle_tunnel(
    state: AppState,
    peer: SocketAddr,
    user: String,
    token: Option<String>,
    headers: HeaderMap,
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
    // The tunnel is the only path that registers self-service users, so it must
    // apply the same username rules as every other handler (H-29 / RT-03).
    if !is_valid_username(&user) {
        error!(%user, "tunnel rejected: invalid username");
        return;
    }

    if !state
        .limiter
        .check(
            client_ip(&state.cfg, &peer, &headers),
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

    // A connection id makes the disconnect cleanup safe under reconnects: the
    // old socket must not remove the new tunnel entry (H-30 / RT-04).
    let conn_id = TUNNEL_CONN_ID.fetch_add(1, Ordering::Relaxed);
    let inflight: Arc<RwLock<HashMap<String, oneshot::Sender<RelayHttpResponse>>>> =
        Arc::new(RwLock::new(HashMap::new()));
    state.tunnels.write().await.insert(
        user.clone(),
        TunnelHandle {
            tx,
            conn_id,
            inflight: inflight.clone(),
        },
    );

    // Fetch peer hello (best-effort) and store it for telemetry/online peer id.
    let hello_state = state.clone();
    let hello_user = user.clone();
    tokio::spawn(async move {
        if let Ok(Some(hello)) = fetch_peer_hello(&hello_state, &hello_user, tx_for_hello).await {
            if let Some(p) = hello.p2p.as_ref().and_then(|p| p.peer_id.clone()) {
                let mut db = hello_state.db.lock().await;
                let _ = db.upsert_peer_seen(&p);
                let actor_url = hello.actor.trim().to_string();
                if !actor_url.is_empty() {
                    let _ = db.upsert_peer_directory(&p, &hello.username, &actor_url);
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
                }
            }
            hello_state
                .peer_hello
                .write()
                .await
                .insert(hello_user, hello);
        }
    });

    let cache_state = state.clone();
    let cache_user = user.clone();
    tokio::spawn(async move {
        let _ = ensure_user_cached(&cache_state, &cache_user).await;
        let _ = index_outbox_for_user(&cache_state, &cache_user).await;
    });

    tokio::spawn(flush_spool_for_user(state.clone(), user.clone()));

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

    {
        let mut tunnels = state.tunnels.write().await;
        if tunnels.get(&user).map(|t| t.conn_id) == Some(conn_id) {
            tunnels.remove(&user);
            drop(tunnels);
            state.peer_hello.write().await.remove(&user);
        }
    }
    info!(%user, "tunnel disconnected");
}

async fn webfinger(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    Query(q): Query<WebfingerQuery>,
) -> impl IntoResponse {
    if let Some(resp) = maybe_redirect_canonical(
        &state.cfg,
        &headers,
        &Method::GET,
        "/.well-known/webfinger",
        raw_query.as_deref(),
    ) {
        return resp.into_response();
    }
    // Reject requests whose Host is outside the configured base domain instead
    // of echoing it back in `subject`/`href` (M-03 / RW-02).
    if !host_allowed_for_cfg(&state.cfg, &authority_from_headers(&headers)) {
        return (StatusCode::MISDIRECTED_REQUEST, "invalid host").into_response();
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
    let mut actor_url = format!("{scheme}://{host}/users/{user}");
    {
        let db = state.db.lock().await;
        let cached = db.get_actor_cache(&user).ok().flatten();
        drop(db);
        if let Some(anchor) = anchor_actor_id(&state, &user, cached.as_deref()) {
            actor_url = anchor;
        }
    }
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

    if !state.cfg.allow_self_register && !is_authorized_admin(&state.cfg, &headers) {
        return (StatusCode::UNAUTHORIZED, "admin token required").into_response();
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

/// Allowlist for uploaded media types (M-16 / RM-03). Anything that can carry
/// active content (e.g. text/html) is rejected.
fn media_type_allowed(content_type: &str) -> bool {
    let base = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    base.starts_with("image/")
        || base.starts_with("video/")
        || base.starts_with("audio/")
        || matches!(
            base.as_str(),
            "application/pdf" | "application/octet-stream"
        )
}

/// Restrict the extension used in storage keys to a short alphanumeric token
/// (M-15 / RM-08); anything else becomes "bin".
fn safe_extension(ext: &str) -> String {
    let ext = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    if ext.is_empty() || ext.len() > 8 || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        "bin".to_string()
    } else {
        ext
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
    let ext = safe_extension(
        FsPath::new(filename)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("bin"),
    );
    let media_type = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");
    if !media_type_allowed(media_type) {
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported media type").into_response();
    }
    let id = generate_media_id(&ext);
    // Per-user quota, measured from the media_items table (RM-04 / H-28).
    if state.cfg.media_max_bytes_per_user > 0 {
        let db = state.db.lock().await;
        let used = db.user_media_bytes(&user).unwrap_or(0);
        drop(db);
        if used.saturating_add(body.len() as u64) > state.cfg.media_max_bytes_per_user {
            return (StatusCode::PAYLOAD_TOO_LARGE, "media quota exceeded").into_response();
        }
    }
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

async fn media_delete(
    State(state): State<AppState>,
    Path((user, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    if !is_valid_username(&user) {
        return (StatusCode::BAD_REQUEST, "invalid user").into_response();
    }
    if id.is_empty() || id.contains("..") || id.contains('/') || id.contains('\\') {
        return (StatusCode::BAD_REQUEST, "invalid media id").into_response();
    }
    let Some(token) = bearer_token(&headers) else {
        return (StatusCode::UNAUTHORIZED, "missing token").into_response();
    };
    let db = state.db.lock().await;
    let ok = db.verify_user_token(&user, &token).unwrap_or(false);
    let item = match db.get_media_item(&user, &id) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_GATEWAY, "db error").into_response(),
    };
    drop(db);
    if !ok {
        return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
    }
    let Some(item) = item else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    if let Err(e) = state.media_backend.delete(&item.storage_key).await {
        return (StatusCode::BAD_GATEWAY, format!("delete failed: {e:#}")).into_response();
    }
    let db = state.db.lock().await;
    if let Err(e) = db.delete_media_item(&user, &id) {
        return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response();
    }
    (StatusCode::OK, "deleted").into_response()
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

async fn healthz() -> impl IntoResponse {
    // Liveness probe: unauthenticated, no DB access, no audit row (H-23 / C6).
    (StatusCode::OK, "ok").into_response()
}

async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    // Readiness probe: unauthenticated, read-only DB ping, no audit side effect.
    let db_ok = {
        let db = state.db.lock().await;
        db.health_check().is_ok()
    };
    if !db_ok {
        return (StatusCode::SERVICE_UNAVAILABLE, "db not ready").into_response();
    }
    if let Err(e) = state.media_backend.health_check().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            format!("media not ready: {e}"),
        )
            .into_response();
    }
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
    if method == Method::GET {
        let cached = {
            let db = state.db.lock().await;
            db.get_actor_cache(&user).ok().flatten()
        };
        if let Some(actor_id) = anchor_actor_id(&state, &user, cached.as_deref()) {
            // Identity lives on a static anchor: send remote servers to the canonical URL.
            return Redirect::temporary(&actor_id).into_response();
        }
    }

    let path = format!("/users/{user}");
    let query = raw_query.map(|q| format!("?{q}")).unwrap_or_default();
    let local = {
        let db = state.db.lock().await;
        db.user_exists(&user).unwrap_or(false)
    };
    if !local {
        if let Some(resp) =
            forward_to_peer(&state, &user, &method, &path, &query, &headers, &body).await
        {
            return resp.into_response();
        }
    }
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
    // Per-user moderation on the inbox path: a blocked sender is acknowledged and dropped
    // before it reaches the core, so the relay enforces the user's blocklist as well.
    if method == Method::POST && rest == "inbox" && sender_blocked(&state, &user, &body).await {
        return (StatusCode::ACCEPTED, "blocked").into_response();
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
    let local = {
        let db = state.db.lock().await;
        db.user_exists(&user).unwrap_or(false)
    };
    if !local {
        if let Some(resp) =
            forward_to_peer(&state, &user, &method, &path, &query, &headers, &body).await
        {
            return resp.into_response();
        }
    }
    forward_to_user(state, user, method, &path, query, headers, body).await
}

/// Route a request for a user hosted on a peer relay (relay mesh). Returns `None` when the
/// user is unknown, the hop budget is exhausted, or the peer call fails.
async fn forward_to_peer(
    state: &AppState,
    user: &str,
    method: &Method,
    path: &str,
    query: &str,
    headers: &HeaderMap,
    body: &Bytes,
) -> Option<Response> {
    let relay_url = {
        let db = state.db.lock().await;
        db.get_remote_user_relay(user).ok().flatten()
    }?;
    let hops = headers
        .get("x-fedi3-hops")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    if hops >= state.cfg.peer_max_hops {
        return None;
    }
    let mut url = format!("{}{}", relay_url.trim_end_matches('/'), path);
    if !query.is_empty() {
        url.push('?');
        url.push_str(query);
    }
    let mut req = state
        .http
        .request(method.clone(), &url)
        .header("x-fedi3-hops", (hops + 1).to_string());
    for (k, v) in headers.iter() {
        if k == header::HOST || k.as_str().eq_ignore_ascii_case("x-fedi3-hops") {
            continue;
        }
        req = req.header(k, v);
    }
    let resp = req.body(body.clone()).send().await.ok()?;
    let status = resp.status();
    let resp_headers = resp.headers().clone();
    let bytes = resp.bytes().await.ok()?;
    let mut out = Response::builder().status(status);
    for (k, v) in resp_headers.iter() {
        if k == header::CONTENT_LENGTH || k == header::TRANSFER_ENCODING || k == header::CONNECTION
        {
            continue;
        }
        out = out.header(k, v);
    }
    out.body(axum::body::Body::from(bytes)).ok()
}

/// Extract the sender actor of an activity body (string or embedded object form).
fn sender_from_body(body: &Bytes) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    let actor = v.get("actor")?;
    if let Some(s) = actor.as_str() {
        return Some(s.trim().to_string());
    }
    actor
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
}

async fn sender_blocked(state: &AppState, user: &str, body: &Bytes) -> bool {
    let Some(actor) = sender_from_body(body) else {
        return false;
    };
    let db = state.db.lock().await;
    db.is_blocked(user, &actor).unwrap_or(false)
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
        return forward_failure(
            &state,
            &user,
            &method,
            path,
            &query,
            &headers,
            &body,
            StatusCode::TOO_MANY_REQUESTS,
            "user inflight limit",
        )
        .await;
    };

    // Clone the sender and release the tunnels lock *before* awaiting. Holding
    // the read guard across `tx.send().await` blocked every other forward and
    // tunnel upgrade behind one slow client (H-10 / C4).
    let (tunnel_tx, tunnel_inflight) = {
        let tunnels = state.tunnels.read().await;
        let Some(tunnel) = tunnels.get(&user) else {
            drop(tunnels);
            return forward_failure(
                &state,
                &user,
                &method,
                path,
                &query,
                &headers,
                &body,
                StatusCode::SERVICE_UNAVAILABLE,
                "user offline",
            )
            .await;
        };
        (tunnel.tx.clone(), tunnel.inflight.clone())
    };

    let headers_vec = headers_to_vec(&headers);
    let id = format!("{user}-{}", REQ_ID.fetch_add(1, Ordering::Relaxed));
    let req = RelayHttpRequest {
        id: id.clone(),
        method: method.to_string(),
        path: path.to_string(),
        query: query.clone(),
        headers: headers_vec,
        body_b64: B64.encode(&body),
    };
    let (resp_tx, resp_rx) = oneshot::channel();
    let msg = TunnelRequest {
        id: id.clone(),
        req,
        resp_tx,
    };

    // Bounded send: a full channel means the client stopped reading.
    match tokio::time::timeout(
        Duration::from_millis(state.cfg.forward_send_timeout_ms),
        tunnel_tx.send(msg),
    )
    .await
    {
        Err(_) => {
            tunnel_inflight.write().await.remove(&id);
            return forward_failure(
                &state,
                &user,
                &method,
                path,
                &query,
                &headers,
                &body,
                StatusCode::SERVICE_UNAVAILABLE,
                "tunnel send timeout",
            )
            .await;
        }
        Ok(Err(_)) => {
            tunnel_inflight.write().await.remove(&id);
            return forward_failure(
                &state,
                &user,
                &method,
                path,
                &query,
                &headers,
                &body,
                StatusCode::SERVICE_UNAVAILABLE,
                "tunnel send failed",
            )
            .await;
        }
        Ok(Ok(())) => {}
    }

    let resp =
        match tokio::time::timeout(Duration::from_secs(state.cfg.tunnel_timeout_secs), resp_rx)
            .await
        {
            Err(_) => {
                // Drop the stale inflight entry so a client that stops answering
                // cannot leak one oneshot sender per request forever (H-31 / RT-09).
                tunnel_inflight.write().await.remove(&id);
                return forward_failure(
                    &state,
                    &user,
                    &method,
                    path,
                    &query,
                    &headers,
                    &body,
                    StatusCode::GATEWAY_TIMEOUT,
                    "tunnel timeout",
                )
                .await;
            }
            Ok(Err(_)) => {
                tunnel_inflight.write().await.remove(&id);
                return forward_failure(
                    &state,
                    &user,
                    &method,
                    path,
                    &query,
                    &headers,
                    &body,
                    StatusCode::BAD_GATEWAY,
                    "tunnel response dropped",
                )
                .await;
            }
            Ok(Ok(resp)) => resp,
        };

    // Any retryable failure is queued for the next reconnect instead of being
    // lost (H-11 / C4). GETs are cache/fetch probes and are not spooled, so a
    // slow client cannot flood the spool with background cache reads.
    if method != Method::GET && (resp.status == 429 || resp.status >= 500) {
        let _ = spool_request(&state, &user, &method, path, &query, &headers, &body).await;
    }

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
                            }
                        }
                    }
                }
            }
        }
    }

    build_response(resp)
}

/// Pseudo-headers that mark a spooled row as end-to-end encrypted. The relay stores only
/// ciphertext and forwards it as an envelope; the private key never leaves the user's core.
const SPOOL_ENC_HEADER: &str = "x-fedi3-spool-enc";
const SPOOL_ENC_ALG: &str = "aes256gcm+rsa-oaep-sha256";
const SPOOL_ENC_KEY: &str = "x-fedi3-spool-key";
const SPOOL_ENC_NONCE: &str = "x-fedi3-spool-nonce";
const SPOOL_ENC_LEN: &str = "x-fedi3-spool-len";

fn user_public_key_pem(actor_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(actor_json).ok()?;
    let pem = v
        .get("publicKey")
        .and_then(|k| k.get("publicKeyPem"))
        .and_then(|p| p.as_str())?;
    let pem = pem.trim();
    if pem.is_empty() {
        None
    } else {
        Some(pem.to_string())
    }
}

/// Hybrid encryption: AES-256-GCM for the body, RSA-OAEP-SHA256 to wrap the one-time key.
/// Returns `(ciphertext, wrapped_key_b64, nonce_b64)`.
fn encrypt_spool_body(pub_pem: &str, plaintext: &[u8]) -> Option<(Vec<u8>, String, String)> {
    use aes_gcm::aead::Aead;
    use rand::RngCore;
    let pub_key = rsa::RsaPublicKey::from_public_key_pem(pub_pem).ok()?;
    let mut key = [0u8; 32];
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut key);
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let cipher = Aes256Gcm::new_from_slice(&key).ok()?;
    let ciphertext = cipher.encrypt(Nonce::from_slice(&nonce), plaintext).ok()?;
    let mut rng = rand::rngs::OsRng;
    let wrapped = pub_key
        .encrypt(&mut rng, rsa::Oaep::new::<Sha256>(), &key)
        .ok()?;
    Some((ciphertext, B64.encode(wrapped), B64.encode(nonce)))
}

/// Queue a request for delivery on the next tunnel reconnect (spool) and
/// answer 503. Returns false when spooling itself failed.
async fn spool_request(
    state: &AppState,
    user: &str,
    method: &Method,
    path: &str,
    query: &str,
    headers: &HeaderMap,
    body: &Bytes,
) -> bool {
    let actor_json = {
        let db = state.db.lock().await;
        db.get_actor_cache(user).ok().flatten()
    };
    let mut stored_headers = headers_to_vec(headers);
    let stored_body = match actor_json
        .as_deref()
        .and_then(user_public_key_pem)
        .and_then(|pem| encrypt_spool_body(&pem, body))
    {
        Some((ciphertext, key_b64, nonce_b64)) => {
            stored_headers.push((SPOOL_ENC_HEADER.to_string(), SPOOL_ENC_ALG.to_string()));
            stored_headers.push((SPOOL_ENC_KEY.to_string(), key_b64));
            stored_headers.push((SPOOL_ENC_NONCE.to_string(), nonce_b64));
            stored_headers.push((SPOOL_ENC_LEN.to_string(), body.len().to_string()));
            B64.encode(ciphertext)
        }
        None => B64.encode(body),
    };
    let db = state.db.lock().await;
    db.enqueue_spool(
        &state.cfg,
        user,
        method.as_str(),
        path,
        query,
        &stored_headers,
        &stored_body,
        body.len() as i64,
    )
    .is_ok()
}

/// Failure path for a forward: spool the request so it is retried after the
/// user reconnects, then return 503/504/429 to the caller (H-11 / C4).
#[allow(clippy::too_many_arguments)]
async fn forward_failure(
    state: &AppState,
    user: &str,
    method: &Method,
    path: &str,
    query: &str,
    headers: &HeaderMap,
    body: &Bytes,
    status: StatusCode,
    reason: &'static str,
) -> Response {
    if *method != Method::GET {
        let _ = spool_request(state, user, method, path, query, headers, body).await;
    }
    (status, reason).into_response()
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
    uri: OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // sharedInbox POST fan-out: route to user tunnels based on recipients.
    if method != Method::POST {
        return (StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response();
    }

    // Cheap abuse gate before the (remote) signature check.
    let ip = client_ip(&state.cfg, &peer, &headers);
    if !state
        .limiter
        .check_weighted(ip, "inbox_pre", state.cfg.rate_limit_inbox_per_min, 1)
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }

    // The relay used to accept every POST on trust (P0-06 / SEC-01). Verify the
    // HTTP signature and its mandatory signed headers before doing any work.
    if let Err(e) =
        verify_actor_signature_for_request(&state, &Method::POST, &uri.0, &headers, Some(&body))
            .await
    {
        return (StatusCode::UNAUTHORIZED, format!("signature invalid: {e}")).into_response();
    }

    let users = match extract_users_from_activity(&body) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("bad json: {e}")).into_response(),
    };
    // Only recipients that exist on this relay are ours (RI-05).
    let users: Vec<String> = {
        let db = state.db.lock().await;
        users
            .into_iter()
            .filter(|u| db.user_exists(u).unwrap_or(false))
            .collect()
    };
    if users.is_empty() {
        return (StatusCode::BAD_REQUEST, "no local recipients").into_response();
    }
    // Per-user and relay-wide moderation: blocked senders are acknowledged but never
    // forwarded, so the block is enforced before any core sees the activity.
    let sender = sender_from_body(&body);
    let users: Vec<String> = {
        let db = state.db.lock().await;
        users
            .into_iter()
            .filter(|u| match sender.as_deref() {
                Some(actor) => !db.is_blocked(u, actor).unwrap_or(false),
                None => true,
            })
            .collect()
    };
    if users.is_empty() {
        return (StatusCode::ACCEPTED, "no local recipients").into_response();
    }

    if users.len() > state.cfg.max_inbox_fanout {
        return (StatusCode::PAYLOAD_TOO_LARGE, "too many recipients").into_response();
    }

    // Weighted rate limiting with the real fan-out cost.
    if !state
        .limiter
        .check_weighted(
            client_ip(&state.cfg, &peer, &headers),
            "inbox",
            state.cfg.rate_limit_inbox_per_min,
            users.len().max(1) as u32,
        )
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }

    let activity_id = extract_activity_id(&body);
    let total = users.len() as u32;
    let mut delivered = 0u32;
    let mut spooled = 0u32;
    let mut duplicate = 0u32;
    let headers_vec = headers_to_vec(&headers);
    let body_b64 = B64.encode(&body);

    if let Err(e) = index_activity_bytes_for_search(&state, &body).await {
        error!("relay search index failed: {e}");
    }

    for user in &users {
        // Dedupe per (activity_id, recipient): sender retries must not deliver
        // the same activity twice (H-20 / RI-06 / SEC-12).
        if let Some(aid) = activity_id.as_deref() {
            let inserted = {
                let db = state.db.lock().await;
                db.insert_inbox_dedupe(aid, user).unwrap_or(true)
            };
            if !inserted {
                duplicate += 1;
                continue;
            }
        }

        let mut ok = false;
        let is_online = { state.tunnels.read().await.contains_key(user) };
        if is_online {
            let resp = forward_to_user(
                state.clone(),
                user.clone(),
                Method::POST,
                "/inbox",
                String::new(),
                headers.clone(),
                body.clone(),
            )
            .await;
            if resp.status().is_success() {
                delivered += 1;
                ok = true;
            } else {
                let status = resp.status();
                if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
                    // forward_to_user already queued the request in the spool.
                    spooled += 1;
                    ok = true;
                } else if spool_request(&state, user, &Method::POST, "/inbox", "", &headers, &body)
                    .await
                {
                    spooled += 1;
                    ok = true;
                }
            }
        } else {
            let db = state.db.lock().await;
            match db.is_user_enabled(user) {
                Ok(true) => {
                    if db
                        .enqueue_spool(
                            &state.cfg,
                            user,
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
                        ok = true;
                    }
                }
                Ok(false) => {}
                Err(e) => error!(%user, "db error: {e}"),
            }
        }

        if !ok {
            if let Some(aid) = activity_id.as_deref() {
                let db = state.db.lock().await;
                let _ = db.delete_inbox_dedupe(aid, user);
            }
        }
    }

    let accounted = delivered + spooled + duplicate;
    if accounted == 0 || accounted < total {
        (StatusCode::SERVICE_UNAVAILABLE, "delivery incomplete").into_response()
    } else {
        (StatusCode::ACCEPTED, "accepted").into_response()
    }
}

/// Extract the ActivityPub activity id used for relay-side dedupe.
fn extract_activity_id(body: &Bytes) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    v.get("id")
        .and_then(|id| id.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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
        for note in extract_notes_from_value(&value) {
            if let Some(idx) = note_to_index(&note) {
                let db = state.db.lock().await;
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
        }
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
    // Prefer the tunnel: the canonical actor may live on a static anchor that is unreachable
    // from the relay, and the tunnel is also the only source of the freshest key material.
    if state.tunnels.read().await.contains_key(user) {
        let resp = forward_to_user(
            state.clone(),
            user.to_string(),
            Method::GET,
            &format!("/users/{user}"),
            String::new(),
            HeaderMap::new(),
            Bytes::new(),
        )
        .await;
        if resp.status().is_success() {
            if let Ok(bytes) = axum::body::to_bytes(resp.into_body(), MAX_PEER_PAGE_BYTES).await {
                if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                    if let Ok(json) = serde_json::to_string(&value) {
                        let db = state.db.lock().await;
                        let _ = db.upsert_actor_cache(user, &json);
                        return Ok(());
                    }
                }
            }
        }
    }
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
    const ACCEPT: &str = "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\", application/json";
    match fetch_peer_json::<serde_json::Value>(state, url, ACCEPT, MAX_PEER_PAGE_BYTES).await {
        Ok(value) => Some(value),
        Err(e) => {
            debug!(%url, "peer JSON fetch skipped: {e:#}");
            None
        }
    }
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

fn origin_host(url: &str) -> Option<String> {
    url.parse::<http::Uri>().ok()?.host().map(|h| h.to_string())
}

fn origin_of(url: &str) -> Option<String> {
    let u = url.parse::<http::Uri>().ok()?;
    let scheme = u.scheme_str()?;
    let host = u.host()?;
    Some(match u.port_u16() {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    })
}

/// Canonical actor id of a local user when it lives on a different origin (identity anchor).
fn anchor_actor_id(state: &AppState, user: &str, cached_json: Option<&str>) -> Option<String> {
    let json = cached_json?;
    let (actor_id, _) = extract_actor_ids_from_json(json);
    let actor_id = actor_id?;
    let relay_origin = origin_of(&user_base_url(&state.cfg, user));
    let self_origin = origin_of(&relay_self_base(&state.cfg));
    let actor_origin = origin_of(&actor_id);
    if actor_origin != relay_origin && actor_origin != self_origin {
        Some(actor_id)
    } else {
        None
    }
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
    let batch = state.cfg.spool_flush_batch.clamp(1, 500);
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
            let encrypted = headers_vec
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case(SPOOL_ENC_HEADER) && v == SPOOL_ENC_ALG);

            let resp = if encrypted {
                let value_of = |name: &str| -> String {
                    headers_vec
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case(name))
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default()
                };
                let original_headers: Vec<(String, String)> = headers_vec
                    .iter()
                    .filter(|(k, _)| !k.to_ascii_lowercase().starts_with("x-fedi3-spool-"))
                    .cloned()
                    .collect();
                let envelope = serde_json::json!({
                    "method": item.method,
                    "path": item.path,
                    "query": item.query,
                    "headers": original_headers,
                    "body_b64": item.body_b64,
                    "key_b64": value_of(SPOOL_ENC_KEY),
                    "nonce_b64": value_of(SPOOL_ENC_NONCE),
                    "spooled_at_ms": item.created_at_ms,
                });
                let mut headers = HeaderMap::new();
                headers.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json"),
                );
                forward_to_user(
                    state.clone(),
                    user.clone(),
                    Method::POST,
                    "/_fedi3/spool/envelope",
                    String::new(),
                    headers,
                    Bytes::from(envelope.to_string()),
                )
                .await
            } else {
                let headers = vec_to_headers(&headers_vec);
                let body_bytes = B64.decode(item.body_b64.as_bytes()).unwrap_or_default();
                let method = item.method.parse::<Method>().unwrap_or(Method::POST);
                forward_to_user(
                    state.clone(),
                    user.clone(),
                    method,
                    &item.path,
                    item.query.clone(),
                    headers,
                    Bytes::from(body_bytes),
                )
                .await
            };

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
    let body = B64.decode(resp.body_b64.as_bytes()).unwrap_or_default();
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
    let Some(expected) = cfg.admin_token.as_deref() else {
        return false;
    };
    if expected.is_empty() {
        return false;
    }
    token_matches(expected, bearer_token(headers).as_deref())
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

/// Numbered, fail-fast schema migration for SQLite. Only the expected
/// "duplicate column name" error is tolerated; any other error aborts startup
/// instead of being swallowed like the old `let _ =` statements (H-24 / RD-02).
fn migrate_sqlite_schema(conn: &Connection) -> Result<()> {
    let current: Option<i64> = conn
        .query_row("SELECT version FROM schema_version WHERE id = 1", [], |r| {
            r.get(0)
        })
        .optional()?;
    if let Some(v) = current {
        if v > SCHEMA_VERSION {
            anyhow::bail!(
                "relay database schema version {v} is newer than this binary supports ({SCHEMA_VERSION})"
            );
        }
    }

    // Migration 2: columns added after the first release.
    const MIGRATIONS: &[(&str, &str)] = &[
        (
            "users.disabled",
            "ALTER TABLE users ADD COLUMN disabled INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "user_cache.actor_id",
            "ALTER TABLE user_cache ADD COLUMN actor_id TEXT NULL",
        ),
        (
            "user_cache.actor_url",
            "ALTER TABLE user_cache ADD COLUMN actor_url TEXT NULL",
        ),
        (
            "relay_registry.sign_pubkey_b64",
            "ALTER TABLE relay_registry ADD COLUMN sign_pubkey_b64 TEXT NULL",
        ),
        (
            "admin_audit.request_id",
            "ALTER TABLE admin_audit ADD COLUMN request_id TEXT NULL",
        ),
        (
            "admin_audit.correlation_id",
            "ALTER TABLE admin_audit ADD COLUMN correlation_id TEXT NULL",
        ),
        (
            "admin_audit.user_agent",
            "ALTER TABLE admin_audit ADD COLUMN user_agent TEXT NULL",
        ),
    ];
    for (label, sql) in MIGRATIONS {
        match conn.execute(sql, []) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(_, Some(msg)))
                if msg.contains("duplicate column name") => {}
            Err(e) => return Err(anyhow::anyhow!("schema migration failed for {label}: {e}")),
        }
    }

    match current {
        Some(v) if v < SCHEMA_VERSION => {
            conn.execute(
                "UPDATE schema_version SET version = ?1 WHERE id = 1",
                params![SCHEMA_VERSION],
            )?;
        }
        None => {
            conn.execute(
                "INSERT INTO schema_version(id, version) VALUES (1, ?1)",
                params![SCHEMA_VERSION],
            )?;
        }
        _ => {}
    }
    Ok(())
}

/// PostgreSQL `search_tsv`/`tag_tsv` migration for databases created by an
/// older schema. Fresh databases get generated columns from
/// `sql/postgres_schema.sql`; upgraded databases may have a plain `tsvector`
/// column (or none), which makes search silently return nothing.
///
/// The statements below are idempotent and safe to run on every startup. Docker
/// is not available in this test environment, so the live Postgres path is
/// covered only by unit tests asserting the shape of these statements; run the
/// upgrade once against a staging database before production rollout.
const PG_SEARCH_TSV_ADD_COLUMN_SQL: &str = r#"ALTER TABLE relay_notes
  ADD COLUMN IF NOT EXISTS search_tsv tsvector GENERATED ALWAYS AS (
    to_tsvector('simple', coalesce(content_text, ''))
  ) STORED"#;

const PG_SEARCH_TSV_BACKFILL_SQL: &str = r#"UPDATE relay_notes
SET search_tsv = to_tsvector('simple', coalesce(content_text, ''))
WHERE search_tsv IS DISTINCT FROM to_tsvector('simple', coalesce(content_text, ''))"#;

const PG_SEARCH_TSV_INDEX_SQL: &str =
    "CREATE INDEX IF NOT EXISTS idx_relay_notes_search_tsv ON relay_notes USING GIN (search_tsv)";

const PG_SEARCH_TSV_TRIGGER_SQL: &str = r#"CREATE OR REPLACE FUNCTION relay_notes_search_tsv_sync() RETURNS trigger AS $$
BEGIN
  NEW.search_tsv := to_tsvector('simple', coalesce(NEW.content_text, ''));
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS relay_notes_search_tsv_tr ON relay_notes;
CREATE TRIGGER relay_notes_search_tsv_tr
  BEFORE INSERT OR UPDATE OF content_text ON relay_notes
  FOR EACH ROW EXECUTE FUNCTION relay_notes_search_tsv_sync()"#;

const PG_TAG_TSV_ADD_COLUMN_SQL: &str = r#"ALTER TABLE relay_note_tags
  ADD COLUMN IF NOT EXISTS tag_tsv tsvector GENERATED ALWAYS AS (
    to_tsvector('simple', coalesce(tag, ''))
  ) STORED"#;

const PG_TAG_TSV_BACKFILL_SQL: &str = r#"UPDATE relay_note_tags
SET tag_tsv = to_tsvector('simple', coalesce(tag, ''))
WHERE tag_tsv IS DISTINCT FROM to_tsvector('simple', coalesce(tag, ''))"#;

const PG_TAG_TSV_INDEX_SQL: &str =
    "CREATE INDEX IF NOT EXISTS idx_relay_note_tags_tsv ON relay_note_tags USING GIN (tag_tsv)";

const PG_TAG_TSV_TRIGGER_SQL: &str = r#"CREATE OR REPLACE FUNCTION relay_note_tags_tag_tsv_sync() RETURNS trigger AS $$
BEGIN
  NEW.tag_tsv := to_tsvector('simple', coalesce(NEW.tag, ''));
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;
DROP TRIGGER IF EXISTS relay_note_tags_tag_tsv_tr ON relay_note_tags;
CREATE TRIGGER relay_note_tags_tag_tsv_tr
  BEFORE INSERT OR UPDATE OF tag ON relay_note_tags
  FOR EACH ROW EXECUTE FUNCTION relay_note_tags_tag_tsv_sync()"#;

/// Returns `Some(true)` for a generated column, `Some(false)` for a plain
/// column, and `None` when the column does not exist.
fn pg_column_is_generated(conn: &mut PgConn, table: &str, column: &str) -> Result<Option<bool>> {
    // Avoid a `$1::regclass` cast: tokio-postgres would infer the parameter as
    // `regclass` while we send text. `to_regclass` resolves the unqualified name
    // through the same search_path as the DDL in the schema file.
    let row = conn.query_opt(
        "SELECT a.attgenerated <> '' AS generated
         FROM pg_attribute a
         WHERE a.attrelid = to_regclass($1)
           AND a.attname::text = $2
           AND NOT a.attisdropped",
        &[&table, &column],
    )?;
    Ok(row.map(|r| r.get(0)))
}

fn migrate_postgres_search_tsv(conn: &mut PgConn) -> Result<()> {
    match pg_column_is_generated(conn, "relay_notes", "search_tsv")? {
        None => {
            conn.batch_execute(PG_SEARCH_TSV_ADD_COLUMN_SQL)?;
            info!("postgres migration: added generated relay_notes.search_tsv column");
        }
        Some(true) => {
            debug!("postgres migration: relay_notes.search_tsv is already generated");
        }
        Some(false) => {
            conn.batch_execute(PG_SEARCH_TSV_BACKFILL_SQL)?;
            conn.batch_execute(PG_SEARCH_TSV_TRIGGER_SQL)?;
            info!(
                "postgres migration: backfilled relay_notes.search_tsv and installed sync trigger"
            );
        }
    }
    conn.batch_execute(PG_SEARCH_TSV_INDEX_SQL)?;

    match pg_column_is_generated(conn, "relay_note_tags", "tag_tsv")? {
        None => {
            conn.batch_execute(PG_TAG_TSV_ADD_COLUMN_SQL)?;
            info!("postgres migration: added generated relay_note_tags.tag_tsv column");
        }
        Some(true) => {
            debug!("postgres migration: relay_note_tags.tag_tsv is already generated");
        }
        Some(false) => {
            conn.batch_execute(PG_TAG_TSV_BACKFILL_SQL)?;
            conn.batch_execute(PG_TAG_TSV_TRIGGER_SQL)?;
            info!(
                "postgres migration: backfilled relay_note_tags.tag_tsv and installed sync trigger"
            );
        }
    }
    conn.batch_execute(PG_TAG_TSV_INDEX_SQL)?;
    Ok(())
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
        let _ = conn.pragma_update(None, "cache_size", self.db_cache_kb);
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

            CREATE TABLE IF NOT EXISTS relay_peers (
              relay_url TEXT PRIMARY KEY,
              added_at_ms INTEGER NOT NULL,
              last_seen_ms INTEGER NULL
            );
            CREATE TABLE IF NOT EXISTS remote_users (
              username TEXT PRIMARY KEY,
              relay_url TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_remote_users_relay ON remote_users(relay_url);
            CREATE TABLE IF NOT EXISTS relay_blocks (
              kind TEXT NOT NULL,
              value TEXT NOT NULL,
              reason TEXT NULL,
              created_at_ms INTEGER NOT NULL,
              created_by TEXT NULL,
              expires_at_ms INTEGER NULL,
              PRIMARY KEY(kind, value)
            );
            CREATE TABLE IF NOT EXISTS user_blocks (
              username TEXT NOT NULL,
              kind TEXT NOT NULL,
              value TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              PRIMARY KEY(username, kind, value)
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

            CREATE TABLE IF NOT EXISTS webrtc_signals (
              signal_id TEXT PRIMARY KEY,
              to_peer_id TEXT NOT NULL,
              from_actor TEXT NOT NULL,
              session_id TEXT NOT NULL,
              kind TEXT NOT NULL,
              payload_json TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_webrtc_to_created ON webrtc_signals(to_peer_id, created_at_ms DESC);

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

            CREATE TABLE IF NOT EXISTS schema_version (
              id INTEGER PRIMARY KEY CHECK(id = 1),
              version INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS inbox_dedupe (
              activity_id TEXT NOT NULL,
              recipient TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL,
              PRIMARY KEY(activity_id, recipient)
            );
            CREATE INDEX IF NOT EXISTS idx_inbox_dedupe_created ON inbox_dedupe(created_at_ms);
            "#,
                )?;
                migrate_sqlite_schema(&conn)?;
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
                            migrate_postgres_search_tsv(&mut conn)?;
                            // Reject databases written by a newer binary instead of
                            // silently running against an unknown schema (RD-03).
                            if let Some(row) = conn
                                .query_opt("SELECT version FROM schema_version WHERE id = 1", &[])?
                            {
                                let version: i64 = row.get(0);
                                if version > SCHEMA_VERSION {
                                    anyhow::bail!(
                                        "relay database schema version {version} is newer than this binary supports ({SCHEMA_VERSION})"
                                    );
                                }
                            }
                            conn.execute(
                                "INSERT INTO schema_version(id, version) VALUES (1, $1) ON CONFLICT (id) DO UPDATE SET version = EXCLUDED.version",
                                &[&SCHEMA_VERSION],
                            )?;
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

    #[allow(clippy::too_many_arguments)]
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

    #[allow(clippy::type_complexity)]
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
        let limit = limit.clamp(1, 500) as i64;
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

    #[allow(clippy::type_complexity)]
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
                let mut stmt = conn.prepare("SELECT key, value FROM relay_meta WHERE key LIKE 'relay_sync_last_ms:%' ESCAPE '\\'")?;
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
                    "SELECT key, value FROM relay_meta WHERE key LIKE 'relay_sync_last_ms:%' ESCAPE '\\'",
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

    fn upsert_peer_seen(&mut self, peer_id: &str) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    r#"
            INSERT INTO peer_registry(peer_id, last_seen_ms) VALUES (?1, ?2)
            ON CONFLICT(peer_id) DO UPDATE SET last_seen_ms=excluded.last_seen_ms
            "#,
                    params![peer_id, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    r#"
            INSERT INTO peer_registry(peer_id, last_seen_ms) VALUES ($1, $2)
            ON CONFLICT(peer_id) DO UPDATE SET last_seen_ms=EXCLUDED.last_seen_ms
            "#,
                    &[&peer_id, &now],
                )?;
                Ok(())
            }
        }
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
                        Ok(ct_eq_str(&stored, &token_hash_hex(token)))
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
                        Ok(ct_eq_str(&stored, &token_hash_hex(token)))
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
                        if ct_eq_str(&stored, &got) {
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
                        if ct_eq_str(&stored, &got) {
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
                Ok(ct_eq_str(&stored, &token_hash_hex(token)))
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
                Ok(ct_eq_str(&stored, &token_hash_hex(token)))
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
        let limit = limit.clamp(1, 1000) as i64;
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
        let limit = limit.clamp(1, 500) as i64;
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

    #[allow(clippy::too_many_arguments)]
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
        let byte_cap = cfg.spool_max_bytes_per_user as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                if byte_cap > 0 {
                    if body_len > byte_cap {
                        anyhow::bail!("spool body exceeds per-user byte cap");
                    }
                    let used: i64 = conn.query_row(
                        "SELECT COALESCE(SUM(body_len), 0) FROM inbox_spool WHERE username=?1",
                        params![username],
                        |r| r.get(0),
                    )?;
                    let mut excess = used.saturating_add(body_len) - byte_cap;
                    if excess > 0 {
                        let mut stmt = conn.prepare(
                            "SELECT id, body_len FROM inbox_spool WHERE username=?1 ORDER BY created_at_ms ASC",
                        )?;
                        let mut rows = stmt.query(params![username])?;
                        let mut to_delete = Vec::new();
                        while let Some(row) = rows.next()? {
                            if excess <= 0 {
                                break;
                            }
                            let id: i64 = row.get(0)?;
                            let len: i64 = row.get(1)?;
                            to_delete.push(id);
                            excess -= len.max(0);
                        }
                        drop(rows);
                        drop(stmt);
                        for id in to_delete {
                            conn.execute("DELETE FROM inbox_spool WHERE id=?1", params![id])?;
                        }
                    }
                }
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
                if byte_cap > 0 {
                    if body_len > byte_cap {
                        anyhow::bail!("spool body exceeds per-user byte cap");
                    }
                    let row = conn.query_one(
                        "SELECT COALESCE(SUM(body_len), 0) FROM inbox_spool WHERE username=$1",
                        &[&username],
                    )?;
                    let used: i64 = row.get(0);
                    let mut excess = used.saturating_add(body_len) - byte_cap;
                    if excess > 0 {
                        let rows = conn.query(
                            "SELECT id, body_len FROM inbox_spool WHERE username=$1 ORDER BY created_at_ms ASC",
                            &[&username],
                        )?;
                        let mut to_delete = Vec::new();
                        for r in rows {
                            if excess <= 0 {
                                break;
                            }
                            let id: i64 = r.get(0);
                            let len: i64 = r.get(1);
                            to_delete.push(id);
                            excess -= len.max(0);
                        }
                        for id in to_delete {
                            conn.execute("DELETE FROM inbox_spool WHERE id=$1", &[&id])?;
                        }
                    }
                }
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
                    "SELECT id, method, path, query, headers_json, body_b64, created_at_ms FROM inbox_spool WHERE username=?1 ORDER BY created_at_ms ASC LIMIT ?2",
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
                        created_at_ms: r.get(6)?,
                    });
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT id, method, path, query, headers_json, body_b64, created_at_ms FROM inbox_spool WHERE username=$1 ORDER BY created_at_ms ASC LIMIT $2",
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
                        created_at_ms: r.get(6),
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
                    let placeholders = std::iter::repeat_n("?", chunk.len())
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
                Ok(deleted)
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
                Ok(deleted)
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
                    "SELECT COUNT(*), COALESCE(SUM(body_len), 0) FROM inbox_spool WHERE username=$1",
                    &[&username],
                )?;
                let count: i64 = row.get(0);
                let bytes: i64 = row.get(1);
                Ok((count.max(0) as u64, bytes.max(0) as u64))
            }
        }
    }

    fn upsert_peer(&self, relay_url: &str) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO relay_peers(relay_url, added_at_ms, last_seen_ms) VALUES (?1, ?2, NULL) ON CONFLICT(relay_url) DO NOTHING",
                    params![relay_url, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO relay_peers(relay_url, added_at_ms, last_seen_ms) VALUES ($1, $2, NULL) ON CONFLICT(relay_url) DO NOTHING",
                    &[&relay_url, &now],
                )?;
                Ok(())
            }
        }
    }

    fn list_peers(&self) -> Result<Vec<(String, Option<i64>)>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT relay_url, last_seen_ms FROM relay_peers ORDER BY added_at_ms ASC",
                )?;
                let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
                Ok(rows.flatten().collect())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT relay_url, last_seen_ms FROM relay_peers ORDER BY added_at_ms ASC",
                    &[],
                )?;
                Ok(rows
                    .iter()
                    .map(|r| (r.get::<_, String>(0), r.get::<_, Option<i64>>(1)))
                    .collect())
            }
        }
    }

    fn mark_peer_seen(&self, relay_url: &str) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "UPDATE relay_peers SET last_seen_ms=?2 WHERE relay_url=?1",
                    params![relay_url, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "UPDATE relay_peers SET last_seen_ms=$2 WHERE relay_url=$1",
                    &[&relay_url, &now],
                )?;
                Ok(())
            }
        }
    }

    fn delete_peer(&self, relay_url: &str) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "DELETE FROM relay_peers WHERE relay_url=?1",
                    params![relay_url],
                )?;
                conn.execute(
                    "DELETE FROM remote_users WHERE relay_url=?1",
                    params![relay_url],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute("DELETE FROM relay_peers WHERE relay_url=$1", &[&relay_url])?;
                conn.execute("DELETE FROM remote_users WHERE relay_url=$1", &[&relay_url])?;
                Ok(())
            }
        }
    }

    fn upsert_remote_user(&self, username: &str, relay_url: &str) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO remote_users(username, relay_url, updated_at_ms) VALUES (?1, ?2, ?3)
                     ON CONFLICT(username) DO UPDATE SET relay_url=excluded.relay_url, updated_at_ms=excluded.updated_at_ms",
                    params![username, relay_url, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO remote_users(username, relay_url, updated_at_ms) VALUES ($1, $2, $3)
                     ON CONFLICT(username) DO UPDATE SET relay_url=EXCLUDED.relay_url, updated_at_ms=EXCLUDED.updated_at_ms",
                    &[&username, &relay_url, &now],
                )?;
                Ok(())
            }
        }
    }

    fn get_remote_user_relay(&self, username: &str) -> Result<Option<String>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT relay_url FROM remote_users WHERE username=?1",
                    params![username],
                    |r| r.get(0),
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT relay_url FROM remote_users WHERE username=$1",
                    &[&username],
                )?;
                Ok(row.map(|r| r.get(0)))
            }
        }
    }

    fn prune_remote_users(&self, older_than_ms: i64) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "DELETE FROM remote_users WHERE updated_at_ms < ?1",
                    params![older_than_ms],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "DELETE FROM remote_users WHERE updated_at_ms < $1",
                    &[&older_than_ms],
                )?;
                Ok(())
            }
        }
    }

    fn list_local_usernames(&self) -> Result<Vec<String>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT username FROM users WHERE disabled=0 ORDER BY username ASC LIMIT 5000",
                )?;
                let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
                Ok(rows.flatten().collect())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT username FROM users WHERE disabled=FALSE ORDER BY username ASC LIMIT 5000",
                    &[],
                )?;
                Ok(rows.iter().map(|r| r.get::<_, String>(0)).collect())
            }
        }
    }

    fn user_token_matches(&self, username: &str, token: &str) -> Result<bool> {
        let hash = token_hash_hex(token);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let row: Option<(String, i64)> = conn
                    .query_row(
                        "SELECT token_sha256, disabled FROM users WHERE username=?1",
                        params![username],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                Ok(matches!(row, Some((h, d)) if h == hash && d == 0))
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT token_sha256, disabled FROM users WHERE username=$1",
                    &[&username],
                )?;
                Ok(matches!(row, Some(r) if r.get::<_, String>(0) == hash && !r.get::<_, bool>(1)))
            }
        }
    }

    fn upsert_relay_block(
        &self,
        kind: &str,
        value: &str,
        reason: Option<&str>,
        created_by: Option<&str>,
        expires_at_ms: Option<i64>,
    ) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO relay_blocks(kind, value, reason, created_at_ms, created_by, expires_at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(kind, value) DO UPDATE SET reason=excluded.reason, created_at_ms=excluded.created_at_ms,
                       created_by=excluded.created_by, expires_at_ms=excluded.expires_at_ms",
                    params![kind, value, reason, now, created_by, expires_at_ms],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO relay_blocks(kind, value, reason, created_at_ms, created_by, expires_at_ms)
                     VALUES ($1, $2, $3, $4, $5, $6)
                     ON CONFLICT(kind, value) DO UPDATE SET reason=EXCLUDED.reason, created_at_ms=EXCLUDED.created_at_ms,
                       created_by=EXCLUDED.created_by, expires_at_ms=EXCLUDED.expires_at_ms",
                    &[&kind, &value, &reason, &now, &created_by, &expires_at_ms],
                )?;
                Ok(())
            }
        }
    }

    fn delete_relay_block(&self, kind: &str, value: &str) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "DELETE FROM relay_blocks WHERE kind=?1 AND value=?2",
                    params![kind, value],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "DELETE FROM relay_blocks WHERE kind=$1 AND value=$2",
                    &[&kind, &value],
                )?;
                Ok(())
            }
        }
    }

    fn list_relay_blocks(&self) -> Result<Vec<RelayBlockRow>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT kind, value, reason, expires_at_ms FROM relay_blocks ORDER BY created_at_ms DESC LIMIT 5000",
                )?;
                let rows = stmt.query_map([], |r| {
                    Ok(RelayBlockRow {
                        kind: r.get(0)?,
                        value: r.get(1)?,
                        reason: r.get(2)?,
                        expires_at_ms: r.get(3)?,
                    })
                })?;
                Ok(rows.flatten().collect())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT kind, value, reason, expires_at_ms FROM relay_blocks ORDER BY created_at_ms DESC LIMIT 5000",
                    &[],
                )?;
                Ok(rows
                    .iter()
                    .map(|r| RelayBlockRow {
                        kind: r.get::<_, String>(0),
                        value: r.get::<_, String>(1),
                        reason: r.get::<_, Option<String>>(2),
                        expires_at_ms: r.get::<_, Option<i64>>(3),
                    })
                    .collect())
            }
        }
    }

    fn replace_user_blocks(
        &self,
        username: &str,
        actors: &[String],
        domains: &[String],
    ) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let mut conn = self.open_sqlite_conn()?;
                let tx = conn.transaction()?;
                tx.execute(
                    "DELETE FROM user_blocks WHERE username=?1",
                    params![username],
                )?;
                for a in actors {
                    tx.execute(
                        "INSERT INTO user_blocks(username, kind, value, updated_at_ms) VALUES (?1, 'actor', ?2, ?3) ON CONFLICT DO NOTHING",
                        params![username, a, now],
                    )?;
                }
                for d in domains {
                    tx.execute(
                        "INSERT INTO user_blocks(username, kind, value, updated_at_ms) VALUES (?1, 'domain', ?2, ?3) ON CONFLICT DO NOTHING",
                        params![username, d, now],
                    )?;
                }
                tx.commit()?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let mut tx = conn.transaction()?;
                tx.execute("DELETE FROM user_blocks WHERE username=$1", &[&username])?;
                for a in actors {
                    tx.execute(
                        "INSERT INTO user_blocks(username, kind, value, updated_at_ms) VALUES ($1, 'actor', $2, $3) ON CONFLICT DO NOTHING",
                        &[&username, &a, &now],
                    )?;
                }
                for d in domains {
                    tx.execute(
                        "INSERT INTO user_blocks(username, kind, value, updated_at_ms) VALUES ($1, 'domain', $2, $3) ON CONFLICT DO NOTHING",
                        &[&username, &d, &now],
                    )?;
                }
                tx.commit()?;
                Ok(())
            }
        }
    }

    fn list_user_blocks(&self, username: &str) -> Result<Vec<(String, String)>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt =
                    conn.prepare("SELECT kind, value FROM user_blocks WHERE username=?1")?;
                let rows = stmt.query_map(params![username], |r| Ok((r.get(0)?, r.get(1)?)))?;
                Ok(rows.flatten().collect())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT kind, value FROM user_blocks WHERE username=$1",
                    &[&username],
                )?;
                Ok(rows
                    .iter()
                    .map(|r| (r.get::<_, String>(0), r.get::<_, String>(1)))
                    .collect())
            }
        }
    }

    /// Relay-wide or per-user block match (actor exact, domain exact/suffix), honoring expiry.
    fn is_blocked(&self, username: &str, sender_actor: &str) -> Result<bool> {
        let now = now_ms();
        let domain = origin_host(sender_actor).unwrap_or_default();
        let mut rules: Vec<(String, String, Option<i64>)> = Vec::new();
        for row in self.list_relay_blocks()? {
            rules.push((row.kind, row.value, row.expires_at_ms));
        }
        for (kind, value) in self.list_user_blocks(username)? {
            rules.push((kind, value, None));
        }
        for (kind, value, expires) in rules {
            if expires.map(|e| e <= now).unwrap_or(false) {
                continue;
            }
            let value = value.trim().trim_end_matches('/');
            if value.is_empty() {
                continue;
            }
            match kind.as_str() {
                "actor" if value == sender_actor.trim().trim_end_matches('/') => return Ok(true),
                "domain" if value == domain || domain.ends_with(&format!(".{value}")) => {
                    return Ok(true)
                }
                _ => {}
            }
        }
        Ok(false)
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

    fn cleanup_webrtc_signals(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms().saturating_sub((ttl_secs as i64) * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                Ok(conn.execute(
                    "DELETE FROM webrtc_signals WHERE created_at_ms < ?1",
                    params![cutoff],
                )? as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM webrtc_signals WHERE created_at_ms < $1",
                    &[&cutoff],
                )?;
                Ok(deleted)
            }
        }
    }

    /// Retention for append-only tables that used to grow forever (M-02 / RD-05).
    fn cleanup_admin_audit(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms().saturating_sub((ttl_secs as i64) * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                Ok(conn.execute(
                    "DELETE FROM admin_audit WHERE created_at_ms < ?1",
                    params![cutoff],
                )? as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                Ok(conn.execute(
                    "DELETE FROM admin_audit WHERE created_at_ms < $1",
                    &[&cutoff],
                )?)
            }
        }
    }

    fn cleanup_relay_notes(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms().saturating_sub((ttl_secs as i64) * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "DELETE FROM relay_note_tags WHERE note_id IN (SELECT note_id FROM relay_notes WHERE created_at_ms < ?1)",
                    params![cutoff],
                )?;
                Ok(conn.execute(
                    "DELETE FROM relay_notes WHERE created_at_ms < ?1",
                    params![cutoff],
                )? as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "DELETE FROM relay_note_tags WHERE note_id IN (SELECT note_id FROM relay_notes WHERE created_at_ms < $1)",
                    &[&cutoff],
                )?;
                Ok(conn.execute(
                    "DELETE FROM relay_notes WHERE created_at_ms < $1",
                    &[&cutoff],
                )?)
            }
        }
    }

    fn cleanup_peer_registry(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms().saturating_sub((ttl_secs as i64) * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                Ok(conn.execute(
                    "DELETE FROM peer_registry WHERE last_seen_ms < ?1",
                    params![cutoff],
                )? as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                Ok(conn.execute(
                    "DELETE FROM peer_registry WHERE last_seen_ms < $1",
                    &[&cutoff],
                )?)
            }
        }
    }

    fn cleanup_relay_registry(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms().saturating_sub((ttl_secs as i64) * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                Ok(conn.execute(
                    "DELETE FROM relay_registry WHERE last_seen_ms < ?1",
                    params![cutoff],
                )? as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                Ok(conn.execute(
                    "DELETE FROM relay_registry WHERE last_seen_ms < $1",
                    &[&cutoff],
                )?)
            }
        }
    }

    fn cleanup_relay_user_directory(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms().saturating_sub((ttl_secs as i64) * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                Ok(conn.execute(
                    "DELETE FROM relay_user_directory WHERE updated_at_ms < ?1",
                    params![cutoff],
                )? as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                Ok(conn.execute(
                    "DELETE FROM relay_user_directory WHERE updated_at_ms < $1",
                    &[&cutoff],
                )?)
            }
        }
    }

    /// Dedupe activities per (activity_id, recipient) with a short TTL (H-20 / RI-06).
    fn insert_inbox_dedupe(&self, activity_id: &str, recipient: &str) -> Result<bool> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let changed = conn.execute(
                    "INSERT OR IGNORE INTO inbox_dedupe(activity_id, recipient, created_at_ms) VALUES (?1, ?2, ?3)",
                    params![activity_id, recipient, now],
                )?;
                Ok(changed > 0)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let changed = conn.execute(
                    "INSERT INTO inbox_dedupe(activity_id, recipient, created_at_ms) VALUES ($1, $2, $3) ON CONFLICT (activity_id, recipient) DO NOTHING",
                    &[&activity_id, &recipient, &now],
                )?;
                Ok(changed > 0)
            }
        }
    }

    fn delete_inbox_dedupe(&self, activity_id: &str, recipient: &str) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "DELETE FROM inbox_dedupe WHERE activity_id = ?1 AND recipient = ?2",
                    params![activity_id, recipient],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "DELETE FROM inbox_dedupe WHERE activity_id = $1 AND recipient = $2",
                    &[&activity_id, &recipient],
                )?;
                Ok(())
            }
        }
    }

    fn cleanup_inbox_dedupe(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms().saturating_sub((ttl_secs as i64) * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                Ok(conn.execute(
                    "DELETE FROM inbox_dedupe WHERE created_at_ms < ?1",
                    params![cutoff],
                )? as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                Ok(conn.execute(
                    "DELETE FROM inbox_dedupe WHERE created_at_ms < $1",
                    &[&cutoff],
                )?)
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

    fn user_media_bytes(&self, username: &str) -> Result<u64> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let used: i64 = conn.query_row(
                    "SELECT COALESCE(SUM(size), 0) FROM media_items WHERE username=?1",
                    params![username],
                    |r| r.get(0),
                )?;
                Ok(used.max(0) as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_one(
                    "SELECT COALESCE(SUM(size), 0) FROM media_items WHERE username=$1",
                    &[&username],
                )?;
                let used: i64 = row.get(0);
                Ok(used.max(0) as u64)
            }
        }
    }

    fn delete_media_item(&self, username: &str, id: &str) -> Result<bool> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let changed = conn.execute(
                    "DELETE FROM media_items WHERE username=?1 AND id=?2",
                    params![username, id],
                )?;
                Ok(changed > 0)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let changed = conn.execute(
                    "DELETE FROM media_items WHERE username=$1 AND id=$2",
                    &[&username, &id],
                )?;
                Ok(changed > 0)
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

    fn search_relay_notes(
        &self,
        q: &str,
        tag: &str,
        limit: u32,
        cursor: Option<i64>,
        since: Option<i64>,
        total_mode: SearchTotalMode,
    ) -> Result<CollectionPage<String>> {
        let limit = limit.clamp(1, 200) as i64;
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
                WHERE lower(t.tag) LIKE ?1 ESCAPE '\\'
                "#,
                            params![tag_like],
                            |r| r.get(0),
                        )
                        .unwrap_or(0)
                    } else {
                        conn.query_row(
                            "SELECT COUNT(*) FROM relay_notes WHERE lower(content_text) LIKE ?1 ESCAPE '\\'",
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
                    WHERE lower(t.tag) LIKE ?1 ESCAPE '\\' AND n.created_at_ms > ?2
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
                    WHERE lower(t.tag) LIKE ?1 ESCAPE '\\' AND n.created_at_ms < ?2
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
                    WHERE lower(t.tag) LIKE ?1 ESCAPE '\\'
                    ORDER BY n.created_at_ms DESC
                    LIMIT ?2
                    "#,
                        )?;
                        rows = stmt.query(params![tag_like, limit])?;
                    }
                } else if let Some(since) = since {
                    stmt = conn.prepare(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE (lower(content_text) LIKE ?1 ESCAPE '\\') AND created_at_ms > ?2 ORDER BY created_at_ms DESC LIMIT ?3",
                    )?;
                    rows = stmt.query(params![q_like, since, limit])?;
                } else if let Some(cur) = cursor {
                    stmt = conn.prepare(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE (lower(content_text) LIKE ?1 ESCAPE '\\') AND created_at_ms < ?2 ORDER BY created_at_ms DESC LIMIT ?3",
                    )?;
                    rows = stmt.query(params![q_like, cur, limit])?;
                } else {
                    stmt = conn.prepare(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE lower(content_text) LIKE ?1 ESCAPE '\\' ORDER BY created_at_ms DESC LIMIT ?2",
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
                                    "SELECT COALESCE(SUM(count), 0) FROM relay_tag_counts WHERE lower(tag) LIKE ?1 ESCAPE '\\'",
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
                } else if let (false, Some(since_val)) = (q_norm.is_empty(), since) {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE search_tsv @@ plainto_tsquery('simple', $1) AND created_at_ms > $2 ORDER BY created_at_ms DESC LIMIT $3",
                        &[&q_norm, &since_val, &limit],
                    )?
                } else if let (false, Some(cur)) = (q_norm.is_empty(), cursor) {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE search_tsv @@ plainto_tsquery('simple', $1) AND created_at_ms < $2 ORDER BY created_at_ms DESC LIMIT $3",
                        &[&q_norm, &cur, &limit],
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
                                "SELECT COALESCE(SUM(count), 0) FROM relay_tag_counts WHERE lower(tag) LIKE $1 ESCAPE '\\'",
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
        let limit = limit.clamp(1, 200) as i64;
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

    fn search_relay_users(
        &self,
        q: &str,
        limit: u32,
        cursor: Option<i64>,
        base_template: &str,
        total_mode: SearchTotalMode,
    ) -> Result<CollectionPage<String>> {
        let limit = limit.clamp(1, 200) as i64;
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
                            "SELECT COUNT(*) FROM user_cache WHERE lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_id) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\'",
                            params![q_like],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    let total_users: u64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM users WHERE disabled=0 AND lower(username) LIKE ?1 ESCAPE '\\'",
                            params![q_like],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    let total_peers: u64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM peer_directory WHERE lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\'",
                            params![q_like],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    let total_relay_users: u64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM relay_user_directory WHERE lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\'",
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
                        "SELECT actor_json, updated_at_ms FROM user_cache WHERE (lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_id) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\') AND updated_at_ms < ?2 ORDER BY updated_at_ms DESC LIMIT ?3",
                    )?
                } else {
                    conn.prepare(
                        "SELECT actor_json, updated_at_ms FROM user_cache WHERE lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_id) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT ?2",
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
                    "SELECT username, created_at_ms FROM users WHERE disabled=0 AND lower(username) LIKE ?1 ESCAPE '\\' ORDER BY created_at_ms DESC LIMIT ?2",
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
                    "SELECT username, actor_url FROM peer_directory WHERE lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT ?2",
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
                    "SELECT username, actor_url FROM relay_user_directory WHERE lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT ?2",
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
                            "SELECT COUNT(*) FROM user_cache WHERE lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_id) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\'",
                            &[&q_like],
                        )?;
                        let n: i64 = row.get(0);
                        n.max(0) as u64
                    };
                    let total_users: u64 = {
                        let row = conn.query_one(
                            "SELECT COUNT(*) FROM users WHERE disabled=false AND lower(username) LIKE $1 ESCAPE '\\'",
                            &[&q_like],
                        )?;
                        let n: i64 = row.get(0);
                        n.max(0) as u64
                    };
                    let total_peers: u64 = {
                        let row = conn.query_one(
                            "SELECT COUNT(*) FROM peer_directory WHERE lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\'",
                            &[&q_like],
                        )?;
                        let n: i64 = row.get(0);
                        n.max(0) as u64
                    };
                    let total_relay_users: u64 = {
                        let row = conn.query_one(
                            "SELECT COUNT(*) FROM relay_user_directory WHERE lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\'",
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
                        "SELECT actor_json, updated_at_ms FROM user_cache WHERE (lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_id) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\') AND updated_at_ms < $2 ORDER BY updated_at_ms DESC LIMIT $3",
                        &[&q_like, &cur, &limit],
                    )?
                } else {
                    conn.query(
                        "SELECT actor_json, updated_at_ms FROM user_cache WHERE lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_id) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT $2",
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
                    "SELECT username, created_at_ms FROM users WHERE disabled=false AND lower(username) LIKE $1 ESCAPE '\\' ORDER BY created_at_ms DESC LIMIT $2",
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
                    "SELECT username, actor_url FROM peer_directory WHERE lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT $2",
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
                    "SELECT username, actor_url FROM relay_user_directory WHERE lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT $2",
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
        let limit = limit.clamp(1, 200) as i64;
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
            WHERE lower(tag) LIKE ?1 ESCAPE '\\'
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
            WHERE lower(tag) LIKE $1 ESCAPE '\\'
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

    fn list_peer_directory(&self, q: &str, limit: u32) -> Result<Vec<(String, String, String)>> {
        let limit = limit.clamp(1, 200) as i64;
        let q_norm = q.trim().to_lowercase();
        let q_like = if q_norm.is_empty() {
            "%".to_string()
        } else {
            format!("%{}%", escape_like(&q_norm))
        };
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT peer_id, username, actor_url FROM peer_directory WHERE lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT ?2",
                )?;
                let mut rows = stmt.query(params![q_like, limit])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    out.push((row.get(0)?, row.get(1)?, row.get(2)?));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT peer_id, username, actor_url FROM peer_directory WHERE lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT $2",
                    &[&q_like, &limit],
                )?;
                let mut out = Vec::new();
                for row in rows {
                    out.push((row.get(0), row.get(1), row.get(2)));
                }
                Ok(out)
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

    #[allow(dead_code)]
    fn list_relay_user_directory(
        &self,
        q: &str,
        limit: u32,
    ) -> Result<Vec<(String, String, String)>> {
        let limit = limit.clamp(1, 200) as i64;
        let q_norm = q.trim().to_lowercase();
        let q_like = if q_norm.is_empty() {
            "%".to_string()
        } else {
            format!("%{}%", escape_like(&q_norm))
        };
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT username, actor_url, relay_url FROM relay_user_directory WHERE lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT ?2",
                )?;
                let mut rows = stmt.query(params![q_like, limit])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    out.push((row.get(0)?, row.get(1)?, row.get(2)?));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT username, actor_url, relay_url FROM relay_user_directory WHERE lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT $2",
                    &[&q_like, &limit],
                )?;
                let mut out = Vec::new();
                for row in rows {
                    out.push((row.get(0), row.get(1), row.get(2)));
                }
                Ok(out)
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

    fn count_webrtc_signals_for_peer(&self, to_peer_id: &str) -> Result<u64> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let n: u64 = conn.query_row(
                    "SELECT COUNT(*) FROM webrtc_signals WHERE to_peer_id=?1",
                    params![to_peer_id],
                    |r| r.get(0),
                )?;
                Ok(n)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_one(
                    "SELECT COUNT(*) FROM webrtc_signals WHERE to_peer_id=$1",
                    &[&to_peer_id],
                )?;
                let n: i64 = row.get(0);
                Ok(n.max(0) as u64)
            }
        }
    }

    fn insert_webrtc_signal(
        &self,
        signal_id: &str,
        to_peer_id: &str,
        from_actor: &str,
        session_id: &str,
        kind: &str,
        payload_json: &str,
    ) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    r#"
            INSERT OR REPLACE INTO webrtc_signals(
              signal_id, to_peer_id, from_actor, session_id, kind, payload_json, created_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
                    params![
                        signal_id,
                        to_peer_id,
                        from_actor,
                        session_id,
                        kind,
                        payload_json,
                        now
                    ],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    r#"
            INSERT INTO webrtc_signals(
              signal_id, to_peer_id, from_actor, session_id, kind, payload_json, created_at_ms
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT(signal_id) DO UPDATE SET
              to_peer_id=EXCLUDED.to_peer_id,
              from_actor=EXCLUDED.from_actor,
              session_id=EXCLUDED.session_id,
              kind=EXCLUDED.kind,
              payload_json=EXCLUDED.payload_json,
              created_at_ms=EXCLUDED.created_at_ms
            "#,
                    &[
                        &signal_id,
                        &to_peer_id,
                        &from_actor,
                        &session_id,
                        &kind,
                        &payload_json,
                        &now,
                    ],
                )?;
                Ok(())
            }
        }
    }

    #[allow(clippy::type_complexity)]
    fn list_webrtc_signals(
        &self,
        to_peer_id: &str,
        limit: u32,
    ) -> Result<Vec<(String, String, String, String, String)>> {
        let limit = limit.clamp(1, 200) as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    r#"
            SELECT signal_id, from_actor, session_id, kind, payload_json
            FROM webrtc_signals
            WHERE to_peer_id=?1
            ORDER BY created_at_ms ASC
            LIMIT ?2
            "#,
                )?;
                let mut rows = stmt.query(params![to_peer_id, limit])?;
                let mut out = Vec::new();
                while let Some(r) = rows.next()? {
                    out.push((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    r#"
            SELECT signal_id, from_actor, session_id, kind, payload_json
            FROM webrtc_signals
            WHERE to_peer_id=$1
            ORDER BY created_at_ms ASC
            LIMIT $2
            "#,
                    &[&to_peer_id, &limit],
                )?;
                let mut out = Vec::new();
                for r in rows {
                    out.push((r.get(0), r.get(1), r.get(2), r.get(3), r.get(4)));
                }
                Ok(out)
            }
        }
    }

    fn delete_webrtc_signals(&self, to_peer_id: &str, ids: &[String]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        match self.driver {
            DbDriver::Sqlite => {
                let mut conn = self.open_sqlite_conn()?;
                let tx = conn.transaction()?;
                let mut deleted: u64 = 0;
                for chunk in ids.chunks(DB_BATCH_DELETE_MAX) {
                    let placeholders = std::iter::repeat_n("?", chunk.len())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let sql = format!(
                        "DELETE FROM webrtc_signals WHERE to_peer_id=?1 AND signal_id IN ({placeholders})"
                    );
                    let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(chunk.len() + 1);
                    params.push(&to_peer_id);
                    params.extend(chunk.iter().map(|id| id as &dyn rusqlite::ToSql));
                    let n = tx.execute(&sql, rusqlite::params_from_iter(params))?;
                    deleted = deleted.saturating_add(n as u64);
                }
                tx.commit()?;
                Ok(deleted)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let mut deleted: u64 = 0;
                for chunk in ids.chunks(DB_BATCH_DELETE_MAX) {
                    let params: &[&(dyn ToSql + Sync)] = &[&to_peer_id, &chunk];
                    let n = conn.execute(
                        "DELETE FROM webrtc_signals WHERE to_peer_id=$1 AND signal_id = ANY($2)",
                        params,
                    )?;
                    deleted = deleted.saturating_add(n);
                }
                Ok(deleted)
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

/// Constant-time comparison for credentials. Length differences are not
/// secret; the byte comparison itself must not short-circuit (H-14 / B5).
fn ct_eq_bytes(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}

fn ct_eq_str(a: &str, b: &str) -> bool {
    ct_eq_bytes(a.as_bytes(), b.as_bytes())
}

fn token_matches(expected: &str, provided: Option<&str>) -> bool {
    match provided {
        Some(p) if !p.is_empty() => ct_eq_str(expected, p),
        _ => false,
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
        if self.noisy_block_remaining(&ip).await.is_some() {
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
        Some((remaining_ms as u64).div_ceil(1000))
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
        let redis = self.redis_handle()?;
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
        let redis = self.redis_handle()?;
        let key = format!("{}:noisy:block:{}", self.redis_prefix, ip);
        let mut conn = redis.lock().await;
        let ttl: redis::RedisResult<i64> = conn.ttl(key).await;
        match ttl {
            Ok(v) if v > 0 => Some(v as u64),
            _ => None,
        }
    }

    async fn redis_register_noisy(&self, ip: &str) -> Option<()> {
        let redis = self.redis_handle()?;
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

    // The relay only trusts these headers when deployed behind a reverse proxy
    // that overwrites them. Caddy *appends* the real client IP to any inbound
    // X-Forwarded-For, so the last element is the only trustworthy one; taking
    // the first let clients spoof the rate-limit/allowlist key (H-13 / RL-06).
    if let Some(v) = headers.get("X-Forwarded-For").and_then(|v| v.to_str().ok()) {
        for part in v.split(',').rev() {
            if let Some(ip) = parse_ip_str(part) {
                return ip;
            }
        }
    }
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
    let s = s.trim().trim_matches('"').trim();
    // Bracketed IPv6, optionally with a port: [::1] or [::1]:443
    if let Some(rest) = s.strip_prefix('[') {
        let ip_part = rest.split(']').next()?.trim();
        return ip_part.parse::<IpAddr>().ok().map(|ip| ip.to_string());
    }
    // Bare address (IPv4 or IPv6).
    if let Ok(ip) = s.parse::<IpAddr>() {
        return Some(ip.to_string());
    }
    // host:port (IPv4 only; bare IPv6 already matched above).
    if let Some((host, port)) = s.rsplit_once(':') {
        if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(ip) = host.parse::<IpAddr>() {
                return Some(ip.to_string());
            }
        }
    }
    None
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
    raw.split([',', ' ', '\n', '\t'])
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
    // Multiple forwarded elements may be present; the last one is the one added
    // by our trusted proxy.
    let mut last = None;
    for part in forwarded.split(';') {
        let part = part.trim();
        let lower = part.to_ascii_lowercase();
        if !lower.starts_with("for=") {
            continue;
        }
        let raw = part.split_once('=')?.1.trim();
        if let Some(ip) = parse_ip_str(raw) {
            last = Some(ip);
        }
    }
    last
}

#[allow(clippy::result_large_err)]
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

#[derive(Debug, Deserialize)]
struct PeerReq {
    url: String,
}

#[derive(Debug, Deserialize)]
struct UserBlocksReq {
    #[serde(default)]
    blocked_actors: Vec<String>,
    #[serde(default)]
    blocked_domains: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BlockReq {
    kind: String,
    value: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    expires_at_ms: Option<i64>,
}

/// Public relay directory used by peers to learn which users are hosted here.
async fn relay_user_directory(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.lock().await;
    let users = db.list_local_usernames().unwrap_or_default();
    drop(db);
    let body = serde_json::json!({
        "relay": relay_self_base(&state.cfg),
        "users": users,
    });
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        body.to_string(),
    )
        .into_response()
}

async fn admin_list_peers(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(resp) = admin_guard(&state, &peer, &headers, "admin_list_peers", None).await {
        return resp;
    }
    let db = state.db.lock().await;
    let peers = db.list_peers().unwrap_or_default();
    drop(db);
    let body = serde_json::json!({
        "peers": peers
            .into_iter()
            .map(|(url, seen)| serde_json::json!({"url": url, "last_seen_ms": seen}))
            .collect::<Vec<_>>()
    });
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        body.to_string(),
    )
        .into_response()
}

async fn admin_add_peer(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    axum::Json(req): axum::Json<PeerReq>,
) -> impl IntoResponse {
    if let Err(resp) = admin_guard(&state, &peer, &headers, "admin_add_peer", None).await {
        return resp;
    }
    let url = req.url.trim().trim_end_matches('/').to_string();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return (StatusCode::BAD_REQUEST, "invalid peer url").into_response();
    }
    let db = state.db.lock().await;
    match db.upsert_peer(&url) {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    }
}

async fn admin_delete_peer(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if let Err(resp) = admin_guard(&state, &peer, &headers, "admin_delete_peer", None).await {
        return resp;
    }
    let Some(url) = q
        .get("url")
        .map(|s| s.trim().trim_end_matches('/').to_string())
    else {
        return (StatusCode::BAD_REQUEST, "missing url").into_response();
    };
    let db = state.db.lock().await;
    match db.delete_peer(&url) {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    }
}

async fn admin_list_blocks(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(resp) = admin_guard(&state, &peer, &headers, "admin_list_blocks", None).await {
        return resp;
    }
    let db = state.db.lock().await;
    let blocks = db.list_relay_blocks().unwrap_or_default();
    drop(db);
    let body = serde_json::json!({
        "blocks": blocks
            .into_iter()
            .map(|row| serde_json::json!({
                "kind": row.kind,
                "value": row.value,
                "reason": row.reason,
                "expires_at_ms": row.expires_at_ms,
            }))
            .collect::<Vec<_>>()
    });
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        body.to_string(),
    )
        .into_response()
}

async fn admin_add_block(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    axum::Json(req): axum::Json<BlockReq>,
) -> impl IntoResponse {
    if let Err(resp) = admin_guard(&state, &peer, &headers, "admin_add_block", None).await {
        return resp;
    }
    let kind = req.kind.trim().to_lowercase();
    if kind != "actor" && kind != "domain" {
        return (StatusCode::BAD_REQUEST, "kind must be actor or domain").into_response();
    }
    let value = req.value.trim().trim_end_matches('/').to_string();
    if value.is_empty() {
        return (StatusCode::BAD_REQUEST, "empty value").into_response();
    }
    let db = state.db.lock().await;
    match db.upsert_relay_block(
        &kind,
        &value,
        req.reason.as_deref(),
        Some("admin"),
        req.expires_at_ms,
    ) {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    }
}

async fn admin_delete_block(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if let Err(resp) = admin_guard(&state, &peer, &headers, "admin_delete_block", None).await {
        return resp;
    }
    let (Some(kind), Some(value)) = (q.get("kind"), q.get("value")) else {
        return (StatusCode::BAD_REQUEST, "missing kind or value").into_response();
    };
    let db = state.db.lock().await;
    match db.delete_relay_block(kind.trim(), value.trim().trim_end_matches('/')) {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    }
}

/// Per-user blocklist pushed by the core over the relay token. The relay enforces it on the
/// inbox path, so a user's moderation choices apply before the core is even reached.
async fn user_blocks_put(
    State(state): State<AppState>,
    Path(user): Path<String>,
    headers: HeaderMap,
    axum::Json(req): axum::Json<UserBlocksReq>,
) -> impl IntoResponse {
    let token = headers
        .get("x-fedi3-relay-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if token.is_empty() {
        return (StatusCode::UNAUTHORIZED, "missing relay token").into_response();
    }
    let db = state.db.lock().await;
    if !db.user_token_matches(&user, token).unwrap_or(false) {
        return (StatusCode::UNAUTHORIZED, "invalid relay token").into_response();
    }
    match db.replace_user_blocks(&user, &req.blocked_actors, &req.blocked_domains) {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    }
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

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct RelaySyncNoteItem {
    note: serde_json::Value,
    created_at_ms: i64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct RelaySyncNotesResponse {
    items: Vec<RelaySyncNoteItem>,
    next: Option<String>,
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
    headers: HeaderMap,
    Query(q): Query<RelaySyncNotesQuery>,
) -> impl IntoResponse {
    if !state
        .limiter
        .check(
            client_ip(&state.cfg, &peer, &headers),
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
        if let Ok(Some((ms, ok))) = db.get_outbox_index_state(user) {
            user_state = Some(serde_json::json!({
                "username": user,
                "last_index_ms": ms,
                "last_ok": ok
            }));
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
            .cloned()
            .collect::<std::collections::HashSet<String>>()
    };
    let db = state.db.lock().await;
    let rows = match db.list_peer_directory(&query, limit) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response(),
    };
    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(peer_id, username, actor_url)| {
            serde_json::json!({
              "peer_id": peer_id,
              "username": username,
              "actor_url": actor_url,
              "online": online_users.contains(&username),
            })
        })
        .collect();
    axum::Json(serde_json::json!({ "items": items })).into_response()
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
        if !ct_eq_str(got, expected) {
            return (StatusCode::UNAUTHORIZED, "telemetry token required").into_response();
        }
    }

    if !state
        .limiter
        .check(
            client_ip(&state.cfg, &peer, &headers),
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
            client_ip(&state.cfg, &peer, &headers),
            "move",
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
            client_ip(&state.cfg, &peer, &headers),
            "move",
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
            client_ip(&state.cfg, &peer, &headers),
            "move_notice",
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
    let authorized = if sig_ok || is_authorized_admin(&state.cfg, &headers) {
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

async fn webrtc_signal_send(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    uri: OriginalUri,
    body: Bytes,
) -> impl IntoResponse {
    if !state
        .limiter
        .check(
            client_ip(&state.cfg, &peer, &headers),
            "webrtc_send",
            state.cfg.rate_limit_forward_per_min,
        )
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }

    if body.len() > 256 * 1024 {
        return (StatusCode::PAYLOAD_TOO_LARGE, "payload too large").into_response();
    }

    let req: WebRtcSignalSendReq = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid json").into_response(),
    };
    let to_peer_id = req.to_peer_id.trim().to_string();
    if to_peer_id.is_empty() || to_peer_id.len() > 128 {
        return (StatusCode::BAD_REQUEST, "invalid to_peer_id").into_response();
    }
    let session_id = req.session_id.trim().to_string();
    if session_id.is_empty() || session_id.len() > 128 {
        return (StatusCode::BAD_REQUEST, "invalid session_id").into_response();
    }
    let kind = req.kind.trim().to_ascii_lowercase();
    if kind != "offer" && kind != "answer" && kind != "candidate" {
        return (StatusCode::BAD_REQUEST, "invalid kind").into_response();
    }

    let (actor, _actor_json) = match verify_actor_signature_for_request(
        &state,
        &Method::POST,
        &uri.0,
        &headers,
        Some(&body),
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::UNAUTHORIZED, format!("signature invalid: {e}")).into_response()
        }
    };

    // Backpressure / anti-abuse: cap queued signals per peer.
    {
        let db = state.db.lock().await;
        match db.count_webrtc_signals_for_peer(&to_peer_id) {
            Ok(n) if n >= state.cfg.webrtc_signal_max_per_peer as u64 => {
                return (StatusCode::TOO_MANY_REQUESTS, "queue full").into_response();
            }
            Ok(_) => {}
            Err(e) => return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response(),
        }
    }

    let signal_id = generate_token();
    let payload_json = serde_json::to_string(&req.payload).unwrap_or_else(|_| "null".to_string());
    {
        let db = state.db.lock().await;
        if let Err(e) = db.insert_webrtc_signal(
            &signal_id,
            &to_peer_id,
            &actor,
            &session_id,
            &kind,
            &payload_json,
        ) {
            return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response();
        }
    }

    axum::Json(serde_json::json!({ "ok": true, "id": signal_id })).into_response()
}

async fn webrtc_signal_poll(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    uri: OriginalUri,
    Query(q): Query<WebRtcSignalPollQuery>,
) -> impl IntoResponse {
    if !state
        .limiter
        .check(
            client_ip(&state.cfg, &peer, &headers),
            "webrtc_poll",
            state.cfg.rate_limit_forward_per_min,
        )
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }

    let to_peer_id = q.to_peer_id.unwrap_or_default().trim().to_string();
    if to_peer_id.is_empty() || to_peer_id.len() > 128 {
        return (StatusCode::BAD_REQUEST, "invalid to_peer_id").into_response();
    }
    let limit = q.limit.unwrap_or(50).clamp(1, 200);

    let (actor, actor_json) = match verify_actor_signature_for_request(
        &state,
        &Method::GET,
        &uri.0,
        &headers,
        None,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::UNAUTHORIZED, format!("signature invalid: {e}")).into_response()
        }
    };
    let actor_peer = extract_peer_id_from_actor_json(&actor_json).unwrap_or_default();
    if actor_peer != to_peer_id {
        return (StatusCode::FORBIDDEN, "peer mismatch").into_response();
    }

    let rows = {
        let db = state.db.lock().await;
        match db.list_webrtc_signals(&to_peer_id, limit) {
            Ok(v) => v,
            Err(e) => return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response(),
        }
    };

    let mut out = Vec::new();
    for (id, from_actor, session_id, kind, payload_json) in rows {
        let payload: serde_json::Value =
            serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null);
        out.push(serde_json::json!({
          "id": id,
          "from_actor": from_actor,
          "session_id": session_id,
          "kind": kind,
          "payload": payload,
        }));
    }

    axum::Json(serde_json::json!({ "ok": true, "actor": actor, "messages": out })).into_response()
}

async fn webrtc_signal_ack(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    uri: OriginalUri,
    body: Bytes,
) -> impl IntoResponse {
    if !state
        .limiter
        .check(
            client_ip(&state.cfg, &peer, &headers),
            "webrtc_ack",
            state.cfg.rate_limit_forward_per_min,
        )
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }

    if body.len() > 64 * 1024 {
        return (StatusCode::PAYLOAD_TOO_LARGE, "payload too large").into_response();
    }

    let req: WebRtcSignalAckReq = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid json").into_response(),
    };
    let to_peer_id = req.to_peer_id.trim().to_string();
    if to_peer_id.is_empty() || to_peer_id.len() > 128 {
        return (StatusCode::BAD_REQUEST, "invalid to_peer_id").into_response();
    }

    let (_actor, actor_json) = match verify_actor_signature_for_request(
        &state,
        &Method::POST,
        &uri.0,
        &headers,
        Some(&body),
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::UNAUTHORIZED, format!("signature invalid: {e}")).into_response()
        }
    };
    let actor_peer = extract_peer_id_from_actor_json(&actor_json).unwrap_or_default();
    if actor_peer != to_peer_id {
        return (StatusCode::FORBIDDEN, "peer mismatch").into_response();
    }

    let deleted = {
        let db = state.db.lock().await;
        match db.delete_webrtc_signals(&to_peer_id, &req.ids) {
            Ok(v) => v,
            Err(e) => return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response(),
        }
    };

    axum::Json(serde_json::json!({ "ok": true, "deleted": deleted })).into_response()
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
    let online_peers = {
        let map = state.peer_hello.read().await;
        let mut s = std::collections::HashSet::new();
        for h in map.values() {
            if let Some(pid) = h.p2p.as_ref().and_then(|p| p.peer_id.as_ref()) {
                s.insert(pid.clone());
            }
        }
        s.len() as u64
    };

    let peers_seen_window_ms: i64 = 30 * 24 * 3600 * 1000;
    let cutoff_ms = now_ms().saturating_sub(peers_seen_window_ms);
    let search_window_ms: i64 = 24 * 3600 * 1000;
    let search_cutoff_ms = now_ms().saturating_sub(search_window_ms);
    let relay_sync_window_ms: i64 = 24 * 3600 * 1000;
    let relay_sync_cutoff_ms = now_ms().saturating_sub(relay_sync_window_ms);
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
            .list_peer_directory("", state.cfg.telemetry_peers_limit)
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
        search_relays_last_sync_ms,
        search_relay_sync_window_ms: Some(relay_sync_window_ms),
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
        if let Err(e) =
            ensure_fetch_target_allowed(&url, state.cfg.allow_private_remote_fetch).await
        {
            debug!(relay_url = %relay_url, "telemetry push skipped: {e:#}");
            continue;
        }
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
        // Telemetry replies can carry hundreds of relay/user/peer entries, so
        // treat them as a collection page rather than tiny metadata.
        let body = match read_body_capped(resp, MAX_PEER_PAGE_BYTES).await {
            Ok(body) => body,
            Err(e) => {
                debug!(relay_url = %relay_url, "telemetry response skipped: {e:#}");
                continue;
            }
        };
        if let Ok(remote) = serde_json::from_slice::<RelayTelemetry>(&body) {
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
            .map(|(url, _, _, _, _)| url)
            .collect::<Vec<_>>();
        for r in &state.cfg.seed_relays {
            out.push(r.clone());
        }
        out.sort();
        out.dedup();
        if let Some(self_url) = &self_url {
            out.retain(|u| u != self_url);
        }
        out
    };

    for relay_url in relays {
        if let Err(e) = sync_relay_notes(state, &relay_url).await {
            error!(relay_url = %relay_url, "relay sync failed: {e:#}");
        }
    }
    Ok(())
}

async fn sync_relay_notes(state: &AppState, relay_url: &str) -> Result<()> {
    let key = format!("relay_sync_last_ms:{relay_url}");
    let last_seen = {
        let db = state.db.lock().await;
        db.relay_meta_get(&key)
            .ok()
            .flatten()
            .and_then(|v| v.parse::<i64>().ok())
    };
    let limit = state.cfg.relay_sync_limit.clamp(1, 200);
    let mut cursor = None;
    let mut since = last_seen;
    let mut max_seen = last_seen.unwrap_or(0);
    let mut pages = 0u32;

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
        let accept = "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\", application/json";
        let data = match fetch_peer_json::<RelaySyncNotesResponse>(
            state,
            &url,
            accept,
            MAX_PEER_PAGE_BYTES,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                debug!(%url, "relay sync page skipped: {e:#}");
                break;
            }
        };
        if data.items.is_empty() {
            break;
        }
        let db = state.db.lock().await;
        for item in data.items {
            if item.created_at_ms > max_seen {
                max_seen = item.created_at_ms;
            }
            if let Some(indexed) = note_to_index(&item.note) {
                let _ = db.upsert_relay_note(&indexed);
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
    Ok(())
}

fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn strip_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for c in input.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ => {
                if !in_tag {
                    out.push(c);
                }
            }
        }
    }
    out
}

fn extract_notes_from_value(value: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    match value {
        serde_json::Value::Object(map) => {
            let ty = map.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if ty == "Note" {
                out.push(value.clone());
                return out;
            }
            if ty == "Create" || ty == "Announce" {
                if let Some(obj) = map.get("object") {
                    if let serde_json::Value::Object(obj_map) = obj {
                        let inner = if obj_map.get("type").and_then(|t| t.as_str()) == Some("Note")
                        {
                            Some(obj)
                        } else {
                            obj_map.get("object")
                        };
                        if let Some(note) = inner {
                            if note.get("type").and_then(|t| t.as_str()) == Some("Note") {
                                out.push(note.clone());
                                return out;
                            }
                        }
                    }
                }
            }
            if ty == "OrderedCollection"
                || ty == "OrderedCollectionPage"
                || ty == "Collection"
                || ty == "CollectionPage"
            {
                if let Some(serde_json::Value::Array(arr)) =
                    map.get("orderedItems").or_else(|| map.get("items"))
                {
                    for item in arr {
                        out.extend(extract_notes_from_value(item));
                    }
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                out.extend(extract_notes_from_value(item));
            }
        }
        _ => {}
    }
    out
}

fn note_to_index(note: &serde_json::Value) -> Option<RelayNoteIndex> {
    let id = note
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if id.is_empty() {
        return None;
    }
    let actor_id = note
        .get("attributedTo")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string());
    let published_ms = note
        .get("published")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp_millis());
    let content_html = note
        .get("content")
        .or_else(|| note.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let content_text = strip_html(&content_html);
    let tags = extract_tags(note.get("tag"));
    let note_json = serde_json::to_string(note).unwrap_or_default();
    Some(RelayNoteIndex {
        note_id: id,
        actor_id,
        published_ms,
        content_text,
        content_html,
        note_json,
        created_at_ms: now_ms(),
        tags,
    })
}

fn extract_tags(tag_value: Option<&serde_json::Value>) -> Vec<String> {
    let Some(tag_value) = tag_value else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut push_tag = |name: &str| {
        let t = name.trim().trim_start_matches('#').to_string();
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    };
    match tag_value {
        serde_json::Value::Array(arr) => {
            for item in arr {
                if let Some(tag) = extract_tag_name(item) {
                    push_tag(&tag);
                }
            }
        }
        _ => {
            if let Some(tag) = extract_tag_name(tag_value) {
                push_tag(&tag);
            }
        }
    }
    out
}

fn extract_tag_name(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => {
            if s.trim().starts_with('#') {
                Some(s.trim().to_string())
            } else {
                None
            }
        }
        serde_json::Value::Object(map) => {
            let ty = map.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let name = map.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if ty == "Hashtag" || name.starts_with('#') {
                Some(name.to_string())
            } else {
                None
            }
        }
        _ => None,
    }
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

/// Full Host authority including an explicit port (M-04).
fn authority_from_headers(headers: &HeaderMap) -> String {
    headers
        .get("Host")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "localhost".to_string())
}

fn split_authority(authority: &str) -> (String, Option<String>) {
    let authority = authority.trim().trim_end_matches('.').to_ascii_lowercase();
    if let Some(rest) = authority.strip_prefix('[') {
        // [ipv6]:port
        if let Some((host, port)) = rest.split_once("]:") {
            return (format!("[{host}]"), Some(port.to_string()));
        }
        return (authority, None);
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => {
            (host.to_string(), Some(port.to_string()))
        }
        _ => (authority, None),
    }
}

fn host_without_port(authority: &str) -> String {
    split_authority(authority)
        .0
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string()
}

/// Normalized `scheme://authority` used for canonical comparisons; default
/// ports are dropped so `http://host` and `http://host:80` compare equal.
fn normalized_origin(scheme: &str, authority: &str) -> String {
    let (host, port) = split_authority(authority);
    let scheme = scheme.trim().to_ascii_lowercase();
    let default_port = match scheme.as_str() {
        "http" => Some("80".to_string()),
        "https" => Some("443".to_string()),
        _ => None,
    };
    match port {
        Some(p) if Some(&p) == default_port.as_ref() => format!("{scheme}://{host}"),
        Some(p) => format!("{scheme}://{host}:{p}"),
        None => format!("{scheme}://{host}"),
    }
}

/// M-03: in host-routing mode only `base_domain` and its subdomains are
/// accepted, so a spoofed `Host: evil.example` cannot leak into actor URLs.
fn host_allowed_for_cfg(cfg: &RelayConfig, authority: &str) -> bool {
    let Some(base) = cfg.base_domain.as_deref() else {
        return true;
    };
    let host = host_without_port(authority);
    host == base || host.ends_with(&format!(".{base}"))
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
    let scheme = scheme_from_headers(headers).to_string();
    let authority = authority_from_headers(headers);
    if cfg.base_domain.is_some() {
        if host_allowed_for_cfg(cfg, &authority) {
            return (scheme, authority);
        }
        // Invalid Host: derive the origin from the configured canonical URL
        // instead of trusting the client-supplied header (M-03).
        if let Some((s, h)) = canonical_origin(cfg) {
            return (s, h);
        }
        return (scheme, cfg.base_domain.clone().unwrap_or(authority));
    }
    if let Some((s, h)) = canonical_origin(cfg) {
        return (s, h);
    }
    (scheme, authority)
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
    let (canon_scheme, canon_authority) = canonical_origin(cfg)?;

    let cur_scheme = scheme_from_headers(headers);
    let cur_authority = authority_from_headers(headers);
    // Compare the full authority, including an explicit port: comparing only
    // the hostname made `http://localhost:8787` redirect to itself forever (M-04).
    if normalized_origin(cur_scheme, &cur_authority)
        == normalized_origin(&canon_scheme, &canon_authority)
    {
        return None;
    }

    let qs = raw_query.map(|q| format!("?{q}")).unwrap_or_default();
    let location = format!("{canon_scheme}://{canon_authority}{path}{qs}");
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
    if let Err(e) = ensure_fetch_target_allowed(&url, state.cfg.allow_private_remote_fetch).await {
        debug!(relay_url = %relay_url, "move notice fan-out skipped: {e:#}");
        let db = state.db.lock().await;
        let _ = db.record_fanout_attempt(notice_id, relay_url, false);
        return Ok(false);
    }
    let target: http::Uri = url
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid relay URL {url:?}: {e}"))?;
    let host = target
        .authority()
        .map(|a| a.as_str().to_string())
        .unwrap_or_default();
    // Sign the fan-out with the relay Ed25519 key; the receiver verifies it
    // against the key pinned in its registry (H-08 / RN-02).
    let signed_headers =
        signed_relay_headers(state, "POST", "/_fedi3/relay/move_notice", &host, body).await?;
    let mut req = state
        .http
        .post(url)
        .header("Content-Type", "application/json")
        .header("X-Fedi3-Notice-Id", notice_id.to_string())
        .header("X-Fedi3-Notice-Hop", hop.to_string())
        .body(body.to_vec());
    for (name, value) in signed_headers {
        req = req.header(name, value);
    }
    let ok = match req.send().await {
        Ok(resp) if resp.status().is_success() => read_body_capped(resp, MAX_PEER_METADATA_BYTES)
            .await
            .is_ok(),
        Ok(_) => false,
        Err(_) => false,
    };
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
    key_id: String,
    algorithm: Option<String>,
    headers: Vec<String>,
    signature: Vec<u8>,
}

/// Parse a `Signature` header with quoted-string support and case-insensitive
/// parameter names. Splitting on commas broke quoted values and `keyId=` was
/// case-sensitive before (H-15).
fn parse_signature_header(value: &str) -> Result<SignatureParams> {
    let mut key_id: Option<String> = None;
    let mut algorithm: Option<String> = None;
    let mut headers_raw: Option<String> = None;
    let mut signature_b64: Option<String> = None;
    let bytes = value.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' {
            i += 1;
        }
        if i >= bytes.len() {
            anyhow::bail!("malformed Signature parameter");
        }
        let key = value[key_start..i].trim().to_ascii_lowercase();
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let val = if i < bytes.len() && bytes[i] == b'"' {
            i += 1;
            let mut out = String::new();
            let mut closed = false;
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' => {
                        i += 1;
                        if i < bytes.len() {
                            out.push(bytes[i] as char);
                            i += 1;
                        }
                    }
                    b'"' => {
                        i += 1;
                        closed = true;
                        break;
                    }
                    b => {
                        out.push(b as char);
                        i += 1;
                    }
                }
            }
            if !closed {
                anyhow::bail!("unterminated quoted string in Signature header");
            }
            out
        } else {
            let start = i;
            while i < bytes.len() && bytes[i] != b',' {
                i += 1;
            }
            value[start..i].trim().to_string()
        };
        match key.as_str() {
            "keyid" => key_id = Some(val),
            "algorithm" => algorithm = Some(val.to_ascii_lowercase()),
            "headers" => headers_raw = Some(val),
            "signature" => signature_b64 = Some(val),
            _ => {}
        }
    }

    let key_id = key_id
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Signature missing keyId"))?;
    let headers_raw = headers_raw.ok_or_else(|| anyhow::anyhow!("Signature missing headers"))?;
    let headers: Vec<String> = headers_raw
        .split_whitespace()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    if headers.is_empty() {
        anyhow::bail!("Signature headers empty");
    }
    let signature_b64 =
        signature_b64.ok_or_else(|| anyhow::anyhow!("Signature missing signature"))?;
    let signature = B64.decode(signature_b64.trim().as_bytes())?;
    if signature.is_empty() {
        anyhow::bail!("Signature empty");
    }
    Ok(SignatureParams {
        key_id,
        algorithm,
        headers,
        signature,
    })
}

/// Minimum signed-header set. Without these an attacker can rewrite the body,
/// the path or the Date while the signature still verifies (H-01 / H-15).
fn require_signed_headers(params: &SignatureParams, has_body: bool) -> Result<()> {
    let mut required = vec!["(request-target)", "host", "date"];
    if has_body {
        required.push("digest");
    }
    for name in required {
        if !params.headers.iter().any(|h| h == name) {
            anyhow::bail!("signed headers missing {name}");
        }
    }
    Ok(())
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

/// Verify a `Digest` header. For requests with a body the digest is mandatory
/// and SHA-256 is the only accepted algorithm (H-01 / B4).
fn verify_digest_header(headers: &HeaderMap, body: Option<&[u8]>, required: bool) -> Result<()> {
    let digest = headers
        .get("Digest")
        .or_else(|| headers.get("digest"))
        .and_then(|v| v.to_str().ok());
    match (body, digest) {
        (Some(body), Some(raw)) => {
            let mut sha256: Option<&str> = None;
            for part in raw.split(',') {
                if let Some((alg, val)) = part.split_once('=') {
                    if alg.trim().eq_ignore_ascii_case("SHA-256") {
                        sha256 = Some(val.trim());
                    }
                }
            }
            let Some(value) = sha256 else {
                anyhow::bail!("unsupported Digest algorithm (SHA-256 required)");
            };
            let expected = B64.decode(value.as_bytes()).unwrap_or_default();
            let actual = Sha256::digest(body);
            if !ct_eq_bytes(&expected, &actual) {
                anyhow::bail!("Digest mismatch");
            }
        }
        (Some(_), None) if required => anyhow::bail!("missing Digest"),
        _ => {}
    }
    Ok(())
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

/// Ed25519 verification for modern ActivityPub implementations and for the
/// relay's own signed move-notice fan-out. Accepts an SPKI PEM or a raw
/// base64-encoded 32-byte public key (the format used by relay telemetry).
fn verify_signature_ed25519(key_material: &str, signing_string: &str, signature: &[u8]) -> bool {
    let key_material = key_material.trim();
    if key_material.is_empty() {
        return false;
    }
    let Ok(sig) = ed25519_dalek::Signature::from_slice(signature) else {
        return false;
    };
    if key_material.contains("BEGIN") {
        use ed25519_dalek::pkcs8::DecodePublicKey as _;
        let Ok(vk) = ed25519_dalek::VerifyingKey::from_public_key_pem(key_material) else {
            return false;
        };
        return vk.verify(signing_string.as_bytes(), &sig).is_ok();
    }
    let Ok(bytes) = B64.decode(key_material.as_bytes()) else {
        return false;
    };
    if bytes.len() != 32 {
        return false;
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    let Ok(vk) = ed25519_dalek::VerifyingKey::from_bytes(&arr) else {
        return false;
    };
    vk.verify(signing_string.as_bytes(), &sig).is_ok()
}

fn verify_signature_with_algorithm(
    key_material: &str,
    signing_string: &str,
    signature: &[u8],
    algorithm: Option<&str>,
) -> bool {
    match algorithm.unwrap_or("rsa-sha256") {
        "ed25519" => verify_signature_ed25519(key_material, signing_string, signature),
        // hs2019 does not pin the algorithm; accept either RSA or Ed25519.
        "hs2019" | "rsa-sha256" => {
            verify_signature_rsa_sha256(key_material, signing_string, signature)
                || verify_signature_ed25519(key_material, signing_string, signature)
        }
        _ => {
            verify_signature_rsa_sha256(key_material, signing_string, signature)
                || verify_signature_ed25519(key_material, signing_string, signature)
        }
    }
}

fn extract_public_key_pem_from_actor_json(actor_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(actor_json).ok()?;
    let pk = v.get("publicKey")?;
    let pem = pk
        .get("publicKeyPem")
        .or_else(|| pk.get("publicKeyBase64"))
        .and_then(|v| v.as_str())?;
    let pem = pem.trim();
    if pem.is_empty() {
        None
    } else {
        Some(pem.to_string())
    }
}

fn extract_peer_id_from_actor_json(actor_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(actor_json).ok()?;
    let endpoints = v.get("endpoints")?;
    let pid = endpoints.get("fedi3PeerId")?.as_str()?;
    let pid = pid.trim();
    if pid.is_empty() {
        None
    } else {
        Some(pid.to_string())
    }
}

fn actor_from_key_id(key_id: &str) -> Option<String> {
    let k = key_id.trim();
    if k.is_empty() {
        return None;
    }
    // Typical ActivityPub `keyId` is `https://host/users/alice#main-key`.
    let actor = k.split('#').next().unwrap_or("").trim();
    if actor.starts_with("http://") || actor.starts_with("https://") {
        Some(actor.to_string())
    } else {
        None
    }
}

/// Origin (`scheme://authority`) of a keyId, used to look up a relay's pinned
/// Ed25519 key in the registry.
fn relay_origin_from_key_id(key_id: &str) -> Option<String> {
    let k = key_id.split('#').next().unwrap_or("").trim();
    let uri: http::Uri = k.parse().ok()?;
    let scheme = uri.scheme_str()?;
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority = uri.authority()?.as_str();
    Some(format!("{scheme}://{authority}"))
}

/// Maximum redirects followed by any relay-initiated fetch.
const MAX_PEER_FETCH_REDIRECTS: usize = 3;
/// Cap for paginated ActivityPub collections (sync pages, outbox pages).
const MAX_PEER_PAGE_BYTES: usize = 2 * 1024 * 1024;
/// Cap for small peer metadata (actor documents, telemetry, receipts).
const MAX_PEER_METADATA_BYTES: usize = 256 * 1024;

/// True only for addresses that are globally routable. Conservative on
/// purpose: a single non-public resolution result is enough to reject a URL.
fn is_public_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            // 100.64.0.0/10 is carrier-grade NAT, not reachable from the
            // public Internet even though `Ipv4Addr::is_private` says no.
            let shared = {
                let o = v4.octets();
                o[0] == 100 && (64..=127).contains(&o[1])
            };
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                || shared)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_ip(&IpAddr::V4(v4));
            }
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || v6.is_multicast())
        }
    }
}

/// Redirect policy shared by every relay-initiated HTTP client: at most three
/// hops, http(s) only, and no hop to a literal non-public address.
fn peer_fetch_redirect_policy(allow_private: bool) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= MAX_PEER_FETCH_REDIRECTS {
            return attempt.error("too many redirects");
        }
        let url = attempt.url();
        if !matches!(url.scheme(), "http" | "https") {
            return attempt.error("redirect to unsupported scheme");
        }
        if !allow_private {
            if let Some(host) = url.host_str() {
                let host = host.trim_start_matches('[').trim_end_matches(']');
                if let Ok(ip) = host.parse::<IpAddr>() {
                    if !is_public_ip(&ip) {
                        return attempt.error("redirect to non-public address");
                    }
                }
            }
        }
        attempt.follow()
    })
}

/// SSRF guard for URLs announced by remote peers (H-35): only http(s), and
/// unless the escape hatch is set, every resolved address must be public.
async fn ensure_fetch_target_allowed(url: &str, allow_private: bool) -> Result<()> {
    let uri: http::Uri = url
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid fetch URL {url:?}: {e}"))?;
    let scheme = uri.scheme_str().unwrap_or("");
    if scheme != "http" && scheme != "https" {
        anyhow::bail!("unsupported fetch scheme {scheme:?} for {url:?}");
    }
    if allow_private {
        return Ok(());
    }
    let host = uri
        .host()
        .ok_or_else(|| anyhow::anyhow!("fetch URL has no host: {url:?}"))?;
    // `http::Uri::host` keeps IPv6 brackets; strip them before parsing.
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !is_public_ip(&ip) {
            anyhow::bail!("fetch URL {url:?} points to non-public address {ip}");
        }
        return Ok(());
    }
    let port = uri
        .port_u16()
        .unwrap_or(if scheme == "https" { 443 } else { 80 });
    let mut resolved = false;
    for addr in tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| anyhow::anyhow!("failed to resolve {host:?}: {e}"))?
    {
        resolved = true;
        if !is_public_ip(&addr.ip()) {
            anyhow::bail!(
                "fetch URL {url:?} resolves to non-public address {}",
                addr.ip()
            );
        }
    }
    if !resolved {
        anyhow::bail!("fetch URL host {host:?} did not resolve");
    }
    Ok(())
}

/// Read a response body with a hard streaming cap. `Content-Length` is checked
/// first, but the streaming limit is authoritative (a peer can lie or omit it).
async fn read_body_capped(resp: reqwest::Response, max_bytes: usize) -> Result<Vec<u8>> {
    if let Some(len) = resp.content_length() {
        if len > max_bytes as u64 {
            anyhow::bail!("response body too large: {len} bytes > {max_bytes}");
        }
    }
    let mut body = Vec::new();
    let mut resp = resp;
    while let Some(chunk) = resp.chunk().await? {
        if body.len().saturating_add(chunk.len()) > max_bytes {
            anyhow::bail!("response body exceeded {max_bytes} bytes");
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Fetch and parse JSON from a peer URL under the shared SSRF/body policy.
async fn fetch_peer_json<T: serde::de::DeserializeOwned>(
    state: &AppState,
    url: &str,
    accept: &str,
    max_bytes: usize,
) -> Result<T> {
    ensure_fetch_target_allowed(url, state.cfg.allow_private_remote_fetch).await?;
    let resp = state
        .http
        .get(url)
        .header(header::ACCEPT, accept)
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("fetch {url:?} failed with status {status}");
    }
    let body = read_body_capped(resp, max_bytes).await?;
    serde_json::from_slice(&body).map_err(|e| anyhow::anyhow!("invalid JSON from {url:?}: {e}"))
}

/// Fetch an actor document for key resolution, rejecting non-public targets
/// (SSRF, SEC-07) and capping the response size. The legacy
/// `FEDI3_RELAY_ALLOW_PRIVATE_ACTOR_FETCH` flag still applies here so existing
/// deployments keep working; the new global escape hatch also unlocks it.
async fn fetch_actor_document(state: &AppState, url: &str) -> Result<String> {
    let allow_private = state.cfg.allow_private_remote_fetch || state.cfg.allow_private_actor_fetch;
    ensure_fetch_target_allowed(url, allow_private).await?;
    let resp = state
        .http
        .get(url)
        .header(
            "Accept",
            "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"",
        )
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("actor fetch failed with status {}", resp.status());
    }
    let bytes = read_body_capped(resp, MAX_PEER_METADATA_BYTES).await?;
    Ok(String::from_utf8(bytes)?)
}

async fn verify_actor_signature_for_request(
    state: &AppState,
    method: &Method,
    uri: &http::Uri,
    headers: &HeaderMap,
    body: Option<&[u8]>,
) -> Result<(String, String)> {
    let sig = headers
        .get("Signature")
        .or_else(|| headers.get("signature"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if sig.trim().is_empty() {
        anyhow::bail!("missing Signature");
    }
    let params = parse_signature_header(sig)?;
    require_signed_headers(&params, body.is_some())?;

    // Date skew (5 minutes).
    let date = headers
        .get("Date")
        .or_else(|| headers.get("date"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if date.is_empty() {
        anyhow::bail!("missing Date");
    }
    let ts = parse_http_date(date)?;
    let now = std::time::SystemTime::now();
    let diff = if now > ts {
        now.duration_since(ts).unwrap_or_default()
    } else {
        ts.duration_since(now).unwrap_or_default()
    };
    if diff > Duration::from_secs(300) {
        anyhow::bail!("Date skew too large");
    }

    // POST bodies must carry a valid Digest (H-01).
    verify_digest_header(headers, body, body.is_some())?;

    let signing_string = build_signing_string(method, uri, headers, &params.headers)?;

    let actor = actor_from_key_id(&params.key_id)
        .ok_or_else(|| anyhow::anyhow!("invalid keyId {:?}", params.key_id))?;
    let actor_json = fetch_actor_document(state, &actor).await?;
    let pem = extract_public_key_pem_from_actor_json(&actor_json)
        .ok_or_else(|| anyhow::anyhow!("actor missing publicKeyPem"))?;
    if !verify_signature_with_algorithm(
        &pem,
        &signing_string,
        &params.signature,
        params.algorithm.as_deref(),
    ) {
        anyhow::bail!("signature invalid");
    }
    Ok((actor, actor_json))
}

/// Sign an outgoing request with the relay's Ed25519 key and return the
/// headers needed by the receiver (Signature, Date, Digest, Host).
async fn signed_relay_headers(
    state: &AppState,
    method: &str,
    path_and_query: &str,
    host: &str,
    body: &[u8],
) -> Result<Vec<(String, String)>> {
    let (_, sk_b64) = {
        let db = state.db.lock().await;
        db.load_or_create_signing_keypair_b64()?
    };
    let date = httpdate::fmt_http_date(std::time::SystemTime::now());
    let digest = format!("SHA-256={}", B64.encode(Sha256::digest(body)));
    let signing_string = format!(
        "(request-target): {} {}\nhost: {}\ndate: {}\ndigest: {}",
        method.to_ascii_lowercase(),
        path_and_query,
        host,
        date,
        digest
    );
    let sk_bytes = B64.decode(sk_b64.as_bytes())?;
    if sk_bytes.len() != 32 {
        anyhow::bail!("bad relay signing key length");
    }
    let mut sk = [0u8; 32];
    sk.copy_from_slice(&sk_bytes);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&sk);
    let signature: ed25519_dalek::Signature = signing_key.sign(signing_string.as_bytes());

    let public_url = state
        .cfg
        .public_url
        .clone()
        .ok_or_else(|| anyhow::anyhow!("FEDI3_RELAY_PUBLIC_URL required for signed fan-out"))?;
    let key_id = format!(
        "{}/_fedi3/relay/move_notice",
        public_url.trim_end_matches('/')
    );
    let signature_header = format!(
        "keyId=\"{}\",algorithm=\"ed25519\",headers=\"(request-target) host date digest\",signature=\"{}\"",
        key_id,
        B64.encode(signature.to_bytes())
    );
    Ok(vec![
        ("Signature".to_string(), signature_header),
        ("Date".to_string(), date),
        ("Digest".to_string(), digest),
        ("Host".to_string(), host.to_string()),
    ])
}

async fn verify_move_notice_signature(
    state: &AppState,
    headers: &HeaderMap,
    user: &str,
    body: &[u8],
) -> Result<bool> {
    let sig = headers
        .get("Signature")
        .or_else(|| headers.get("signature"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if sig.trim().is_empty() {
        return Ok(false);
    }
    let Ok(params) = parse_signature_header(sig) else {
        return Ok(false);
    };
    if require_signed_headers(&params, true).is_err() {
        return Ok(false);
    }

    // Date skew (5 minutes).
    let date = headers
        .get("Date")
        .or_else(|| headers.get("date"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if date.is_empty() {
        return Ok(false);
    }
    let Ok(ts) = parse_http_date(date) else {
        return Ok(false);
    };
    let now = std::time::SystemTime::now();
    let diff = if now > ts {
        now.duration_since(ts).unwrap_or_default()
    } else {
        ts.duration_since(now).unwrap_or_default()
    };
    if diff > Duration::from_secs(300) {
        return Ok(false);
    }

    if verify_digest_header(headers, Some(body), true).is_err() {
        return Ok(false);
    }

    let uri: http::Uri = "/_fedi3/relay/move_notice".parse()?;
    let Ok(signing_string) = build_signing_string(&Method::POST, &uri, headers, &params.headers)
    else {
        return Ok(false);
    };

    // Fan-out notices are signed by the sending relay with its Ed25519 key; the
    // public key is pinned in relay_registry (H-08 / RN-02).
    let key_path = params.key_id.split('#').next().unwrap_or("").trim();
    if key_path.contains("/_fedi3/relay/") {
        if let Some(origin) = relay_origin_from_key_id(&params.key_id) {
            let pinned = {
                let db = state.db.lock().await;
                db.get_relay_pubkey_b64(&origin).ok().flatten()
            };
            if let Some(pk) = pinned.filter(|v| !v.trim().is_empty()) {
                return Ok(verify_signature_ed25519(
                    &pk,
                    &signing_string,
                    &params.signature,
                ));
            }
        }
    }

    // Legacy/actor-signed notices: verify against the cached actor key.
    let mut pem = {
        let db = state.db.lock().await;
        db.get_actor_cache(user)
            .ok()
            .flatten()
            .and_then(|actor_json| extract_public_key_pem_from_actor_json(&actor_json))
            .unwrap_or_default()
    };
    if pem.trim().is_empty() {
        if let Ok(notice) = serde_json::from_slice::<RelayMoveNotice>(body) {
            if let Some(old_actor) = notice
                .old_actor
                .as_deref()
                .map(str::trim)
                .filter(|v| v.starts_with("http://") || v.starts_with("https://"))
            {
                if let Ok(text) = fetch_actor_document(state, old_actor).await {
                    if let Some(got) = extract_public_key_pem_from_actor_json(&text) {
                        pem = got;
                    }
                }
            }
        }
    }
    if pem.trim().is_empty() {
        return Ok(false);
    }
    Ok(verify_signature_with_algorithm(
        &pem,
        &signing_string,
        &params.signature,
        params.algorithm.as_deref(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
    use std::sync::Mutex as StdMutex;

    static TEST_DIR_ID: AtomicU64 = AtomicU64::new(1);
    static ENV_LOCK: StdMutex<()> = StdMutex::new(());

    fn test_cfg() -> RelayConfig {
        RelayConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            base_domain: None,
            trust_proxy_headers: false,
            allow_self_register: true,
            admin_token: Some("a".repeat(48)),
            public_url: Some("http://127.0.0.1:8787".to_string()),
            telemetry_token: None,
            seed_relays: Vec::new(),
            p2p_infra_peer_id: None,
            p2p_infra_multiaddrs: Vec::new(),
            p2p_infra_host: None,
            p2p_infra_port: 4001,
            telemetry_interval_secs: 30,
            max_body_bytes: 1024 * 1024,
            http_timeout_secs: 5,
            http_connect_timeout_secs: 2,
            http_pool_idle_timeout_secs: 90,
            http_pool_max_idle_per_host: 8,
            hsts_max_age_secs: 31_536_000,
            csp: None,
            tunnel_timeout_secs: 1,
            rate_limit_register_per_min: 10_000,
            rate_limit_tunnel_per_min: 10_000,
            rate_limit_inbox_per_min: 10_000,
            rate_limit_forward_per_min: 10_000,
            rate_limit_admin_per_min: 10_000,
            search_backend: "db".to_string(),
            search_total_mode: SearchTotalMode::Exact,
            search_cache_ttl_secs: 0,
            search_cache_max_entries: 0,
            meili_url: None,
            meili_api_key: None,
            meili_timeout_secs: 10,
            meili_notes_index: "relay_notes".to_string(),
            meili_users_index: "relay_users".to_string(),
            meili_batch_max: 64,
            meili_flush_ms: 250,
            meili_queue_max: 2000,
            db_driver: DbDriver::Sqlite,
            db_url: None,
            db_synchronous: "NORMAL".to_string(),
            db_cache_kb: -65536,
            db_busy_timeout_ms: 2000,
            pg_pool_max_size: 16,
            pg_pool_wait_ms: None,
            pg_pool_create_timeout_ms: None,
            pg_pool_recycle_timeout_ms: None,
            pg_pool_queue_mode: QueueMode::Fifo,
            pg_init_retries: 1,
            pg_init_backoff_ms: 50,
            redis_url: None,
            redis_prefix: "fedi3".to_string(),
            redis_pool_size: 4,
            ip_allowlist: Vec::new(),
            ip_denylist: Vec::new(),
            noisy_backoff_base_secs: 0,
            noisy_backoff_max_secs: 600,
            max_inbox_fanout: 25,
            max_inflight_per_user: 32,
            spool_ttl_secs: 3600,
            move_notice_ttl_secs: 3600,
            move_notice_fanout_interval_secs: 60,
            webrtc_signal_ttl_secs: 600,
            webrtc_signal_cleanup_interval_secs: 30,
            webrtc_signal_max_per_peer: 200,
            spool_max_rows_per_user: 100,
            spool_flush_batch: 50,
            peer_directory_ttl_days: 30,
            media_backend: "local".to_string(),
            media_dir: PathBuf::from("test-media"),
            media_prefix: String::new(),
            media_webdav_base_url: None,
            media_webdav_username: None,
            media_webdav_password: None,
            media_webdav_bearer_token: None,
            media_s3_region: None,
            media_s3_bucket: None,
            media_s3_endpoint: None,
            media_s3_access_key: None,
            media_s3_secret_key: None,
            media_s3_path_style: false,
            outbox_index_interval_secs: 300,
            outbox_index_pages: 5,
            outbox_index_page_limit: 40,
            telemetry_users_limit: 200,
            telemetry_peers_limit: 200,
            relay_sync_interval_secs: 120,
            relay_sync_limit: 200,
            allow_private_actor_fetch: true,
            allow_private_remote_fetch: true,
            forward_send_timeout_ms: 50,
            inbox_dedupe_ttl_secs: 3600,
            admin_audit_ttl_secs: 3600,
            relay_notes_ttl_secs: 3600,
            peer_registry_ttl_secs: 3600,
            relay_registry_ttl_secs: 3600,
            relay_user_directory_ttl_secs: 3600,
            media_max_bytes_per_user: 1024 * 1024,
            spool_max_bytes_per_user: 1024 * 1024,
            peer_sync_secs: 60,
            peer_max_hops: 3,
        }
    }

    fn temp_dir() -> PathBuf {
        let id = TEST_DIR_ID.fetch_add(1, AtomicOrdering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("fedi3-relay-unit-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn test_db() -> (Db, PathBuf) {
        let dir = temp_dir();
        let path = dir.join("relay.db");
        let db = Db {
            driver: DbDriver::Sqlite,
            path: path.clone(),
            db_url: None,
            db_synchronous: "NORMAL".to_string(),
            db_cache_kb: -65536,
            db_busy_timeout_ms: 2000,
            pg_pool_max_size: 16,
            pg_pool_wait_ms: None,
            pg_pool_create_timeout_ms: None,
            pg_pool_recycle_timeout_ms: None,
            pg_pool_queue_mode: QueueMode::Fifo,
            pg_init_retries: 1,
            pg_init_backoff_ms: 50,
            pg_pool: OnceLock::new(),
        };
        db.init().unwrap();
        (db, dir)
    }

    async fn test_state(cfg: RelayConfig) -> (AppState, PathBuf) {
        let dir = temp_dir();
        let mut db = test_db().0;
        db.path = dir.join("state.db");
        db.init().unwrap();
        let media_cfg = media_store::MediaConfig {
            backend: "local".to_string(),
            local_dir: dir.join("media"),
            webdav_base_url: None,
            webdav_username: None,
            webdav_password: None,
            webdav_bearer_token: None,
            s3_region: None,
            s3_bucket: None,
            s3_endpoint: None,
            s3_access_key: None,
            s3_secret_key: None,
            s3_path_style: false,
        };
        let http = reqwest::Client::new();
        let media_backend = media_store::build_media_backend(&media_cfg, http.clone())
            .await
            .unwrap();
        let limiter = Arc::new(RateLimiter::new(0, 600, None, "fedi3".to_string(), 1).await);
        let state = AppState {
            tunnels: Arc::new(RwLock::new(HashMap::new())),
            inflight_per_user: Arc::new(RwLock::new(HashMap::new())),
            peer_hello: Arc::new(RwLock::new(HashMap::new())),
            cfg,
            db: Arc::new(Mutex::new(db)),
            limiter,
            http,
            search: None,
            meili_indexer: None,
            search_cache: None,
            media_cfg,
            media_backend: Arc::from(media_backend),
        };
        (state, dir)
    }

    fn hmap(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (k, v) in pairs {
            headers.insert(
                HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        headers
    }

    #[test]
    fn ct_eq_is_exact() {
        assert!(ct_eq_str("abcdef", "abcdef"));
        assert!(!ct_eq_str("abcdef", "abcdeg"));
        assert!(!ct_eq_str("abc", "abcd"));
        assert!(ct_eq_bytes(b"", b""));
    }

    #[test]
    fn token_matches_rejects_empty_and_prefixes() {
        assert!(token_matches(
            "secret-token-value",
            Some("secret-token-value")
        ));
        assert!(!token_matches(
            "secret-token-value",
            Some("secret-token-valu")
        ));
        assert!(!token_matches(
            "secret-token-value",
            Some("secret-token-valueX")
        ));
        assert!(!token_matches("secret-token-value", Some("")));
        assert!(!token_matches("secret-token-value", None));
    }

    #[test]
    fn bearer_token_parses_and_rejects_empty() {
        assert_eq!(
            bearer_token(&hmap(&[("Authorization", "Bearer abc123")])),
            Some("abc123".to_string())
        );
        assert_eq!(
            bearer_token(&hmap(&[("Authorization", "bearer abc123")])),
            Some("abc123".to_string())
        );
        assert_eq!(bearer_token(&hmap(&[("Authorization", "Bearer ")])), None);
        assert_eq!(bearer_token(&HeaderMap::new()), None);
    }

    #[test]
    fn admin_authorization_cases() {
        let cfg = test_cfg();
        let token = cfg.admin_token.clone().unwrap();
        assert!(is_authorized_admin(
            &cfg,
            &hmap(&[("Authorization", &format!("Bearer {token}"))])
        ));
        assert!(!is_authorized_admin(
            &cfg,
            &hmap(&[("Authorization", "Bearer wrong")])
        ));
        // Correct prefix, wrong suffix must fail.
        let wrong = format!("{}x", &token[..token.len() - 1]);
        assert!(!is_authorized_admin(
            &cfg,
            &hmap(&[("Authorization", &format!("Bearer {wrong}"))])
        ));
        assert!(!is_authorized_admin(&cfg, &HeaderMap::new()));
        let mut empty_cfg = test_cfg();
        empty_cfg.admin_token = None;
        assert!(!is_authorized_admin(&empty_cfg, &HeaderMap::new()));
    }

    #[test]
    fn token_hash_is_sha256_hex() {
        // SHA-256("abc")
        assert_eq!(
            token_hash_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn env_validation_rejects_invalid_values() {
        let _guard = ENV_LOCK.lock().unwrap();
        let names = [
            "FEDI3_RELAY_MAX_BODY_BYTES",
            "FEDI3_RELAY_DB_DRIVER",
            "FEDI3_RELAY_SEARCH_BACKEND",
        ];
        for name in names {
            std::env::remove_var(name);
        }
        std::env::set_var("FEDI3_RELAY_MAX_BODY_BYTES", "64M");
        let err = validate_env_parsing().unwrap_err();
        assert!(format!("{err}").contains("MAX_BODY_BYTES"));

        std::env::set_var("FEDI3_RELAY_MAX_BODY_BYTES", "67108864");
        std::env::set_var("FEDI3_RELAY_RL_INBOX_PER_MIN", "not-a-number");
        assert!(validate_env_parsing().is_err());

        std::env::remove_var("FEDI3_RELAY_RL_INBOX_PER_MIN");
        assert!(validate_env_parsing().is_ok());
        for name in names {
            std::env::remove_var(name);
        }
    }

    #[test]
    fn client_ip_prefers_last_forwarded_element() {
        let mut cfg = test_cfg();
        cfg.trust_proxy_headers = true;
        let peer: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let headers = hmap(&[("X-Forwarded-For", "6.6.6.6, 203.0.113.9")]);
        assert_eq!(client_ip(&cfg, &peer, &headers), "203.0.113.9");
        let headers = hmap(&[("X-Forwarded-For", "6.6.6.6")]);
        assert_eq!(client_ip(&cfg, &peer, &headers), "6.6.6.6");
        let headers = hmap(&[("X-Real-IP", "198.51.100.4")]);
        assert_eq!(client_ip(&cfg, &peer, &headers), "198.51.100.4");
    }

    #[test]
    fn client_ip_ignores_forwarding_headers_when_untrusted() {
        let cfg = test_cfg();
        let peer: SocketAddr = "192.0.2.10:5000".parse().unwrap();
        let headers = hmap(&[("X-Forwarded-For", "6.6.6.6")]);
        assert_eq!(client_ip(&cfg, &peer, &headers), "192.0.2.10");
    }

    #[test]
    fn parse_ip_handles_ports_and_ipv6() {
        assert_eq!(parse_ip_str("203.0.113.7"), Some("203.0.113.7".to_string()));
        assert_eq!(
            parse_ip_str("203.0.113.7:443"),
            Some("203.0.113.7".to_string())
        );
        assert_eq!(parse_ip_str("[::1]:443"), Some("::1".to_string()));
        assert_eq!(parse_ip_str("::1"), Some("::1".to_string()));
        assert_eq!(parse_ip_str("not-an-ip"), None);
    }

    #[test]
    fn cidr_rules_and_allowlist() {
        assert!(ip_in_cidr(
            "10.1.2.3".parse().unwrap(),
            "10.0.0.0".parse().unwrap(),
            8
        ));
        assert!(!ip_in_cidr(
            "11.0.0.1".parse().unwrap(),
            "10.0.0.0".parse().unwrap(),
            8
        ));
        assert!(ip_in_cidr(
            "1.2.3.4".parse().unwrap(),
            "0.0.0.0".parse().unwrap(),
            0
        ));
        let rule = parse_ip_rule("192.0.2.1/32").unwrap();
        assert!(ip_in_rules(&[rule], "192.0.2.1".parse().unwrap()));
        let mut cfg = test_cfg();
        cfg.ip_denylist = vec![parse_ip_rule("10.0.0.0/8").unwrap()];
        assert!(!is_ip_allowed(&cfg, "10.9.9.9".parse().unwrap()));
        assert!(is_ip_allowed(&cfg, "8.8.8.8".parse().unwrap()));
        cfg.ip_allowlist = vec![parse_ip_rule("8.8.8.8").unwrap()];
        assert!(!is_ip_allowed(&cfg, "1.1.1.1".parse().unwrap()));
    }

    #[tokio::test]
    async fn rate_limiter_memory_fallback() {
        let limiter = RateLimiter::new(0, 600, None, "fedi3".to_string(), 1).await;
        assert!(limiter.check("1.1.1.1".to_string(), "b", 2).await);
        assert!(limiter.check("1.1.1.1".to_string(), "b", 2).await);
        assert!(!limiter.check("1.1.1.1".to_string(), "b", 2).await);
        // Separate bucket is independent.
        assert!(limiter.check("1.1.1.1".to_string(), "other", 2).await);
        // Weighted requests count against the limit.
        assert!(
            !limiter
                .check_weighted("2.2.2.2".to_string(), "w", 3, 5)
                .await
        );
    }

    #[tokio::test]
    async fn rate_limiter_noisy_backoff() {
        let limiter = RateLimiter::new(1, 10, None, "fedi3".to_string(), 1).await;
        assert!(limiter.noisy_block_remaining("9.9.9.9").await.is_none());
        limiter.register_noisy("9.9.9.9", now_ms()).await;
        assert!(limiter.noisy_block_remaining("9.9.9.9").await.is_some());
        assert!(!limiter.check("9.9.9.9".to_string(), "b", 100).await);
    }

    #[test]
    fn normalized_origin_handles_default_ports() {
        assert_eq!(
            normalized_origin("http", "localhost:80"),
            normalized_origin("http", "localhost")
        );
        assert_eq!(
            normalized_origin("https", "relay.example:443"),
            normalized_origin("https", "relay.example")
        );
        assert_ne!(
            normalized_origin("http", "localhost:8787"),
            normalized_origin("http", "localhost")
        );
    }

    #[test]
    fn host_allowed_for_base_domain() {
        let mut cfg = test_cfg();
        cfg.base_domain = Some("relay.example".to_string());
        assert!(host_allowed_for_cfg(&cfg, "alice.relay.example"));
        assert!(host_allowed_for_cfg(&cfg, "relay.example"));
        assert!(!host_allowed_for_cfg(&cfg, "evil.example"));
        assert!(!host_allowed_for_cfg(&cfg, "alice.relay.example.evil.com"));
        let mut no_base = test_cfg();
        no_base.base_domain = None;
        assert!(host_allowed_for_cfg(&no_base, "anything.example"));
    }

    #[test]
    fn canonical_redirect_with_explicit_port() {
        let cfg = test_cfg();
        let headers = hmap(&[("Host", "127.0.0.1:8787")]);
        assert!(maybe_redirect_canonical(&cfg, &headers, &Method::GET, "/users/u", None).is_none());

        let headers = hmap(&[("Host", "127.0.0.1")]);
        let resp = maybe_redirect_canonical(&cfg, &headers, &Method::GET, "/users/u", Some("a=1"))
            .expect("redirect expected");
        assert_eq!(resp.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(
            resp.headers().get("Location").unwrap().to_str().unwrap(),
            "http://127.0.0.1:8787/users/u?a=1"
        );
        // POST must never redirect (signature would break).
        assert!(maybe_redirect_canonical(&cfg, &headers, &Method::POST, "/inbox", None).is_none());
    }

    #[test]
    fn signature_header_quoted_parsing() {
        let raw = r#"keyId="https://relay.example/users/a#k",algorithm="ed25519",headers="(request-target) host date digest",signature="c2ln""#;
        let params = parse_signature_header(raw).unwrap();
        assert_eq!(params.key_id, "https://relay.example/users/a#k");
        assert_eq!(params.algorithm.as_deref(), Some("ed25519"));
        assert_eq!(
            params.headers,
            vec!["(request-target)", "host", "date", "digest"]
        );
        assert_eq!(params.signature, b"sig");
    }

    #[test]
    fn signature_header_case_insensitive_and_quoted_commas() {
        let raw = r#"KEYID="https://x.example/users/a#k",Headers="(request-target) host date",Signature="c2ln",Algorithm="HS2019""#;
        let params = parse_signature_header(raw).unwrap();
        assert_eq!(params.algorithm.as_deref(), Some("hs2019"));
        assert_eq!(params.signature, b"sig");
        assert!(parse_signature_header(r#"keyId="x""#).is_err());
        assert!(parse_signature_header(r#"headers="date",signature="c2ln""#).is_err());
    }

    #[test]
    fn signed_header_minimum_set_is_enforced() {
        let params = parse_signature_header(
            r#"keyId="https://x/users/a#k",headers="(request-target) host date",signature="c2ln""#,
        )
        .unwrap();
        assert!(require_signed_headers(&params, false).is_ok());
        assert!(require_signed_headers(&params, true).is_err());
    }

    #[test]
    fn signing_string_includes_request_target() {
        let headers = hmap(&[
            ("Host", "relay.example"),
            ("Date", "Tue, 01 Jan 2030 00:00:00 GMT"),
        ]);
        let uri: http::Uri = "/inbox?x=1".parse().unwrap();
        let signed = build_signing_string(
            &Method::POST,
            &uri,
            &headers,
            &[
                "(request-target)".to_string(),
                "host".to_string(),
                "date".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(
            signed,
            "(request-target): post /inbox?x=1\nhost: relay.example\ndate: Tue, 01 Jan 2030 00:00:00 GMT"
        );
        assert!(build_signing_string(
            &Method::POST,
            &uri,
            &headers,
            &["host".to_string(), "missing".to_string()]
        )
        .is_err());
    }

    #[test]
    fn digest_verification() {
        let body = b"hello world";
        let digest = format!("SHA-256={}", B64.encode(Sha256::digest(body)));
        let headers = hmap(&[("Digest", &digest)]);
        assert!(verify_digest_header(&headers, Some(body), true).is_ok());
        // Multi-algorithm header: SHA-256 entry is accepted.
        let multi = format!("SHA-512=abc,{}", digest);
        let headers = hmap(&[("Digest", &multi)]);
        assert!(verify_digest_header(&headers, Some(body), true).is_ok());
        // Wrong digest.
        let headers = hmap(&[("Digest", "SHA-256=AAAA")]);
        assert!(verify_digest_header(&headers, Some(body), true).is_err());
        // Missing digest on POST.
        assert!(verify_digest_header(&HeaderMap::new(), Some(body), true).is_err());
        // Unsupported algorithm only.
        let headers = hmap(&[("Digest", "SHA-512=abc")]);
        assert!(verify_digest_header(&headers, Some(body), true).is_err());
    }

    #[test]
    fn ed25519_signature_roundtrip() {
        use rand::rngs::OsRng;
        let signing = ed25519_dalek::SigningKey::generate(&mut OsRng);
        let verifying = signing.verifying_key();
        let message = b"relay-signed-request";
        let sig = signing.sign(message);
        let raw_pk = B64.encode(verifying.to_bytes());
        assert!(verify_signature_ed25519(
            &raw_pk,
            "relay-signed-request",
            &sig.to_bytes()
        ));
        assert!(!verify_signature_ed25519(
            &raw_pk,
            "relay-signed-requesT",
            &sig.to_bytes()
        ));
        assert!(verify_signature_with_algorithm(
            &raw_pk,
            "relay-signed-request",
            &sig.to_bytes(),
            Some("ed25519")
        ));
    }

    #[test]
    fn rsa_signature_roundtrip() {
        use rand::rngs::OsRng;
        use rsa::pkcs1v15::SigningKey;
        use rsa::pkcs8::{EncodePublicKey, LineEnding};
        use rsa::signature::{SignatureEncoding, Signer};
        let private = rsa::RsaPrivateKey::new(&mut OsRng, 2048).unwrap();
        let public = private.to_public_key();
        let pem = public.to_public_key_pem(LineEnding::LF).unwrap();
        let signing = SigningKey::<Sha256>::new(private);
        let message = "signed-string";
        let sig = signing.sign(message.as_bytes());
        assert!(verify_signature_rsa_sha256(&pem, message, &sig.to_bytes()));
        assert!(!verify_signature_rsa_sha256(
            &pem,
            "tampered",
            &sig.to_bytes()
        ));
        assert!(verify_signature_with_algorithm(
            &pem,
            message,
            &sig.to_bytes(),
            Some("rsa-sha256")
        ));
    }

    #[test]
    fn escape_like_and_strip_html() {
        assert_eq!(escape_like("100%_x\\y"), "100\\%\\_x\\\\y");
        assert_eq!(strip_html("<b>hello</b> world"), "hello world");
        assert_eq!(strip_html("no tags"), "no tags");
    }

    #[test]
    fn activity_id_and_username_validation() {
        let body = Bytes::from(r#"{"id":"https://x.example/activities/1","type":"Create"}"#);
        assert_eq!(
            extract_activity_id(&body),
            Some("https://x.example/activities/1".to_string())
        );
        assert_eq!(extract_activity_id(&Bytes::from("{}")), None);
        assert!(is_valid_username("alice_01-x"));
        assert!(!is_valid_username("Alice"));
        assert!(!is_valid_username(""));
        assert!(!is_valid_username("a.b"));
    }

    #[test]
    fn media_type_and_extension_policy() {
        assert!(media_type_allowed("image/png"));
        assert!(media_type_allowed("video/mp4; codecs=avc1"));
        assert!(media_type_allowed("application/pdf"));
        assert!(media_type_allowed("application/octet-stream"));
        assert!(!media_type_allowed("text/html"));
        assert_eq!(safe_extension("PNG"), "png");
        assert_eq!(safe_extension("tar.gz"), "bin");
        assert_eq!(safe_extension("#weird?"), "bin");
    }

    #[test]
    fn sanitize_key_rejects_traversal() {
        assert_eq!(media_store::sanitize_key("../etc/passwd"), "etc/passwd");
        assert_eq!(media_store::sanitize_key("a//b/./c"), "a/b/c");
        assert_eq!(
            media_store::sanitize_key("/prefix/alice/x.png"),
            "prefix/alice/x.png"
        );
    }

    #[test]
    fn sqlite_schema_migration_is_idempotent() {
        let (db, _dir) = test_db();
        db.init().unwrap();
        let conn = db.open_sqlite_conn().unwrap();
        let version: i64 = conn
            .query_row("SELECT version FROM schema_version WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        // A newer database must be rejected explicitly (RD-03).
        conn.execute(
            "UPDATE schema_version SET version = ?1 WHERE id = 1",
            params![SCHEMA_VERSION + 1],
        )
        .unwrap();
        let err = db.init().unwrap_err();
        assert!(format!("{err}").contains("newer"));
    }

    #[test]
    fn inbox_dedupe_prevents_double_delivery() {
        let (db, _dir) = test_db();
        assert!(db.insert_inbox_dedupe("act-1", "alice").unwrap());
        assert!(!db.insert_inbox_dedupe("act-1", "alice").unwrap());
        assert!(db.insert_inbox_dedupe("act-1", "bob").unwrap());
        db.delete_inbox_dedupe("act-1", "alice").unwrap();
        assert!(db.insert_inbox_dedupe("act-1", "alice").unwrap());
    }

    #[test]
    fn spool_byte_cap_drops_oldest() {
        let (db, _dir) = test_db();
        let mut cfg = test_cfg();
        cfg.spool_max_bytes_per_user = 10;
        cfg.spool_max_rows_per_user = 100;
        db.enqueue_spool(&cfg, "alice", "POST", "/inbox", "", &[], "b64", 8)
            .unwrap();
        db.enqueue_spool(&cfg, "alice", "POST", "/inbox", "", &[], "b64", 8)
            .unwrap();
        let items = db.list_spool("alice", 10).unwrap();
        assert_eq!(items.len(), 1);
        // A body larger than the whole cap is rejected instead of evicting all.
        assert!(db
            .enqueue_spool(&cfg, "alice", "POST", "/inbox", "", &[], "b64", 11)
            .is_err());
    }

    #[test]
    fn retention_cleanup_removes_old_rows() {
        let (db, _dir) = test_db();
        let old = now_ms() - 10_000;
        {
            let conn = db.open_sqlite_conn().unwrap();
            conn.execute(
                "INSERT INTO admin_audit(action, ok, created_at_ms) VALUES ('old', 1, ?1)",
                params![old],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO peer_registry(peer_id, last_seen_ms) VALUES ('old-peer', ?1)",
                params![old],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO relay_registry(relay_url, last_seen_ms) VALUES ('https://old.example', ?1)",
                params![old],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO relay_user_directory(actor_url, username, relay_url, updated_at_ms) VALUES ('https://old.example/users/a','a','https://old.example', ?1)",
                params![old],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO inbox_dedupe(activity_id, recipient, created_at_ms) VALUES ('act','a', ?1)",
                params![old],
            )
            .unwrap();
        }
        let note = RelayNoteIndex {
            note_id: "note-old".to_string(),
            actor_id: None,
            published_ms: None,
            content_text: "old".to_string(),
            content_html: "old".to_string(),
            note_json: "{}".to_string(),
            created_at_ms: old,
            tags: Vec::new(),
        };
        db.upsert_relay_note(&note).unwrap();

        assert_eq!(db.cleanup_admin_audit(1).unwrap(), 1);
        assert_eq!(db.cleanup_peer_registry(1).unwrap(), 1);
        assert_eq!(db.cleanup_relay_registry(1).unwrap(), 1);
        assert_eq!(db.cleanup_relay_user_directory(1).unwrap(), 1);
        assert_eq!(db.cleanup_inbox_dedupe(1).unwrap(), 1);
        assert_eq!(db.cleanup_relay_notes(1).unwrap(), 1);

        let conn = db.open_sqlite_conn().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM relay_notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn relay_search_uses_escape_and_ignores_html() {
        let (db, _dir) = test_db();
        for (id, text, html) in [
            ("n1", "100% off", "<p>100% off</p>"),
            ("n2", "foo_bar", "<p>foo_bar</p>"),
            ("n3", "fooXbar", "<p>fooXbar</p>"),
            ("n4", "plain text", "<b>marker</b>"),
        ] {
            let note = RelayNoteIndex {
                note_id: id.to_string(),
                actor_id: Some("https://x.example/users/a".to_string()),
                published_ms: None,
                content_text: text.to_string(),
                content_html: html.to_string(),
                note_json: format!(r#"{{"id":"{id}","type":"Note"}}"#),
                created_at_ms: now_ms(),
                tags: Vec::new(),
            };
            db.upsert_relay_note(&note).unwrap();
        }
        let page = db
            .search_relay_notes("100%", "", 10, None, None, SearchTotalMode::Exact)
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert!(page.items[0].contains("n1"));

        let page = db
            .search_relay_notes("foo_bar", "", 10, None, None, SearchTotalMode::Exact)
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert!(page.items[0].contains("n2"));

        // content_html is no longer part of the search (M-01).
        let page = db
            .search_relay_notes("marker", "", 10, None, None, SearchTotalMode::Exact)
            .unwrap();
        assert_eq!(page.items.len(), 0);
    }

    #[tokio::test]
    async fn forward_timeout_spools_and_clears_inflight() {
        let cfg = test_cfg();
        let (state, _dir) = test_state(cfg).await;
        {
            let mut db = state.db.lock().await;
            db.create_user("alice", &"t".repeat(32)).unwrap();
        }
        let (tx, _rx) = mpsc::channel::<TunnelRequest>(4);
        let inflight = Arc::new(RwLock::new(HashMap::new()));
        state.tunnels.write().await.insert(
            "alice".to_string(),
            TunnelHandle {
                tx,
                conn_id: 1,
                inflight: inflight.clone(),
            },
        );
        let resp = forward_to_user(
            state.clone(),
            "alice".to_string(),
            Method::POST,
            "/users/alice/outbox",
            String::new(),
            HeaderMap::new(),
            Bytes::from_static(b"{}"),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::GATEWAY_TIMEOUT);
        assert!(inflight.read().await.is_empty());
        let db = state.db.lock().await;
        assert_eq!(db.list_spool("alice", 10).unwrap().len(), 1);
    }

    #[test]
    fn public_ip_classification_rejects_ssrf_targets() {
        let public = ["8.8.8.8", "1.1.1.1", "2001:4860::1", "2606:4700::1111"];
        let non_public = [
            "127.0.0.1",
            "127.0.0.2",
            "::1",
            "0.0.0.0",
            "::",
            "10.0.0.1",
            "10.255.255.254",
            "172.16.0.1",
            "172.31.255.254",
            "192.168.0.1",
            "192.168.255.254",
            "169.254.169.254",
            "100.64.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "fc00::1",
            "fd12:3456:789a::1",
            "fe80::1",
            "::ffff:127.0.0.1",
        ];
        for raw in public {
            let ip: IpAddr = raw.parse().unwrap();
            assert!(is_public_ip(&ip), "{raw} should be public");
        }
        for raw in non_public {
            let ip: IpAddr = raw.parse().unwrap();
            assert!(!is_public_ip(&ip), "{raw} should be rejected");
        }
    }

    #[tokio::test]
    async fn fetch_policy_rejects_non_http_and_private_literals() {
        assert!(ensure_fetch_target_allowed("ftp://example.com/x", false)
            .await
            .is_err());
        assert!(
            ensure_fetch_target_allowed("http://127.0.0.1:5432/x", false)
                .await
                .is_err()
        );
        assert!(ensure_fetch_target_allowed("http://[::1]:8080/x", false)
            .await
            .is_err());
        // The escape hatch bypasses only the address check, never the scheme.
        assert!(ensure_fetch_target_allowed("http://127.0.0.1:5432/x", true)
            .await
            .is_ok());
        assert!(ensure_fetch_target_allowed("ftp://127.0.0.1/x", true)
            .await
            .is_err());
    }

    #[test]
    fn postgres_search_tsv_migration_is_idempotent_and_complete() {
        // Live Postgres is unavailable in this environment (no Docker), so the
        // migration statements themselves are asserted here.
        assert!(
            PG_SEARCH_TSV_ADD_COLUMN_SQL.contains("ADD COLUMN IF NOT EXISTS search_tsv tsvector")
        );
        assert!(PG_SEARCH_TSV_ADD_COLUMN_SQL.contains("GENERATED ALWAYS AS"));
        assert!(PG_SEARCH_TSV_ADD_COLUMN_SQL.contains("to_tsvector('simple'"));
        assert!(PG_SEARCH_TSV_BACKFILL_SQL.contains("UPDATE relay_notes"));
        assert!(PG_SEARCH_TSV_BACKFILL_SQL.contains("IS DISTINCT FROM"));
        assert!(PG_SEARCH_TSV_INDEX_SQL.contains("CREATE INDEX IF NOT EXISTS"));
        assert!(PG_SEARCH_TSV_INDEX_SQL.contains("USING GIN (search_tsv)"));
        assert!(PG_SEARCH_TSV_TRIGGER_SQL.contains("CREATE OR REPLACE FUNCTION"));
        assert!(PG_SEARCH_TSV_TRIGGER_SQL.contains("DROP TRIGGER IF EXISTS"));
        assert!(PG_SEARCH_TSV_TRIGGER_SQL.contains("CREATE TRIGGER relay_notes_search_tsv_tr"));
        assert!(PG_SEARCH_TSV_TRIGGER_SQL.contains("BEFORE INSERT OR UPDATE OF content_text"));

        assert!(PG_TAG_TSV_ADD_COLUMN_SQL.contains("ADD COLUMN IF NOT EXISTS tag_tsv tsvector"));
        assert!(PG_TAG_TSV_BACKFILL_SQL.contains("UPDATE relay_note_tags"));
        assert!(PG_TAG_TSV_INDEX_SQL.contains("CREATE INDEX IF NOT EXISTS"));
        assert!(PG_TAG_TSV_INDEX_SQL.contains("USING GIN (tag_tsv)"));
        assert!(PG_TAG_TSV_TRIGGER_SQL.contains("CREATE OR REPLACE FUNCTION"));
        assert!(PG_TAG_TSV_TRIGGER_SQL.contains("DROP TRIGGER IF EXISTS"));
        assert!(PG_TAG_TSV_TRIGGER_SQL.contains("CREATE TRIGGER relay_note_tags_tag_tsv_tr"));

        // Fresh installs must get the same DDL from the shipped schema.
        let schema = include_str!("../sql/postgres_schema.sql");
        assert!(schema.contains("ADD COLUMN IF NOT EXISTS search_tsv tsvector"));
        assert!(schema.contains("ADD COLUMN IF NOT EXISTS tag_tsv tsvector"));
        assert!(schema.contains("idx_relay_notes_search_tsv"));
        assert!(schema.contains("idx_relay_note_tags_tsv"));
        assert!(schema.contains("relay_notes_search_tsv_sync"));
        assert!(schema.contains("relay_note_tags_tag_tsv_sync"));
        assert!(schema.contains("CREATE TRIGGER relay_notes_search_tsv_tr"));
        assert!(schema.contains("CREATE TRIGGER relay_note_tags_tag_tsv_tr"));
    }
}
