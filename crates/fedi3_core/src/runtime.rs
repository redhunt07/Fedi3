/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use crate::ap::{handle_request, ApConfig, ApState, GlobalIngestPolicy, InboxRateLimits};
use crate::delivery::Delivery;
use crate::delivery_queue::{DeliveryQueue, PostDeliveryMode, QueueSettings};
use crate::http_retry::send_with_retry;
use crate::http_sig::{sign_request_rsa_sha256, KeyResolver};
use crate::keys::{default_data_dir, did_from_public_key_pem, load_or_generate_identity};
use crate::nat::UpnpController;
use crate::net_metrics::NetMetrics;
use crate::object_fetch::ObjectFetchWorker;
use crate::p2p::{self, P2pConfig};
use crate::social_db::SocialDb;
use anyhow::{Context, Result};
use axum::{routing::any, Router};
use http::{HeaderMap, HeaderValue, Method as HttpMethod, Uri};
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use tokio::sync::{watch, Mutex as TokioMutex};
use tower::util::BoxCloneService;
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};
use urlencoding::encode;

static HANDLE_SEQ: AtomicU64 = AtomicU64::new(1);

struct RunningCore {
    shutdown_tx: watch::Sender<bool>,
    join: Option<thread::JoinHandle<()>>,
}

#[derive(Debug, serde::Deserialize)]
struct RelayP2pInfraResponse {
    peer_id: Option<String>,
    multiaddrs: Vec<String>,
}

async fn maybe_seed_p2p_from_relay(
    cfg: &CoreStartConfig,
    http: &reqwest::Client,
    p2p_cfg: &mut P2pConfig,
) {
    if !p2p_cfg.enable {
        return;
    }
    let needs_relay = p2p_cfg
        .relay_reserve
        .as_ref()
        .map(|v| v.is_empty())
        .unwrap_or(true);
    let needs_bootstrap = p2p_cfg
        .bootstrap
        .as_ref()
        .map(|v| v.is_empty())
        .unwrap_or(true);
    if !needs_relay && !needs_bootstrap {
        return;
    }
    let Ok(base) = infer_http_base_from_relay_ws(&cfg.relay_ws) else {
        return;
    };
    let url = format!("{}/_fedi3/relay/p2p_infra", base.trim_end_matches('/'));
    let resp = match http.get(url).send().await {
        Ok(v) => v,
        Err(_) => return,
    };
    if !resp.status().is_success() {
        return;
    }
    let Ok(info) = resp.json::<RelayP2pInfraResponse>().await else {
        return;
    };
    let mut addrs = info
        .multiaddrs
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        return;
    }
    addrs.sort();
    addrs.dedup();
    if needs_relay {
        p2p_cfg.relay_reserve = Some(addrs.clone());
    }
    if needs_bootstrap {
        p2p_cfg.bootstrap = Some(addrs);
    }
}

static REGISTRY: Mutex<Vec<(u64, RunningCore)>> = Mutex::new(Vec::new());

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[derive(Clone, serde::Deserialize)]
pub struct CoreStartConfig {
    pub username: String,
    pub domain: String,
    #[serde(default, alias = "base_url")]
    pub public_base_url: String,
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
    #[serde(default)]
    pub upnp_port_start: Option<u16>,
    #[serde(default)]
    pub upnp_port_end: Option<u16>,
    #[serde(default)]
    pub upnp_lease_secs: Option<u32>,
    #[serde(default)]
    pub upnp_timeout_secs: Option<u64>,
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
    /// Optional list of actor handles/URLs to follow once (bootstrap social/federated timelines).
    #[serde(default)]
    pub bootstrap_follow_actors: Option<Vec<String>>,
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
            bootstrap_follow_actors: None,
            http_timeout_secs: None,
            max_body_bytes: None,
            post_delivery_mode: None,
            p2p_relay_fallback_secs: None,
            p2p_cache_ttl_secs: None,
            upnp_port_start: None,
            upnp_port_end: None,
            upnp_lease_secs: None,
            upnp_timeout_secs: None,
        }
    }
}

