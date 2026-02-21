/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use axum::{routing::any, Router};
use fedi3_core::ap::{handle_request, ApConfig, ApState, GlobalIngestPolicy, InboxRateLimits};
use fedi3_core::delivery::Delivery;
use fedi3_core::delivery_queue::{DeliveryQueue, QueueSettings};
use fedi3_core::http_sig::KeyResolver;
use fedi3_core::keys::{default_data_dir, load_or_generate_identity};
use fedi3_core::media_backend::{build_media_backend, MediaConfig};
use fedi3_core::nat::UpnpController;
use fedi3_core::net_metrics::NetMetrics;
use fedi3_core::object_fetch::ObjectFetchWorker;
use fedi3_core::social_db::SocialDb;
use fedi3_core::ui_events::UiEvent;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("info".parse().unwrap()),
        )
        .init();

    let username = std::env::var("FEDI3_USER").unwrap_or_else(|_| "alice".to_string());
    let domain = std::env::var("FEDI3_DOMAIN").unwrap_or_else(|_| "example.invalid".to_string());
    let base_url =
        std::env::var("FEDI3_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8787".to_string());
    let relay_ws =
        std::env::var("FEDI3_RELAY_WS").unwrap_or_else(|_| "ws://127.0.0.1:8787".to_string());
    let relay_token = std::env::var("FEDI3_RELAY_TOKEN").unwrap_or_else(|_| "devtoken".to_string());
    let bind = std::env::var("FEDI3_BIND").unwrap_or_else(|_| "127.0.0.1:8788".to_string());

    let data_dir = default_data_dir()?.join("dev_client").join(&username);
    let identity = load_or_generate_identity(&data_dir)?;
    info!("identity dir: {}", data_dir.display());

    let cfg = ApConfig {
        username: username.clone(),
        domain,
        public_base_url: base_url.clone(),
        relay_base_url: Some(base_url.clone()),
        relay_token: Some(relay_token.clone()),
        public_key_pem: identity.public_key_pem.clone(),
        also_known_as: Vec::new(),
        p2p_peer_id: None,
        p2p_peer_addrs: Vec::new(),
        display_name: None,
        summary: None,
        icon_url: None,
        icon_media_type: None,
        image_url: None,
        image_media_type: None,
        profile_fields: Vec::new(),
        manually_approves_followers: false,
        published_ms: None,
        blocked_domains: Vec::new(),
        blocked_actors: Vec::new(),
        p2p_cache_ttl_secs: QueueSettings::default().p2p_cache_ttl_secs,
    };

    let db_path = data_dir.join("fedi3.db");
    let queue = Arc::new(DeliveryQueue::open(&db_path)?);
    let social = Arc::new(SocialDb::open(&db_path)?);
    let net = Arc::new(NetMetrics::new());
    let (ui_events, _) = tokio::sync::broadcast::channel::<UiEvent>(128);

    let http = reqwest::Client::new();
    let (media_cfg, media_backend) =
        build_media_backend(MediaConfig::default(), data_dir.clone(), http.clone())?;

    let state = ApState {
        cfg: cfg.clone(),
        private_key_pem: identity.private_key_pem.clone(),
        key_resolver: Arc::new(KeyResolver::new()),
        delivery: Arc::new(Delivery::new()),
        queue: queue.clone(),
        social: social.clone(),
        http: http.clone(),
        object_fetch: ObjectFetchWorker::default(),
        max_date_skew: Duration::from_secs(3600),
        data_dir: data_dir.clone(),
        media_cfg,
        media_backend: Arc::from(media_backend),
        net: net.clone(),
        ui_events,
        upnp: Arc::new(tokio::sync::Mutex::new(UpnpController::new(
            None,
            3600,
            "dev-client".to_string(),
            Duration::from_secs(5),
        ))),
        p2p_cfg: fedi3_core::p2p::P2pConfig::default(),
        internal_token: random_token(),
        global_ingest: GlobalIngestPolicy {
            max_items_per_actor_per_min: 20,
            max_bytes_per_actor_per_min: 256 * 1024,
        },
        post_delivery_mode: QueueSettings::default().post_delivery_mode,
        inbox_limits: InboxRateLimits {
            max_reqs_per_min: 120,
            max_bytes_per_min: 2 * 1024 * 1024,
            max_reqs_per_day: 20_000,
            max_bytes_per_day: 200 * 1024 * 1024,
        },
        inbox_limiter: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
    };

    // Start queue worker (dev). Shutdown isn't wired here; stop the process to stop.
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let key_id = format!(
        "{}/users/{}#main-key",
        cfg.public_base_url.trim_end_matches('/'),
        username
    );
    queue.start_worker(
        shutdown_rx.clone(),
        state.delivery.clone(),
        state.private_key_pem.clone(),
        key_id,
        QueueSettings::default(),
        state.ui_events.clone(),
    );
    state.object_fetch.start(
        shutdown_rx.clone(),
        state.social.clone(),
        state.http.clone(),
    );

    let state_for_router = state.clone();
    let router = Router::new().fallback(any(move |req| async move {
        handle_request(&state_for_router, req).await
    }));

    let addr: SocketAddr = bind.parse()?;
    info!("local AP server on http://{addr} (debug)");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let server =
        tokio::spawn(async move { axum::serve(listener, router.into_make_service()).await });

    let state_for_tunnel = state.clone();
    let router2 = Router::new().fallback(any(move |req| async move {
        handle_request(&state_for_tunnel, req).await
    }));
    let tunnel = tokio::spawn(async move {
        fedi3_core::tunnel::run_tunnel(&username, &relay_ws, &relay_token, router2).await
    });

    let _ = tokio::try_join!(
        async {
            server.await??;
            Ok::<(), anyhow::Error>(())
        },
        async {
            tunnel.await??;
            Ok::<(), anyhow::Error>(())
        }
    )?;

    Ok(())
}

fn random_token() -> String {
    let mut b = [0u8; 32];
    use rand::RngCore as _;
    rand::rngs::OsRng.fill_bytes(&mut b);
    b.iter().map(|v| format!("{v:02x}")).collect()
}
