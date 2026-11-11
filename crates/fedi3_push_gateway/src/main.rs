/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

//! Self-hosted Web Push gateway for Fedi3 mobile wakeups.
//!
//! The relay POSTs an opaque wakeup to `/push`; this service validates the
//! destination and either encrypts `{"event":"sync"}` for a Web Push
//! subscription (VAPID) or publishes it to a topic-style endpoint such as
//! ntfy. It never sees message contents.

mod webpush;

use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header::AUTHORIZATION, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use subtle::ConstantTimeEq;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
struct Config {
    token: String,
    vapid: Option<Arc<webpush::VapidKey>>,
    subject: String,
    providers: Vec<String>,
    coalesce: Duration,
    client: reqwest::Client,
}

struct AppState {
    config: Config,
    coalescer: Mutex<HashMap<String, Instant>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WakeRequest {
    provider: String,
    token: String,
    event: String,
    ttl_secs: u64,
    #[serde(default)]
    webpush_keys: Option<webpush::WebPushKeys>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    if env::args().any(|arg| arg == "--generate-vapid") {
        let vapid = webpush::VapidKey::generate();
        println!(
            "FEDI3_PUSH_GATEWAY_VAPID_PRIVATE_KEY={}",
            vapid.private_b64()
        );
        println!("FEDI3_RELAY_PUSH_VAPID_PUBLIC_KEY={}", vapid.public_b64());
        return Ok(());
    }

    let bind: SocketAddr = env::var("FEDI3_PUSH_GATEWAY_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8090".into())
        .parse()
        .context("invalid FEDI3_PUSH_GATEWAY_BIND")?;
    // Per-relay mount point: a dedicated subdomain can keep `/push`, while a
    // relay without DNS subdomains can mount it under its own domain path.
    let route_path = env::var("FEDI3_PUSH_GATEWAY_PATH").unwrap_or_else(|_| "/push".into());
    if !route_path.starts_with('/') {
        bail!("FEDI3_PUSH_GATEWAY_PATH must start with /");
    }
    let token = env::var("FEDI3_PUSH_GATEWAY_TOKEN")
        .context("FEDI3_PUSH_GATEWAY_TOKEN is required")?;
    if token.len() < 32 {
        bail!("FEDI3_PUSH_GATEWAY_TOKEN must be at least 32 characters");
    }
    let vapid = env::var("FEDI3_PUSH_GATEWAY_VAPID_PRIVATE_KEY")
        .ok()
        .filter(|key| !key.is_empty())
        .map(|key| webpush::VapidKey::from_private_b64(&key))
        .transpose()?
        .map(Arc::new);
    if vapid.is_none() {
        warn!("VAPID key not configured; Web Push distributors are rejected, plain endpoints still work");
    }
    let subject = env::var("FEDI3_PUSH_GATEWAY_VAPID_SUBJECT")
        .unwrap_or_else(|_| "mailto:admin@localhost".into());
    if !subject.starts_with("mailto:") && !subject.starts_with("https://") {
        bail!("FEDI3_PUSH_GATEWAY_VAPID_SUBJECT must be a mailto: or https: URI");
    }
    let providers: Vec<String> = env::var("FEDI3_PUSH_GATEWAY_PROVIDERS")
        .unwrap_or_else(|_| "unifiedpush".into())
        .split(',')
        .map(str::trim)
        .filter(|provider| !provider.is_empty())
        .map(str::to_owned)
        .collect();
    if providers.iter().any(|provider| provider != "unifiedpush") {
        warn!("only unifiedpush is implemented; other providers are accepted and ignored");
    }
    let coalesce = Duration::from_secs(
        env::var("FEDI3_PUSH_GATEWAY_COALESCE_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(30),
    );
    let timeout = Duration::from_secs(
        env::var("FEDI3_PUSH_GATEWAY_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(10),
    );
    let client = fedi3_protocol::net::guard_client(reqwest::Client::builder(), false)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout)
        .build()
        .context("failed to build HTTP client")?;

    info!(
        %bind,
        public_vapid_key = vapid.as_ref().map(|key| key.public_b64()),
        ?providers,
        "push gateway starting"
    );

    let state = Arc::new(AppState {
        config: Config {
            token,
            vapid,
            subject,
            providers,
            coalesce,
            client,
        },
        coalescer: Mutex::new(HashMap::new()),
    });
    let app = Router::new()
        .route("/healthz", get(health))
        .route("/vapid", get(vapid_key))
        .route(&route_path, post(wake))
        .layer(DefaultBodyLimit::max(8 * 1024))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!(address = %listener.local_addr()?, "push gateway listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
    info!("push gateway shutting down");
}

async fn health() -> &'static str {
    "ok"
}

async fn vapid_key(State(state): State<Arc<AppState>>) -> Response {
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "vapid_public_key": state.config.vapid.as_ref().map(|key| key.public_b64())
        })),
    )
        .into_response()
}

