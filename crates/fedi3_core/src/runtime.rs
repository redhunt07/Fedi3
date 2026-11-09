/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use crate::ap::{handle_request, ApConfig, ApState, GlobalIngestPolicy, InboxRateLimits};
use crate::delivery::Delivery;
use crate::delivery_queue::{DeliveryQueue, PostDeliveryMode, QueueSettings};
use crate::http_retry::send_with_retry;
use crate::http_sig::{sign_request_rsa_sha256, KeyResolver};
use crate::keys::{default_data_dir, did_from_public_key_pem};
use crate::net_metrics::NetMetrics;
use crate::object_fetch::ObjectFetchWorker;
use crate::social_db::SocialDb;
use anyhow::{Context, Result};
use axum::{routing::any, Router};
use http::{HeaderMap, HeaderValue, Method as HttpMethod, Uri};
use std::{
    net::{SocketAddr, TcpListener as StdTcpListener},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};
use tokio::sync::watch;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};

static HANDLE_SEQ: AtomicU64 = AtomicU64::new(1);

struct RunningCore {
    shutdown_tx: watch::Sender<bool>,
    join: Option<thread::JoinHandle<()>>,
    bind: SocketAddr,
    data_dir: std::path::PathBuf,
}

static REGISTRY: Mutex<Vec<(u64, RunningCore)>> = Mutex::new(Vec::new());

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[derive(Debug, serde::Deserialize)]
pub struct RelayP2pInfra {
    pub peer_id: Option<String>,
    #[serde(default)]
    pub multiaddrs: Vec<String>,
}

/// Fetches the relay's P2P multiaddresses, normalized with the relay peer ID.
async fn fetch_relay_infra(http: &reqwest::Client, url: &str) -> Option<Vec<String>> {
    let response = http
        .get(url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let info: RelayP2pInfra = response.json().await.ok()?;
    let relay_peer = info.peer_id.as_deref()?.trim().to_string();
    if relay_peer.is_empty() {
        return None;
    }
    let mut addrs = Vec::new();
    for addr in info
        .multiaddrs
        .iter()
        .map(|a| a.trim())
        .filter(|a| !a.is_empty())
    {
        let full = if addr.contains("/p2p/") {
            addr.to_string()
        } else {
            format!("{addr}/p2p/{relay_peer}")
        };
        if !addrs.contains(&full) {
            addrs.push(full);
        }
    }
    (!addrs.is_empty()).then_some(addrs)
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct CoreStartConfig {
    pub username: String,
    pub domain: String,
    #[serde(default, alias = "base_url")]
    pub public_base_url: String,
    /// Optional canonical identity anchor (e.g. `https://alice.example`). When set, the actor
    /// id/key live there while inbox/outbox stay on the relay transport.
    #[serde(default)]
    pub anchor_actor_url: Option<String>,
    pub previous_public_base_url: Option<String>,
    /// Relay token for the previous relay when performing a migration (used to register a moved-to mapping).
    #[serde(default)]
    pub previous_relay_token: Option<String>,
    /// Safety guard: refuse changing relay/public_base_url when legacy followers exist unless a migration is configured.
    /// Set true only for development/testing.
    #[serde(default)]
    pub allow_relay_switch_without_migration: Option<bool>,
    pub relay_ws: String,
    pub relay_token: Option<String>,
    pub bind: String,
    pub p2p: Option<crate::p2p::P2pConfig>,
    pub media: Option<crate::media_backend::MediaConfig>,
    pub storage: Option<crate::storage_gc::StorageConfig>,
    pub data_dir: Option<String>,
    pub max_date_skew_secs: Option<u64>,
    #[serde(default)]
    pub internal_token: Option<String>,
    #[serde(default)]
    pub global_ingest_max_items_per_actor_per_min: Option<u32>,
    #[serde(default)]
    pub global_ingest_max_bytes_per_actor_per_min: Option<u64>,
    #[serde(default)]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(default)]
    pub blocked_actors: Option<Vec<String>>,
    /// Add an external legacy account as alias (useful when migrating from a legacy instance to this Fedi3 account).
    /// Example: `https://mastodon.example/@alice` actor URL.
    #[serde(default)]
    pub legacy_aliases: Option<Vec<String>>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub icon_media_type: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub image_media_type: Option<String>,
    /// Optional profile fields shown on the actor (Mastodon/Misskey-style key/value pairs).
    #[serde(default)]
    pub profile_fields: Option<Vec<crate::ap::ProfileField>>,
    /// When true, inbound Follow requests require manual approval (a.k.a. "locked account").
    #[serde(default)]
    pub manually_approves_followers: Option<bool>,
    /// Optional list of ActivityPub relay actor URLs to follow (Misskey/Mastodon-style relay),
    /// to populate the federated timeline without manually following every remote account.
    #[serde(default)]
    pub ap_relays: Option<Vec<String>>,
    /// HTTP client timeout for outbound requests (seconds).
    #[serde(default)]
    pub http_timeout_secs: Option<u64>,
    /// Max inbound request body size (bytes).
    #[serde(default)]
    pub max_body_bytes: Option<usize>,
    /// Post delivery mode: "p2p_only" or "p2p_relay".
    #[serde(default)]
    pub post_delivery_mode: Option<String>,
    /// Seconds to wait after a failed P2P attempt before using relay (P2P+relay mode).
    #[serde(default)]
    pub p2p_relay_fallback_secs: Option<u64>,
    /// Mailbox cache TTL for P2P store-and-forward (seconds).
    #[serde(default)]
    pub p2p_cache_ttl_secs: Option<u64>,
}

impl Default for CoreStartConfig {
    fn default() -> Self {
        Self {
            username: "alice".to_string(),
            domain: "example.invalid".to_string(),
            public_base_url: "http://127.0.0.1:8787".to_string(),
            anchor_actor_url: None,
            previous_public_base_url: None,
            previous_relay_token: None,
            allow_relay_switch_without_migration: None,
            relay_ws: "ws://127.0.0.1:8787".to_string(),
            relay_token: Some("devtoken".to_string()),
            bind: "127.0.0.1:8788".to_string(),
            p2p: None,
            media: None,
            storage: None,
            data_dir: None,
            max_date_skew_secs: Some(3600),
            internal_token: None,
            global_ingest_max_items_per_actor_per_min: None,
            global_ingest_max_bytes_per_actor_per_min: None,
            blocked_domains: None,
            blocked_actors: None,
            legacy_aliases: None,
            display_name: None,
            summary: None,
            icon_url: None,
            icon_media_type: None,
            image_url: None,
            image_media_type: None,
            profile_fields: None,
            manually_approves_followers: None,
            ap_relays: None,
            http_timeout_secs: None,
            max_body_bytes: None,
            post_delivery_mode: None,
            p2p_relay_fallback_secs: None,
            p2p_cache_ttl_secs: None,
        }
    }
}

fn validate_username(username: &str) -> bool {
    !username.is_empty()
        && username.len() <= 64
        && username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn validate_domain(domain: &str) -> bool {
    let domain = domain.trim();
    if domain.is_empty() || domain.len() > 253 {
        return false;
    }
    if domain.contains("://") || domain.contains('/') || domain.contains(' ') {
        return false;
    }
    domain.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    })
}

fn data_dir_for(cfg: &CoreStartConfig) -> Result<std::path::PathBuf> {
    if let Some(dir) = &cfg.data_dir {
        Ok(std::path::PathBuf::from(dir))
    } else {
        Ok(default_data_dir()?.join("app").join(&cfg.username))
    }
}

/// Start the core. This function returns `Ok(handle)` only after the local server is
/// actually bound to `cfg.bind` and the database has been opened successfully.
/// Appends tracing output to a file; Android drops native stdout, so the core
/// keeps a log next to its database and mirrors it to logcat for diagnostics.
struct FileWriter {
    file: Option<Arc<std::fs::File>>,
}

struct FileWriterStream {
    file: Option<Arc<std::fs::File>>,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for FileWriter {
    type Writer = FileWriterStream;

    fn make_writer(&'a self) -> Self::Writer {
        FileWriterStream {
            file: self.file.clone(),
        }
    }
}

impl std::io::Write for FileWriterStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Some(file) = &self.file {
            let mut file = &**file;
            let _ = std::io::Write::write_all(&mut file, buf);
        }
        #[cfg(target_os = "android")]
        android_log::write("fedi3-core", buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(file) = &self.file {
            let mut file = &**file;
            let _ = std::io::Write::flush(&mut file);
        }
        Ok(())
    }
}

#[cfg(target_os = "android")]
mod android_log {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int};

    #[link(name = "log")]
    extern "C" {
        fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }

    /// Android drops native stdout/stderr, so mirror tracing output to logcat.
    pub fn write(tag: &str, line: &[u8]) {
        let text: Vec<u8> = line
            .iter()
            .copied()
            .filter(|b| *b != 0 && *b != b'\x1b')
            .collect();
        // Drop the "[0m" tails left behind when only the ESC byte is removed.
        let text = String::from_utf8_lossy(&text).replace("[0m", "");
        let Ok(text) = CString::new(text.as_bytes()) else {
            return;
        };
        let Ok(tag) = CString::new(tag) else { return };
        // ANDROID_LOG_INFO = 4.
        unsafe {
            __android_log_write(4, tag.as_ptr(), text.as_ptr());
        }
    }
}