pub fn start(cfg: CoreStartConfig) -> Result<u64> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("info".parse().unwrap()),
        )
        .try_init()
        .ok();

    let handle = HANDLE_SEQ.fetch_add(1, Ordering::Relaxed);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let join = thread::spawn(move || {
        if let Err(e) = run_core(cfg, shutdown_rx) {
            error!("core runtime failed: {e:#}");
        }
    });

    let mut reg = REGISTRY.lock().unwrap();
    reg.push((
        handle,
        RunningCore {
            shutdown_tx,
            join: Some(join),
        },
    ));

    Ok(handle)
}

pub fn stop(handle: u64) -> Result<()> {
    let running = {
        let mut reg = REGISTRY.lock().unwrap();
        let idx = reg
            .iter()
            .position(|(h, _)| *h == handle)
            .context("invalid handle")?;
        let (_, mut running) = reg.swap_remove(idx);
        let _ = running.shutdown_tx.send(true);
        // Join in background to avoid blocking the UI thread.
        running.join.take()
    };

    if let Some(j) = running {
        thread::spawn(move || {
            let _ = j.join();
        });
    }
    Ok(())
}

fn run_core(cfg: CoreStartConfig, mut shutdown_rx: watch::Receiver<bool>) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;

    rt.block_on(async move {
        let public_base_url = if cfg.public_base_url.trim().is_empty() {
            infer_http_base_from_relay_ws(&cfg.relay_ws)?
        } else {
            cfg.public_base_url.trim_end_matches('/').to_string()
        };

        let data_dir = if let Some(dir) = &cfg.data_dir {
            std::path::PathBuf::from(dir)
        } else {
            default_data_dir()?.join("app").join(&cfg.username)
        };

        let identity = load_or_generate_identity(&data_dir)?;
        info!("identity dir: {}", data_dir.display());
        let did = did_from_public_key_pem(&identity.public_key_pem);
        let net = Arc::new(NetMetrics::new());
        let mut p2p_cfg: P2pConfig = cfg.p2p.clone().unwrap_or_default();
        if cfg.p2p.is_none() {
            // Enable P2P by default for client cores to populate the social timeline.
            p2p_cfg.enable = true;
        }
        let p2p_keypair = if p2p_cfg.enable {
            Some(p2p::load_or_generate_keypair(&data_dir)?)
        } else {
            None
        };
        let p2p_peer_id = p2p_keypair.as_ref().map(p2p::peer_id_string);
        let mut p2p_peer_addrs = Vec::new();

        let db_path = data_dir.join("fedi3.db");
        let queue = Arc::new(DeliveryQueue::open(&db_path)?);
        let social = Arc::new(SocialDb::open(&db_path)?);
        let http_timeout_secs = cfg.http_timeout_secs.unwrap_or(30).clamp(5, 120);
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(http_timeout_secs))
            .build()
            .context("build http client")?;

        maybe_seed_p2p_from_relay(&cfg, &http, &mut p2p_cfg).await;

        if p2p_cfg.enable {
            if let Some(list) = p2p_cfg.announce.as_ref() {
                p2p_peer_addrs = list
                    .iter()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            if p2p_peer_addrs.is_empty() {
                if let Some(list) = p2p_cfg.relay_reserve.as_ref() {
                    p2p_peer_addrs = list
                        .iter()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
            }
        }

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

        let p2p_cache_ttl_secs = cfg
            .p2p_cache_ttl_secs
            .unwrap_or(QueueSettings::default().p2p_cache_ttl_secs)
            .clamp(60, 90 * 24 * 3600);

        let ap_cfg = ApConfig {
            username: cfg.username.clone(),
            domain: cfg.domain.clone(),
            public_base_url: public_base_url.clone(),
            relay_base_url: infer_http_base_from_relay_ws(&cfg.relay_ws).ok(),
            relay_token: cfg.relay_token.clone(),
            public_key_pem: identity.public_key_pem.clone(),
            also_known_as: Vec::new(),
            p2p_peer_id: p2p_peer_id.clone(),
            p2p_peer_addrs: p2p_peer_addrs.clone(),
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

        let key_id = format!(
            "{}/users/{}#main-key",
            public_base_url.trim_end_matches('/'),
            cfg.username
        );
        let key_id_for_fetch = key_id.clone();

        let internal_token = cfg.internal_token.clone().unwrap_or_else(random_token);
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
        if let Some(fallback_secs) = cfg.p2p_relay_fallback_secs {
            queue_settings.p2p_relay_fallback_secs = fallback_secs.clamp(0, 600);
        }
        queue_settings.p2p_cache_ttl_secs = p2p_cache_ttl_secs;

        let upnp_range = match (cfg.upnp_port_start, cfg.upnp_port_end) {
            (Some(start), Some(end)) if start > 0 && start <= end => Some(start..=end),
            _ => None,
        };
        let upnp_lease_secs = cfg.upnp_lease_secs.unwrap_or(3600);
        let upnp_timeout_secs = cfg.upnp_timeout_secs.unwrap_or(10);
        let upnp_description = format!("{} UPnP", cfg.username);
        let upnp_controller = UpnpController::new(
            upnp_range,
            upnp_lease_secs,
            upnp_description,
            Duration::from_secs(upnp_timeout_secs),
        );

        let state = ApState {
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
            max_date_skew: Duration::from_secs(cfg.max_date_skew_secs.unwrap_or(3600)),
            data_dir: data_dir.clone(),
            media_cfg,
            media_backend: Arc::from(media_backend),
            net: net.clone(),
            ui_events,
            upnp: Arc::new(TokioMutex::new(upnp_controller)),
            p2p_cfg: p2p_cfg.clone(),
            internal_token,
            global_ingest,
            post_delivery_mode: queue_settings.post_delivery_mode,
            inbox_limits: InboxRateLimits {
                max_reqs_per_min: 120,
                max_bytes_per_min: 2 * 1024 * 1024,
                max_reqs_per_day: 20_000,
                max_bytes_per_day: 200 * 1024 * 1024,
            },
            inbox_limiter: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        };

        let max_body_bytes = cfg
            .max_body_bytes
            .unwrap_or(20 * 1024 * 1024)
            .clamp(1 * 1024 * 1024, 100 * 1024 * 1024);

        if p2p_cfg.enable {
            if let Some(keypair) = p2p_keypair.clone() {
                if let Some(relay_reserve) = p2p_cfg.relay_reserve.clone() {
                    state
                        .delivery
                        .set_mailbox_targets_from_relay_reserve(relay_reserve)
                        .await;
                }

                let p2p_handler_state = state.clone();
                let p2p_handler = Router::new()
                    .fallback(any(move |req| {
                        let st = p2p_handler_state.clone();
                        async move { handle_request(&st, req).await }
                    }))
                    .layer(axum::extract::DefaultBodyLimit::max(max_body_bytes))
                    .layer(TraceLayer::new_for_http());

                let self_actor_url = format!(
                    "{}/users/{}",
                    public_base_url_norm.trim_end_matches('/'),
                    cfg.username
                );
                let p2p_handle = p2p::start_p2p(
                    p2p_cfg.clone(),
                    keypair,
                    did.clone(),
                    self_actor_url,
                    state.internal_token.clone(),
                    state.private_key_pem.clone(),
                    state.social.clone(),
                    state.net.clone(),
                    p2p_handler.clone(),
                    shutdown_rx.clone(),
                )?;
                state.delivery.set_p2p(p2p_handle).await;

                let webrtc_handler = Arc::new(TokioMutex::new(BoxCloneService::new(p2p_handler)));
                let relay_base = state
                    .cfg
                    .relay_base_url
                    .clone()
                    .unwrap_or_else(|| public_base_url_norm.clone());
                let webrtc_handle = crate::webrtc_p2p::start_webrtc(
                    p2p_cfg.clone(),
                    p2p_peer_id.clone().unwrap_or_default(),
                    relay_base,
                    state.private_key_pem.clone(),
                    key_id.clone(),
                    webrtc_handler,
                    state.http.clone(),
                    shutdown_rx.clone(),
                    state.net.clone(),
                )?;
                state.delivery.set_webrtc(webrtc_handle).await;

                crate::p2p_sync::start_p2p_sync_worker(
                    state.clone(),
                    p2p_cfg.clone(),
                    shutdown_rx.clone(),
                );
            } else {
                warn!("p2p enabled but keypair missing");
            }
        }

        // Seed relay registry and start relay discovery sync.
        let _ = state
            .social
            .upsert_relay_entry(
                state.cfg.relay_base_url.as_deref().unwrap_or(&public_base_url),
                Some(&cfg.relay_ws),
                "self",
            );
        crate::relay_sync::start_relay_sync_worker(state.clone(), shutdown_rx.clone());
        crate::ap::start_chat_retry_worker(state.clone(), shutdown_rx.clone());

        crate::legacy_sync::start_legacy_sync_worker(
            state.clone(),
            state.delivery.clone(),
            state.social.clone(),
            state.http.clone(),
            shutdown_rx.clone(),
        );

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
                let last_ok_ms = social
                    .get_local_meta("legacy_sync_last_ok_ms")
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
                let include_fedi3 =
                    last_ok_ms == 0 || now_ms.saturating_sub(last_ok_ms) > 7 * 24 * 60 * 60 * 1000;
                match crate::legacy_sync::run_legacy_sync_now(&st, 10, 400, include_fedi3).await {
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

        crate::storage_gc::start_storage_gc_worker(
            storage_cfg,
            state.social.clone(),
            data_dir.clone(),
            shutdown_rx.clone(),
        );

        // One-time best-effort backfill: ensure our own older public outbox items appear in the
        // local DHT/global timeline (useful after upgrades).
        {
            let social = state.social.clone();
            tokio::spawn(async move {
                let _ = tokio::task::spawn_blocking(move || social.backfill_global_feed_from_outbox(200)).await;
            });
        }

        let key_id_for_queue = key_id.clone();
        queue.start_worker(
            shutdown_rx.clone(),
            state.delivery.clone(),
            state.private_key_pem.clone(),
            key_id_for_queue,
            queue_settings,
            state.ui_events.clone(),
        );

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

        // Optional bootstrap follows (idempotent): used to prefill social/federated timelines.
        {
            let mut bootstrap = cfg
                .bootstrap_follow_actors
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>();
            if bootstrap.is_empty() {
                if cfg.username.trim().eq_ignore_ascii_case("announce") {
                    bootstrap = vec![
                        "@redhunt07@www.foxyhole.io".to_string(),
                        "@engineering@newsmast.community".to_string(),
                        "@mullvadnet@mastodon.online".to_string(),
                        "@omgubuntu@floss.social".to_string(),
                        "@tassoman@misskey.social".to_string(),
                        "@informapirata@poliverso.org".to_string(),
                        "@lealternative@mastodon.uno".to_string(),
                        "@fsf@hostux.social".to_string(),
                        "@informapirata@mastodon.uno".to_string(),
                    ];
                } else {
                    bootstrap = vec![format!("{public_base_url_norm}/users/announce")];
                }
            }
            bootstrap.sort();
            bootstrap.dedup();
            if !bootstrap.is_empty() {
                let list_key = bootstrap.join(",");
                let prev = state.social.get_local_meta("bootstrap_follow_list").ok().flatten();
                if prev.as_deref() != Some(list_key.as_str()) {
                    let _ = state.social.set_local_meta("bootstrap_follow_list", &list_key);
                    let st = state.clone();
                    tokio::spawn(async move {
                        for raw in bootstrap {
                            if let Some(actor_url) = resolve_actor_input_http(&st, &raw).await {
                                if let Err(e) = ensure_follow_actor(&st, &actor_url).await {
                                    error!("bootstrap follow failed ({actor_url}): {e:#}");
                                }
                            }
                        }
                    });
                }
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

        state.object_fetch.start_with_signing(
            shutdown_rx.clone(),
            state.social.clone(),
            state.http.clone(),
            Some(crate::object_fetch::SignedFetchConfig {
                private_key_pem: state.private_key_pem.clone(),
                key_id: key_id_for_fetch,
            }),
        );

        let state_for_router = state.clone();
        let router = Router::new().fallback(any(move |req| {
            let st = state_for_router.clone();
            async move { handle_request(&st, req).await }
        }))
        .layer(axum::extract::DefaultBodyLimit::max(max_body_bytes))
        .layer(TraceLayer::new_for_http());

        let addr: SocketAddr = cfg.bind.parse().context("parse bind")?;
        let listener = tokio::net::TcpListener::bind(addr).await.context("bind")?;
        info!("core local server http://{addr}");

        let (server_shutdown_tx, mut server_shutdown_rx) = watch::channel(false);
        let server = tokio::spawn(async move {
            let shutdown = async move {
                let _ = server_shutdown_rx.changed().await;
            };
            axum::serve(listener, router.into_make_service())
                .with_graceful_shutdown(shutdown)
                .await
        });

        let relay_ws = cfg.relay_ws.clone();
        let relay_token = cfg
            .relay_token
            .clone()
            .or_else(|| std::env::var("FEDI3_RELAY_TOKEN").ok())
            .unwrap_or_default();
        let relay_token = relay_token.trim().to_string();
        if relay_token.len() < 16 {
            anyhow::bail!("relay_token missing/too short (min 16 chars); set it in the app settings");
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
        let router2 = Router::new().fallback(any(move |req| {
            let st = state_for_tunnel.clone();
            async move { handle_request(&st, req).await }
        }))
        .layer(axum::extract::DefaultBodyLimit::max(max_body_bytes))
        .layer(TraceLayer::new_for_http());

        let shutdown_for_tunnel = shutdown_rx.clone();
        let net_for_tunnel = net.clone();
        let tunnel = tokio::spawn(async move {
            crate::tunnel::run_tunnel_with_shutdown(&user, &relay_ws, &relay_token, router2, shutdown_for_tunnel, net_for_tunnel).await
        });

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

        let _ = tunnel.await;
        let _ = server.await;
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

    let mut cursor: Option<i64> = None;
    let mut all: Vec<String> = Vec::new();
    loop {
        let page = state.social.list_followers(200, cursor)?;
        all.extend(page.items);
        cursor = page.next.and_then(|v| v.parse::<i64>().ok());
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

async fn resolve_actor_input_http(state: &ApState, input: &str) -> Option<String> {
    let raw = input.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.starts_with("http://") || raw.starts_with("https://") {
        return Some(raw.to_string());
    }
    let handle = raw.trim_start_matches('@');
    let mut parts = handle.split('@');
    let user = parts.next()?.trim();
    let domain = parts.next()?.trim();
    if user.is_empty() || domain.is_empty() {
        return None;
    }
    let resource = format!("acct:{user}@{domain}");
    let url = format!("https://{domain}/.well-known/webfinger?resource={}", encode(&resource));
    let resp = state.http.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: serde_json::Value = resp.json().await.ok()?;
    let links = v.get("links")?.as_array()?;
    for link in links {
        let rel = link.get("rel").and_then(|v| v.as_str()).unwrap_or("");
        if rel != "self" {
            continue;
        }
        let href = link.get("href").and_then(|v| v.as_str()).unwrap_or("").trim();
        if href.is_empty() {
            continue;
        }
        let t = link.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if t.contains("application/activity+json") || t.contains("application/ld+json") || t.is_empty() {
            return Some(href.to_string());
        }
    }
    None
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