async fn wake(State(state): State<Arc<AppState>>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(raw) = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return (StatusCode::UNAUTHORIZED, "missing bearer token").into_response();
    };
    let Some(token) = raw.strip_prefix("Bearer ") else {
        return (StatusCode::UNAUTHORIZED, "missing bearer token").into_response();
    };
    if token.as_bytes().ct_eq(state.config.token.as_bytes()).unwrap_u8() != 1 {
        return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
    }
    let request: WakeRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return (StatusCode::BAD_REQUEST, format!("invalid request: {error}")).into_response();
        }
    };
    if request.event != "sync" {
        return (StatusCode::BAD_REQUEST, "unsupported event").into_response();
    }
    if !state
        .config
        .providers
        .iter()
        .any(|provider| provider == &request.provider)
    {
        return (StatusCode::UNPROCESSABLE_ENTITY, "unsupported provider").into_response();
    }
    if request.provider != "unifiedpush" {
        return (StatusCode::ACCEPTED, "accepted (provider not implemented)").into_response();
    }
    let endpoint = match webpush::validate_endpoint(&request.token) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("invalid endpoint: {error}"),
            )
                .into_response();
        }
    };

    let now = Instant::now();
    {
        let mut seen = state.coalescer.lock().unwrap();
        if let Some(last) = seen.get(&request.token) {
            if now.duration_since(*last) < state.config.coalesce {
                return (StatusCode::ACCEPTED, "coalesced").into_response();
            }
        }
        seen.insert(request.token.clone(), now);
        if seen.len() > 4096 {
            seen.retain(|_, at| now.duration_since(*at) < state.config.coalesce.saturating_mul(4));
        }
    }

    let result = match request.webpush_keys.as_ref() {
        Some(keys) => {
            let Some(vapid) = state.config.vapid.as_ref() else {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "VAPID key not configured",
                )
                    .into_response();
            };
            let target = match webpush::target(endpoint, keys) {
                Ok(target) => target,
                Err(error) => {
                    return (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        format!("invalid webpush keys: {error}"),
                    )
                        .into_response();
                }
            };
            let encrypted = match webpush::encrypt(&target, br#"{"event":"sync"}"#) {
                Ok(encrypted) => encrypted,
                Err(error) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("encryption failed: {error}"),
                    )
                        .into_response();
                }
            };
            webpush::send_webpush(
                &state.config.client,
                &target,
                &encrypted,
                vapid,
                &state.config.subject,
                request.ttl_secs,
            )
            .await
        }
        None => webpush::send_plain(&state.config.client, &endpoint, request.ttl_secs).await,
    };

    match result {
        Ok(status) if status.is_success() => (StatusCode::ACCEPTED, "sent").into_response(),
        Ok(status) if status.as_u16() == 404 || status.as_u16() == 410 => {
            (StatusCode::GONE, "endpoint gone").into_response()
        }
        Ok(status) => {
            warn!(%status, "push endpoint rejected the wakeup");
            (
                StatusCode::BAD_GATEWAY,
                format!("push endpoint returned {status}"),
            )
                .into_response()
        }
        Err(error) => {
            warn!(error = %error, "push endpoint unreachable");
            (
                StatusCode::BAD_GATEWAY,
                format!("push endpoint unreachable: {error}"),
            )
                .into_response()
        }
    }
}