fn init_tracing(data_dir: Option<&std::path::Path>) {
    let filter =
        tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse().unwrap());
    const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
    let file_writer = data_dir
        .map(|dir| dir.join("fedi3.log"))
        .and_then(|path| {
            // Keep the log bounded: start over once it grows past the cap.
            let oversized = std::fs::metadata(&path)
                .map(|m| m.len() > MAX_LOG_BYTES)
                .unwrap_or(false);
            let mut options = std::fs::OpenOptions::new();
            options.create(true);
            if oversized {
                options.write(true).truncate(true);
            } else {
                options.append(true);
            }
            options.open(path).ok()
        })
        .map(|file| Arc::new(file));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    match file_writer {
        Some(file) => {
            builder
                .with_writer(FileWriter { file: Some(file) })
                .try_init()
                .ok();
        }
        None => {
            // Without a log file, keep stdout on desktop; Android has neither,
            // so it still mirrors to logcat.
            #[cfg(target_os = "android")]
            {
                builder
                    .with_writer(FileWriter { file: None })
                    .try_init()
                    .ok();
            }
            #[cfg(not(target_os = "android"))]
            {
                builder.try_init().ok();
            }
        }
    }
}

pub fn start(cfg: CoreStartConfig) -> Result<u64> {
    crate::init_tls_provider();
    let log_dir = data_dir_for(&cfg).ok();
    init_tracing(log_dir.as_deref());

    // Fail-fast config validation (H-43/M-33).
    if !validate_username(&cfg.username) {
        anyhow::bail!(
            "invalid username {:?}: must match ^[A-Za-z0-9_]{{1,64}}$",
            cfg.username
        );
    }
    if !validate_domain(&cfg.domain) {
        anyhow::bail!("invalid domain {:?}: expected a hostname", cfg.domain);
    }
    let addr: SocketAddr = cfg.bind.parse().context("parse bind address")?;
    let data_dir = data_dir_for(&cfg)?;

    // A panic in a background worker is otherwise silent on Android, where
    // native stderr is dropped. Route it through tracing (logcat + file).
    std::panic::set_hook(Box::new(|info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("unknown panic");
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        error!("panic at {location}: {message}");
    }));

    // Starting twice with the same bind address and data directory is not an
    // error: the FFI runtime survives a Dart isolate restart (for example when
    // Android recreates the activity while the process is still alive), so the
    // new isolate must be able to adopt the running core instead of failing on
    // a busy port.
    {
        let reg = REGISTRY
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((handle, _)) = reg
            .iter()
            .find(|(_, core)| core.bind == addr && core.data_dir == data_dir)
        {
            return Ok(*handle);
        }
    }

    // Synchronous bind on the calling thread: a busy port must be an Err, never a live handle.
    let std_listener = StdTcpListener::bind(addr).with_context(|| format!("bind {addr}"))?;
    std_listener
        .set_nonblocking(true)
        .context("set listener nonblocking")?;

    // Open the DB before spawning so a corrupt/locked DB fails the start.
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("create data dir {}", data_dir.display()))?;
    crate::keys::secure_dir(&data_dir)?;
    let db_path = data_dir.join("fedi3.db");
    crate::social_db::SocialDb::open(&db_path).context("open social db")?;
    crate::delivery_queue::DeliveryQueue::open(&db_path).context("open delivery queue db")?;

    // Load (or generate) the identity on the caller thread too: a corrupted key file must not
    // silently produce a new DID after start returned success.
    let identity = crate::keys::load_or_generate_identity(&data_dir)?;

    let handle = HANDLE_SEQ.fetch_add(1, Ordering::Relaxed);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let registered_data_dir = data_dir.clone();

    let join = thread::spawn(move || {
        if let Err(e) = run_core(cfg, shutdown_rx, std_listener, data_dir, identity) {
            error!("core runtime failed: {e:#}");
        }
    });

    let mut reg = REGISTRY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reg.push((
        handle,
        RunningCore {
            shutdown_tx,
            join: Some(join),
            bind: addr,
            data_dir: registered_data_dir,
        },
    ));

    Ok(handle)
}

