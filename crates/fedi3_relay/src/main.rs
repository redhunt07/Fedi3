/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, OriginalUri, Path, Query, RawQuery, State,
    },
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    middleware::{from_fn, from_fn_with_state, Next},
    response::{IntoResponse, Response},
    routing::{any, get, post},
    Router,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use bytes::Bytes;
use fedi3_protocol::{RelayHttpRequest, RelayHttpResponse};
use futures_util::{SinkExt, StreamExt};
use anyhow::Result;
use httpdate::parse_http_date;
use http::header;
use deadpool_postgres::{ManagerConfig, Pool, PoolConfig, RecyclingMethod, Runtime, Timeouts};
use deadpool::managed::QueueMode;
use std::future::Future;
use std::sync::OnceLock;
use tokio_postgres::types::ToSql;
use tokio_postgres::{NoTls, Row};
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::Path as FsPath,
    path::PathBuf,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
    sync::Arc,
    time::Duration,
};
use std::net::IpAddr;
use tokio::sync::Mutex;
use tokio::sync::RwLock as TokioRwLock;
use tokio::sync::{mpsc, oneshot, RwLock, Semaphore};
use tokio_util::sync::CancellationToken;
use tower_http::trace::TraceLayer;
use tracing::{error, info, info_span, warn};

use rusqlite::{params, Connection, OptionalExtension};
use ed25519_dalek::{Signer as _, Verifier as _};

mod media_store;