pub fn stop(handle: u64) -> Result<()> {
    let running = {
        let mut reg = REGISTRY
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let idx = reg
            .iter()
            .position(|(h, _)| *h == handle)
            .context("invalid handle")?;
        let (_, mut running) = reg.swap_remove(idx);
        let _ = running.shutdown_tx.send(true);
        running.join.take()
    };

    if let Some(j) = running {
        // Wait for a clean shutdown (DB checkpoint, workers stopped), but never hang forever.
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = j.join();
            let _ = tx.send(());
        });
        let _ = rx.recv_timeout(Duration::from_secs(15));
    }
    Ok(())
}

fn run_core(
    cfg: CoreStartConfig,
    mut shutdown_rx: watch::Receiver<bool>,
    std_listener: StdTcpListener,
    data_dir: std::path::PathBuf,
    identity: crate::keys::Identity,
) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;

    rt.block_on(async move {
        let relay_ws_enabled = !cfg.relay_ws.trim().is_empty();
        let public_base_url = if cfg.public_base_url.trim().is_empty() {
            if relay_ws_enabled {
                infer_http_base_from_relay_ws(&cfg.relay_ws)?
            } else {
                format!("http://{}", cfg.bind)
            }
        } else {
            cfg.public_base_url.trim_end_matches('/').to_string()
        };

        info!("identity dir: {}", data_dir.display());
        let did = did_from_public_key_pem(&identity.public_key_pem);
        let net = Arc::new(NetMetrics::new());

        // H-48: prune the peer metric maps on a timer, not only when the UI polls.
        {
            let net = net.clone();
            let mut prune_shutdown = shutdown_rx.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(60)) => net.prune_seen(60_000),
                        _ = prune_shutdown.changed() => {
                            if *prune_shutdown.borrow() { break; }
                        }
                    }
                }
            });
        }

        let db_path = data_dir.join("fedi3.db");
        let queue = Arc::new(DeliveryQueue::open(&db_path)?);
        let social = Arc::new(SocialDb::open(&db_path)?);
        let http_timeout_secs = cfg.http_timeout_secs.unwrap_or(30).clamp(5, 120);
        // Federation fetches share one policy: bounded redirects and an explicit user agent
        // (the per-request body caps live in net_policy).
        let http = fedi3_protocol::net::guard_client(reqwest::Client::builder(), crate::net_policy::allow_private_fetch())
            .timeout(Duration::from_secs(http_timeout_secs))
            .connect_timeout(Duration::from_secs(10))
            .redirect(crate::net_policy::redirect_policy())
            .user_agent(format!("fedi3/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .context("build http client")?;

        // Load persisted legacy aliases (used for migrations from legacy instances).
        let legacy_aliases_persisted: Vec<String> = match social.get_local_meta("legacy_aliases_json") {
            Ok(Some(v)) => serde_json::from_str::<Vec<String>>(&v).unwrap_or_default(),
            _ => Vec::new(),
        };

        // Guard: relay changes with legacy followers require explicit migration.
        let public_base_url_norm = public_base_url.trim_end_matches('/').to_string();
        let prev_base_stored = social.get_local_meta("public_base_url").ok().flatten();
        if let Some(prev) = prev_base_stored.as_deref() {
            if prev.trim_end_matches('/') != public_base_url_norm {
                let legacy_followers = social.count_legacy_followers().unwrap_or(0);
                let allow = cfg.allow_relay_switch_without_migration.unwrap_or(false);
                let prev_cfg = cfg
                    .previous_public_base_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(|v| v.trim_end_matches('/').to_string());
                if legacy_followers > 0 && !allow {
                    if prev_cfg.as_deref() != Some(prev.trim_end_matches('/')) {
                        anyhow::bail!(
                            "relay change detected ({prev} -> {public_base_url_norm}) with {legacy_followers} legacy follower(s): set previous_public_base_url={prev} to run migration (Move + alsoKnownAs), or set allow_relay_switch_without_migration=true"
                        );
                    }
                }
            }
        }

        // Stable "published" timestamp for the local actor (RFC3339 in actor JSON).
        let published_ms: i64 = match social.get_local_meta("published_ms") {
            Ok(Some(v)) => v.parse::<i64>().unwrap_or_else(|_| now_ms()),
            _ => {
                let v = now_ms();
                let _ = social.set_local_meta("published_ms", &v.to_string());
                v
            }
        };

        let mut media_cfg = cfg.media.clone().unwrap_or_default();
        if cfg.media.is_none() {
            let base = public_base_url.trim_end_matches('/');
            let is_loopback = base.contains("127.0.0.1") || base.contains("localhost");
            let has_relay_token = cfg
                .relay_token
                .as_deref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if !is_loopback && has_relay_token {
                media_cfg.backend = Some("relay".to_string());
            }
        } else if media_cfg
            .backend
            .as_deref()
            .map(|v| v.eq_ignore_ascii_case("local"))
            .unwrap_or(false)
        {
            let base = public_base_url.trim_end_matches('/');
            let is_loopback = base.contains("127.0.0.1") || base.contains("localhost");
            let has_relay_token = cfg
                .relay_token
                .as_deref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if !is_loopback && has_relay_token {
                media_cfg.backend = Some("relay".to_string());
            }
        }
        if let Some(backend) = media_cfg.backend.clone() {
            if backend.trim().eq_ignore_ascii_case("relay") {
                if media_cfg.relay_base_url.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()).is_none() {
                    media_cfg.relay_base_url = Some(public_base_url.clone());
                }
                if media_cfg.relay_token.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()).is_none() {
                    media_cfg.relay_token = cfg.relay_token.clone();
                }
            }
        }
        let (media_cfg, media_backend) = crate::media_backend::build_media_backend(media_cfg, data_dir.clone(), http.clone())?;
        let mut storage_cfg = cfg.storage.clone().unwrap_or_default();
        if storage_cfg.media_max_local_cache_bytes.is_none() {
            storage_cfg.media_max_local_cache_bytes = media_cfg.max_local_cache_bytes;
        }

        let mut p2p_cfg = cfg.p2p.clone().unwrap_or_default();
        info!(
            enable = p2p_cfg.enable,
            relay_reserve = p2p_cfg.relay_reserve.as_ref().map(|v| v.len()).unwrap_or(0),
            "p2p configuration"
        );
        // The relay advertises its P2P infrastructure address. Reserve a circuit
        // on it so the actor document publishes a dialable relayed address even
        // when the device is behind NAT (the usual situation for mobile clients).
        // The first fetch is best-effort: a mobile core can start before its
        // network is ready, so a background task keeps retrying and hands new
        // addresses to the running swarm.
        let relay_infra_url = if p2p_cfg.enable {
            let base = public_base_url.trim_end_matches('/').to_string();
            let is_loopback = base.contains("127.0.0.1") || base.contains("localhost");
            (!is_loopback).then(|| format!("{base}/_fedi3/relay/p2p_infra"))
        } else {
            None
        };
        if let Some(url) = relay_infra_url.as_deref() {
            match fetch_relay_infra(&http, url).await {
                Some(addrs) => {
                    let reserve = p2p_cfg.relay_reserve.get_or_insert_with(Vec::new);
                    for addr in addrs {
                        if !reserve.iter().any(|r| r.trim() == addr) {
                            reserve.push(addr);
                        }
                    }
                }
                None => tracing::debug!(%url, "relay p2p infra fetch failed; will retry"),
            }
        }
        let (relay_update_tx, relay_update_rx) = tokio::sync::mpsc::channel::<Vec<String>>(4);
        let mut relay_refresh = None;
        if let Some(url) = relay_infra_url.clone() {
            let http = http.clone();
            let mut shutdown = shutdown_rx.clone();
            relay_refresh = Some(tokio::spawn(async move {
                let mut delay = 5u64;
                loop {
                    let addrs = fetch_relay_infra(&http, &url).await;
                    let succeeded = addrs.is_some();
                    if let Some(addrs) = addrs {
                        if relay_update_tx.send(addrs).await.is_err() {
                            break;
                        }
                    }
                    let sleep_for = if succeeded { 300 } else { delay };
                    tokio::select! {
                        _ = tokio::time::sleep(std::time::Duration::from_secs(sleep_for)) => {}
                        _ = shutdown.changed() => {
                            if *shutdown.borrow() { break; }
                        }
                    }
                    if !succeeded {
                        delay = (delay * 2).min(300);
                    } else {
                        delay = 5;
                    }
                }
            }));
        }
        let p2p_keypair = if p2p_cfg.enable {
            Some(crate::p2p::load_or_generate_keypair(&data_dir)?)
        } else {
            None
        };
        let p2p_peer_id = p2p_keypair.as_ref().map(crate::p2p::peer_id_string);
        let p2p_peer_addrs = if p2p_cfg.enable {
            let mut out = p2p_cfg.announce.clone().unwrap_or_default();
            if let Some(peer_id) = p2p_peer_id.clone() {
                if let Some(relays) = p2p_cfg.relay_reserve.as_ref() {
                    for r in relays {
                        let r = r.trim();
                        if r.is_empty() {
                            continue;
                        }
                        // Publish a dialable relayed address for this peer:
                        // <relay-addr>/p2p-circuit/p2p/<self>
                        out.push(format!("{r}/p2p-circuit/p2p/{peer_id}"));
                    }
                }
            }
            out
        } else {
            Vec::new()
        };

        let default_p2p_cache_ttl_secs = QueueSettings::default().p2p_cache_ttl_secs;
        let p2p_cache_ttl_secs = cfg
            .p2p_cache_ttl_secs
            .unwrap_or(default_p2p_cache_ttl_secs);

        let ap_cfg = ApConfig {
            username: cfg.username.clone(),
            domain: cfg.domain.clone(),
            public_base_url: public_base_url.clone(),
            anchor_actor_url: cfg
                .anchor_actor_url
                .clone()
                .map(|s| s.trim().trim_end_matches('/').to_string())
                .filter(|s| !s.is_empty()),
            relay_base_url: infer_http_base_from_relay_ws(&cfg.relay_ws).ok(),
            relay_token: cfg.relay_token.clone(),
            public_key_pem: identity.public_key_pem.clone(),
            also_known_as: Vec::new(),
            p2p_peer_id: p2p_peer_id.clone(),
            p2p_peer_addrs,
            display_name: cfg.display_name.clone().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            summary: cfg.summary.clone().map(|s| s.trim().to_string()),
            icon_url: cfg.icon_url.clone().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            icon_media_type: cfg.icon_media_type.clone().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            image_url: cfg.image_url.clone().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            image_media_type: cfg.image_media_type.clone().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            profile_fields: cfg.profile_fields.clone().unwrap_or_default(),
            manually_approves_followers: cfg.manually_approves_followers.unwrap_or(false),
            published_ms: Some(published_ms),
            blocked_domains: cfg
                .blocked_domains
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            blocked_actors: cfg
                .blocked_actors
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            p2p_cache_ttl_secs,
        };

        let previous_public_base_url = cfg
            .previous_public_base_url
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|v| v.trim_end_matches('/').to_string());
        let old_actor = previous_public_base_url
            .as_ref()
            .map(|b| format!("{b}/users/{}", cfg.username));
        let new_actor = format!("{}/users/{}", public_base_url.trim_end_matches('/'), cfg.username);
        if let Some(old) = old_actor.as_deref() {
            if let Err(e) = social.migrate_chat_actor_identity(old, &new_actor) {
                tracing::warn!("chat actor identity migration failed: {e:#}");
            }
        }

        // The signing key id must be the canonical actor (the anchor when configured), or
        // remote servers reject the signature because the advertised publicKey id differs.
        let key_id = format!("{}#main-key", crate::ap::canonical_actor_url(&ap_cfg));
        let key_id_for_fetch = key_id.clone();

        let internal_token = cfg.internal_token.as_deref().map(str::trim)
            .filter(|token| !token.is_empty()).map(str::to_owned).unwrap_or_else(random_token);
        let (ui_events, _) = tokio::sync::broadcast::channel::<crate::ui_events::UiEvent>(512);
        let global_ingest = GlobalIngestPolicy {
            max_items_per_actor_per_min: cfg.global_ingest_max_items_per_actor_per_min.unwrap_or(20).max(1),
            max_bytes_per_actor_per_min: cfg
                .global_ingest_max_bytes_per_actor_per_min
                .unwrap_or(256 * 1024)
                .max(1024),
        };

        let mut queue_settings = QueueSettings::default();
        if let Some(mode) = cfg.post_delivery_mode.as_deref() {
            if let Some(parsed) = PostDeliveryMode::from_str(mode) {
                queue_settings.post_delivery_mode = parsed;
            }
        }
        if let Some(secs) = cfg.p2p_relay_fallback_secs {
            queue_settings.p2p_relay_fallback_secs = secs;
        }
        queue_settings.p2p_cache_ttl_secs = p2p_cache_ttl_secs;

        let max_body_bytes = cfg
            .max_body_bytes
            .unwrap_or(20 * 1024 * 1024)
            .clamp(1 * 1024 * 1024, 100 * 1024 * 1024);
        let state = ApState {
            max_body_bytes,
            cfg: {
                let mut c = ap_cfg;
                if let Some(list) = cfg.legacy_aliases.as_ref() {
                    for a in list {
                        let a = a.trim();
                        if !a.is_empty() && (a.starts_with("http://") || a.starts_with("https://")) {
                            c.also_known_as.push(a.to_string());
                        }
                    }
                }
                for a in &legacy_aliases_persisted {
                    let a = a.trim();
                    if !a.is_empty() && (a.starts_with("http://") || a.starts_with("https://")) {
                        c.also_known_as.push(a.to_string());
                    }
                }
                if let Some(old) = &old_actor {
                    if old != &new_actor {
                        c.also_known_as.push(old.clone());
                    }
                }
                c.also_known_as.push(did.clone());
                c.also_known_as.sort();
                c.also_known_as.dedup();
                c
            },
            private_key_pem: identity.private_key_pem.clone(),
            key_resolver: Arc::new(KeyResolver::new()),
            delivery: Arc::new(Delivery::new()),
            queue: queue.clone(),
            social: social.clone(),
            http: http.clone(),
            object_fetch: ObjectFetchWorker::default(),
            max_date_skew: Duration::from_secs(cfg.max_date_skew_secs.unwrap_or(300)),
            data_dir: data_dir.clone(),
            media_cfg,
            media_backend: Arc::from(media_backend),
            net: net.clone(),
            ui_events,
            internal_token,
            global_ingest,
            post_delivery_mode: queue_settings.post_delivery_mode,
            p2p_relay_fallback: Duration::from_secs(queue_settings.p2p_relay_fallback_secs),
            inbox_limits: InboxRateLimits {
                max_reqs_per_min: 120,
                max_bytes_per_min: 2 * 1024 * 1024,
                max_reqs_per_day: 20_000,
                max_bytes_per_day: 200 * 1024 * 1024,
            },
            inbox_limiter: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        };

        // Publish the static anchor documents (actor + WebFinger) so the user can host them on a
        // cheap static domain. This is what makes the identity independent from the relay.
        {
            let (actor_doc, webfinger_doc) = crate::ap::anchor_documents(&state.cfg);
            let anchor_dir = data_dir.join("anchor");
            if let Err(e) = std::fs::create_dir_all(&anchor_dir) {
                warn!("anchor dir creation failed: {e}");
            } else {
                if let Err(e) = std::fs::write(
                    anchor_dir.join("actor.json"),
                    serde_json::to_vec_pretty(&actor_doc).unwrap_or_default(),
                ) {
                    warn!("anchor actor write failed: {e}");
                }
                if let Err(e) = std::fs::write(
                    anchor_dir.join("webfinger"),
                    serde_json::to_vec_pretty(&webfinger_doc).unwrap_or_default(),
                ) {
                    warn!("anchor webfinger write failed: {e}");
                }
            }
        }

        // Best-effort: push the local blocklist to the relay so moderation is enforced
        // before inbound traffic reaches this core.
        {
            let st = state.clone();
            tokio::spawn(async move {
                crate::ap::sync_blocks_to_relay(&st).await;
            });
        }

        net.set_p2p_enabled(p2p_cfg.enable);
        // H-42: every background worker is registered here so shutdown can join it.
        let mut shutdown_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
        // Seed relay registry and start relay discovery sync.
        let _ = state
            .social
            .upsert_relay_entry(
                state.cfg.relay_base_url.as_deref().unwrap_or(&public_base_url),
                Some(&cfg.relay_ws),
                "self",
            );
        shutdown_handles.push(crate::relay_sync::start_relay_sync_worker(
            state.clone(),
            shutdown_rx.clone(),
        ));

        if p2p_cfg.enable {
            if let Some(relays) = p2p_cfg.relay_reserve.clone() {
                state
                    .delivery
                    .set_mailbox_targets_from_relay_reserve(relays)
                    .await;
            }
        }

        shutdown_handles.push(crate::legacy_sync::start_legacy_sync_worker(
            state.clone(),
            state.delivery.clone(),
            state.social.clone(),
            state.http.clone(),
            shutdown_rx.clone(),
        ));

        // Aggressive "catch up" on startup: pull from followed legacy actors to fill the
        // home/unified/federated feeds after the device was offline.
        //
        // Guarded by a local meta timestamp to avoid hammering remote instances on repeated restarts.
        {
            let social = state.social.clone();
            let st = state.clone();
            tokio::spawn(async move {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64;

                let last_run_ms = social
                    .get_local_meta("legacy_sync_last_run_ms")
                    .ok()
                    .flatten()
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(0);

                // Default: at most once every 30 minutes per device.
                if now_ms.saturating_sub(last_run_ms) < 30 * 60 * 1000 {
                    return;
                }

                let _ = social.set_local_meta("legacy_sync_last_run_ms", &now_ms.to_string());

                // Best-effort: larger limits for startup burst.
                match crate::legacy_sync::run_legacy_sync_now(&st, 10, 400).await {
                    Ok(_) => {
                        let _ = social.set_local_meta("legacy_sync_last_ok_ms", &now_ms.to_string());
                    }
                    Err(e) => {
                        tracing::debug!("startup legacy sync failed: {e:#}");
                    }
                }
            });
        }

        // Persist current public base url for future relay-change detection.
        let _ = state
            .social
            .set_local_meta("public_base_url", &public_base_url_norm);

        shutdown_handles.push(crate::storage_gc::start_storage_gc_worker(
            storage_cfg,
            state.social.clone(),
            queue.clone(),
            data_dir.clone(),
            shutdown_rx.clone(),
        ));

        // One-time best-effort backfill: ensure our own older public outbox items appear in the
        // local DHT/global timeline (useful after upgrades).
        {
            let social = state.social.clone();
            tokio::spawn(async move {
                let _ = tokio::task::spawn_blocking(move || social.backfill_global_feed_from_outbox(200)).await;
            });
        }

        shutdown_handles.push(crate::ap::start_chat_retry_worker(
            state.clone(),
            shutdown_rx.clone(),
        ));

        let key_id_for_queue = key_id.clone();
        shutdown_handles.push(queue.start_worker(
            shutdown_rx.clone(),
            state.delivery.clone(),
            state.private_key_pem.clone(),
            key_id_for_queue,
            queue_settings,
        ));

        // Optional ActivityPub relay subscriptions (Misskey-style):
        // follow a relay actor to receive a broader firehose into our inbox.
        if let Some(list) = cfg.ap_relays.as_ref() {
            let mut relays = list
                .iter()
                .map(|s| s.trim().to_string())
                .filter(|s| s.starts_with("http://") || s.starts_with("https://"))
                .collect::<Vec<_>>();
            relays.sort();
            relays.dedup();
            if !relays.is_empty() {
                let st = state.clone();
                tokio::spawn(async move {
                    for relay_actor in relays {
                        if let Err(e) = ensure_follow_actor(&st, &relay_actor).await {
                            error!("ap relay follow failed ({relay_actor}): {e:#}");
                        }
                    }
                });
            }
        }

        if let (Some(old_base), Some(old_actor)) = (previous_public_base_url.clone(), old_actor.clone()) {
            if old_actor != new_actor {
                let state_for_move = state.clone();
                let new_actor_for_move = new_actor.clone();
                let username_for_move = cfg.username.clone();
                let old_base_for_move = old_base.clone();
                let prev_relay_token = cfg.previous_relay_token.clone();
                let new_base_for_notice = public_base_url_norm.clone();
                tokio::spawn(async move {
                    // Automatic relay migration hint: post a signed MoveNotice to the new relay and the old relay.
                    let notice = serde_json::json!({
                        "username": username_for_move,
                        "moved_to_actor": new_actor_for_move,
                        "old_actor": old_actor,
                        "ts_ms": now_ms(),
                        "nonce": random_token(),
                    });
                    let notice_bytes = serde_json::to_vec(&notice).unwrap_or_default();
                    if !notice_bytes.is_empty() {
                        let _ = post_signed_move_notice(
                            &state_for_move.http,
                            &state_for_move.private_key_pem,
                            &format!("{new_base_for_notice}/users/{}#main-key", state_for_move.cfg.username),
                            &new_base_for_notice,
                            notice_bytes.clone(),
                        )
                        .await;
                        let _ = post_signed_move_notice(
                            &state_for_move.http,
                            &state_for_move.private_key_pem,
                            &format!("{}/users/{}#main-key", old_base_for_move.trim_end_matches('/'), state_for_move.cfg.username),
                            &old_base_for_move,
                            notice_bytes,
                        )
                        .await;
                    }

                    // Best-effort: register a moved-to mapping on the previous relay so legacy servers fetching the old actor
                    // still get a `movedTo` hint even when the client is no longer connected there.
                    if let Some(tok) = prev_relay_token.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                        let url = format!("{}/_fedi3/relay/move", old_base_for_move.trim_end_matches('/'));
                        let body = serde_json::json!({
                            "username": username_for_move,
                            "moved_to_actor": new_actor_for_move,
                        });
                        let _ = state_for_move
                            .http
                            .post(url)
                            .header("Authorization", format!("Bearer {tok}"))
                            .json(&body)
                            .send()
                            .await;
                    }
                    if let Err(e) =
                        send_move_to_followers(&state_for_move, &old_base, &old_actor, &new_actor_for_move).await
                    {
                        error!("migration Move failed: {e:#}");
                    }
                });
            }
        }

        shutdown_handles.push(state.object_fetch.start_with_signing(
            shutdown_rx.clone(),
            state.social.clone(),
            state.http.clone(),
            Some(crate::object_fetch::SignedFetchConfig {
                private_key_pem: state.private_key_pem.clone(),
                key_id: key_id_for_fetch,
            }),
        ));

        let state_for_router = state.clone();
        let router = Router::new().fallback(any(move |req| {
            let st = state_for_router.clone();
            async move { handle_request(&st, req).await }
        }))
        .layer(axum::extract::DefaultBodyLimit::max(max_body_bytes))
        .layer(TraceLayer::new_for_http());

        let addr: SocketAddr = cfg.bind.parse().context("parse bind")?;
        let listener = tokio::net::TcpListener::from_std(std_listener)
            .context("adopt pre-bound listener")?;
        info!("core local server http://{addr}");

        let (server_shutdown_tx, mut server_shutdown_rx) = watch::channel(false);
        let server = tokio::spawn(async move {
            let shutdown = async move {
                let _ = server_shutdown_rx.changed().await;
            };
            axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>())
                .with_graceful_shutdown(shutdown)
                .await
        });

        let relay_ws = cfg.relay_ws.trim().to_string();
        let relay_token = cfg
            .relay_token
            .clone()
            .or_else(|| std::env::var("FEDI3_RELAY_TOKEN").ok())
            .unwrap_or_default();
        let relay_token = relay_token.trim().to_string();
        // H-09: the relay is optional. A missing/short token must not kill the local core:
        // without a relay we still serve the UI and P2P.
        let tunnel_enabled = !relay_ws.is_empty() && !relay_token.is_empty();
        if !relay_ws.is_empty() && relay_token.len() < 16 {
            warn!(
                "relay token missing/too short (min 16 chars): tunnel disabled, local core keeps running"
            );
        }

        // Best-effort: if the user previously had a different relay token (e.g. old devtoken),
        // try to rotate it on the relay before opening the tunnel.
        if let Some(prev) = cfg.previous_relay_token.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            if prev != relay_token {
                let base = public_base_url.trim_end_matches('/').to_string();
                let url = format!("{base}/register");
                let body = serde_json::json!({
                    "username": cfg.username,
                    "token": relay_token,
                });
                let _ = state
                    .http
                    .post(url)
                    .header("Authorization", format!("Bearer {prev}"))
                    .json(&body)
                    .send()
                    .await;
            }
        }

        let user = cfg.username.clone();
        let state_for_tunnel = state.clone();
        let router2 = Router::new().fallback(any(move |mut req: http::Request<axum::body::Body>| {
            let st = state_for_tunnel.clone();
            req.extensions_mut().insert(crate::ap::AuthenticatedRelayForward);
            async move { crate::forward_receipt::handle(&st, req).await }
        }))
        .layer(axum::extract::DefaultBodyLimit::max(max_body_bytes))
        .layer(TraceLayer::new_for_http());

        let shutdown_for_tunnel = shutdown_rx.clone();
        let net_for_tunnel = net.clone();
        let tunnel = if tunnel_enabled {
            Some(tokio::spawn(async move {
                crate::tunnel::run_tunnel_with_shutdown(
                    &user,
                    &relay_ws,
                    &relay_token,
                    router2,
                    shutdown_for_tunnel,
                    net_for_tunnel,
                )
                .await
            }))
        } else {
            net.set_relay_connected(false);
            None
        };

        if let Some(kp) = p2p_keypair {
            let p2p_cfg_for_sync = p2p_cfg.clone();
            let p2p_cfg_for_device = p2p_cfg.clone();
            let state_for_p2p = state.clone();
            let router3 = Router::new().fallback(any(move |req| {
                let st = state_for_p2p.clone();
                async move { handle_request(&st, req).await }
            }))
            .layer(axum::extract::DefaultBodyLimit::max(max_body_bytes))
            .layer(TraceLayer::new_for_http());
            if let Some(handle) =
                crate::p2p::start_p2p_with_relay_updates(
                    p2p_cfg,
                    kp,
                    did.clone(),
                    new_actor.clone(),
                    state.internal_token.clone(),
                    state.private_key_pem.clone(),
                    state.social.clone(),
                    net.clone(),
                    router3,
                    shutdown_rx.clone(),
                    Some(relay_update_rx),
                )?
            {
                state.delivery.set_p2p(Some(handle)).await;
                if let Some(h) =
                    crate::p2p_sync::start_p2p_sync_worker(state.clone(), p2p_cfg_for_sync, shutdown_rx.clone())
                {
                    shutdown_handles.push(h);
                }
                if let Some(h) =
                    crate::device_sync::start_device_sync_worker(state.clone(), p2p_cfg_for_device, shutdown_rx.clone())
                {
                    shutdown_handles.push(h);
                }
            }
        }
        if let Some(h) = relay_refresh {
            shutdown_handles.push(h);
        }

        // WebRTC (ICE/TURN) fallback for “zero-setup” NAT traversal.
        if let Some(peer_id) = state.cfg.p2p_peer_id.clone() {
            let p2p_cfg_for_webrtc = cfg.p2p.clone().unwrap_or_default();
            let state_for_webrtc = state.clone();
            let router_webrtc = Router::new().fallback(any(move |req| {
                let st = state_for_webrtc.clone();
                async move { handle_request(&st, req).await }
            }))
            .layer(axum::extract::DefaultBodyLimit::max(max_body_bytes))
            .layer(TraceLayer::new_for_http());
            if let Ok(Some(w)) = crate::webrtc_p2p::start_webrtc(
                p2p_cfg_for_webrtc,
                peer_id,
                public_base_url.trim_end_matches('/').to_string(),
                state.private_key_pem.clone(),
                key_id.clone(),
                router_webrtc,
                state.http.clone(),
                shutdown_rx.clone(),
                net.clone(),
            ) {
                state.delivery.set_webrtc(Some(w)).await;
            }
        }

        // Wait for shutdown, then stop.
        loop {
            if *shutdown_rx.borrow() {
                break;
            }
            if shutdown_rx.changed().await.is_err() {
                break;
            }
        }
        let _ = server_shutdown_tx.send(true);

        // H-42: join every registered worker before tearing down the runtime, with a global
        // deadline so a stuck worker cannot block the app exit forever. The shutdown watch has
        // already been signalled, so workers observe it at their next select.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        let mut joined = 0usize;
        for handle in shutdown_handles {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                handle.abort();
                warn!("shutdown deadline reached: aborting remaining workers");
                continue;
            }
            match tokio::time::timeout(remaining, handle).await {
                Ok(Ok(())) => joined += 1,
                Ok(Err(e)) => warn!("worker join error: {e}"),
                Err(_) => warn!("worker did not stop within the shutdown deadline"),
            }
        }
        debug!("joined {joined} background workers");

        if let Some(tunnel) = tunnel {
            let _ = tokio::time::timeout(Duration::from_secs(5), tunnel).await;
        }
        let _ = tokio::time::timeout(Duration::from_secs(5), server).await;

        // Flush the SQLite WAL so an immediate restart never races the old writer.
        let _ = state.social.checkpoint();
        Ok::<(), anyhow::Error>(())
    })
}

async fn send_move_to_followers(
    state: &ApState,
    old_base: &str,
    old_actor: &str,
    new_actor: &str,
) -> Result<()> {
    let move_id = state.social.new_activity_id(old_actor);
    let followers_collection = format!("{old_actor}/followers");
    let activity = serde_json::json!({
      "@context": "https://www.w3.org/ns/activitystreams",
      "id": move_id,
      "type": "Move",
      "actor": old_actor,
      "object": old_actor,
      "target": new_actor,
      "to": ["https://www.w3.org/ns/activitystreams#Public"],
      "cc": [followers_collection]
    });

    let bytes = serde_json::to_vec(&activity)?;
    if let Some(id) = activity.get("id").and_then(|v| v.as_str()) {
        let _ = state.social.store_outbox(id, bytes.clone());
    }

    let old_key_id = format!(
        "{}/users/{}#main-key",
        old_base.trim_end_matches('/'),
        state.cfg.username
    );

    let mut cursor: Option<String> = None;
    let mut all: Vec<String> = Vec::new();
    loop {
        let page = state.social.list_followers(200, cursor)?;
        all.extend(page.items);
        cursor = page.next;
        if cursor.is_none() {
            break;
        }
    }
    if all.is_empty() {
        return Ok(());
    }

    let _pending = state
        .queue
        .enqueue_activity_with_key_id(bytes, all, Some(old_key_id))
        .await?;
    Ok(())
}