static REQ_ID: AtomicU64 = AtomicU64::new(1);
const DB_BATCH_DELETE_MAX: usize = 500;

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
        let resp = self.req(
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
        let resp = self.req(
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
        let filter = if filters.is_empty() { None } else { Some(filters.join(" AND ")) };
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
        let filter = if filters.is_empty() { None } else { Some(filters.join(" AND ")) };
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
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
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

    let cleanup_state = state.clone();
    let spool_ttl_secs = cleanup_state.cfg.spool_ttl_secs;
    let webrtc_ttl_secs = cleanup_state.cfg.webrtc_signal_ttl_secs;
    let peer_directory_ttl_days = cleanup_state.cfg.peer_directory_ttl_days;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(
            cleanup_state.cfg.webrtc_signal_cleanup_interval_secs.max(10),
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
        .route("/admin/users/:user", get(admin_get_user).delete(admin_delete_user))
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
        .route("/_fedi3/relay/move/:user", axum::routing::delete(relay_move_delete))
        .route("/_fedi3/relay/move_notice", post(relay_move_notice_post))
        .route("/_fedi3/webrtc/send", post(webrtc_signal_send))
        .route("/_fedi3/webrtc/poll", get(webrtc_signal_poll))
        .route("/_fedi3/webrtc/ack", post(webrtc_signal_ack))
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
        let mut interval = tokio::time::interval(Duration::from_secs(sync_state.cfg.telemetry_interval_secs.max(10)));
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
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}

fn load_config() -> RelayConfig {
    let bind = std::env::var("FEDI3_RELAY_BIND").unwrap_or_else(|_| "0.0.0.0:8787".to_string());
    let bind: SocketAddr = bind.parse().expect("FEDI3_RELAY_BIND invalid");
    let base_domain = std::env::var("FEDI3_RELAY_BASE_DOMAIN").ok().map(normalize_host);
    let trust_proxy_headers = std::env::var("FEDI3_RELAY_TRUST_PROXY_HEADERS")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let allow_self_register = std::env::var("FEDI3_RELAY_ALLOW_SELF_REGISTER")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let admin_token = std::env::var("FEDI3_RELAY_ADMIN_TOKEN").ok();
    let public_url = std::env::var("FEDI3_RELAY_PUBLIC_URL").ok().map(|s| s.trim_end_matches('/').to_string());
    let telemetry_token = std::env::var("FEDI3_RELAY_TELEMETRY_TOKEN").ok();
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
                    if t.is_empty() { None } else { Some(t) }
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
        .unwrap_or(64 * 1024 * 1024);
    let hsts_max_age_secs = std::env::var("FEDI3_RELAY_HSTS_MAX_AGE_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let csp = std::env::var("FEDI3_RELAY_CSP").ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
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
        .unwrap_or(7 * 24 * 60 * 60);
    let move_notice_ttl_secs = std::env::var("FEDI3_RELAY_MOVE_NOTICE_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(7 * 24 * 60 * 60);
    let move_notice_fanout_interval_secs = std::env::var("FEDI3_RELAY_MOVE_NOTICE_FANOUT_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60);
    let peer_directory_ttl_days = std::env::var("FEDI3_RELAY_PEER_DIRECTORY_TTL_DAYS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(30)
        .min(3650);

    let webrtc_signal_ttl_secs = std::env::var("FEDI3_RELAY_WEBRTC_SIGNAL_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10 * 60);
    let webrtc_signal_cleanup_interval_secs = std::env::var("FEDI3_RELAY_WEBRTC_SIGNAL_CLEANUP_INTERVAL_SECS")
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
    let media_backend = std::env::var("FEDI3_RELAY_MEDIA_BACKEND").unwrap_or_else(|_| "local".to_string());
    let media_dir = std::env::var("FEDI3_RELAY_MEDIA_DIR").unwrap_or_else(|_| "fedi3_relay_media".to_string());
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
    RelayConfig {
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

async fn handle_tunnel(state: AppState, peer: SocketAddr, user: String, token: Option<String>, socket: WebSocket) {
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
        .check(peer_ip(&peer), "tunnel", state.cfg.rate_limit_tunnel_per_min)
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

    state.tunnels.write().await.insert(user.clone(), TunnelHandle { tx });

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
            hello_state.peer_hello.write().await.insert(hello_user, hello);
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
            inflight_writer.write().await.insert(id.clone(), msg.resp_tx);
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
    info!(%user, "tunnel disconnected");
}

async fn webfinger(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(q): Query<WebfingerQuery>,
) -> impl IntoResponse {
    if let Some(resp) = maybe_redirect_canonical(&state.cfg, &headers, &Method::GET, "/.well-known/webfinger", None) {
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
    if matches!(result, Ok(UpsertUserResult::Created | UpsertUserResult::Updated)) {
        let actor_url = format!("{}/users/{}", relay_self_base(&state.cfg), req.username);
        let stub = actor_stub_from_actor_url(&req.username, &actor_url, &user_base_template(&state.cfg));
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
        Ok(UpsertUserResult::Unauthorized) => (StatusCode::UNAUTHORIZED, "invalid token").into_response(),
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
        .check_weighted(client_ip(&state.cfg, &peer, &headers), "media_upload", state.cfg.rate_limit_inbox_per_min, 1)
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
    let prefix = state
        .cfg
        .media_prefix
        .trim()
        .trim_matches('/')
        .to_string();
    let prefix = if prefix.is_empty() { String::new() } else { format!("{}/", prefix) };
    let storage_key = media_store::sanitize_key(&format!("{prefix}{user}/{id}"));
    let saved = match state.media_backend.save_upload(&storage_key, media_type, &bytes).await {
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
    (StatusCode::CREATED, [(http::header::CONTENT_TYPE, "application/json; charset=utf-8")], body.to_string()).into_response()
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
            headers_out.insert(http::header::CONTENT_TYPE, HeaderValue::from_str(&item.media_type).unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")));
            headers_out.insert(http::header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=31536000, immutable"));
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
        return (StatusCode::SERVICE_UNAVAILABLE, format!("media not ready: {e}")).into_response();
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
            HeaderValue::from_str(&correlation).unwrap_or_else(|_| HeaderValue::from_static("corr")),
        );
    }
    headers.entry("X-Content-Type-Options").or_insert(HeaderValue::from_static("nosniff"));
    headers.entry("X-Frame-Options").or_insert(HeaderValue::from_static("DENY"));
    headers.entry("Referrer-Policy").or_insert(HeaderValue::from_static("no-referrer"));
    headers
        .entry("Permissions-Policy")
        .or_insert(HeaderValue::from_static("geolocation=(), microphone=(), camera=()"));
    if state.cfg.hsts_max_age_secs > 0 {
        let value = format!("max-age={}; includeSubDomains; preload", state.cfg.hsts_max_age_secs);
        headers.insert(
            "Strict-Transport-Security",
            HeaderValue::from_str(&value).unwrap_or_else(|_| HeaderValue::from_static("max-age=0")),
        );
    }
    if let Some(csp) = &state.cfg.csp {
        headers.insert(
            "Content-Security-Policy",
            HeaderValue::from_str(csp).unwrap_or_else(|_| HeaderValue::from_static("default-src 'none'")),
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
            HeaderValue::from_str(&retry_secs.to_string()).unwrap_or_else(|_| HeaderValue::from_static("60")),
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
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("telemetry error: {e}")).into_response(),
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
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("telemetry error: {e}")).into_response(),
    };
    let mut out = String::new();
    out.push_str("# TYPE fedi3_relay_online_users gauge\n");
    out.push_str(&format!("fedi3_relay_online_users {}\n", telemetry.online_users));
    out.push_str("# TYPE fedi3_relay_online_peers gauge\n");
    out.push_str(&format!("fedi3_relay_online_peers {}\n", telemetry.online_peers));
    out.push_str("# TYPE fedi3_relay_total_users gauge\n");
    out.push_str(&format!("fedi3_relay_total_users {}\n", telemetry.total_users));
    out.push_str("# TYPE fedi3_relay_total_peers_seen gauge\n");
    out.push_str(&format!("fedi3_relay_total_peers_seen {}\n", telemetry.total_peers_seen));
    out.push_str("# TYPE fedi3_relay_relays_total gauge\n");
    out.push_str(&format!("fedi3_relay_relays_total {}\n", telemetry.relays.len()));
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
    if let Some(resp) = maybe_redirect_canonical(&state.cfg, &headers, &Method::GET, "/.well-known/host-meta", None) {
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
    if let Some(resp) = maybe_redirect_canonical(&state.cfg, &headers, &Method::GET, "/.well-known/nodeinfo", None) {
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
    if rest.starts_with("tunnel/") || rest == "register" || rest == "healthz" || rest == "readyz" || rest.starts_with(".well-known/") {
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
    if method == Method::GET && (path == format!("/users/{user}") || is_cached_collection_path(&user, path)) {
        let is_online = { state.tunnels.read().await.contains_key(&user) };
        if !is_online {
            let db = state.db.lock().await;
            if let Ok(Some((moved_to, _moved_at_ms))) = db.get_user_move(&user) {
                if path == format!("/users/{user}") {
                    if wants_activity_json(&headers) {
                        // Prefer serving a movedTo stub actor so legacy servers can pick up the migration.
                        if let Ok(Some(actor_json)) = db.get_actor_cache(&user) {
                            if let Some(patched) = patch_actor_with_moved_to(&actor_json, &moved_to) {
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
                    return (
                        StatusCode::PERMANENT_REDIRECT,
                        [("Location", moved_to)],
                        "",
                    )
                        .into_response();
                }

                // For collections, redirect to the new actor URL + same suffix.
                let suffix = path
                    .strip_prefix(&format!("/users/{user}"))
                    .unwrap_or("");
                let location = format!("{}{}", moved_to.trim_end_matches('/'), suffix);
                return (
                    StatusCode::PERMANENT_REDIRECT,
                    [("Location", location)],
                    "",
                )
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

    let Ok(resp) = tokio::time::timeout(Duration::from_secs(state.cfg.tunnel_timeout_secs), resp_rx).await else {
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
                        let meili_raw_id = if actor_url.is_empty() { format!("user:{user}") } else { actor_url.clone() };
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
    let accept = headers.get("Accept").and_then(|v| v.to_str().ok()).unwrap_or("");
    let accept = accept.to_ascii_lowercase();
    accept.contains("application/activity+json")
        || accept.contains("application/ld+json")
        || accept.contains("application/json")
        || accept.contains("*/*")
        || accept.is_empty()
}

fn moved_actor_stub_json(cfg: &RelayConfig, headers: &HeaderMap, user: &str, moved_to_actor: &str) -> String {
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
            v["alsoKnownAs"] = serde_json::Value::Array(vec![serde_json::Value::String(moved_to_actor.to_string())]);
        }
        None => {
            v["alsoKnownAs"] = serde_json::Value::Array(vec![serde_json::Value::String(moved_to_actor.to_string())]);
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
        let Some(value) = fetch_json_url(state, &url).await else { break };
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

fn next_url_from_collection(state: &AppState, user: &str, value: &serde_json::Value) -> Option<String> {
    let next = value.get("next")?;
    let raw = if let Some(s) = next.as_str() {
        s.to_string()
    } else if let Some(obj) = next.as_object() {
        obj.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string()
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
    let actor_id = v.get("id").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
    let actor_url = extract_actor_url_from_value(&v).or_else(|| actor_id.clone());
    (actor_id, actor_url)
}

fn extract_actor_url_from_value(v: &serde_json::Value) -> Option<String> {
    let url_val = v.get("url")?;
    if let Some(s) = url_val.as_str() {
        let s = s.trim();
        return if s.is_empty() { None } else { Some(s.to_string()) };
    }
    if let Some(obj) = url_val.as_object() {
        if let Some(s) = obj.get("href").and_then(|v| v.as_str()) {
            let s = s.trim();
            return if s.is_empty() { None } else { Some(s.to_string()) };
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
    let Some(expected) = &cfg.admin_token else { return false };
    let auth = headers.get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or("");
    let Some(token) = auth.strip_prefix("Bearer ") else { return false };
    token == expected
}

fn is_valid_username(user: &str) -> bool {
    if user.is_empty() || user.len() > 64 {
        return false;
    }
    user.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
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
    if is_valid_username(&user) { Some(user) } else { None }
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
            "#,
                )?;
                // Migrate existing dbs.
                let _ = conn.execute("ALTER TABLE users ADD COLUMN disabled INTEGER NOT NULL DEFAULT 0", []);
                let _ = conn.execute("ALTER TABLE user_cache ADD COLUMN actor_id TEXT NULL", []);
                let _ = conn.execute("ALTER TABLE user_cache ADD COLUMN actor_url TEXT NULL", []);
                let _ = conn.execute("ALTER TABLE relay_registry ADD COLUMN sign_pubkey_b64 TEXT NULL", []);
                let _ = conn.execute("ALTER TABLE admin_audit ADD COLUMN request_id TEXT NULL", []);
                let _ = conn.execute("ALTER TABLE admin_audit ADD COLUMN correlation_id TEXT NULL", []);
                let _ = conn.execute("ALTER TABLE admin_audit ADD COLUMN user_agent TEXT NULL", []);
                Ok(())
            }
            DbDriver::Postgres => {
                let url = self
                    .db_url
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("FEDI3_RELAY_DB_URL is required for postgres"))?;
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

    fn list_relays(&self, limit: u32) -> Result<Vec<(String, Option<String>, i64, Option<String>, Option<String>)>> {
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
                conn.query_row("SELECT value FROM relay_meta WHERE key=?1", params![key], |r| r.get(0))
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
                conn.execute("INSERT OR REPLACE INTO relay_meta(key,value) VALUES (?1,?2)", params![key, value])?;
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
                let mut stmt = conn.prepare("SELECT key, value FROM relay_meta WHERE key LIKE 'relay_sync_last_ms:%'")?;
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
                let n: u64 = conn.query_row("SELECT COUNT(*) FROM users WHERE disabled=0", [], |r| r.get(0))?;
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
            return Ok(if created { UpsertUserResult::Created } else { UpsertUserResult::Exists });
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
                let Some((stored, disabled)) = row else { return Ok(false) };
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
                let _ = conn.execute("DELETE FROM user_moves WHERE username=?1", params![username])?;
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

    fn list_recent_move_notices(&self, cutoff_ms: i64, limit: u32) -> Result<Vec<(String, String, i64)>> {
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

    fn get_fanout_status(&self, notice_id: &str, relay_url: &str) -> Result<Option<(i64, i64, i64)>> {
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
                let _ = conn.execute("DELETE FROM inbox_spool WHERE username=?1", params![username])?;
                let _ = conn.execute("DELETE FROM user_cache WHERE username=?1", params![username])?;
                let _ = conn.execute("DELETE FROM user_collection_cache WHERE username=?1", params![username])?;
                let _ = conn.execute("DELETE FROM media_items WHERE username=?1", params![username])?;
                let _ = conn.execute("DELETE FROM peer_directory WHERE username=?1", params![username])?;
                let changed = conn.execute("DELETE FROM users WHERE username=?1", params![username])?;
                Ok(changed > 0)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let _ = conn.execute("DELETE FROM inbox_spool WHERE username=$1", &[&username])?;
                let _ = conn.execute("DELETE FROM user_cache WHERE username=$1", &[&username])?;
                let _ = conn.execute("DELETE FROM user_collection_cache WHERE username=$1", &[&username])?;
                let _ = conn.execute("DELETE FROM media_items WHERE username=$1", &[&username])?;
                let _ = conn.execute("DELETE FROM peer_directory WHERE username=$1", &[&username])?;
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
                let row = conn.query_opt("SELECT disabled FROM users WHERE username=$1", &[&username])?;
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
                    let params: Vec<&dyn rusqlite::ToSql> = chunk.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
                    let _ = tx.execute(&sql, rusqlite::params_from_iter(params))?;
                }
                tx.commit()?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                for chunk in ids.chunks(DB_BATCH_DELETE_MAX) {
                    let _ = conn.execute("DELETE FROM inbox_spool WHERE id = ANY($1)", &[&chunk])?;
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
                let deleted = conn.execute("DELETE FROM inbox_spool WHERE created_at_ms < ?1", params![cutoff])?;
                Ok(deleted as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let deleted = conn.execute("DELETE FROM inbox_spool WHERE created_at_ms < $1", &[&cutoff])?;
                Ok(deleted as u64)
            }
        }
    }

    fn cleanup_move_notices(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms() - (ttl_secs as i64 * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let deleted = conn.execute("DELETE FROM move_notices WHERE created_at_ms < ?1", params![cutoff])?;
                let _ = conn.execute(
                    "DELETE FROM move_notice_fanout WHERE notice_id NOT IN (SELECT notice_id FROM move_notices)",
                    [],
                )?;
                Ok(deleted as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let deleted = conn.execute("DELETE FROM move_notices WHERE created_at_ms < $1", &[&cutoff])?;
                let _ = conn.execute(
                    "DELETE FROM move_notice_fanout WHERE notice_id NOT IN (SELECT notice_id FROM move_notices)",
                    &[],
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
                    "SELECT COUNT(*), COALESCE(SUM(body_len), 0) FROM inbox_spool WHERE username=$1",
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
                let deleted = conn.execute("DELETE FROM peer_directory WHERE updated_at_ms < ?1", params![cutoff])?;
                Ok(deleted as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let deleted = conn.execute("DELETE FROM peer_directory WHERE updated_at_ms < $1", &[&cutoff])?;
                Ok(deleted)
            }
        }
    }

    fn cleanup_webrtc_signals(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms().saturating_sub((ttl_secs as i64) * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                Ok(conn.execute("DELETE FROM webrtc_signals WHERE created_at_ms < ?1", params![cutoff])? as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let deleted = conn.execute("DELETE FROM webrtc_signals WHERE created_at_ms < $1", &[&cutoff])?;
                Ok(deleted as u64)
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
                tx.execute("DELETE FROM relay_note_tags WHERE note_id=?1", params![note.note_id])?;
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
                tx.execute("DELETE FROM relay_note_tags WHERE note_id=$1", &[&note.note_id])?;
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
                                .query_row("SELECT count FROM relay_notes_count WHERE id = 1", [], |r| r.get(0))
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
                            let row = conn.query_one("SELECT count FROM relay_notes_count WHERE id = 1", &[])?;
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
                Ok(CollectionPage { total: items.len() as u64, items, next })
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
                Ok(CollectionPage { total: items.len() as u64, items, next })
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

    fn list_peer_directory(&self, q: &str, limit: u32) -> Result<Vec<(String, String, String)>> {
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
                let mut stmt = conn.prepare(
                    "SELECT peer_id, username, actor_url FROM peer_directory WHERE lower(username) LIKE ?1 OR lower(actor_url) LIKE ?1 ORDER BY updated_at_ms DESC LIMIT ?2",
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
                    "SELECT peer_id, username, actor_url FROM peer_directory WHERE lower(username) LIKE $1 OR lower(actor_url) LIKE $1 ORDER BY updated_at_ms DESC LIMIT $2",
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

    fn upsert_relay_user_directory(&self, username: &str, actor_url: &str, relay_url: &str) -> Result<()> {
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
    fn list_relay_user_directory(&self, q: &str, limit: u32) -> Result<Vec<(String, String, String)>> {
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
                let mut stmt = conn.prepare(
                    "SELECT username, actor_url, relay_url FROM relay_user_directory WHERE lower(username) LIKE ?1 OR lower(actor_url) LIKE ?1 ORDER BY updated_at_ms DESC LIMIT ?2",
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
                    "SELECT username, actor_url, relay_url FROM relay_user_directory WHERE lower(username) LIKE $1 OR lower(actor_url) LIKE $1 ORDER BY updated_at_ms DESC LIMIT $2",
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
                    &[&signal_id, &to_peer_id, &from_actor, &session_id, &kind, &payload_json, &now],
                )?;
                Ok(())
            }
        }
    }

    fn list_webrtc_signals(&self, to_peer_id: &str, limit: u32) -> Result<Vec<(String, String, String, String, String)>> {
        let limit = limit.min(200).max(1) as i64;
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
                    let placeholders = std::iter::repeat("?")
                        .take(chunk.len())
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
                    deleted = deleted.saturating_add(n as u64);
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
    let v = headers.get("Authorization")?.to_str().ok()?.trim().to_string();
    let v = v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer "))?;
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
                if conns.is_empty() { None } else { Some(conns) }
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
        if let Some(ok) = self.redis_check_weighted(&ip, bucket, per_minute, weight).await {
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
        let backoff = base.min(self.noisy_backoff_max_secs.max(self.noisy_backoff_base_secs));
        entry.blocked_until_ms = now + (backoff as i64).saturating_mul(1000);
        entry.last_hit_ms = now;
    }

    async fn redis_check_weighted(&self, ip: &str, bucket: &str, per_minute: u32, weight: u32) -> Option<bool> {
        let Some(redis) = self.redis_handle() else { return None };
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
        let Some(redis) = self.redis_handle() else { return None };
        let key = format!("{}:noisy:block:{}", self.redis_prefix, ip);
        let mut conn = redis.lock().await;
        let ttl: redis::RedisResult<i64> = conn.ttl(key).await;
        match ttl {
            Ok(v) if v > 0 => Some(v as u64),
            _ => None,
        }
    }

    async fn redis_register_noisy(&self, ip: &str) -> Option<()> {
        let Some(redis) = self.redis_handle() else { return None };
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
    let Some(raw) = env else { return Vec::new(); };
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
            let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - prefix) };
            (u32::from(ip) & mask) == (u32::from(base) & mask)
        }
        (IpAddr::V6(ip), IpAddr::V6(base)) => {
            let prefix = prefix.min(128);
            let mask = if prefix == 0 { 0 } else { u128::MAX << (128 - prefix) };
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
        let _ = state
            .db
            .lock()
            .await
            .insert_admin_audit(
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
        let _ = state
            .db
            .lock()
            .await
            .insert_admin_audit(
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
    let limit = q.get("limit").and_then(|v| v.parse::<u32>().ok()).unwrap_or(100).min(500);
    let offset = q.get("offset").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
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
    let audit = match admin_guard(&state, &peer, &headers, "admin_disable_user", Some(&user)).await {
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
    let audit = match admin_guard(&state, &peer, &headers, "admin_rotate_token", Some(&user)).await {
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

async fn relay_stats(State(state): State<AppState>, Query(q): Query<RelayTelemetryQuery>) -> impl IntoResponse {
    let _ = q;
    let telemetry = match build_self_telemetry(&state).await {
        Ok(t) => t,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("telemetry error: {e}")).into_response(),
    };
    axum::Json(telemetry).into_response()
}

async fn relay_me(State(state): State<AppState>, headers: HeaderMap, Query(q): Query<RelayMeQuery>) -> impl IntoResponse {
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

async fn relay_search_notes(State(state): State<AppState>, headers: HeaderMap, Query(q): Query<RelaySearchQuery>) -> impl IntoResponse {
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
        match search.search_notes(&query, &tag, limit, cursor, since).await {
            Ok(p) => p,
            Err(e) => return (StatusCode::BAD_GATEWAY, format!("search error: {e}")).into_response(),
        }
    } else {
        let db = state.db.lock().await;
        match db.search_relay_notes(&query, &tag, limit, cursor, since, state.cfg.search_total_mode) {
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
        .check(peer_ip(&peer), "relay_sync", state.cfg.rate_limit_forward_per_min)
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
                .map(|note| RelaySyncNoteItem { note, created_at_ms })
        })
        .collect::<Vec<_>>();
    axum::Json(RelaySyncNotesResponse { items, next: page.next }).into_response()
}

async fn relay_search_users(State(state): State<AppState>, headers: HeaderMap, Query(q): Query<RelaySearchQuery>) -> impl IntoResponse {
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
        match search.search_users(&query, limit, cursor, &base_template).await {
            Ok(p) => p,
            Err(e) => return (StatusCode::BAD_GATEWAY, format!("search error: {e}")).into_response(),
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

async fn relay_search_hashtags(State(state): State<AppState>, headers: HeaderMap, Query(q): Query<RelaySearchQuery>) -> impl IntoResponse {
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

async fn relay_search_coverage(State(state): State<AppState>, headers: HeaderMap, Query(q): Query<RelayCoverageQuery>) -> impl IntoResponse {
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

async fn relay_list(State(state): State<AppState>, Query(q): Query<RelayTelemetryQuery>) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(200).min(500);
    let db = state.db.lock().await;
    let rows = match db.list_relays(limit) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, format!("db error: {e}")).into_response(),
    };
    drop(db);

    let mut relays = Vec::new();
    for (url, base_domain, last_seen_ms, last_json, sign_pubkey_b64) in rows {
        let parsed: Option<serde_json::Value> = last_json.as_deref().and_then(|s| serde_json::from_str(s).ok());
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

async fn relay_peers(State(state): State<AppState>, Query(q): Query<RelayPeersQuery>) -> impl IntoResponse {
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
        if got != expected {
            return (StatusCode::UNAUTHORIZED, "telemetry token required").into_response();
        }
    }

    if !state
        .limiter
        .check(peer_ip(&peer), "telemetry", state.cfg.rate_limit_forward_per_min)
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
    if input.signature_b64.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true) {
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
    let _ = db.upsert_relay(&input.relay_url, input.base_domain.clone(), telemetry_json, Some(provided_pk.clone()));
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
        .check(peer_ip(&peer), "forward", state.cfg.rate_limit_forward_per_min)
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
        .check(peer_ip(&peer), "forward", state.cfg.rate_limit_forward_per_min)
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
        .check(peer_ip(&peer), "forward", state.cfg.rate_limit_forward_per_min)
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
        return (StatusCode::UNAUTHORIZED, "signature or admin/user token required").into_response();
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
    let _ = db.upsert_move_notice(&notice_id, &serde_json::to_string(&notice).unwrap_or_default());
    drop(db);

    // Fan-out the signed notice to other relays (best-effort).
    tokio::spawn(fanout_move_notice(state.clone(), notice_id, body.to_vec(), hop + 1));

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
        .check(peer_ip(&peer), "forward", state.cfg.rate_limit_forward_per_min)
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

    let (actor, _actor_json) = match verify_actor_signature_for_request(&state, &Method::POST, &uri.0, &headers, Some(&body)).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::UNAUTHORIZED, format!("signature invalid: {e}")).into_response(),
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
        if let Err(e) = db.insert_webrtc_signal(&signal_id, &to_peer_id, &actor, &session_id, &kind, &payload_json) {
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
        .check(peer_ip(&peer), "forward", state.cfg.rate_limit_forward_per_min)
        .await
    {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }

    let to_peer_id = q.to_peer_id.unwrap_or_default().trim().to_string();
    if to_peer_id.is_empty() || to_peer_id.len() > 128 {
        return (StatusCode::BAD_REQUEST, "invalid to_peer_id").into_response();
    }
    let limit = q.limit.unwrap_or(50).max(1).min(200);

    let (actor, actor_json) = match verify_actor_signature_for_request(&state, &Method::GET, &uri.0, &headers, None).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::UNAUTHORIZED, format!("signature invalid: {e}")).into_response(),
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
        let payload: serde_json::Value = serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null);
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
        .check(peer_ip(&peer), "forward", state.cfg.rate_limit_forward_per_min)
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

    let (_actor, actor_json) = match verify_actor_signature_for_request(&state, &Method::POST, &uri.0, &headers, Some(&body)).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::UNAUTHORIZED, format!("signature invalid: {e}")).into_response(),
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
    let limit = q.get("limit").and_then(|v| v.parse::<u32>().ok()).unwrap_or(200).min(500);
    let offset = q.get("offset").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
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
                rows.into_iter().map(|(id, action, username, actor, ip, ok, detail, created_at_ms, request_id, correlation_id, user_agent)| {
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
                }).collect::<Vec<_>>()
            ).into_response()
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

async fn fetch_peer_hello(state: &AppState, user: &str, tunnel_tx: mpsc::Sender<TunnelRequest>) -> Result<Option<PeerHello>> {
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
    let msg = TunnelRequest { id: id.clone(), req, resp_tx };
    if tunnel_tx.send(msg).await.is_err() {
        return Ok(None);
    }
    let Ok(resp) = tokio::time::timeout(Duration::from_secs(state.cfg.tunnel_timeout_secs), resp_rx).await else {
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
    let (total_users, total_peers_seen, relays, users, peers, search_indexed_users, search_last_index_ms, search_relays_total, search_relays_synced, search_relays_last_sync_ms) = {
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
                actor_url: format!("{}/users/{}", user_base_url(&state.cfg, &username), username),
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
        relay_url: state.cfg.public_url.clone().unwrap_or_else(|| "unknown".to_string()),
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
            let _ = db.upsert_relay(&remote.relay_url, remote.base_domain.clone(), telemetry_json, remote.sign_pubkey_b64.clone());
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
            for p in &remote.peers {
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
    let limit = state.cfg.relay_sync_limit.min(200).max(1);
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
    input.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
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
                        let inner = if obj_map.get("type").and_then(|t| t.as_str()) == Some("Note") {
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
            if ty == "OrderedCollection" || ty == "OrderedCollectionPage" || ty == "Collection" || ty == "CollectionPage" {
                if let Some(items) = map.get("orderedItems").or_else(|| map.get("items")) {
                    if let serde_json::Value::Array(arr) = items {
                        for item in arr {
                            out.extend(extract_notes_from_value(item));
                        }
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
    let id = note.get("id").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if id.is_empty() {
        return None;
    }
    let actor_id = note.get("attributedTo").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
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
    let Some(tag_value) = tag_value else { return Vec::new() };
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

fn actor_stub_json(username: &str, base_template: &str) -> serde_json::Value {
    let base = base_template.replace("{user}", username).trim_end_matches('/').to_string();
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

fn actor_stub_from_actor_url(username: &str, actor_url: &str, fallback_base: &str) -> serde_json::Value {
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
        return (scheme_from_headers(headers).to_string(), host_only(headers).to_string());
    }
    if let Some((s, h)) = canonical_origin(cfg) {
        return (s, h);
    }
    (scheme_from_headers(headers).to_string(), host_only(headers).to_string())
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
    if cur_scheme.eq_ignore_ascii_case(&canon_scheme) && cur_host.eq_ignore_ascii_case(&canon_host) {
        return None;
    }

    let qs = raw_query.map(|q| format!("?{q}")).unwrap_or_default();
    let location = format!("{canon_scheme}://{canon_host}{path}{qs}");
    Some((StatusCode::PERMANENT_REDIRECT, [("Location", location)], "").into_response())
}

fn scheme_from_headers(headers: &HeaderMap) -> &str {
    if let Some(v) = headers.get("X-Forwarded-Proto").and_then(|v| v.to_str().ok()) {
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
        if let Ok(Some((tries, last_try_ms, sent_ok))) = db.get_fanout_status(notice_id, relay_url) {
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

    let url = format!("{}/_fedi3/relay/move_notice", relay_url.trim_end_matches('/'));
    let resp = state
        .http
        .post(url)
        .header("Content-Type", "application/json")
        .header("X-Fedi3-Notice-Id", notice_id.to_string())
        .header("X-Fedi3-Notice-Hop", hop.to_string())
        .body(body.to_vec())
        .send()
        .await;
    let ok = resp.as_ref().map(|r| r.status().is_success()).unwrap_or(false);
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
        let Some((k, v)) = part.split_once('=') else { continue };
        let v = v.trim().trim_matches('"');
        map.insert(k.trim().to_string(), v.to_string());
    }

    let headers = map.get("headers").cloned().unwrap_or_else(|| "date".to_string());
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

fn build_signing_string(method: &Method, uri: &http::Uri, headers: &HeaderMap, signed_headers: &[String]) -> Result<String> {
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

fn verify_signature_rsa_sha256(public_key_pem: &str, signing_string: &str, signature: &[u8]) -> bool {
    use rsa::{
        pkcs1v15::VerifyingKey,
        pkcs8::DecodePublicKey,
        signature::Verifier,
        RsaPublicKey,
    };
    let Ok(public_key) = RsaPublicKey::from_public_key_pem(public_key_pem) else { return false };
    let verifying_key = VerifyingKey::<Sha256>::new(public_key);
    let Ok(sig) = rsa::pkcs1v15::Signature::try_from(signature) else { return false };
    verifying_key.verify(signing_string.as_bytes(), &sig).is_ok()
}

fn extract_public_key_pem_from_actor_json(actor_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(actor_json).ok()?;
    let pk = v.get("publicKey")?;
    let pem = pk.get("publicKeyPem")?.as_str()?;
    let pem = pem.trim();
    if pem.is_empty() { None } else { Some(pem.to_string()) }
}

fn extract_peer_id_from_actor_json(actor_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(actor_json).ok()?;
    let endpoints = v.get("endpoints")?;
    let pid = endpoints.get("fedi3PeerId")?.as_str()?;
    let pid = pid.trim();
    if pid.is_empty() { None } else { Some(pid.to_string()) }
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

async fn verify_actor_signature_for_request(
    state: &AppState,
    method: &Method,
    uri: &http::Uri,
    headers: &HeaderMap,
    body: Option<&[u8]>,
) -> Result<(String, String)> {
    let sig = headers.get("Signature").or_else(|| headers.get("signature")).and_then(|v| v.to_str().ok()).unwrap_or("");
    if sig.trim().is_empty() {
        anyhow::bail!("missing Signature");
    }
    let params = parse_signature_header(sig)?;

    // Date skew (5 minutes).
    let date = headers.get("Date").or_else(|| headers.get("date")).and_then(|v| v.to_str().ok()).unwrap_or("");
    if date.is_empty() {
        anyhow::bail!("missing Date");
    }
    let ts = parse_http_date(date)?;
    let now = std::time::SystemTime::now();
    let diff = if now > ts { now.duration_since(ts).unwrap_or_default() } else { ts.duration_since(now).unwrap_or_default() };
    if diff > Duration::from_secs(300) {
        anyhow::bail!("Date skew too large");
    }

    // Digest check if present and body provided.
    if let Some(body) = body {
        if let Some(d) = headers.get("Digest").or_else(|| headers.get("digest")).and_then(|v| v.to_str().ok()) {
            let Some((alg, value)) = d.split_once('=') else { anyhow::bail!("bad Digest") };
            if !alg.trim().eq_ignore_ascii_case("SHA-256") {
                anyhow::bail!("unsupported Digest alg");
            }
            let expected = B64.decode(value.trim().as_bytes()).unwrap_or_default();
            let actual = Sha256::digest(body);
            if expected.as_slice() != actual.as_slice() {
                anyhow::bail!("Digest mismatch");
            }
        }
    }

    let signing_string = build_signing_string(method, uri, headers, &params.headers)?;

    // Parse keyId from Signature header.
    let key_id = {
        let sig = headers.get("Signature").or_else(|| headers.get("signature")).and_then(|v| v.to_str().ok()).unwrap_or("");
        let mut key_id = None;
        for part in sig.split(',') {
            let part = part.trim();
            if let Some(v) = part.strip_prefix("keyId=") {
                let v = v.trim().trim_matches('"').trim();
                if !v.is_empty() {
                    key_id = Some(v.to_string());
                }
            }
        }
        key_id.ok_or_else(|| anyhow::anyhow!("Signature missing keyId"))?
    };

    let actor = actor_from_key_id(&key_id).ok_or_else(|| anyhow::anyhow!("invalid keyId"))?;
    let resp = state
        .http
        .get(&actor)
        .header("Accept", "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"")
        .send()
        .await?;
    let actor_json = resp.text().await?;
    let pem = extract_public_key_pem_from_actor_json(&actor_json).ok_or_else(|| anyhow::anyhow!("actor missing publicKeyPem"))?;
    if !verify_signature_rsa_sha256(&pem, &signing_string, &params.signature) {
        anyhow::bail!("signature invalid");
    }
    Ok((actor, actor_json))
}

async fn verify_move_notice_signature(state: &AppState, headers: &HeaderMap, user: &str, body: &[u8]) -> Result<bool> {
    let sig = headers.get("Signature").and_then(|v| v.to_str().ok()).unwrap_or("");
    if sig.trim().is_empty() {
        return Ok(false);
    }

    // Date skew (5 minutes).
    let date = headers.get("Date").and_then(|v| v.to_str().ok()).unwrap_or("");
    if date.is_empty() {
        return Ok(false);
    }
    let ts = parse_http_date(date)?;
    let now = std::time::SystemTime::now();
    let diff = if now > ts { now.duration_since(ts).unwrap_or_default() } else { ts.duration_since(now).unwrap_or_default() };
    if diff > Duration::from_secs(300) {
        return Ok(false);
    }

    // Digest check if present.
    if let Some(d) = headers.get("Digest").and_then(|v| v.to_str().ok()) {
        let Some((alg, value)) = d.split_once('=') else { return Ok(false) };
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
            if let Some(old_actor) = notice.old_actor.as_deref().map(str::trim).filter(|v| v.starts_with("http://") || v.starts_with("https://")) {
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
    Ok(verify_signature_rsa_sha256(&pem, &signing_string, &params.signature))
}