async fn ensure_follow_actor(state: &ApState, target_actor: &str) -> Result<()> {
    let target_actor = target_actor.trim();
    if target_actor.is_empty() {
        return Ok(());
    }

    // Avoid duplicate follows.
    if let Ok(Some(_)) = state.social.get_following_status(target_actor) {
        return Ok(());
    }

    let base = state.cfg.public_base_url.trim_end_matches('/');
    let me = format!("{base}/users/{}", state.cfg.username);
    let id = state.social.new_activity_id(&me);
    let activity = serde_json::json!({
      "@context": "https://www.w3.org/ns/activitystreams",
      "id": id,
      "type": "Follow",
      "actor": me,
      "object": target_actor,
      "to": [target_actor],
    });
    let bytes = serde_json::to_vec(&activity)?;

    let act_id = activity.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if !act_id.is_empty() {
        let _ = state.social.store_outbox(act_id, bytes.clone());
    }
    let _ = state
        .social
        .set_following(target_actor, crate::social_db::FollowingStatus::Pending);
    let _pending = state
        .queue
        .enqueue_activity(bytes, vec![target_actor.to_string()])
        .await?;
    Ok(())
}

fn infer_http_base_from_relay_ws(relay_ws: &str) -> Result<String> {
    let relay_ws = relay_ws.trim();
    if relay_ws.is_empty() {
        anyhow::bail!("relay_ws empty and public_base_url not provided");
    }

    let (scheme, rest) = if let Some(r) = relay_ws.strip_prefix("wss://") {
        ("https://", r)
    } else if let Some(r) = relay_ws.strip_prefix("ws://") {
        ("http://", r)
    } else if let Some(r) = relay_ws.strip_prefix("https://") {
        ("https://", r)
    } else if let Some(r) = relay_ws.strip_prefix("http://") {
        ("http://", r)
    } else {
        anyhow::bail!("relay_ws must start with ws:// or wss:// (got: {relay_ws})");
    };

    let host_port = rest.split('/').next().unwrap_or("").trim();
    if host_port.is_empty() {
        anyhow::bail!("relay_ws missing host (got: {relay_ws})");
    }
    Ok(format!("{scheme}{}", host_port.trim_end_matches('/')))
}

fn random_token() -> String {
    let mut b = [0u8; 32];
    use rand::RngCore as _;
    rand::rngs::OsRng.fill_bytes(&mut b);
    b.iter().map(|v| format!("{v:02x}")).collect()
}

async fn post_signed_move_notice(
    client: &reqwest::Client,
    private_key_pem: &str,
    key_id: &str,
    relay_base: &str,
    body: Vec<u8>,
) -> Result<()> {
    let url = format!(
        "{}/_fedi3/relay/move_notice",
        relay_base.trim_end_matches('/')
    );
    let uri: Uri = url.parse().context("parse move_notice uri")?;
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        http::header::ACCEPT,
        HeaderValue::from_static("application/json"),
    );

    sign_request_rsa_sha256(
        private_key_pem,
        key_id,
        &HttpMethod::POST,
        &uri,
        &mut headers,
        &body,
        &["(request-target)", "host", "date", "digest"],
    )?;

    let build_req = || {
        let mut req = client.post(url.clone()).body(body.clone());
        for (k, v) in headers.iter() {
            req = req.header(k, v);
        }
        req
    };
    let _ = send_with_retry(build_req, 3).await;
    Ok(())
}
