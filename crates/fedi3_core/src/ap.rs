/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use axum::{
    body::Body,
    http::{header, HeaderValue, Request, Response, StatusCode},
    response::IntoResponse,
};
use anyhow::Context;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc, time::Duration};
use std::sync::atomic::Ordering;
use http::{HeaderMap, Method, Uri};
use tokio::sync::Mutex;
use fedi3_protocol::RelayHttpRequest;

use crate::delivery::{extract_recipients, is_public_activity, Delivery};
use crate::delivery_queue::{DeliveryQueue, PostDeliveryMode};
use crate::object_fetch::ObjectFetchWorker;
use crate::social_db::{FollowingStatus, SocialDb};
use crate::chat;
use crate::net_metrics::NetMetrics;
use crate::ui_events::UiEvent;
use crate::http_retry::send_with_retry_metrics;
use crate::http_sig::{
    build_signing_string, parse_signature_header, verify_date, verify_digest_if_present,
    verify_signature_rsa_sha256, KeyResolver, sign_request_rsa_sha256,
};
use crate::media_backend as media_store;
use serde_json::Value;
use tracing::warn;
use tokio::sync::broadcast;
use tokio::time::sleep;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::unfold;
use std::convert::Infallible;

#[derive(Clone)]
pub struct ApConfig {
    pub username: String,
    pub domain: String,
    pub public_base_url: String,
    pub relay_base_url: Option<String>,
    pub relay_token: Option<String>,
    pub public_key_pem: String,
    pub also_known_as: Vec<String>,
    pub p2p_peer_id: Option<String>,
    pub p2p_peer_addrs: Vec<String>,
    pub display_name: Option<String>,
    pub summary: Option<String>,
    pub icon_url: Option<String>,
    pub icon_media_type: Option<String>,
    pub image_url: Option<String>,
    pub image_media_type: Option<String>,
    pub profile_fields: Vec<ProfileField>,
    pub manually_approves_followers: bool,
    pub published_ms: Option<i64>,
    /// Block inbound/outbound interactions with these domains (exact or suffix, e.g. `example.com` or `*.example.com`).
    pub blocked_domains: Vec<String>,
    /// Block inbound/outbound interactions with these actors (exact actor id URL).
    pub blocked_actors: Vec<String>,
    /// Mailbox cache TTL for P2P store-and-forward (seconds).
    pub p2p_cache_ttl_secs: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ProfileField {
    pub name: String,
    pub value: String,
}

#[derive(Clone)]
pub struct ApState {
    pub cfg: ApConfig,
    pub private_key_pem: String,
    pub key_resolver: Arc<KeyResolver>,
    pub delivery: Arc<Delivery>,
    pub queue: Arc<DeliveryQueue>,
    pub social: Arc<SocialDb>,
    pub http: reqwest::Client,
    pub object_fetch: ObjectFetchWorker,
    pub max_date_skew: Duration,
    pub data_dir: std::path::PathBuf,
    pub media_cfg: media_store::MediaConfig,
    pub media_backend: std::sync::Arc<dyn media_store::MediaBackend>,
    pub net: Arc<NetMetrics>,
    pub ui_events: broadcast::Sender<UiEvent>,
    /// Token (best-effort) to prevent exposure of internal endpoints if the embedded server is reachable.
    pub internal_token: String,
    pub global_ingest: GlobalIngestPolicy,
    pub post_delivery_mode: PostDeliveryMode,
    pub p2p_relay_fallback: Duration,
    pub inbox_limits: InboxRateLimits,
    pub inbox_limiter: Arc<Mutex<HashMap<String, RateState>>>,
}

#[derive(Clone, Debug)]
pub struct GlobalIngestPolicy {
    pub max_items_per_actor_per_min: u32,
    pub max_bytes_per_actor_per_min: u64,
}

#[derive(Clone, Debug)]
pub struct InboxRateLimits {
    pub max_reqs_per_min: u32,
    pub max_bytes_per_min: u64,
    /// Persistent quota window (best-effort), applied after signature validation.
    pub max_reqs_per_day: u32,
    pub max_bytes_per_day: u64,
}

#[derive(Debug, Default)]
pub struct RateState {
    window_start_ms: i64,
    reqs: u32,
    bytes: u64,
}

#[derive(Serialize)]
struct Webfinger {
    subject: String,
    links: Vec<WebfingerLink>,
}

#[derive(Serialize)]
struct WebfingerLink {
    rel: String,
    #[serde(rename = "type")]
    ty: String,
    href: String,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
struct PublicKey {
    id: String,
    owner: String,
    publicKeyPem: String,
}

#[derive(Serialize)]
struct ActorEndpoints {
    #[serde(rename = "sharedInbox")]
    shared_inbox: String,
    #[serde(rename = "fedi3PeerId", skip_serializing_if = "Option::is_none")]
    fedi3_peer_id: Option<String>,
    #[serde(rename = "fedi3PeerAddrs", skip_serializing_if = "Vec::is_empty")]
    fedi3_peer_addrs: Vec<String>,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
struct Actor {
    #[serde(rename = "@context")]
    context: Vec<String>,
    id: String,
    #[serde(rename = "type")]
    ty: String,
    preferredUsername: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    published: Option<String>,
    #[serde(rename = "manuallyApprovesFollowers", skip_serializing_if = "Option::is_none")]
    manually_approves_followers: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    attachment: Vec<ActorField>,
    #[serde(rename = "alsoKnownAs", skip_serializing_if = "Vec::is_empty")]
    also_known_as: Vec<String>,
    followers: String,
    following: String,
    inbox: String,
    outbox: String,
    endpoints: ActorEndpoints,
    publicKey: PublicKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<ActorImage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<ActorImage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    discoverable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    indexable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    featured: Option<String>,
    #[serde(rename = "featuredTags", skip_serializing_if = "Option::is_none")]
    featured_tags: Option<String>,
}

#[derive(Serialize)]
struct ActorField {
    #[serde(rename = "type")]
    ty: String,
    name: String,
    value: String,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
struct ActorImage {
    #[serde(rename = "type")]
    ty: String,
    mediaType: String,
    url: String,
}

pub async fn handle_request(state: &ApState, req: Request<Body>) -> Response<Body> {
    let path = req.uri().path().to_string();
    let accept = accept_activity(req.headers());
    let resp = match (req.method().as_str(), path.as_str()) {
        ("GET", "/healthz") => simple(StatusCode::OK, "ok"),
        ("GET", "/readyz") => readyz_get(state),
        // NodeInfo/host-meta (compat).
        ("GET", "/.well-known/nodeinfo") => nodeinfo_links(&state.cfg),
        ("GET", "/nodeinfo/2.0") => nodeinfo_2_0(&state.cfg),
        ("GET", "/.well-known/host-meta") => host_meta(&state.cfg),
        ("GET", "/.well-known/host-meta.json") => host_meta_json(&state.cfg),
        ("GET", "/.well-known/links") => well_known_links(&state.cfg),
        ("GET", "/.well-known/webfinger") => webfinger(&state.cfg, req),
        ("GET", p) if p == format!("/users/{}", state.cfg.username) => actor_get(state, req, accept).await,
        ("GET", p) if p.starts_with(&format!("/users/{}/objects/", state.cfg.username)) => object_deref_get(state, req).await,
        ("GET", p) if p == format!("/users/{}/followers", state.cfg.username) => followers_get(state, req, accept).await,
        ("GET", p) if p == format!("/users/{}/following", state.cfg.username) => following_get(state, req, accept).await,
        ("GET", p) if p == format!("/users/{}/outbox", state.cfg.username) => outbox_get(state, req, accept).await,
        ("POST", p) if p == format!("/users/{}/outbox", state.cfg.username) => outbox_post(state, req).await,
        ("POST", p) if p == format!("/users/{}/media", state.cfg.username) => media_upload(state, req).await,
        ("GET", p) if p.starts_with(&format!("/users/{}/media/", state.cfg.username)) => media_get(state, req).await,
        ("GET", p) if p == format!("/users/{}/collections/featured", state.cfg.username) => empty_collection_get(state, req, accept, "featured").await,
        ("GET", p) if p == format!("/users/{}/collections/featuredTags", state.cfg.username) => empty_collection_get(state, req, accept, "featuredTags").await,
        ("GET", "/.fedi3/media") => media_p2p_get(state, req).await,
        // Endpoint interno per la UI: fetch di un oggetto già in cache/DB.
        // Esempio: `GET /_fedi3/object?url=<urlencoded>`
        ("GET", "/_fedi3/object") => object_get(state, req).await,
        ("GET", "/_fedi3/blocks") => blocks_list(state, req).await,
        ("POST", "/_fedi3/blocks") => blocks_update(state, req).await,
        ("GET", "/_fedi3/audit/recent") => audit_recent(state, req).await,
        // Endpoint interno per il relay/telemetria: info minime sul peer.
        ("GET", "/_fedi3/hello") => hello_get(state).await,
        ("GET", "/_fedi3/relays") => relays_list_get(state, req).await,
        ("POST", "/_fedi3/relays") => relays_update_post(state, req).await,
        ("POST", "/_fedi3/relays/refresh") => relays_refresh_post(state, req).await,
        ("POST", "/_fedi3/profile/refresh") => profile_refresh_post(state, req).await,
        ("GET", "/.fedi3/relays") => relays_public_get(state).await,
        // Migration helper (internal).
        ("GET", "/_fedi3/migration/status") => migration_status(state, req).await,
        ("POST", "/_fedi3/migration/legacy_aliases") => migration_legacy_aliases_set(state, req).await,
        // UI timelines (internal).
        ("GET", "/_fedi3/timeline/home") => timeline_home(state, req).await,
        ("GET", "/_fedi3/timeline/unified") => timeline_unified(state, req).await,
        ("GET", "/_fedi3/timeline/federated") => timeline_federated(state, req).await,
        ("GET", "/_fedi3/timeline/dht") => global_timeline(state, req).await,
        ("GET", "/_fedi3/note/replies") => note_replies_get(state, req).await,
        ("GET", "/_fedi3/search/notes") => search_notes(state, req).await,
        ("GET", "/_fedi3/search/users") => search_users(state, req).await,
        ("GET", "/_fedi3/search/hashtags") => search_hashtags(state, req).await,
        ("GET", "/_fedi3/chat/bundle") => chat_bundle_get(state, req).await,
        ("POST", "/_fedi3/chat/inbox") => chat_inbox_post(state, req).await,
        ("GET", "/_fedi3/chat/threads") => chat_threads_get(state, req).await,
        ("GET", p) if p.starts_with("/_fedi3/chat/threads/") => chat_thread_messages_get(state, req).await,
        ("POST", "/_fedi3/chat/send") => chat_send_post(state, req).await,
        ("POST", "/_fedi3/chat/typing") => chat_typing_post(state, req).await,
        ("POST", "/_fedi3/chat/react") => chat_react_post(state, req).await,
        ("POST", "/_fedi3/chat/reactions") => chat_reactions_post(state, req).await,
        ("POST", "/_fedi3/chat/edit") => chat_edit_post(state, req).await,
        ("POST", "/_fedi3/chat/delete") => chat_delete_post(state, req).await,
        ("POST", "/_fedi3/chat/seen") => chat_seen_post(state, req).await,
        ("POST", "/_fedi3/chat/status") => chat_status_post(state, req).await,
        ("GET", "/_fedi3/chat/thread/members") => chat_thread_members_get(state, req).await,
        ("POST", "/_fedi3/chat/thread/update") => chat_thread_update_post(state, req).await,
        ("POST", "/_fedi3/chat/thread/delete") => chat_thread_delete_post(state, req).await,
        ("POST", "/_fedi3/chat/thread/members") => chat_thread_members_post(state, req).await,
        ("GET", "/_fedi3/reactions") => reactions_get(state, req).await,
        ("GET", "/_fedi3/reactions/me") => reactions_me_get(state, req).await,
        ("GET", "/_fedi3/reactions/actors") => reactions_actors_get(state, req).await,
        ("GET", "/_fedi3/notifications") => notifications_get(state, req).await,
        ("POST", "/_fedi3/sync/legacy") => legacy_sync_trigger(state, req).await,
        // Global timeline (P2P gossip) endpoints.
        ("POST", "/_fedi3/global/ingest") => global_ingest(state, req).await,
        ("GET", "/_fedi3/global/timeline") => global_timeline(state, req).await,
        // P2P discovery (internal).
        ("GET", "/_fedi3/p2p/resolve") => p2p_resolve(state, req).await,
        ("GET", "/_fedi3/p2p/resolve_did") => p2p_resolve_did(state, req).await,
        ("POST", "/_fedi3/p2p/follow") => p2p_follow(state, req).await,
        ("POST", "/_fedi3/social/follow") => p2p_follow(state, req).await,
        ("POST", "/_fedi3/p2p/unfollow") => social_unfollow(state, req).await,
        ("POST", "/_fedi3/social/unfollow") => social_unfollow(state, req).await,
        ("GET", "/_fedi3/social/status") => social_follow_status_get(state, req).await,
        ("GET", "/_fedi3/net/metrics") => net_metrics_get(state, req).await,
        ("GET", "/_fedi3/net/metrics.prom") => net_metrics_prom(state, req).await,
        ("GET", "/_fedi3/health") => core_health(state, req).await,
        ("GET", "/_fedi3/stream") => ui_stream_get(state, req).await,
        // P2P sync endpoints (peer-to-peer, public-only).
        ("GET", "/.fedi3/sync/outbox") => p2p_sync_outbox(state, req).await,
        // Delivery receipts (peer-to-peer, requires HTTP Signature).
        ("POST", "/.fedi3/receipt") => receipt_post(state, req).await,
        // Device sync endpoints (peer-to-peer, requires shared identity key).
        ("GET", "/.fedi3/device/outbox") => device_outbox(state, req).await,
        ("GET", "/.fedi3/device/inbox") => device_inbox(state, req).await,
        ("GET", "/inbox") => simple(StatusCode::METHOD_NOT_ALLOWED, "method not allowed"),
        ("POST", "/inbox") => inbox(state, req).await,
        ("GET", p) if p == format!("/users/{}/inbox", state.cfg.username) => simple(StatusCode::METHOD_NOT_ALLOWED, "method not allowed"),
        ("POST", p) if p == format!("/users/{}/inbox", state.cfg.username) => inbox(state, req).await,
        _ => simple(StatusCode::NOT_FOUND, "not found"),
    };
    add_security_headers(resp)
}

fn readyz_get(state: &ApState) -> Response<Body> {
    if state.social.health_check().is_ok() {
        simple(StatusCode::OK, "ready")
    } else {
        simple(StatusCode::SERVICE_UNAVAILABLE, "db not ready")
    }
}

fn add_security_headers(mut resp: Response<Body>) -> Response<Body> {
    let headers = resp.headers_mut();
    headers.entry("X-Content-Type-Options").or_insert(HeaderValue::from_static("nosniff"));
    headers.entry("X-Frame-Options").or_insert(HeaderValue::from_static("DENY"));
    headers.entry("Referrer-Policy").or_insert(HeaderValue::from_static("no-referrer"));
    headers
        .entry("Permissions-Policy")
        .or_insert(HeaderValue::from_static("geolocation=(), microphone=(), camera=()"));
    resp
}

async fn legacy_sync_trigger(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let query = parts.uri.query().unwrap_or("");
    let pages = query
        .split('&')
        .find(|p| p.starts_with("pages="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(6)
        .clamp(1, 40);
    let items_per_actor = query
        .split('&')
        .find(|p| p.starts_with("items="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(200)
        .clamp(20, 2000);

    let st = state.clone();
    tokio::spawn(async move {
        let _ = crate::legacy_sync::run_legacy_sync_now(&st, pages, items_per_actor).await;
    });

    simple(StatusCode::ACCEPTED, "ok")
}

async fn object_deref_get(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = verify_signature_if_present(state, &parts).await {
        return resp;
    }

    let path = parts.uri.path().to_string();
    let prefix = format!("/users/{}/objects/", state.cfg.username);
    let Some(suffix) = path.strip_prefix(&prefix) else {
        return simple(StatusCode::NOT_FOUND, "not found");
    };
    if suffix.is_empty() || suffix.contains("..") || suffix.contains('/') || suffix.contains('\\') {
        return simple(StatusCode::BAD_REQUEST, "invalid object id");
    }

    let base = state.cfg.public_base_url.trim_end_matches('/');
    let object_url = format!("{base}{prefix}{suffix}");

    match state.social.get_object_json(&object_url) {
        Ok(Some(bytes)) => (
            StatusCode::OK,
            [("Content-Type", "application/activity+json; charset=utf-8")],
            bytes,
        )
            .into_response(),
        Ok(None) => simple(StatusCode::NOT_FOUND, "not found"),
        Err(e) => simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    }
}

async fn verify_signature_if_present(state: &ApState, parts: &http::request::Parts) -> Result<(), Response<Body>> {
    // For GET requests, Signature is optional in the wild.
    let Some(sig_header) = parts
        .headers
        .get("Signature")
        .or_else(|| parts.headers.get("signature"))
        .and_then(|v| v.to_str().ok())
    else {
        return Ok(());
    };

    if let Err(e) = verify_date(&parts.headers, state.max_date_skew) {
        return Err(simple(StatusCode::UNAUTHORIZED, &format!("date invalid: {e}")));
    }

    let sig = match parse_signature_header(sig_header) {
        Ok(v) => v,
        Err(e) => return Err(simple(StatusCode::UNAUTHORIZED, &format!("bad Signature: {e}"))),
    };
    let signing_string = match build_signing_string(&parts.method, &parts.uri, &parts.headers, &sig.headers) {
        Ok(s) => s,
        Err(e) => return Err(simple(StatusCode::UNAUTHORIZED, &format!("bad signed headers: {e}"))),
    };
    let summary = match state.key_resolver.resolve_actor_summary_for_key_id(&sig.key_id).await {
        Ok(s) => s,
        Err(e) => return Err(simple(StatusCode::UNAUTHORIZED, &format!("key resolve failed: {e}"))),
    };
    if let Err(e) = verify_signature_rsa_sha256(&summary.public_key_pem, &signing_string, &sig.signature) {
        return Err(simple(StatusCode::UNAUTHORIZED, &format!("signature invalid: {e}")));
    }
    Ok(())
}

fn nodeinfo_links(cfg: &ApConfig) -> Response<Body> {
    let base = cfg.public_base_url.trim_end_matches('/');
    let body = serde_json::json!({
      "links": [{
        "rel": "http://nodeinfo.diaspora.software/ns/schema/2.0",
        "href": format!("{base}/nodeinfo/2.0")
      }]
    });
    (
        StatusCode::OK,
        [("Content-Type", "application/json; charset=utf-8"), ("Cache-Control", "no-store")],
        body.to_string(),
    )
        .into_response()
}

fn nodeinfo_2_0(cfg: &ApConfig) -> Response<Body> {
    let base = cfg.public_base_url.trim_end_matches('/');
    let body = serde_json::json!({
      "version": "2.0",
      "software": { "name": "fedi3", "version": env!("CARGO_PKG_VERSION") },
      "protocols": ["activitypub"],
      "services": { "inbound": [], "outbound": [] },
      "openRegistrations": false,
      "usage": { "users": { "total": 1 } },
      "metadata": {
        "nodeName": "Fedi3",
        "actor": format!("{base}/users/{}", cfg.username),
      }
    });
    (
        StatusCode::OK,
        [("Content-Type", "application/json; charset=utf-8"), ("Cache-Control", "no-store")],
        body.to_string(),
    )
        .into_response()
}

fn host_meta(cfg: &ApConfig) -> Response<Body> {
    // Minimal host-meta for LRDD/WebFinger discovery.
    let base = cfg.public_base_url.trim_end_matches('/');
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<XRD xmlns="http://docs.oasis-open.org/ns/xri/xrd-1.0">
  <Link rel="lrdd" type="application/jrd+json" template="{base}/.well-known/webfinger?resource={{uri}}"/>
</XRD>"#
    );
    (
        StatusCode::OK,
        [("Content-Type", "application/xrd+xml; charset=utf-8"), ("Cache-Control", "no-store")],
        xml,
    )
        .into_response()
}

fn host_meta_json(cfg: &ApConfig) -> Response<Body> {
    let base = cfg.public_base_url.trim_end_matches('/');
    let body = serde_json::json!({
      "links": [{
        "rel": "lrdd",
        "type": "application/jrd+json",
        "template": format!("{base}/.well-known/webfinger?resource={{uri}}")
      }]
    });
    (
        StatusCode::OK,
        [("Content-Type", "application/json; charset=utf-8"), ("Cache-Control", "no-store")],
        body.to_string(),
    )
        .into_response()
}

fn well_known_links(cfg: &ApConfig) -> Response<Body> {
    let base = cfg.public_base_url.trim_end_matches('/');
    let actor = format!("{base}/users/{}", cfg.username);
    let body = serde_json::json!({
      "links": [
        {
          "rel": "lrdd",
          "type": "application/jrd+json",
          "template": format!("{base}/.well-known/webfinger?resource={{uri}}")
        },
        {
          "rel": "self",
          "type": "application/activity+json",
          "href": actor
        },
        {
          "rel": "http://nodeinfo.diaspora.software/ns/schema/2.0",
          "href": format!("{base}/nodeinfo/2.0")
        }
      ]
    });
    (
        StatusCode::OK,
        [("Content-Type", "application/json; charset=utf-8"), ("Cache-Control", "no-store")],
        body.to_string(),
    )
        .into_response()
}

async fn media_upload(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b.to_vec(),
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    if bytes.is_empty() {
        return simple(StatusCode::BAD_REQUEST, "empty body");
    }
    if let Some(ip) = client_ip_from_headers(&parts.headers) {
        if let Err(resp) = inbox_rate_limit(state, &format!("ip:{ip}"), bytes.len() as u64).await {
            state.net.rate_limit_hit();
            let _ = state
                .social
                .insert_audit_event("rate_limit", None, None, None, false, Some("429"), Some("media_upload ip"));
            return resp;
        }
    }

    let filename = parts
        .headers
        .get("X-Filename")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("upload.bin")
        .to_string();
    let content_type = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let saved = if should_mirror_to_relay(state) {
        match relay_upload_and_cache(state, &filename, content_type.as_deref(), &bytes).await {
            Ok(v) => v,
            Err(e) => {
                warn!("relay mirror failed: {e:#}");
                match state
                    .media_backend
                    .save_upload(
                        &state.cfg.username,
                        &state.cfg.public_base_url,
                        &filename,
                        content_type.as_deref(),
                        &bytes,
                    )
                    .await
                {
                    Ok(v) => v,
                    Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("store failed: {e:#}")),
                }
            }
        }
    } else {
        match state
            .media_backend
            .save_upload(
                &state.cfg.username,
                &state.cfg.public_base_url,
                &filename,
                content_type.as_deref(),
                &bytes,
            )
            .await
        {
            Ok(v) => v,
            Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("store failed: {e:#}")),
        }
    };

    let mut response = saved.response.clone();
    if response.media_type.to_ascii_lowercase().starts_with("image/") {
        if let Some((w, h)) = media_store::probe_image_dimensions(&bytes) {
            response.width = Some(w);
            response.height = Some(h);
        }
        if response.blurhash.is_none() {
            response.blurhash = compute_blurhash(&bytes);
        }
    }

    let base = state.cfg.public_base_url.trim_end_matches('/');
    let me = format!("{base}/users/{}", state.cfg.username);
    let _ = state.social.upsert_media(
        &response.id,
        &response.url,
        &response.media_type,
        response.size as i64,
        saved.local_name.as_deref(),
        Some(&me),
        response.width.map(|v| v as i64),
        response.height.map(|v| v as i64),
        response.blurhash.as_deref(),
    );

    let body = serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string());
    (
        StatusCode::CREATED,
        [("Content-Type", "application/json; charset=utf-8")],
        body,
    )
        .into_response()
}

fn should_mirror_to_relay(state: &ApState) -> bool {
    if state.post_delivery_mode == PostDeliveryMode::P2pOnly {
        return false;
    }
    let backend = state
        .media_cfg
        .backend
        .as_deref()
        .unwrap_or("local")
        .to_ascii_lowercase();
    if backend == "relay" {
        return false;
    }
    let base = state.cfg.relay_base_url.as_deref().unwrap_or("").trim();
    let token = state.cfg.relay_token.as_deref().unwrap_or("").trim();
    !base.is_empty() && !token.is_empty()
}

async fn relay_upload_and_cache(
    state: &ApState,
    filename: &str,
    content_type: Option<&str>,
    bytes: &[u8],
) -> anyhow::Result<crate::media_backend::MediaSaved> {
    let relay_base = state.cfg.relay_base_url.as_deref().unwrap_or("").trim();
    let token = state.cfg.relay_token.as_deref().unwrap_or("").trim();
    if relay_base.is_empty() || token.is_empty() {
        anyhow::bail!("missing relay_base_url or relay_token");
    }
    let media_type = content_type
        .map(|s| s.to_string())
        .or_else(|| mime_guess::from_path(filename).first().map(|m| m.to_string()))
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let url = format!("{}/users/{}/media", relay_base.trim_end_matches('/'), state.cfg.username);
    let resp = state
        .http
        .post(url)
        .header("Authorization", format!("Bearer {}", token))
        .header("X-Filename", filename)
        .header(header::CONTENT_TYPE, &media_type)
        .body(bytes.to_vec())
        .send()
        .await
        .context("relay upload request failed")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("relay upload failed: {} {}", status, text);
    }
    let mut response = resp
        .json::<crate::media_backend::MediaUploadResponse>()
        .await
        .context("relay upload json")?;
    if response.media_type.trim().is_empty() {
        response.media_type = media_type.clone();
    }
    let dir = media_store::media_dir(&state.data_dir);
    std::fs::create_dir_all(&dir).ok();
    let stored_name = response.id.clone();
    let path = dir.join(&stored_name);
    let _ = std::fs::write(&path, bytes);
    Ok(crate::media_backend::MediaSaved {
        response,
        local_name: Some(stored_name),
    })
}

fn compute_blurhash(bytes: &[u8]) -> Option<String> {
    let img = image::load_from_memory(bytes).ok()?;
    let mut rgb = img.to_rgb8();
    let (mut w, mut h) = rgb.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    const MAX_DIM: u32 = 256;
    if w > MAX_DIM || h > MAX_DIM {
        let max_dim = w.max(h) as f32;
        let scale = (max_dim / MAX_DIM as f32).max(1.0);
        let new_w = ((w as f32) / scale).round().max(1.0) as u32;
        let new_h = ((h as f32) / scale).round().max(1.0) as u32;
        rgb = image::imageops::resize(&rgb, new_w, new_h, image::imageops::FilterType::Triangle);
        w = new_w;
        h = new_h;
    }
    // Keep CPU bounded for very large images.
    if (w as u64).saturating_mul(h as u64) > 12_000_000 {
        return None;
    }
    // blurhash expects pixels in RGB order.
    let pixels = rgb.into_raw();
    let expected = (w as usize)
        .saturating_mul(h as usize)
        .saturating_mul(3);
    if pixels.len() != expected {
        return None;
    }

    // `blurhash` has had occasional panics on some inputs; never crash the core due to a preview hash.
    match std::panic::catch_unwind(|| blurhash::encode(4, 3, w, h, &pixels)) {
        Ok(Ok(v)) => Some(v),
        Ok(Err(_)) => None,
        Err(_) => {
            warn!(w, h, bytes = pixels.len(), "blurhash encode panicked; skipping");
            None
        }
    }
}

async fn media_get(state: &ApState, req: Request<Body>) -> Response<Body> {
    let path = req.uri().path().to_string();
    let prefix = format!("/users/{}/media/", state.cfg.username);
    let Some(stored_name) = path.strip_prefix(&prefix) else {
        return simple(StatusCode::NOT_FOUND, "not found");
    };
    if stored_name.is_empty() || stored_name.contains("..") || stored_name.contains('/') || stored_name.contains('\\') {
        return simple(StatusCode::BAD_REQUEST, "invalid media id");
    }

    if let Ok(Some(item)) = state.social.get_media(stored_name) {
        if item.local_name.is_some() {
            if let Ok((bytes, mime)) = media_store::load_media(&state.data_dir, stored_name) {
                return (StatusCode::OK, [("Content-Type", mime)], bytes).into_response();
            }
        }
        if let Ok(Some((bytes, mime))) = fetch_and_cache_media(state, &item).await {
            return (StatusCode::OK, [("Content-Type", mime)], bytes).into_response();
        }
        if state.post_delivery_mode == PostDeliveryMode::P2pOnly {
            return simple(StatusCode::NOT_FOUND, "not found");
        }
        return (StatusCode::FOUND, [("Location", item.url)], Vec::<u8>::new()).into_response();
    }

    match media_store::load_media(&state.data_dir, stored_name) {
        Ok((bytes, mime)) => (StatusCode::OK, [("Content-Type", mime)], bytes).into_response(),
        Err(_) => simple(StatusCode::NOT_FOUND, "not found"),
    }
}

async fn fetch_and_cache_media(state: &ApState, item: &crate::social_db::MediaItem) -> Result<Option<(Vec<u8>, String)>, anyhow::Error> {
    let url = item.url.trim();
    if url.is_empty() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    let mut mime = item.media_type.clone();
    let mut ok = false;
    let mut p2p_attempted = false;

    p2p_attempted = true;
    if let Ok(Some((b, m))) = fetch_media_from_peer(state, item).await {
        bytes = b;
        mime = m;
        ok = true;
        p2p_attempted = true;
    }

    if !ok {
        if state.post_delivery_mode == PostDeliveryMode::P2pOnly {
            return Ok(None);
        }
        if p2p_attempted && state.p2p_relay_fallback.as_secs() > 0 {
            sleep(state.p2p_relay_fallback).await;
        }
        if let Ok(resp) = send_with_retry_metrics(|| state.http.get(url), 3, &state.net).await {
            let status = resp.status();
            let headers = resp.headers().clone();
            if status.is_success() {
                bytes = resp.bytes().await?.to_vec();
                if !bytes.is_empty() {
                    mime = headers
                        .get(header::CONTENT_TYPE)
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| item.media_type.clone());
                    ok = true;
                }
            }
        }
    }

    if !ok || bytes.is_empty() {
        return Ok(None);
    }
    let dir = media_store::media_dir(&state.data_dir);
    std::fs::create_dir_all(&dir).ok();
    let stored_name = item.id.clone();
    let path = dir.join(&stored_name);
    if std::fs::write(&path, &bytes).is_ok() {
        let base = state.cfg.public_base_url.trim_end_matches('/');
        let me = format!("{base}/users/{}", state.cfg.username);
        let _ = state.social.upsert_media(
            &item.id,
            &item.url,
            &item.media_type,
            item.size,
            Some(&stored_name),
            Some(&me),
            item.width,
            item.height,
            item.blurhash.as_deref(),
        );
    }
    Ok(Some((bytes, mime)))
}

async fn media_p2p_get(state: &ApState, req: Request<Body>) -> Response<Body> {
    let query = req.uri().query().unwrap_or("");
    let media_id = query
        .split('&')
        .find(|p| p.starts_with("id="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_default();
    if media_id.is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing id");
    }
    if let Ok(Some(item)) = state.social.get_media(&media_id) {
        if let Some(local) = item.local_name.clone() {
            if let Ok((bytes, mime)) = media_store::load_media(&state.data_dir, &local) {
                return (StatusCode::OK, [("Content-Type", mime)], bytes).into_response();
            }
        }
    }
    simple(StatusCode::NOT_FOUND, "not found")
}

async fn fetch_media_from_peer(
    state: &ApState,
    item: &crate::social_db::MediaItem,
) -> Result<Option<(Vec<u8>, String)>, anyhow::Error> {
    let actor = item.actor_id.as_deref().unwrap_or("").trim();
    if actor.is_empty() {
        return Ok(None);
    }
    let info = match state.delivery.resolve_actor_info(actor).await {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let Some(peer_id) = info.p2p_peer_id else {
        return Ok(None);
    };
    if !info.p2p_peer_addrs.is_empty() {
        let _ = state
            .delivery
            .p2p_add_peer_addrs(&peer_id, info.p2p_peer_addrs)
            .await;
    }
    let query = format!("?id={}", urlencoding::encode(&item.id));
    let req = RelayHttpRequest {
        id: format!("media-{}", now_ms()),
        method: "GET".to_string(),
        path: "/.fedi3/media".to_string(),
        query,
        headers: vec![("accept".to_string(), "*/*".to_string())],
        body_b64: "".to_string(),
    };
    let resp = match state.delivery.p2p_request(&peer_id, req).await {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    if !(200..300).contains(&resp.status) {
        return Ok(None);
    }
    let bytes = B64.decode(resp.body_b64.as_bytes()).unwrap_or_default();
    if bytes.is_empty() {
        return Ok(None);
    }
    let mut mime = item.media_type.clone();
    for (k, v) in resp.headers {
        if k.to_ascii_lowercase() == "content-type" && !v.trim().is_empty() {
            mime = v;
            break;
        }
    }
    Ok(Some((bytes, mime)))
}

async fn object_get(state: &ApState, req: Request<Body>) -> Response<Body> {
    let query = req.uri().query().unwrap_or("");
    let object_url = query
        .split('&')
        .find(|p| p.starts_with("url="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_default();

    if object_url.is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing url");
    }

    match state.social.get_object_json(&object_url) {
        Ok(Some(bytes)) => {
            // Prefer ActivityPub content-type.
            (
                StatusCode::OK,
                [("Content-Type", "application/activity+json; charset=utf-8")],
                bytes,
            )
                .into_response()
        }
        Ok(None) => simple(StatusCode::NOT_FOUND, "not found"),
        Err(e) => simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    }
}

async fn hello_get(state: &ApState) -> Response<Body> {
    let base = state.cfg.public_base_url.trim_end_matches('/');
    let actor_id = format!("{base}/users/{}", state.cfg.username);
    let did = state
        .cfg
        .also_known_as
        .iter()
        .find(|s| s.starts_with("did:fedi3:"))
        .cloned();
    let body = serde_json::json!({
      "username": state.cfg.username,
      "actor": actor_id,
      "core_version": env!("CARGO_PKG_VERSION"),
      "did": did,
      "p2p": {
        "peer_id": state.cfg.p2p_peer_id,
        "addrs": state.cfg.p2p_peer_addrs,
      }
    });
    (
        StatusCode::OK,
        [("Content-Type", "application/json; charset=utf-8")],
        body.to_string(),
    )
        .into_response()
}

async fn relays_list_get(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let query = parts.uri.query().unwrap_or("");
    let limit = query
        .split('&')
        .find_map(|kv| kv.strip_prefix("limit="))
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(200);
    let items = state.social.list_relay_entries(limit).unwrap_or_default();
    axum::Json(serde_json::json!({ "items": items })).into_response()
}

async fn relays_public_get(state: &ApState) -> Response<Body> {
    let items = state.social.list_relay_entries(200).unwrap_or_default();
    axum::Json(serde_json::json!({ "items": items })).into_response()
}

async fn relays_update_post(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let bytes = match axum::body::to_bytes(body, 256 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    let input: RelayUpdateReq = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };
    if let Some(add) = input.add {
        for item in add {
            let _ = state
                .social
                .upsert_relay_entry(&item.relay_base_url, item.relay_ws_url.as_deref(), "manual");
        }
    }
    if let Some(remove) = input.remove {
        for base in remove {
            let _ = state.social.remove_relay_entry(&base);
        }
    }
    simple(StatusCode::OK, "ok")
}

async fn relays_refresh_post(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let _ = crate::relay_sync::sync_once(state).await;
    simple(StatusCode::OK, "ok")
}

async fn profile_refresh_post(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    match send_profile_update(state).await {
        Ok(pending) => simple(StatusCode::OK, &format!("ok (pending={pending})")),
        Err(e) => {
            warn!("profile update failed: {e:#}");
            simple(StatusCode::BAD_GATEWAY, &format!("update failed: {e:#}"))
        }
    }
}

async fn send_profile_update(state: &ApState) -> anyhow::Result<u64> {
    let base = state.cfg.public_base_url.trim_end_matches('/');
    let actor_id = format!("{base}/users/{}", state.cfg.username);
    let actor = build_local_actor(&state.cfg);
    let activity_id = state.social.new_activity_id(&actor_id);
    let update = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": activity_id,
        "type": "Update",
        "actor": actor_id,
        "object": actor,
    });

    let mut targets = Vec::new();
    let mut cursor: Option<i64> = None;
    loop {
        let page = state.social.list_followers(500, cursor)?;
        targets.extend(page.items.into_iter().filter(|v| !v.trim().is_empty()));
        let next = match page.next {
            Some(v) => v.parse::<i64>().ok(),
            None => None,
        };
        if next.is_none() {
            break;
        }
        cursor = next;
    }

    targets.sort();
    targets.dedup();

    if targets.is_empty() {
        return Ok(0);
    }
    let bytes = serde_json::to_vec(&update).context("encode update activity")?;
    let pending = state.queue.enqueue_activity(bytes, targets).await?;
    Ok(pending)
}

async fn global_ingest(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if !state.internal_token.is_empty() {
        let token = parts
            .headers
            .get("X-Fedi3-Internal")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if token != state.internal_token {
            return simple(StatusCode::FORBIDDEN, "forbidden");
        }
    }
    let bytes = match axum::body::to_bytes(body, 256 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    if bytes.is_empty() {
        return simple(StatusCode::BAD_REQUEST, "empty body");
    }
    if let Some(ip) = client_ip_from_headers(&parts.headers) {
        if let Err(resp) = inbox_rate_limit(state, &format!("ingest:{ip}"), bytes.len() as u64).await {
            state.net.rate_limit_hit();
            let _ = state
                .social
                .insert_audit_event("rate_limit", None, None, None, false, Some("429"), Some("global_ingest ip"));
            return resp;
        }
    }

    let activity: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };
    if !is_public_activity(&activity) {
        return simple(StatusCode::ACCEPTED, "ignored (not public)");
    }

    let ty = activity.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match ty {
        "Create" => {
            // Keep it simple for now: only accept Create of Note/Article.
            let obj = activity.get("object");
            let obj_ty = obj
                .and_then(|v| v.get("type"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if obj_ty != "Note" && obj_ty != "Article" {
                return simple(StatusCode::ACCEPTED, "ignored (object type)");
            }
        }
        "Announce" => {}
        _ => return simple(StatusCode::ACCEPTED, "ignored (type)"),
    }

    let activity_id = activity.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let actor_id = activity.get("actor").and_then(|v| v.as_str());
    let Some(actor_id) = actor_id else {
        return simple(StatusCode::ACCEPTED, "ignored (missing actor)");
    };

    // Basic per-actor rate limiting (best-effort) to prevent a single peer from dominating.
    let now = now_ms();
    let since = now.saturating_sub(60_000);
    if let Ok((count, bytes_sum)) = state.social.global_feed_actor_stats_since(actor_id, since) {
        if count >= state.global_ingest.max_items_per_actor_per_min as u64
            || bytes_sum >= state.global_ingest.max_bytes_per_actor_per_min
        {
            return simple(StatusCode::TOO_MANY_REQUESTS, "rate limited");
        }
    }
    let id = if activity_id.is_empty() {
        // best-effort stable id: sha256 of payload
        let mut hasher = sha2::Sha256::new();
        use sha2::Digest as _;
        hasher.update(&bytes);
        format!("urn:fedi3:global:{}", hex::encode(hasher.finalize()))
    } else {
        activity_id.to_string()
    };

    if let Err(e) = state.social.insert_global_feed_item(&id, Some(actor_id), bytes) {
        return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}"));
    }
    simple(StatusCode::ACCEPTED, "ok")
}

fn is_internal(state: &ApState, headers: &HeaderMap) -> bool {
    if state.internal_token.is_empty() {
        return true;
    }
    headers
        .get("X-Fedi3-Internal")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == state.internal_token)
        .unwrap_or(false)
}

async fn net_metrics_get(state: &ApState, req: Request<Body>) -> Response<Body> {
    let parts = req.into_parts().0;
    if !is_internal(state, &parts.headers) {
        return simple(StatusCode::UNAUTHORIZED, "internal token required");
    }
    let queue_stats = state.queue.stats().await.ok();
    let db_ok = state.social.health_check().is_ok();
    let mut snapshot = state.net.snapshot_json();
    if let Value::Object(map) = &mut snapshot {
        if let Some(stats) = queue_stats {
            map.insert(
                "queue".to_string(),
                serde_json::json!({
                    "pending": stats.pending,
                    "delivered": stats.delivered,
                    "dead": stats.dead
                }),
            );
        }
        map.insert("db".to_string(), serde_json::json!({ "ok": db_ok }));
        map.insert(
            "p2p_config".to_string(),
            serde_json::json!({
                "peer_id": state.cfg.p2p_peer_id,
                "peer_addrs": state.cfg.p2p_peer_addrs,
            }),
        );
        if let Some(Value::Object(relay)) = map.get_mut("relay") {
            relay.insert(
                "base_url".to_string(),
                serde_json::json!(state.cfg.relay_base_url),
            );
        } else {
            map.insert(
                "relay".to_string(),
                serde_json::json!({
                    "base_url": state.cfg.relay_base_url,
                }),
            );
        }
    }
    (
        StatusCode::OK,
        [("Content-Type", "application/json; charset=utf-8"), ("Cache-Control", "no-store")],
        snapshot.to_string(),
    )
        .into_response()
}

async fn net_metrics_prom(state: &ApState, req: Request<Body>) -> Response<Body> {
    let parts = req.into_parts().0;
    if !is_internal(state, &parts.headers) {
        return simple(StatusCode::UNAUTHORIZED, "internal token required");
    }
    let queue_stats = state.queue.stats().await.ok();
    let db_ok = state.social.health_check().is_ok();
    let net = &state.net;
    let mut out = String::new();
    out.push_str("# TYPE fedi3_core_relay_connected gauge\n");
    out.push_str(&format!(
        "fedi3_core_relay_connected {}\n",
        if net.relay_connected.load(Ordering::Relaxed) { 1 } else { 0 }
    ));
    out.push_str("# TYPE fedi3_core_relay_rx_bytes counter\n");
    out.push_str(&format!(
        "fedi3_core_relay_rx_bytes {}\n",
        net.relay_rx_bytes.load(Ordering::Relaxed)
    ));
    out.push_str("# TYPE fedi3_core_relay_tx_bytes counter\n");
    out.push_str(&format!(
        "fedi3_core_relay_tx_bytes {}\n",
        net.relay_tx_bytes.load(Ordering::Relaxed)
    ));
    out.push_str("# TYPE fedi3_core_relay_rtt_ms gauge\n");
    out.push_str(&format!(
        "fedi3_core_relay_rtt_ms {}\n",
        net.relay_rtt_ema_ms.load(Ordering::Relaxed)
    ));
    out.push_str("# TYPE fedi3_core_p2p_enabled gauge\n");
    out.push_str(&format!(
        "fedi3_core_p2p_enabled {}\n",
        if net.p2p_enabled.load(Ordering::Relaxed) { 1 } else { 0 }
    ));
    out.push_str("# TYPE fedi3_core_p2p_connected_peers gauge\n");
    out.push_str(&format!(
        "fedi3_core_p2p_connected_peers {}\n",
        net.p2p_connected_peers.load(Ordering::Relaxed)
    ));
    out.push_str("# TYPE fedi3_core_p2p_active_peers gauge\n");
    out.push_str(&format!(
        "fedi3_core_p2p_active_peers {}\n",
        net.p2p_active_peers.load(Ordering::Relaxed)
    ));
    out.push_str("# TYPE fedi3_core_p2p_rx_bytes counter\n");
    out.push_str(&format!(
        "fedi3_core_p2p_rx_bytes {}\n",
        net.p2p_rx_bytes.load(Ordering::Relaxed)
    ));
    out.push_str("# TYPE fedi3_core_p2p_tx_bytes counter\n");
    out.push_str(&format!(
        "fedi3_core_p2p_tx_bytes {}\n",
        net.p2p_tx_bytes.load(Ordering::Relaxed)
    ));
    out.push_str("# TYPE fedi3_core_p2p_rtt_ms gauge\n");
    out.push_str(&format!(
        "fedi3_core_p2p_rtt_ms {}\n",
        net.p2p_rtt_ema_ms.load(Ordering::Relaxed)
    ));
    out.push_str("# TYPE fedi3_core_webrtc_active_peers gauge\n");
    out.push_str(&format!(
        "fedi3_core_webrtc_active_peers {}\n",
        net.webrtc_active_peers.load(Ordering::Relaxed)
    ));
    out.push_str("# TYPE fedi3_core_webrtc_rx_bytes counter\n");
    out.push_str(&format!(
        "fedi3_core_webrtc_rx_bytes {}\n",
        net.webrtc_rx_bytes.load(Ordering::Relaxed)
    ));
    out.push_str("# TYPE fedi3_core_webrtc_tx_bytes counter\n");
    out.push_str(&format!(
        "fedi3_core_webrtc_tx_bytes {}\n",
        net.webrtc_tx_bytes.load(Ordering::Relaxed)
    ));
    out.push_str("# TYPE fedi3_core_http_timeouts counter\n");
    out.push_str(&format!(
        "fedi3_core_http_timeouts {}\n",
        net.http_timeouts.load(Ordering::Relaxed)
    ));
    out.push_str("# TYPE fedi3_core_http_errors counter\n");
    out.push_str(&format!(
        "fedi3_core_http_errors {}\n",
        net.http_errors.load(Ordering::Relaxed)
    ));
    out.push_str("# TYPE fedi3_core_auth_failures counter\n");
    out.push_str(&format!(
        "fedi3_core_auth_failures {}\n",
        net.auth_failures.load(Ordering::Relaxed)
    ));
    out.push_str("# TYPE fedi3_core_rate_limit_hits counter\n");
    out.push_str(&format!(
        "fedi3_core_rate_limit_hits {}\n",
        net.rate_limit_hits.load(Ordering::Relaxed)
    ));
    if let Some(stats) = queue_stats {
        out.push_str("# TYPE fedi3_core_queue_pending gauge\n");
        out.push_str(&format!("fedi3_core_queue_pending {}\n", stats.pending));
        out.push_str("# TYPE fedi3_core_queue_delivered counter\n");
        out.push_str(&format!("fedi3_core_queue_delivered {}\n", stats.delivered));
        out.push_str("# TYPE fedi3_core_queue_dead counter\n");
        out.push_str(&format!("fedi3_core_queue_dead {}\n", stats.dead));
    }
    out.push_str("# TYPE fedi3_core_db_ok gauge\n");
    out.push_str(&format!("fedi3_core_db_ok {}\n", if db_ok { 1 } else { 0 }));
    (
        StatusCode::OK,
        [("Content-Type", "text/plain; version=0.0.4"), ("Cache-Control", "no-store")],
        out,
    )
        .into_response()
}

async fn core_health(state: &ApState, req: Request<Body>) -> Response<Body> {
    let parts = req.into_parts().0;
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let db_ok = state.social.health_check().is_ok();
    let queue_stats = state.queue.stats().await.ok();
    let net = &state.net;
    let body = serde_json::json!({
        "ok": db_ok,
        "db": { "ok": db_ok },
        "queue": queue_stats.map(|s| serde_json::json!({
            "pending": s.pending,
            "delivered": s.delivered,
            "dead": s.dead,
        })),
        "relay": {
            "connected": net.relay_connected.load(Ordering::Relaxed),
            "rtt_ms": net.relay_rtt_ema_ms.load(Ordering::Relaxed),
        },
        "p2p": {
            "enabled": net.p2p_enabled.load(Ordering::Relaxed),
            "connected_peers": net.p2p_connected_peers.load(Ordering::Relaxed),
        },
        "webrtc": {
            "sessions": net.webrtc_sessions.load(Ordering::Relaxed),
        }
    });
    (
        StatusCode::OK,
        [("Content-Type", "application/json; charset=utf-8"), ("Cache-Control", "no-store")],
        body.to_string(),
    )
        .into_response()
}

#[derive(serde::Deserialize)]
struct BlocksUpdateReq {
    actor: String,
    blocked: bool,
}

async fn blocks_list(state: &ApState, req: Request<Body>) -> Response<Body> {
    let parts = req.into_parts().0;
    if !is_internal(state, &parts.headers) {
        return simple(StatusCode::UNAUTHORIZED, "internal token required");
    }
    let query = parts.uri.query().unwrap_or("");
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(100)
        .min(1000);
    let offset = query
        .split('&')
        .find(|p| p.starts_with("offset="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(0);
    let items = state.social.list_blocked_actors(limit, offset).unwrap_or_default();
    (
        StatusCode::OK,
        [("Content-Type", "application/json; charset=utf-8")],
        serde_json::json!({ "actors": items }).to_string(),
    )
        .into_response()
}

#[derive(serde::Serialize)]
struct AuditEventResp {
    id: i64,
    kind: String,
    created_at_ms: i64,
    actor_id: Option<String>,
    key_id: Option<String>,
    activity_id: Option<String>,
    ok: bool,
    status: Option<String>,
    detail: Option<String>,
}

async fn audit_recent(state: &ApState, req: Request<Body>) -> Response<Body> {
    let parts = req.into_parts().0;
    if !is_internal(state, &parts.headers) {
        return simple(StatusCode::FORBIDDEN, "forbidden");
    }

    let query = parts.uri.query().unwrap_or("");
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(200)
        .min(500);

    let rows = match state.social.list_audit_events(limit) {
        Ok(v) => v,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };
    let out = rows
        .into_iter()
        .map(|e| AuditEventResp {
            id: e.id,
            kind: e.kind,
            created_at_ms: e.created_at_ms,
            actor_id: e.actor_id,
            key_id: e.key_id,
            activity_id: e.activity_id,
            ok: e.ok,
            status: e.status,
            detail: e.detail,
        })
        .collect::<Vec<_>>();

    axum::Json(serde_json::json!({ "events": out })).into_response()
}

async fn blocks_update(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if !is_internal(state, &parts.headers) {
        return simple(StatusCode::UNAUTHORIZED, "internal token required");
    }
    let body_bytes = match axum::body::to_bytes(body, 64 * 1024).await {
        Ok(b) => b,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    let upd: BlocksUpdateReq = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };
    if upd.actor.trim().is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing actor");
    }
    if upd.blocked {
        let _ = state.social.block_actor(&upd.actor);
    } else {
        let _ = state.social.unblock_actor(&upd.actor);
    }
    simple(StatusCode::OK, "ok")
}

#[derive(Debug, serde::Serialize)]
struct MigrationStatusResp {
    actor: String,
    key_id: String,
    did: Option<String>,
    domain: String,
    public_base_url: String,
    followers_count: u64,
    legacy_followers_count: u64,
    also_known_as: Vec<String>,
    legacy_aliases: Vec<String>,
    relay_migration: RelayMigrationStatus,
    legacy_guides: serde_json::Value,
}

#[derive(Debug, serde::Serialize)]
struct RelayMigrationStatus {
    has_previous_actor_alias: bool,
    note: String,
}

async fn migration_status(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }

    let base = state.cfg.public_base_url.trim_end_matches('/').to_string();
    let actor = format!("{base}/users/{}", state.cfg.username);
    let key_id = format!("{actor}#main-key");
    let did = fedi3_did(state);
    let followers_count = state.social.count_followers().unwrap_or(0);
    let legacy_followers_count = state.social.count_legacy_followers().unwrap_or(followers_count);

    let also_known_as = state.cfg.also_known_as.clone();
    let legacy_aliases = also_known_as
        .iter()
        .filter(|v| v.starts_with("http://") || v.starts_with("https://"))
        .filter(|v| !v.contains(&base))
        .cloned()
        .collect::<Vec<_>>();

    let has_previous_actor_alias = also_known_as.iter().any(|v| v.contains("/users/") && v != &actor);
    let relay_note = if legacy_followers_count > 0 && !has_previous_actor_alias {
        "Hai follower legacy: per cambiare relay usa migrazione (Move + alsoKnownAs) e mantieni il vecchio relay attivo per un po' (movedTo/redirect).".to_string()
    } else {
        "Ok".to_string()
    };

    // Generic, UI-friendly guides for common legacy implementations.
    // We cannot force a uniform UX across servers, so we provide the canonical data and steps.
    let legacy_guides = serde_json::json!({
      "common": {
        "new_account_actor": actor,
        "new_account_acct": format!("acct:{}@{}", state.cfg.username, state.cfg.domain),
        "alias_on_new": "Assicurati che il nuovo account Fedi3 esponga l'account legacy in alsoKnownAs (config `legacy_aliases`).",
        "move_on_old": "Sul vecchio account legacy avvia la procedura di 'spostamento/migrazione' verso il nuovo account (se supportata) o pubblica un post fissato con link al nuovo account."
      },
      "mastodon": {
        "notes": [
          "Mastodon usa l'ActivityPub Move per trasferire follower (quando l'istanza lo consente).",
          "Se l'istanza ha 'authorized fetch', potrebbe richiedere che il nuovo actor sia raggiungibile e firmi i fetch."
        ],
        "inputs": {
          "new_actor_url": actor,
          "new_handle": format!("{}@{}", state.cfg.username, state.cfg.domain)
        }
      },
      "pleroma_akkoma": {
        "notes": [
          "Pleroma/Akkoma supportano migrazione in modo simile (Move + aliases), ma UI e policy variano per istanza.",
          "Se non supportato, usa redirect/alias e annuncio pubblico."
        ],
        "inputs": {
          "new_actor_url": actor,
          "new_handle": format!("{}@{}", state.cfg.username, state.cfg.domain)
        }
      },
      "misskey": {
        "notes": [
          "Misskey e fork non sono uniformi sulla migrazione follower; in molte installazioni il flusso è diverso da Mastodon.",
          "Fedi3 resta compatibile ActivityPub: in assenza di migrazione follower automatica, usa alias + annuncio + (opzionale) follow-back automation."
        ],
        "inputs": {
          "new_actor_url": actor,
          "new_handle": format!("{}@{}", state.cfg.username, state.cfg.domain)
        }
      }
    });

    axum::Json(MigrationStatusResp {
        actor,
        key_id,
        did,
        domain: state.cfg.domain.clone(),
        public_base_url: base,
        followers_count,
        legacy_followers_count,
        also_known_as,
        legacy_aliases,
        relay_migration: RelayMigrationStatus {
            has_previous_actor_alias,
            note: relay_note,
        },
        legacy_guides,
    })
    .into_response()
}

#[derive(Debug, serde::Deserialize)]
struct LegacyAliasesSetReq {
    aliases: Vec<String>,
}

async fn migration_legacy_aliases_set(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }

    let body_bytes = match axum::body::to_bytes(body, 64 * 1024).await {
        Ok(b) => b,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    let mut input: LegacyAliasesSetReq = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };

    // Normalize + validate.
    input.aliases.retain(|s| {
        let s = s.trim();
        !s.is_empty() && (s.starts_with("http://") || s.starts_with("https://"))
    });
    input.aliases = input
        .aliases
        .into_iter()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| s.len() <= 2048)
        .collect();
    input.aliases.sort();
    input.aliases.dedup();
    if input.aliases.len() > 10 {
        input.aliases.truncate(10);
    }

    let json = match serde_json::to_string(&input.aliases) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "encode failed"),
    };
    if let Err(e) = state.social.set_local_meta("legacy_aliases_json", &json) {
        return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}"));
    }

    // Note: the embedded server config is immutable at runtime; restart is required to affect actor JSON.
    axum::Json(serde_json::json!({
      "ok": true,
      "aliases": input.aliases,
      "restart_required": true
    }))
    .into_response()
}

async fn p2p_resolve(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }

    let query = parts.uri.query().unwrap_or("");
    let peer_id = query
        .split('&')
        .find(|p| p.starts_with("peer_id="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_default();
    if peer_id.is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing peer_id");
    }

    match state.delivery.p2p_resolve_peer(&peer_id).await {
        Ok(Some(rec)) => axum::Json(rec).into_response(),
        Ok(None) => simple(StatusCode::NOT_FOUND, "not found"),
        Err(e) => simple(StatusCode::BAD_GATEWAY, &format!("resolve failed: {e}")),
    }
}

async fn p2p_resolve_did(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }

    let query = parts.uri.query().unwrap_or("");
    let did = query
        .split('&')
        .find(|p| p.starts_with("did="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_default();
    if did.is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing did");
    }

    match state.delivery.p2p_resolve_did(&did).await {
        Ok(Some(rec)) => axum::Json(rec).into_response(),
        Ok(None) => simple(StatusCode::NOT_FOUND, "not found"),
        Err(e) => simple(StatusCode::BAD_GATEWAY, &format!("resolve failed: {e}")),
    }
}

#[derive(Debug, serde::Deserialize)]
struct P2pFollowReq {
    peer_id: Option<String>,
    actor: Option<String>,
}

async fn p2p_follow(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }

    let bytes = match axum::body::to_bytes(body, 32 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    let follow: P2pFollowReq = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };

    let target_actor = if let Some(peer_id) = follow.peer_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        match state.delivery.p2p_resolve_peer(peer_id).await {
            Ok(Some(rec)) => rec.actor,
            Ok(None) => return simple(StatusCode::NOT_FOUND, "peer not found"),
            Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("resolve failed: {e}")),
        }
    } else if let Some(actor) = follow.actor.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        actor.to_string()
    } else {
        return simple(StatusCode::BAD_REQUEST, "missing peer_id or actor");
    };
    let target_actor = target_actor.trim_end_matches('/').to_string();

    let base = state.cfg.public_base_url.trim_end_matches('/');
    let me = format!("{base}/users/{}", state.cfg.username);
    let id = state.social.new_activity_id(&me);
    let activity = serde_json::json!({
      "@context": "https://www.w3.org/ns/activitystreams",
      "id": id,
      "type": "Follow",
      "actor": me,
      "object": &target_actor,
      "to": [&target_actor],
    });

    let bytes = match serde_json::to_vec(&activity) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "encode failed"),
    };
    let act_id = activity.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if !act_id.is_empty() {
        let _ = state.social.store_outbox(act_id, bytes.clone());
    }
    if let Some(obj) = activity.get("object").and_then(|v| v.as_str()) {
        let obj = obj.trim_end_matches('/');
        let _ = state.social.set_following(obj, FollowingStatus::Pending);
        match state.queue.enqueue_activity(bytes, vec![obj.to_string()]).await {
            Ok(pending) => axum::Json(serde_json::json!({"ok": true, "pending": pending, "target": obj})).into_response(),
            Err(e) => simple(StatusCode::BAD_GATEWAY, &format!("enqueue failed: {e}")),
        }
    } else {
        simple(StatusCode::BAD_REQUEST, "missing object")
    }
}

fn require_internal(state: &ApState, headers: &http::HeaderMap) -> std::result::Result<(), Response<Body>> {
    if state.internal_token.is_empty() {
        return Ok(());
    }
    let token = headers
        .get("X-Fedi3-Internal")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if token != state.internal_token {
        state.net.auth_failure();
        let _ = state
            .social
            .insert_audit_event("internal_auth", None, None, None, false, Some("403"), Some("invalid internal token"));
        return Err(simple(StatusCode::FORBIDDEN, "forbidden"));
    }
    Ok(())
}

async fn p2p_sync_outbox(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    let query = parts.uri.query().unwrap_or("");
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(50)
        .min(200);
    let since = query
        .split('&')
        .find(|p| p.starts_with("since="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<i64>().ok())
        .unwrap_or(0);

    let (rows, latest_ms) = match state.social.list_outbox_since(since, limit) {
        Ok(v) => v,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };

    let mut items = Vec::<serde_json::Value>::new();
    for bytes in rows {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            if is_public_activity(&v) {
                items.push(v);
            }
        }
    }

    axum::Json(serde_json::json!({
      "items": items,
      "latest_ms": latest_ms,
    }))
    .into_response()
}

fn fedi3_did(state: &ApState) -> Option<String> {
    state
        .cfg
        .also_known_as
        .iter()
        .find(|s| s.starts_with("did:fedi3:"))
        .cloned()
}

fn verify_device_signature(state: &ApState, parts: &http::request::Parts, body: &[u8]) -> std::result::Result<(), Response<Body>> {
    let sig = parts
        .headers
        .get("Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if sig.is_empty() {
        return Err(simple(StatusCode::UNAUTHORIZED, "missing Signature header"));
    }

    if let Err(e) = verify_digest_if_present(&parts.headers, body) {
        return Err(simple(StatusCode::UNAUTHORIZED, &format!("bad Digest: {e}")));
    }
    if let Err(e) = verify_date(&parts.headers, Duration::from_secs(300)) {
        return Err(simple(StatusCode::UNAUTHORIZED, &format!("bad Date: {e}")));
    }

    let params = match parse_signature_header(sig) {
        Ok(v) => v,
        Err(e) => return Err(simple(StatusCode::UNAUTHORIZED, &format!("bad Signature: {e}"))),
    };
    let signing = match build_signing_string(&parts.method, &parts.uri, &parts.headers, &params.headers) {
        Ok(v) => v,
        Err(e) => return Err(simple(StatusCode::UNAUTHORIZED, &format!("bad signing string: {e}"))),
    };
    if let Err(e) = verify_signature_rsa_sha256(&state.cfg.public_key_pem, &signing, &params.signature) {
        return Err(simple(StatusCode::UNAUTHORIZED, &format!("signature verify failed: {e}")));
    }
    Ok(())
}

#[derive(Debug, serde::Serialize)]
struct DeviceOutboxItem {
    id: String,
    created_at_ms: i64,
    activity: serde_json::Value,
}

#[derive(Debug, serde::Serialize)]
struct DeviceOutboxResp {
    did: Option<String>,
    items: Vec<DeviceOutboxItem>,
    latest_ms: i64,
}

async fn device_outbox(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    let body_bytes = match axum::body::to_bytes(body, 64 * 1024).await {
        Ok(b) => b,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    if let Err(resp) = verify_device_signature(state, &parts, &body_bytes) {
        return resp;
    }

    let want_did = parts
        .headers
        .get("X-Fedi3-Did")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string());
    let my_did = fedi3_did(state);
    if let (Some(w), Some(m)) = (want_did.as_deref(), my_did.as_deref()) {
        if w != m {
            return simple(StatusCode::FORBIDDEN, "did mismatch");
        }
    }

    let query = parts.uri.query().unwrap_or("");
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(50)
        .min(200);
    let since = query
        .split('&')
        .find(|p| p.starts_with("since="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<i64>().ok())
        .unwrap_or(0);

    let (rows, latest_ms) = match state.social.list_outbox_since_with_ts(since, limit) {
        Ok(v) => v,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };

    let mut items = Vec::<DeviceOutboxItem>::new();
    for (bytes, created_at_ms) in rows {
        let Ok(activity) = serde_json::from_slice::<serde_json::Value>(&bytes) else { continue };
        let Some(id) = activity.get("id").and_then(|v| v.as_str()).map(str::trim).filter(|v| !v.is_empty()) else { continue };
        items.push(DeviceOutboxItem {
            id: id.to_string(),
            created_at_ms,
            activity,
        });
    }

    axum::Json(DeviceOutboxResp {
        did: my_did,
        items,
        latest_ms,
    })
    .into_response()
}

#[derive(Debug, serde::Serialize)]
struct DeviceInboxItem {
    id: String,
    created_at_ms: i64,
    activity: serde_json::Value,
}

#[derive(Debug, serde::Serialize)]
struct DeviceInboxResp {
    did: Option<String>,
    items: Vec<DeviceInboxItem>,
    latest_ms: i64,
}

async fn device_inbox(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    let body_bytes = match axum::body::to_bytes(body, 64 * 1024).await {
        Ok(b) => b,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    if let Err(resp) = verify_device_signature(state, &parts, &body_bytes) {
        return resp;
    }

    let want_did = parts
        .headers
        .get("X-Fedi3-Did")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string());
    let my_did = fedi3_did(state);
    if let (Some(w), Some(m)) = (want_did.as_deref(), my_did.as_deref()) {
        if w != m {
            return simple(StatusCode::FORBIDDEN, "did mismatch");
        }
    }

    let query = parts.uri.query().unwrap_or("");
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(50)
        .min(200);
    let since = query
        .split('&')
        .find(|p| p.starts_with("since="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<i64>().ok())
        .unwrap_or(0);

    let (rows, latest_ms) = match state.social.list_inbox_since_with_ts(since, limit) {
        Ok(v) => v,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };

    let mut items = Vec::<DeviceInboxItem>::new();
    for (bytes, created_at_ms) in rows {
        let Ok(activity) = serde_json::from_slice::<serde_json::Value>(&bytes) else { continue };
        let Some(id) = activity.get("id").and_then(|v| v.as_str()).map(str::trim).filter(|v| !v.is_empty()) else { continue };
        items.push(DeviceInboxItem {
            id: id.to_string(),
            created_at_ms,
            activity,
        });
    }

    axum::Json(DeviceInboxResp {
        did: my_did,
        items,
        latest_ms,
    })
    .into_response()
}

async fn global_timeline(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let query = parts.uri.query().unwrap_or("");
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(50)
        .min(200);
    let cursor = query
        .split('&')
        .find(|p| p.starts_with("cursor="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<i64>().ok());

    let page = match state.social.list_global_feed(limit, cursor) {
        Ok(p) => p,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };
    let mut items = Vec::<serde_json::Value>::new();
    let mut cache: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
    for it in page.items {
        if let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(&it.activity_json) {
            hydrate_activity(state, &mut v, &mut cache);
            items.push(v);
        }
    }
    let next = page.next;
    axum::Json(serde_json::json!({
      "total": page.total,
      "items": items,
      "next": next,
    }))
    .into_response()
}

async fn timeline_federated(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let query = parts.uri.query().unwrap_or("");
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(50)
        .min(200);
    let cursor = query
        .split('&')
        .find(|p| p.starts_with("cursor="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<i64>().ok());

    let page = match state.social.list_federated_feed(limit, cursor) {
        Ok(p) => p,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };
    let mut items = Vec::<serde_json::Value>::new();
    let mut cache: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
    for it in page.items {
        if let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(&it.activity_json) {
            hydrate_activity(state, &mut v, &mut cache);
            items.push(v);
        }
    }
    axum::Json(serde_json::json!({
      "total": page.total,
      "items": items,
      "next": page.next,
    }))
    .into_response()
}

async fn timeline_home(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let query = parts.uri.query().unwrap_or("");
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(50)
        .min(200);
    let cursor = query
        .split('&')
        .find(|p| p.starts_with("cursor="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<i64>().ok());

    let page = match state.social.list_home_feed(limit, cursor) {
        Ok(p) => p,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };
    let mut items = Vec::<serde_json::Value>::new();
    let mut cache: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
    for it in page.items {
        if let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(&it.activity_json) {
            hydrate_activity(state, &mut v, &mut cache);
            items.push(v);
        }
    }
    axum::Json(serde_json::json!({
      "total": page.total,
      "items": items,
      "next": page.next,
    }))
    .into_response()
}

async fn timeline_unified(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let query = parts.uri.query().unwrap_or("");
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(50)
        .min(200);
    let cursor = query
        .split('&')
        .find(|p| p.starts_with("cursor="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<i64>().ok());

    let page = match state.social.list_unified_feed(limit, cursor) {
        Ok(p) => p,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };
    let mut items = Vec::<serde_json::Value>::new();
    let mut cache: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
    for it in page.items {
        if let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(&it.activity_json) {
            hydrate_activity(state, &mut v, &mut cache);
            items.push(v);
        }
    }
    axum::Json(serde_json::json!({
      "total": page.total,
      "items": items,
      "next": page.next,
    }))
    .into_response()
}

async fn note_replies_get(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let query = parts.uri.query().unwrap_or("");
    let note_id = query
        .split('&')
        .find(|p| p.starts_with("note="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_default();
    if note_id.trim().is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing note");
    }
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(20)
        .min(100);
    let cursor = query
        .split('&')
        .find(|p| p.starts_with("cursor="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<i64>().ok());

    let page = match state.social.list_note_replies(&note_id, limit, cursor) {
        Ok(p) => p,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };
    let mut items = Vec::<serde_json::Value>::new();
    let mut cache: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
    for it in page.items {
        if let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(&it.activity_json) {
            hydrate_activity(state, &mut v, &mut cache);
            items.push(v);
        }
    }
    axum::Json(serde_json::json!({
      "total": page.total,
      "items": items,
      "next": page.next,
    }))
    .into_response()
}

async fn search_notes(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let query = parts.uri.query().unwrap_or("");
    let q = query
        .split('&')
        .find(|p| p.starts_with("q="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_default();
    let tag = query
        .split('&')
        .find(|p| p.starts_with("tag="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_default();
    if q.trim().is_empty() && tag.trim().is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing query");
    }
    let source = query
        .split('&')
        .find(|p| p.starts_with("source="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_else(|| "all".to_string());
    let consistency = query
        .split('&')
        .find(|p| p.starts_with("consistency="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_else(|| "full".to_string());
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(30)
        .min(100);
    let cursor = query
        .split('&')
        .find(|p| p.starts_with("cursor="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<i64>().ok());

    let source = normalize_search_source(&source);
    let consistency = normalize_search_consistency(&consistency);
    let mut items = Vec::<serde_json::Value>::new();
    let mut cache: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
    let mut total = 0u64;
    let mut next = None;
    let mut seen = std::collections::HashSet::new();

    if source != "relay" {
        let page = if tag.trim().is_empty() {
            match state.social.search_notes_by_text(&q, limit, cursor) {
                Ok(p) => p,
                Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
            }
        } else {
            match state.social.search_notes_by_tag(&tag, limit, cursor) {
                Ok(p) => p,
                Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
            }
        };
        total = total.saturating_add(page.total);
        if next.is_none() {
            next = page.next.clone();
        }
        for it in page.items {
            let Ok(note_value) = serde_json::from_slice::<serde_json::Value>(&it.object_json) else { continue };
            if note_value.get("type").and_then(|t| t.as_str()) != Some("Note") {
                continue;
            }
            let Some(activity) = activity_from_note(&note_value) else { continue };
            let mut hydrated = activity;
            hydrate_activity(state, &mut hydrated, &mut cache);
            set_search_source(&mut hydrated, "local");
            if let Some(note_id) = note_id_from_activity(&hydrated) {
                if seen.insert(note_id.to_string()) {
                    items.push(hydrated);
                }
            } else {
                items.push(hydrated);
            }
        }
    }

    if source != "local" {
        if consistency == "full" {
            if let Err(resp) = require_relay_search_coverage(state).await {
                return resp;
            }
        }
        if let Ok(Some(relay_page)) = relay_search_notes(state, &q, &tag, limit, cursor).await {
            total = total.saturating_add(relay_page.total);
            if next.is_none() {
                next = relay_page.next.clone();
            }
            for note_value in relay_page.items {
                if note_value.get("type").and_then(|t| t.as_str()) != Some("Note") {
                    continue;
                }
                let Some(activity) = activity_from_note(&note_value) else { continue };
                let mut hydrated = activity;
                hydrate_activity(state, &mut hydrated, &mut cache);
                set_search_source(&mut hydrated, "relay");
                if let Some(note_id) = note_id_from_activity(&hydrated) {
                    if seen.insert(note_id.to_string()) {
                        items.push(hydrated);
                    }
                } else {
                    items.push(hydrated);
                }
            }
        }
    }

    axum::Json(serde_json::json!({
      "total": total,
      "items": items,
      "next": next,
    }))
    .into_response()
}

async fn search_users(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let query = parts.uri.query().unwrap_or("");
    let q = query
        .split('&')
        .find(|p| p.starts_with("q="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_default();
    if q.trim().is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing query");
    }
    let source = query
        .split('&')
        .find(|p| p.starts_with("source="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_else(|| "all".to_string());
    let consistency = query
        .split('&')
        .find(|p| p.starts_with("consistency="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_else(|| "full".to_string());
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(30)
        .min(100);
    let cursor = query
        .split('&')
        .find(|p| p.starts_with("cursor="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<i64>().ok());

    let source = normalize_search_source(&source);
    let consistency = normalize_search_consistency(&consistency);
    let mut items = Vec::<serde_json::Value>::new();
    let mut total = 0u64;
    let mut next = None;
    let mut seen = std::collections::HashSet::new();

    if source != "relay" {
        let page = match state.social.search_actors_by_text(&q, limit, cursor) {
            Ok(p) => p,
            Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
        };
        total = total.saturating_add(page.total);
        if next.is_none() {
            next = page.next.clone();
        }
        for it in page.items {
            let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(&it.object_json) else { continue };
            if !is_actor_value(&v) {
                continue;
            }
            set_search_source(&mut v, "local");
            if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                if seen.insert(id.to_string()) {
                    items.push(v);
                }
            } else {
                items.push(v);
            }
        }
    }

    if source != "local" {
        if consistency == "full" {
            if let Err(resp) = require_relay_search_coverage(state).await {
                return resp;
            }
        }
        if let Ok(Some(relay_page)) = relay_search_users(state, &q, limit, cursor).await {
            total = total.saturating_add(relay_page.total);
            if next.is_none() {
                next = relay_page.next.clone();
            }
            for mut v in relay_page.items {
                if !is_actor_value(&v) {
                    continue;
                }
                set_search_source(&mut v, "relay");
                if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                    if seen.insert(id.to_string()) {
                        items.push(v);
                    }
                } else {
                    items.push(v);
                }
            }
        }
    }

    axum::Json(serde_json::json!({
      "total": total,
      "items": items,
      "next": next,
    }))
    .into_response()
}

async fn search_hashtags(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let query = parts.uri.query().unwrap_or("");
    let q = query
        .split('&')
        .find(|p| p.starts_with("q="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_default();
    let source = query
        .split('&')
        .find(|p| p.starts_with("source="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_else(|| "all".to_string());
    let consistency = query
        .split('&')
        .find(|p| p.starts_with("consistency="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_else(|| "full".to_string());
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(30)
        .min(100);

    let source = normalize_search_source(&source);
    let consistency = normalize_search_consistency(&consistency);
    let mut counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

    if source != "relay" {
        if let Ok(rows) = state.social.search_hashtags(&q, limit) {
            for (name, count) in rows {
                let cleaned = name.trim().trim_start_matches('#').to_string();
                if cleaned.is_empty() {
                    continue;
                }
                let entry = counts.entry(cleaned).or_default();
                *entry = entry.saturating_add(count);
            }
        }
    }
    let mut relay_ready = true;
    if source != "local" {
        if consistency == "full" {
            if require_relay_search_coverage(state).await.is_err() {
                relay_ready = false;
            }
        }
        if relay_ready {
            if let Ok(Some(rows)) = relay_search_hashtags(state, &q, limit).await {
                for (name, count) in rows {
                    let cleaned = name.trim().trim_start_matches('#').to_string();
                    if cleaned.is_empty() {
                        continue;
                    }
                    let entry = counts.entry(cleaned).or_default();
                    *entry = entry.saturating_add(count);
                }
            }
        }
    }

    let mut items: Vec<serde_json::Value> = counts
        .into_iter()
        .map(|(name, count)| serde_json::json!({"name": name, "count": count}))
        .collect();
    items.sort_by(|a, b| b.get("count").and_then(|v| v.as_u64()).cmp(&a.get("count").and_then(|v| v.as_u64())));
    if items.len() > limit as usize {
        items.truncate(limit as usize);
    }
    axum::Json(serde_json::json!({
      "items": items,
      "partial": !relay_ready,
      "relay_ready": relay_ready,
    }))
    .into_response()
}

#[derive(Debug, serde::Deserialize)]
struct ChatSendReq {
    thread_id: Option<String>,
    recipients: Vec<String>,
    text: String,
    reply_to: Option<String>,
    title: Option<String>,
    attachments: Option<Vec<ChatAttachmentReq>>,
}

#[derive(Debug, serde::Deserialize)]
struct ChatAttachmentReq {
    id: String,
    url: String,
    #[serde(rename = "mediaType")]
    media_type: String,
    name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    blurhash: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ChatEditReq {
    message_id: String,
    text: String,
}

#[derive(Debug, serde::Deserialize)]
struct ChatDeleteReq {
    message_id: String,
}

#[derive(Debug, serde::Deserialize)]
struct ChatSeenReq {
    thread_id: String,
    message_id: String,
}

#[derive(Debug, serde::Deserialize)]
struct ChatStatusReq {
    message_ids: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ChatTypingReq {
    thread_id: String,
}

#[derive(Debug, serde::Deserialize)]
struct ChatReactReq {
    message_id: String,
    reaction: String,
    remove: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
struct ChatReactionsReq {
    message_ids: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ChatThreadUpdateReq {
    thread_id: String,
    title: String,
}

#[derive(Debug, serde::Deserialize)]
struct ChatThreadMembersReq {
    thread_id: String,
    add: Option<Vec<String>>,
    remove: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
struct ChatThreadDeleteReq {
    thread_id: String,
}

async fn chat_bundle_get(state: &ApState, _req: Request<Body>) -> Response<Body> {
    match chat::build_chat_bundle(state) {
        Ok(bundle) => (
            StatusCode::OK,
            [("Content-Type", "application/json; charset=utf-8")],
            serde_json::to_string(&bundle).unwrap_or_else(|_| "{}".to_string()),
        )
            .into_response(),
        Err(e) => simple(StatusCode::BAD_GATEWAY, &format!("chat bundle error: {e}")),
    }
}

async fn chat_inbox_post(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (_parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, 256 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    let env: chat::ChatEnvelope = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid envelope"),
    };
    let payload = match chat::decrypt_envelope(state, &env).await {
        Ok(v) => v,
        Err(e) => return simple(StatusCode::BAD_REQUEST, &format!("decrypt failed: {e}")),
    };
    if payload.op == "typing" {
        let ty = format!("typing:{}", env.sender_actor);
        let _ = state
            .ui_events
            .send(UiEvent::new("chat", Some(ty), Some(env.thread_id.clone())));
        return simple(StatusCode::OK, "ok");
    }
    if let Err(e) = chat::store_incoming_payload(state, &env, &payload).await {
        return simple(StatusCode::BAD_GATEWAY, &format!("store failed: {e}"));
    }

    if payload.op == "message" {
        let _ = state
            .ui_events
            .send(UiEvent::new("chat", Some("message".to_string()), Some(env.thread_id.clone())));
        let _ = send_chat_receipt(state, &env, "delivered").await;
    } else if payload.op == "react" {
        let _ = state
            .ui_events
            .send(UiEvent::new("chat", Some("react".to_string()), Some(env.thread_id.clone())));
    } else if payload.op == "system" {
        let _ = state
            .ui_events
            .send(UiEvent::new("chat", Some("system".to_string()), Some(env.thread_id.clone())));
    }

    simple(StatusCode::OK, "ok")
}

async fn chat_threads_get(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let query = parts.uri.query().unwrap_or("");
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(50)
        .min(200);
    let cursor = query
        .split('&')
        .find(|p| p.starts_with("cursor="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<i64>().ok());
    match chat::list_threads(state, limit, cursor) {
        Ok(page) => axum::Json(serde_json::json!({
            "total": page.total,
            "items": page.items,
            "next": page.next,
        }))
        .into_response(),
        Err(e) => simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    }
}

async fn chat_thread_messages_get(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let path = parts.uri.path();
    let thread_id = path.strip_prefix("/_fedi3/chat/threads/").unwrap_or("");
    if thread_id.is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing thread_id");
    }
    let query = parts.uri.query().unwrap_or("");
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(50)
        .min(200);
    let cursor = query
        .split('&')
        .find(|p| p.starts_with("cursor="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<i64>().ok());
    match chat::list_messages(state, thread_id, limit, cursor) {
        Ok(page) => axum::Json(serde_json::json!({
            "total": page.total,
            "items": page.items,
            "next": page.next,
        }))
        .into_response(),
        Err(e) => simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    }
}

async fn chat_send_post(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let bytes = match axum::body::to_bytes(body, 256 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    let input: ChatSendReq = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };
    if input.text.trim().is_empty() {
        return simple(StatusCode::BAD_REQUEST, "empty text");
    }
    let self_actor = format!(
        "{}/users/{}",
        state.cfg.public_base_url.trim_end_matches('/'),
        state.cfg.username
    );
    let mut members = if let Some(id) = input.thread_id.as_deref() {
        state
            .social
            .list_chat_members(id)
            .unwrap_or_default()
            .into_iter()
            .map(|(actor, _role)| actor)
            .collect::<Vec<_>>()
    } else {
        input.recipients.clone()
    };
    if members.is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing recipients");
    }
    if !members.contains(&self_actor) {
        members.push(self_actor.clone());
    }
    members.sort();
    members.dedup();

    let (thread_id, kind) = if let Some(id) = input.thread_id.as_deref() {
        (id.to_string(), "group".to_string())
    } else if members.len() == 2 {
        (chat_dm_thread_id(&members), "dm".to_string())
    } else {
        (format!("urn:fedi3:chat:{}", chat::random_id()), "group".to_string())
    };

    if input.thread_id.is_none() {
        let _ = state.social.create_chat_thread(&thread_id, &kind, input.title.as_deref());
        for actor in &members {
            let role = if actor == &self_actor { "owner" } else { "member" };
            let _ = state.social.upsert_chat_member(&thread_id, actor, role);
        }
    }

    let message_id = format!("urn:fedi3:chat:msg:{}", chat::random_id());
    let payload = chat::ChatPayload {
        op: "message".to_string(),
        text: Some(input.text.clone()),
        reply_to: input.reply_to.clone(),
        message_id: None,
        status: None,
        thread_id: Some(thread_id.clone()),
        attachments: input.attachments.as_ref().map(|list| {
            list.iter()
                .map(|a| chat::ChatAttachment {
                    id: a.id.clone(),
                    url: a.url.clone(),
                    media_type: a.media_type.clone(),
                    name: a.name.clone(),
                    width: a.width,
                    height: a.height,
                    blurhash: a.blurhash.clone(),
                })
                .collect::<Vec<_>>()
        }),
        action: None,
        targets: None,
        members: None,
        title: None,
        reaction: None,
    };

    let msg = crate::social_db::ChatMessage {
        message_id: message_id.clone(),
        thread_id: thread_id.clone(),
        sender_actor: self_actor.clone(),
        sender_device: state.social.get_local_meta("chat_device_id").unwrap_or(None).unwrap_or_default(),
        created_at_ms: now_ms(),
        edited_at_ms: None,
        deleted: false,
        body_json: serde_json::to_string(&payload).unwrap_or_default(),
    };
    let _ = state.social.insert_chat_message(&msg);
    let _ = state.social.touch_chat_thread(&thread_id);

    let mut sent = 0u32;
    let mut queued = 0u32;
    for actor in members {
        if actor == self_actor {
            continue;
        }
        if let Ok(peers) = resolve_actor_peers(state, &actor).await {
            for peer_id in peers {
                if let Ok(Some(bundle)) = fetch_chat_bundle(state, &peer_id).await {
                    if chat::verify_bundle(state, &bundle).await.is_err() {
                        continue;
                    }
                    if let Ok(env) = chat::encrypt_payload_for_bundle(state, &bundle, &thread_id, &message_id, &payload) {
                        match chat::send_envelope_to_peer(state, &peer_id, &env).await {
                            Ok(chat::ChatSendOutcome::Sent) => {
                                let _ = state.social.upsert_chat_message_status(&message_id, &actor, "sent");
                                sent = sent.saturating_add(1);
                            }
                            Ok(chat::ChatSendOutcome::Queued) => {
                                let _ = state.social.upsert_chat_message_status(&message_id, &actor, "queued");
                                queued = queued.saturating_add(1);
                            }
                            Err(_) => {}
                        }
                    }
                }
            }
        }
    }

    axum::Json(serde_json::json!({
        "ok": true,
        "thread_id": thread_id,
        "message_id": message_id,
        "sent": sent,
        "queued": queued
    }))
    .into_response()
}

async fn send_chat_payload_to_members(
    state: &ApState,
    thread_id: &str,
    payload: &chat::ChatPayload,
    envelope_message_id: &str,
) -> u32 {
    let self_actor = format!(
        "{}/users/{}",
        state.cfg.public_base_url.trim_end_matches('/'),
        state.cfg.username
    );
    let members = state.social.list_chat_members(thread_id).unwrap_or_default();
    let mut sent = 0u32;
    for (actor, _role) in members {
        if actor == self_actor {
            continue;
        }
        if let Ok(peers) = resolve_actor_peers(state, &actor).await {
            for peer_id in peers {
                if let Ok(Some(bundle)) = fetch_chat_bundle(state, &peer_id).await {
                    if chat::verify_bundle(state, &bundle).await.is_err() {
                        continue;
                    }
                    if let Ok(env) =
                        chat::encrypt_payload_for_bundle(state, &bundle, thread_id, envelope_message_id, payload)
                    {
                        if matches!(chat::send_envelope_to_peer(state, &peer_id, &env).await, Ok(_)) {
                            sent = sent.saturating_add(1);
                        }
                    }
                }
            }
        }
    }
    sent
}

async fn chat_typing_post(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let bytes = match axum::body::to_bytes(body, 32 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    let input: ChatTypingReq = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };
    if input.thread_id.trim().is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing thread_id");
    }
    let self_actor = format!(
        "{}/users/{}",
        state.cfg.public_base_url.trim_end_matches('/'),
        state.cfg.username
    );
    let members = state
        .social
        .list_chat_members(&input.thread_id)
        .unwrap_or_default()
        .into_iter()
        .map(|(actor, _role)| actor)
        .collect::<Vec<_>>();
    if members.is_empty() {
        return simple(StatusCode::OK, "ok");
    }

    let payload = chat::ChatPayload {
        op: "typing".to_string(),
        text: None,
        reply_to: None,
        message_id: None,
        status: None,
        thread_id: Some(input.thread_id.clone()),
        attachments: None,
        action: None,
        targets: None,
        members: None,
        title: None,
        reaction: None,
    };

    let mut sent = 0u32;
    for actor in members {
        if actor == self_actor {
            continue;
        }
        if let Ok(peers) = resolve_actor_peers(state, &actor).await {
            for peer_id in peers {
                if let Ok(Some(bundle)) = fetch_chat_bundle(state, &peer_id).await {
                    if chat::verify_bundle(state, &bundle).await.is_err() {
                        continue;
                    }
                    if let Ok(env) = chat::encrypt_payload_for_bundle(state, &bundle, &input.thread_id, &chat::random_id(), &payload) {
                        if matches!(chat::send_envelope_to_peer(state, &peer_id, &env).await, Ok(_)) {
                            sent = sent.saturating_add(1);
                        }
                    }
                }
            }
        }
    }

    axum::Json(serde_json::json!({ "ok": true, "sent": sent })).into_response()
}

async fn chat_react_post(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let bytes = match axum::body::to_bytes(body, 64 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    let input: ChatReactReq = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };
    let message_id = input.message_id.trim();
    let reaction = input.reaction.trim();
    if message_id.is_empty() || reaction.is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing fields");
    }
    let self_actor = format!(
        "{}/users/{}",
        state.cfg.public_base_url.trim_end_matches('/'),
        state.cfg.username
    );
    let remove = input.remove.unwrap_or(false);
    if remove {
        let _ = state.social.remove_chat_reaction(message_id, &self_actor, reaction);
    } else {
        let _ = state.social.add_chat_reaction(message_id, &self_actor, reaction);
    }
    let Some(thread_id) = state.social.get_chat_message_thread_id(message_id).unwrap_or(None) else {
        return simple(StatusCode::OK, "ok");
    };
    let members = state.social.list_chat_members(&thread_id).unwrap_or_default();
    let member_ids = members.into_iter().map(|(a, _)| a).collect::<Vec<_>>();
    let payload = chat::ChatPayload {
        op: "react".to_string(),
        text: None,
        reply_to: None,
        message_id: Some(message_id.to_string()),
        status: None,
        thread_id: Some(thread_id.clone()),
        attachments: None,
        action: Some(if remove { "remove".to_string() } else { "add".to_string() }),
        targets: None,
        members: None,
        title: None,
        reaction: Some(reaction.to_string()),
    };
    let mut sent = 0u32;
    for actor in member_ids {
        if actor == self_actor {
            continue;
        }
        if let Ok(peers) = resolve_actor_peers(state, &actor).await {
            for peer_id in peers {
                if let Ok(Some(bundle)) = fetch_chat_bundle(state, &peer_id).await {
                    if chat::verify_bundle(state, &bundle).await.is_err() {
                        continue;
                    }
                    if let Ok(env) = chat::encrypt_payload_for_bundle(state, &bundle, &thread_id, &chat::random_id(), &payload) {
                        if matches!(chat::send_envelope_to_peer(state, &peer_id, &env).await, Ok(_)) {
                            sent = sent.saturating_add(1);
                        }
                    }
                }
            }
        }
    }
    let _ = state
        .ui_events
        .send(UiEvent::new("chat", Some("react".to_string()), Some(thread_id)));
    axum::Json(serde_json::json!({ "ok": true, "sent": sent })).into_response()
}

async fn chat_reactions_post(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let bytes = match axum::body::to_bytes(body, 128 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    let input: ChatReactionsReq = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };
    if input.message_ids.is_empty() {
        return axum::Json(serde_json::json!({ "items": [] })).into_response();
    }
    let self_actor = format!(
        "{}/users/{}",
        state.cfg.public_base_url.trim_end_matches('/'),
        state.cfg.username
    );
    let rows = match state.social.list_chat_reactions(&input.message_ids, &self_actor) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_GATEWAY, "list reactions failed"),
    };
    let mut map: std::collections::HashMap<String, Vec<serde_json::Value>> = std::collections::HashMap::new();
    for (message_id, reaction, count, me) in rows {
        map.entry(message_id)
            .or_default()
            .push(serde_json::json!({ "reaction": reaction, "count": count, "me": me }));
    }
    let items = map
        .into_iter()
        .map(|(message_id, reactions)| serde_json::json!({ "message_id": message_id, "reactions": reactions }))
        .collect::<Vec<_>>();
    axum::Json(serde_json::json!({ "items": items })).into_response()
}

fn chat_dm_thread_id(members: &[String]) -> String {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    for (idx, member) in members.iter().enumerate() {
        if idx > 0 {
            hasher.update(b"|");
        }
        hasher.update(member.as_bytes());
    }
    let hash = hasher.finalize();
    let mut out = String::with_capacity(hash.len() * 2);
    for b in hash {
        out.push_str(&format!("{b:02x}"));
    }
    format!("urn:fedi3:chat:dm:{out}")
}

async fn chat_edit_post(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let bytes = match axum::body::to_bytes(body, 256 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    let input: ChatEditReq = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };
    if input.message_id.trim().is_empty() || input.text.trim().is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing fields");
    }
    let self_actor = format!(
        "{}/users/{}",
        state.cfg.public_base_url.trim_end_matches('/'),
        state.cfg.username
    );
    let meta = match state.social.get_chat_message_meta(&input.message_id) {
        Ok(v) => v,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };
    let Some((sender, deleted)) = meta else {
        return simple(StatusCode::NOT_FOUND, "message not found");
    };
    if sender != self_actor {
        return simple(StatusCode::FORBIDDEN, "not owner");
    }
    if deleted {
        return simple(StatusCode::CONFLICT, "message deleted");
    }
    let payload = chat::ChatPayload {
        op: "edit".to_string(),
        text: Some(input.text.clone()),
        reply_to: None,
        message_id: Some(input.message_id.clone()),
        status: None,
        thread_id: None,
        attachments: None,
        action: None,
        targets: None,
        members: None,
        title: None,
        reaction: None,
    };
    let _ = state.social.update_chat_message_edit(&input.message_id, &serde_json::to_string(&payload).unwrap_or_default());
    if let Ok(Some(thread_id)) = state.social.get_chat_message_thread_id(&input.message_id) {
        let env_id = chat::random_id();
        let _sent = send_chat_payload_to_members(state, &thread_id, &payload, &env_id).await;
        let _ = state
            .ui_events
            .send(UiEvent::new("chat", Some("edit".to_string()), Some(thread_id)));
    }
    simple(StatusCode::OK, "ok")
}

async fn chat_delete_post(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let bytes = match axum::body::to_bytes(body, 256 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    let input: ChatDeleteReq = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };
    if input.message_id.trim().is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing message_id");
    }
    let self_actor = format!(
        "{}/users/{}",
        state.cfg.public_base_url.trim_end_matches('/'),
        state.cfg.username
    );
    let meta = match state.social.get_chat_message_meta(&input.message_id) {
        Ok(v) => v,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };
    let Some((sender, deleted)) = meta else {
        return simple(StatusCode::NOT_FOUND, "message not found");
    };
    if sender != self_actor {
        return simple(StatusCode::FORBIDDEN, "not owner");
    }
    if deleted {
        return simple(StatusCode::CONFLICT, "message deleted");
    }
    if state
        .social
        .chat_message_seen_by_others(&input.message_id, &self_actor)
        .unwrap_or(false)
    {
        return simple(StatusCode::CONFLICT, "message already seen");
    }
    let _ = state.social.mark_chat_message_deleted(&input.message_id);
    if let Ok(Some(thread_id)) = state.social.get_chat_message_thread_id(&input.message_id) {
        let payload = chat::ChatPayload {
            op: "delete".to_string(),
            text: None,
            reply_to: None,
            message_id: Some(input.message_id.clone()),
            status: None,
            thread_id: Some(thread_id.clone()),
            attachments: None,
            action: None,
            targets: None,
            members: None,
            title: None,
            reaction: None,
        };
        let env_id = chat::random_id();
        let _sent = send_chat_payload_to_members(state, &thread_id, &payload, &env_id).await;
        let _ = state
            .ui_events
            .send(UiEvent::new("chat", Some("delete".to_string()), Some(thread_id)));
    }
    simple(StatusCode::OK, "ok")
}

async fn chat_seen_post(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let bytes = match axum::body::to_bytes(body, 256 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    let input: ChatSeenReq = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };
    if input.thread_id.trim().is_empty() || input.message_id.trim().is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing fields");
    }
    let members = state.social.list_chat_members(&input.thread_id).unwrap_or_default();
    let self_actor = format!(
        "{}/users/{}",
        state.cfg.public_base_url.trim_end_matches('/'),
        state.cfg.username
    );
    for (actor, _role) in members {
        if actor == self_actor {
            continue;
        }
        if let Ok(peers) = resolve_actor_peers(state, &actor).await {
            for peer_id in peers {
    let payload = chat::ChatPayload {
        op: "receipt".to_string(),
        text: None,
        reply_to: None,
        message_id: Some(input.message_id.clone()),
        status: Some("seen".to_string()),
        thread_id: Some(input.thread_id.clone()),
        attachments: None,
        action: None,
        targets: None,
        members: None,
        title: None,
        reaction: None,
    };
                if let Ok(Some(bundle)) = fetch_chat_bundle(state, &peer_id).await {
                    if chat::verify_bundle(state, &bundle).await.is_err() {
                        continue;
                    }
                    if let Ok(env) = chat::encrypt_payload_for_bundle(state, &bundle, &input.thread_id, &input.message_id, &payload) {
                        let _ = chat::send_envelope_to_peer(state, &peer_id, &env).await;
                    }
                }
            }
        }
    }
    simple(StatusCode::OK, "ok")
}

async fn chat_status_post(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let bytes = match axum::body::to_bytes(body, 256 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    let input: ChatStatusReq = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };
    if input.message_ids.is_empty() {
        return axum::Json(serde_json::json!({"items": []})).into_response();
    }
    let statuses = match state.social.list_chat_message_statuses(&input.message_ids) {
        Ok(v) => v,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };
    let mut items = Vec::new();
    for (message_id, actor_id, status, updated_at_ms) in statuses {
        items.push(serde_json::json!({
            "message_id": message_id,
            "actor_id": actor_id,
            "status": status,
            "updated_at_ms": updated_at_ms,
        }));
    }
    axum::Json(serde_json::json!({ "items": items })).into_response()
}

async fn chat_thread_members_get(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let query = parts.uri.query().unwrap_or("");
    let thread_id = query
        .split('&')
        .find(|p| p.starts_with("thread_id="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_default();
    if thread_id.trim().is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing thread_id");
    }
    let members = match state.social.list_chat_members(&thread_id) {
        Ok(v) => v,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };
    let items = members
        .into_iter()
        .map(|(actor, role)| serde_json::json!({"actor_id": actor, "role": role}))
        .collect::<Vec<_>>();
    axum::Json(serde_json::json!({ "items": items })).into_response()
}

async fn chat_thread_update_post(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let bytes = match axum::body::to_bytes(body, 256 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    let input: ChatThreadUpdateReq = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };
    if input.thread_id.trim().is_empty() || input.title.trim().is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing fields");
    }

    let _ = state.social.update_chat_thread_title(&input.thread_id, Some(input.title.trim()));
    let members = state.social.list_chat_members(&input.thread_id).unwrap_or_default();
    let member_ids = members.into_iter().map(|(a, _)| a).collect::<Vec<_>>();
    let text = format!("Renamed: {}", input.title.trim());
    let _ = send_chat_system(
        state,
        &input.thread_id,
        &member_ids,
        "rename",
        None,
        Some(input.title.trim().to_string()),
        Some(text),
        None,
    )
    .await;
    let _ = state
        .ui_events
        .send(UiEvent::new("chat", Some("thread".to_string()), Some(input.thread_id.clone())));
    simple(StatusCode::OK, "ok")
}

async fn chat_thread_delete_post(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let bytes = match axum::body::to_bytes(body, 256 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    let input: ChatThreadDeleteReq = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };
    if input.thread_id.trim().is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing thread_id");
    }
    let self_actor = format!(
        "{}/users/{}",
        state.cfg.public_base_url.trim_end_matches('/'),
        state.cfg.username
    );
    let role = match state.social.get_chat_member_role(&input.thread_id, &self_actor) {
        Ok(v) => v,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };
    if role.as_deref() != Some("owner") {
        return simple(StatusCode::FORBIDDEN, "not owner");
    }
    let payload = chat::ChatPayload {
        op: "system".to_string(),
        text: Some("Thread deleted".to_string()),
        reply_to: None,
        message_id: None,
        status: None,
        thread_id: Some(input.thread_id.clone()),
        attachments: None,
        action: Some("delete_thread".to_string()),
        targets: None,
        members: None,
        title: None,
        reaction: None,
    };
    let env_id = chat::random_id();
    let _sent = send_chat_payload_to_members(state, &input.thread_id, &payload, &env_id).await;
    if let Err(e) = state.social.delete_chat_thread(&input.thread_id) {
        return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}"));
    }
    let _ = state
        .ui_events
        .send(UiEvent::new("chat", Some("thread".to_string()), Some(input.thread_id.clone())));
    simple(StatusCode::OK, "ok")
}

async fn chat_thread_members_post(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let bytes = match axum::body::to_bytes(body, 256 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    let input: ChatThreadMembersReq = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };
    if input.thread_id.trim().is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing thread_id");
    }

    let add = input.add.unwrap_or_default();
    let remove = input.remove.unwrap_or_default();
    let current = state.social.list_chat_members(&input.thread_id).unwrap_or_default();
    let mut current_ids = current.into_iter().map(|(a, _)| a).collect::<Vec<_>>();
    current_ids.sort();
    current_ids.dedup();

    let mut new_members = current_ids.clone();
    for a in &add {
        if !new_members.contains(a) {
            new_members.push(a.clone());
        }
    }
    new_members.retain(|m| !remove.contains(m));

    let _ = state.social.set_chat_members(&input.thread_id, &new_members);

    let mut notify_targets = current_ids.clone();
    for a in &remove {
        if !notify_targets.contains(a) {
            notify_targets.push(a.clone());
        }
    }
    let text = if !add.is_empty() {
        format!("Added: {}", add.join(", "))
    } else if !remove.is_empty() {
        format!("Removed: {}", remove.join(", "))
    } else {
        "Group updated".to_string()
    };
    let _ = send_chat_system(
        state,
        &input.thread_id,
        &notify_targets,
        if !add.is_empty() { "add_member" } else { "remove_member" },
        Some(if !add.is_empty() { add } else { remove }),
        None,
        Some(text),
        Some(new_members),
    )
    .await;
    simple(StatusCode::OK, "ok")
}

async fn send_chat_receipt(state: &ApState, env: &chat::ChatEnvelope, status: &str) -> anyhow::Result<()> {
    let payload = chat::ChatPayload {
        op: "receipt".to_string(),
        text: None,
        reply_to: None,
        message_id: Some(env.message_id.clone()),
        status: Some(status.to_string()),
        thread_id: Some(env.thread_id.clone()),
        attachments: None,
        action: None,
        targets: None,
        members: None,
        title: None,
        reaction: None,
    };
    let peer_id = env.sender_peer_id.trim();
    if peer_id.is_empty() {
        return Ok(());
    }
    let Some(bundle) = fetch_chat_bundle(state, peer_id).await? else {
        return Ok(());
    };
    if chat::verify_bundle(state, &bundle).await.is_err() {
        return Ok(());
    }
    let receipt_env = chat::encrypt_payload_for_bundle(state, &bundle, &env.thread_id, &env.message_id, &payload)?;
    chat::send_envelope_to_peer(state, peer_id, &receipt_env)
        .await
        .map(|_| ())?;
    Ok(())
}

async fn send_chat_system(
    state: &ApState,
    thread_id: &str,
    members: &[String],
    action: &str,
    targets: Option<Vec<String>>,
    title: Option<String>,
    text: Option<String>,
    members_full: Option<Vec<String>>,
) -> anyhow::Result<u32> {
    if members.is_empty() {
        return Ok(0);
    }
    let self_actor = format!(
        "{}/users/{}",
        state.cfg.public_base_url.trim_end_matches('/'),
        state.cfg.username
    );
    let mut all_members = members.to_vec();
    if !all_members.contains(&self_actor) {
        all_members.push(self_actor.clone());
    }
    let message_id = format!("urn:fedi3:chat:msg:{}", chat::random_id());
    let payload = chat::ChatPayload {
        op: "system".to_string(),
        text,
        reply_to: None,
        message_id: None,
        status: None,
        thread_id: Some(thread_id.to_string()),
        attachments: None,
        action: Some(action.to_string()),
        targets,
        members: members_full,
        title,
        reaction: None,
    };
    let msg = crate::social_db::ChatMessage {
        message_id: message_id.clone(),
        thread_id: thread_id.to_string(),
        sender_actor: self_actor.clone(),
        sender_device: state.social.get_local_meta("chat_device_id").unwrap_or(None).unwrap_or_default(),
        created_at_ms: now_ms(),
        edited_at_ms: None,
        deleted: false,
        body_json: serde_json::to_string(&payload).unwrap_or_default(),
    };
    let _ = state.social.insert_chat_message(&msg);
    let _ = state.social.touch_chat_thread(thread_id);

    let mut sent = 0u32;
    for actor in all_members {
        if actor == self_actor {
            continue;
        }
        if let Ok(peers) = resolve_actor_peers(state, &actor).await {
            for peer_id in peers {
                if let Ok(Some(bundle)) = fetch_chat_bundle(state, &peer_id).await {
                    if chat::verify_bundle(state, &bundle).await.is_err() {
                        continue;
                    }
                    if let Ok(env) = chat::encrypt_payload_for_bundle(state, &bundle, thread_id, &message_id, &payload) {
                        match chat::send_envelope_to_peer(state, &peer_id, &env).await {
                            Ok(chat::ChatSendOutcome::Sent) => {
                                let _ = state.social.upsert_chat_message_status(&message_id, &actor, "sent");
                                sent = sent.saturating_add(1);
                            }
                            Ok(chat::ChatSendOutcome::Queued) => {
                                let _ = state.social.upsert_chat_message_status(&message_id, &actor, "queued");
                                sent = sent.saturating_add(1);
                            }
                            Err(_) => {}
                        }
                    }
                }
            }
        }
    }
    Ok(sent)
}

async fn fetch_chat_bundle(state: &ApState, peer_id: &str) -> anyhow::Result<Option<chat::ChatBundle>> {
    let req = RelayHttpRequest {
        id: format!("chat-bundle-{}", chat::random_id()),
        method: "GET".to_string(),
        path: "/_fedi3/chat/bundle".to_string(),
        query: "".to_string(),
        headers: vec![("accept".to_string(), "application/json".to_string())],
        body_b64: "".to_string(),
    };
    let resp = match state.delivery.p2p_request(peer_id, req).await {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    if !(200..300).contains(&resp.status) {
        return Ok(None);
    }
    let body = B64.decode(resp.body_b64.as_bytes()).unwrap_or_default();
    let bundle = serde_json::from_slice::<chat::ChatBundle>(&body).ok();
    Ok(bundle)
}

async fn resolve_actor_peers(state: &ApState, actor_id: &str) -> anyhow::Result<Vec<String>> {
    let resp = state
        .http
        .get(actor_id)
        .header(
            "Accept",
            "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"",
        )
        .send()
        .await?;
    let text = resp.text().await?;
    let actor_json: serde_json::Value = serde_json::from_str(&text)?;
    let peer_id = actor_json
        .get("endpoints")
        .and_then(|e| e.get("fedi3PeerId"))
        .and_then(|v| v.as_str())
        .or_else(|| actor_json.get("fedi3PeerId").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let did = actor_json
        .get("alsoKnownAs")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .find(|s| s.starts_with("did:fedi3:"))
                .map(|s| s.to_string())
        });

    if let Some(did) = did {
        if let Ok(Some(rec)) = state.delivery.p2p_resolve_did(&did).await {
            let mut out = Vec::new();
            for p in rec.peers {
                if !p.addrs.is_empty() {
                    let _ = state.delivery.p2p_add_peer_addrs(&p.peer_id, p.addrs.clone()).await;
                }
                out.push(p.peer_id);
            }
            if !out.is_empty() {
                return Ok(out);
            }
        }
    }
    if let Some(peer_id) = peer_id {
        return Ok(vec![peer_id]);
    }
    Ok(Vec::new())
}

async fn social_unfollow(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }

    let bytes = match axum::body::to_bytes(body, 32 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    let follow: P2pFollowReq = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };

    let target_actor = if let Some(peer_id) = follow.peer_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        match state.delivery.p2p_resolve_peer(peer_id).await {
            Ok(Some(rec)) => rec.actor,
            Ok(None) => return simple(StatusCode::NOT_FOUND, "peer not found"),
            Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("resolve failed: {e}")),
        }
    } else if let Some(actor) = follow.actor.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        actor.to_string()
    } else {
        return simple(StatusCode::BAD_REQUEST, "missing peer_id or actor");
    };
    let target_actor = target_actor.trim_end_matches('/').to_string();

    let base = state.cfg.public_base_url.trim_end_matches('/');
    let me = format!("{base}/users/{}", state.cfg.username);
    let undo_id = state.social.new_activity_id(&me);
    let activity = serde_json::json!({
      "@context": "https://www.w3.org/ns/activitystreams",
      "id": undo_id,
      "type": "Undo",
      "actor": me,
      "object": {
        "type": "Follow",
        "actor": format!("{base}/users/{}", state.cfg.username),
        "object": &target_actor,
      },
      "to": [&target_actor],
    });

    let bytes = match serde_json::to_vec(&activity) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "encode failed"),
    };
    let act_id = activity.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if !act_id.is_empty() {
        let _ = state.social.store_outbox(act_id, bytes.clone());
    }

    let _ = state.social.remove_following(&target_actor);
    match state.queue.enqueue_activity(bytes, vec![target_actor.clone()]).await {
        Ok(pending) => axum::Json(serde_json::json!({"ok": true, "pending": pending, "target": target_actor})).into_response(),
        Err(e) => simple(StatusCode::BAD_GATEWAY, &format!("enqueue failed: {e}")),
    }
}

async fn social_follow_status_get(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let query = parts.uri.query().unwrap_or("");
    let actor = query
        .split('&')
        .find(|p| p.starts_with("actor="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_default()
        .trim()
        .to_string();
    if actor.is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing actor");
    }
    let actor = actor.trim_end_matches('/');

    let status = match state.social.get_following_status(actor) {
        Ok(Some(FollowingStatus::Pending)) => "pending",
        Ok(Some(FollowingStatus::Accepted)) => "accepted",
        _ => "none",
    };
    (
        StatusCode::OK,
        [("Content-Type", "application/json; charset=utf-8")],
        serde_json::json!({ "status": status }).to_string(),
    )
        .into_response()
}

fn webfinger(cfg: &ApConfig, req: Request<Body>) -> Response<Body> {
    let resource = req
        .uri()
        .query()
        .and_then(|q| q.split('&').find(|p| p.starts_with("resource=")))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_default();

    let actor_url = format!(
        "{}/users/{}",
        cfg.public_base_url.trim_end_matches('/'),
        cfg.username
    );
    let expected_acct = format!("acct:{}@{}", cfg.username, cfg.domain);

    if resource.is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing resource");
    }

    // Respond only for our local user; tolerate `acct:` with different domain in dev,
    // but always require matching username.
    let mut ok = false;
    if resource == expected_acct {
        ok = true;
    } else if resource == actor_url {
        ok = true;
    } else if let Some(rest) = resource.strip_prefix("acct:") {
        if let Some((user, _domain)) = rest.split_once('@') {
            ok = user == cfg.username;
        }
    }
    if !ok {
        return simple(StatusCode::NOT_FOUND, "not found");
    }

    let body = Webfinger {
        subject: expected_acct,
        links: vec![WebfingerLink {
            rel: "self".to_string(),
            ty: "application/activity+json".to_string(),
            href: actor_url,
        },
        WebfingerLink {
            rel: "http://webfinger.net/rel/profile-page".to_string(),
            ty: "text/html".to_string(),
            href: format!("{}/users/{}", cfg.public_base_url.trim_end_matches('/'), cfg.username),
        }],
    };

    jrd(StatusCode::OK, &body)
}

fn build_local_actor(cfg: &ApConfig) -> Actor {
    let base = cfg.public_base_url.trim_end_matches('/');
    let id = format!("{base}/users/{}", cfg.username);
    let inbox = format!("{id}/inbox");
    let outbox = format!("{id}/outbox");
    let followers = format!("{id}/followers");
    let following = format!("{id}/following");
    let shared_inbox = format!("{base}/inbox");
    let key_id = format!("{id}#main-key");

    let icon = cfg
        .icon_url
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|url| ActorImage {
            ty: "Image".to_string(),
            mediaType: cfg
                .icon_media_type
                .clone()
                .unwrap_or_else(|| "image/png".to_string()),
            url: url.to_string(),
        });
    let image = cfg
        .image_url
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|url| ActorImage {
            ty: "Image".to_string(),
            mediaType: cfg
                .image_media_type
                .clone()
                .unwrap_or_else(|| "image/png".to_string()),
            url: url.to_string(),
        });

    Actor {
        context: vec![
            "https://www.w3.org/ns/activitystreams".to_string(),
            "https://w3id.org/security/v1".to_string(),
            "http://joinmastodon.org/ns#".to_string(),
        ],
        id: id.clone(),
        ty: "Person".to_string(),
        preferredUsername: cfg.username.clone(),
        name: cfg
            .display_name
            .clone()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| Some(cfg.username.clone())),
        url: Some(format!("{base}/users/{}", cfg.username)),
        summary: cfg.summary.clone().or_else(|| Some(String::new())),
        published: cfg.published_ms.and_then(|ms| ms_to_rfc3339(ms)),
        manually_approves_followers: Some(cfg.manually_approves_followers),
        attachment: cfg
            .profile_fields
            .iter()
            .map(|f| ActorField {
                ty: "PropertyValue".to_string(),
                name: f.name.trim().to_string(),
                value: f.value.trim().to_string(),
            })
            .filter(|f| !f.name.is_empty() && !f.value.is_empty())
            .collect(),
        also_known_as: cfg.also_known_as.clone(),
        followers,
        following,
        inbox,
        outbox,
        endpoints: ActorEndpoints {
            shared_inbox,
            fedi3_peer_id: cfg.p2p_peer_id.clone(),
            fedi3_peer_addrs: cfg.p2p_peer_addrs.clone(),
        },
        publicKey: PublicKey {
            id: key_id,
            owner: id.clone(),
            publicKeyPem: cfg.public_key_pem.clone(),
        },
        icon,
        image,
        discoverable: Some(true),
        indexable: Some(true),
        featured: Some(format!("{id}/collections/featured")),
        featured_tags: Some(format!("{id}/collections/featuredTags")),
    }
}

async fn actor_get(state: &ApState, req: Request<Body>, accept: Option<ActivityAccept>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = verify_signature_if_present(state, &parts).await {
        return resp;
    }

    let actor = build_local_actor(&state.cfg);

    json_activity(StatusCode::OK, accept, &actor)
}

fn ms_to_rfc3339(ms: i64) -> Option<String> {
    let secs = ms.checked_div(1000)?;
    let nanos = (ms.rem_euclid(1000) * 1_000_000) as u32;
    let dt = time::OffsetDateTime::from_unix_timestamp(secs).ok()? + time::Duration::nanoseconds(nanos as i64);
    dt.format(&time::format_description::well_known::Rfc3339).ok()
}

#[derive(Serialize)]
#[allow(non_snake_case)]
struct OrderedCollection {
    #[serde(rename = "@context")]
    context: String,
    id: String,
    #[serde(rename = "type")]
    ty: String,
    totalItems: u64,
    first: String,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
struct OrderedCollectionPage<T> {
    #[serde(rename = "@context")]
    context: String,
    id: String,
    #[serde(rename = "type")]
    ty: String,
    partOf: String,
    orderedItems: Vec<T>,
    next: Option<String>,
}

async fn followers_get(state: &ApState, req: Request<Body>, accept: Option<ActivityAccept>) -> Response<Body> {
    collection_get_string(state, req, accept, "followers").await
}

async fn following_get(state: &ApState, req: Request<Body>, accept: Option<ActivityAccept>) -> Response<Body> {
    collection_get_string(state, req, accept, "following").await
}

async fn collection_get_string(
    state: &ApState,
    req: Request<Body>,
    accept: Option<ActivityAccept>,
    kind: &str,
) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = verify_signature_if_present(state, &parts).await {
        return resp;
    }
    let req = Request::from_parts(parts, body);
    let base = state.cfg.public_base_url.trim_end_matches('/');
    let col = format!("{base}/users/{}/{}", state.cfg.username, kind);
    let query = req.uri().query().unwrap_or("");
    let is_page = query.contains("page=");
    let cursor = query
        .split('&')
        .find(|p| p.starts_with("cursor="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<i64>().ok())
        .or_else(|| {
            // Some client/server implementations send Mastodon-like paging params.
            query
                .split('&')
                .find(|p| p.starts_with("max_id=") || p.starts_with("since_id=") || p.starts_with("min_id="))
                .and_then(|p| p.split_once('='))
                .and_then(|(_, v)| v.parse::<i64>().ok())
        });

    if !is_page {
        // Top-level OrderedCollection
        let total = match kind {
            "followers" => state.social.list_followers(0, None).map(|p| p.total),
            "following" => state.social.list_following(0, None).map(|p| p.total),
            _ => Ok(0),
        };
        let total_items = total.unwrap_or(0);
        let body = OrderedCollection {
            context: "https://www.w3.org/ns/activitystreams".to_string(),
            id: col.clone(),
            ty: "OrderedCollection".to_string(),
            totalItems: total_items,
            first: format!("{col}?page=true"),
        };
        return json_activity(StatusCode::OK, accept, &body);
    }

    let limit = 20;
    let page = match kind {
        "followers" => state.social.list_followers(limit, cursor),
        "following" => state.social.list_following(limit, cursor),
        _ => Ok(crate::social_db::CollectionPage { total: 0, items: vec![], next: None }),
    };
    let page = match page {
        Ok(p) => p,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };

    let next = page
        .next
        .as_ref()
        .map(|c| format!("{col}?page=true&cursor={c}"));
    let id = if let Some(c) = cursor {
        format!("{col}?page=true&cursor={c}")
    } else {
        format!("{col}?page=true")
    };
    let body = OrderedCollectionPage {
        context: "https://www.w3.org/ns/activitystreams".to_string(),
        id,
        ty: "OrderedCollectionPage".to_string(),
        partOf: col,
        orderedItems: page.items,
        next,
    };
    json_activity(StatusCode::OK, accept, &body)
}

async fn empty_collection_get(
    state: &ApState,
    req: Request<Body>,
    accept: Option<ActivityAccept>,
    kind: &str,
) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = verify_signature_if_present(state, &parts).await {
        return resp;
    }
    let req = Request::from_parts(parts, body);
    let base = state.cfg.public_base_url.trim_end_matches('/');
    let col = format!("{base}/users/{}/collections/{kind}", state.cfg.username);
    let query = req.uri().query().unwrap_or("");
    let is_page = query.contains("page=");

    if !is_page {
        let body = OrderedCollection {
            context: "https://www.w3.org/ns/activitystreams".to_string(),
            id: col.clone(),
            ty: "OrderedCollection".to_string(),
            totalItems: 0,
            first: format!("{col}?page=true"),
        };
        return json_activity(StatusCode::OK, accept, &body);
    }

    let id = format!("{col}?page=true");
    let body = OrderedCollectionPage::<serde_json::Value> {
        context: "https://www.w3.org/ns/activitystreams".to_string(),
        id,
        ty: "OrderedCollectionPage".to_string(),
        partOf: col,
        orderedItems: Vec::new(),
        next: None,
    };
    json_activity(StatusCode::OK, accept, &body)
}

async fn outbox_get(state: &ApState, req: Request<Body>, accept: Option<ActivityAccept>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    if let Err(resp) = verify_signature_if_present(state, &parts).await {
        return resp;
    }
    let req = Request::from_parts(parts, body);
    let base = state.cfg.public_base_url.trim_end_matches('/');
    let col = format!("{base}/users/{}/outbox", state.cfg.username);
    let query = req.uri().query().unwrap_or("");
    let is_page = query.contains("page=");
    let cursor = query
        .split('&')
        .find(|p| p.starts_with("cursor="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<i64>().ok());

    if !is_page {
        let total = state.social.list_outbox(0, None).map(|p| p.total).unwrap_or(0);
        let body = OrderedCollection {
            context: "https://www.w3.org/ns/activitystreams".to_string(),
            id: col.clone(),
            ty: "OrderedCollection".to_string(),
            totalItems: total,
            first: format!("{col}?page=true"),
        };
        return json_activity(StatusCode::OK, accept, &body);
    }

    let limit = 20;
    let page = match state.social.list_outbox(limit, cursor) {
        Ok(p) => p,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };

    let mut items = Vec::<serde_json::Value>::new();
    for raw in page.items {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&raw) {
            items.push(v);
        }
    }

    let next = page.next.as_ref().map(|c| format!("{col}?page=true&cursor={c}"));
    let id = if let Some(c) = cursor {
        format!("{col}?page=true&cursor={c}")
    } else {
        format!("{col}?page=true")
    };
    let body = OrderedCollectionPage {
        context: "https://www.w3.org/ns/activitystreams".to_string(),
        id,
        ty: "OrderedCollectionPage".to_string(),
        partOf: col,
        orderedItems: items,
        next,
    };
    json_activity(StatusCode::OK, accept, &body)
}

async fn outbox_post(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    let body_bytes = match axum::body::to_bytes(body, 2 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };

    let mut activity: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };

    // Basic guard: l'actor deve essere il nostro (se presente).
    let expected_actor = format!(
        "{}/users/{}",
        state.cfg.public_base_url.trim_end_matches('/'),
        state.cfg.username
    );
    let local_followers = format!(
        "{}/users/{}/followers",
        state.cfg.public_base_url.trim_end_matches('/'),
        state.cfg.username
    );
    if let Some(actor) = activity.get("actor").and_then(|v| v.as_str()) {
        if actor != expected_actor {
            return simple(StatusCode::FORBIDDEN, "actor mismatch");
        }
    } else {
        if let serde_json::Value::Object(map) = &mut activity {
            map.insert("actor".to_string(), serde_json::Value::String(expected_actor.clone()));
        }
    }

    // For public posts, ensure Followers is present so legacy delivery works as expected.
    if is_public_activity(&activity) && !activity_has_recipient(&activity, &local_followers) {
        activity_add_cc(&mut activity, &local_followers);
    }

    let activity_id = activity
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| state.social.new_activity_id(&expected_actor));

    if let serde_json::Value::Object(map) = &mut activity {
        map.insert("id".to_string(), serde_json::Value::String(activity_id.clone()));
    }

    let activity_to = activity.get("to").cloned();
    let activity_cc = activity.get("cc").cloned();

    let activity_type = activity.get("type").and_then(|v| v.as_str()).unwrap_or("");

    // If posting a Create(Note/Article) with embedded object, ensure the object has a stable local dereferenceable id.
    if activity_type == "Create" {
        if let Some(obj) = activity.get_mut("object") {
            if let serde_json::Value::Object(obj_map) = obj {
                let obj_id = obj_map
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .filter(|s| s.contains(&format!("/users/{}/objects/", state.cfg.username)));
                let object_id = obj_id.unwrap_or_else(|| {
                    let base = state.cfg.public_base_url.trim_end_matches('/');
                    let suffix = short_hash(&activity_id);
                    format!("{base}/users/{}/objects/{suffix}", state.cfg.username)
                });
                obj_map.insert("id".to_string(), serde_json::Value::String(object_id.clone()));
                obj_map
                    .entry("attributedTo".to_string())
                    .or_insert_with(|| serde_json::Value::String(expected_actor.clone()));

                // Legacy servers frequently dereference the object id later; propagate audience to the object
                // so visibility remains correct even out-of-context of the Create activity.
                if !obj_map.contains_key("to") {
                    if let Some(v) = activity_to.as_ref() {
                        obj_map.insert("to".to_string(), v.clone());
                    }
                }
                if !obj_map.contains_key("cc") {
                    if let Some(v) = activity_cc.as_ref() {
                        obj_map.insert("cc".to_string(), v.clone());
                    }
                }
                if !obj_map.contains_key("published") {
                    if let Some(p) = ms_to_rfc3339(now_ms()) {
                        obj_map.insert("published".to_string(), serde_json::Value::String(p));
                    }
                }

                normalize_note_for_compat(&state.cfg, obj_map, activity_to.as_ref(), activity_cc.as_ref());

                // If the UI references uploaded media by id, expand to ActivityStreams attachments.
                expand_media_attachments(state, obj_map);

                if let Ok(obj_bytes) = serde_json::to_vec(&serde_json::Value::Object(obj_map.clone())) {
                    let _ = state.social.upsert_object_with_actor(&object_id, Some(&expected_actor), obj_bytes);
                }
            }
        }
    } else if activity_type == "Update" {
        let Some(obj) = activity.get_mut("object") else {
            return simple(StatusCode::BAD_REQUEST, "missing object");
        };
        let Some(obj_map) = obj.as_object_mut() else {
            return simple(StatusCode::BAD_REQUEST, "invalid object");
        };
        let obj_id = obj_map
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_default();
        if obj_id.is_empty() {
            return simple(StatusCode::BAD_REQUEST, "missing object id");
        }
        let local_prefix = format!("/users/{}/objects/", state.cfg.username);
        if !obj_id.contains(&local_prefix) {
            return simple(StatusCode::FORBIDDEN, "not owner");
        }
        if let Some(attributed) = obj_map.get("attributedTo").and_then(|v| v.as_str()) {
            if attributed != expected_actor {
                return simple(StatusCode::FORBIDDEN, "not owner");
            }
        } else {
            obj_map.insert("attributedTo".to_string(), serde_json::Value::String(expected_actor.clone()));
        }
        if !obj_map.contains_key("updated") {
            if let Some(p) = ms_to_rfc3339(now_ms()) {
                obj_map.insert("updated".to_string(), serde_json::Value::String(p));
            }
        }
        if !obj_map.contains_key("to") {
            if let Some(v) = activity_to.as_ref() {
                obj_map.insert("to".to_string(), v.clone());
            }
        }
        if !obj_map.contains_key("cc") {
            if let Some(v) = activity_cc.as_ref() {
                obj_map.insert("cc".to_string(), v.clone());
            }
        }
        normalize_note_for_compat(&state.cfg, obj_map, activity_to.as_ref(), activity_cc.as_ref());
        expand_media_attachments(state, obj_map);
        if let Ok(obj_bytes) = serde_json::to_vec(&serde_json::Value::Object(obj_map.clone())) {
            let _ = state.social.upsert_object_with_actor(&obj_id, Some(&expected_actor), obj_bytes);
        }
    } else if activity_type == "Delete" {
        let obj_id = extract_object_id(&activity).unwrap_or_default();
        if obj_id.is_empty() {
            return simple(StatusCode::BAD_REQUEST, "missing object");
        }
        let local_prefix = format!("/users/{}/objects/", state.cfg.username);
        if !obj_id.contains(&local_prefix) {
            return simple(StatusCode::FORBIDDEN, "not owner");
        }
        if let Some((oid, tombstone_json, is_tombstone)) = extract_object_or_tombstone(&activity) {
            if is_tombstone {
                let _ = state.social.upsert_object_with_actor(&oid, Some(&expected_actor), tombstone_json);
                let _ = state.social.mark_object_deleted(&oid);
            } else {
                let _ = state.social.mark_object_deleted(&oid);
            }
        } else {
            let _ = state.social.mark_object_deleted(&obj_id);
        }
    }

    // Encode once: i server legacy si aspettano Activity JSON in inbox.
    let activity_bytes = match serde_json::to_vec(&activity) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "unable to encode activity"),
    };

    if let Err(e) = state.social.store_outbox(&activity_id, activity_bytes.clone()) {
        return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}"));
    }

    // Mirror local Like/Announce/EmojiReact and handle Undo for immediate UI counts.
    if let Some(ty) = activity.get("type").and_then(|v| v.as_str()) {
        let actor = expected_actor.as_str();
        if ty == "Like" || ty == "Announce" || ty == "EmojiReact" {
            if let Some(obj_id) = extract_object_id(&activity) {
                let content = activity
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let _ = state
                    .social
                    .upsert_reaction_with_content(&activity_id, ty, actor, &obj_id, content);
            }
        } else if ty == "Undo" {
            // Undo { object: { id?, type: Like|Announce|EmojiReact, object: <id>, content? } }
            let inner = activity.get("object");
            if let Some(obj) = inner {
                let inner_ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if inner_ty == "Like" || inner_ty == "Announce" || inner_ty == "EmojiReact" {
                    let object_id = obj.get("object").and_then(|v| v.as_str()).unwrap_or("").trim();
                    if !object_id.is_empty() {
                        let content = obj
                            .get("content")
                            .and_then(|v| v.as_str())
                            .map(str::trim)
                            .filter(|s| !s.is_empty());
                        let rid = obj
                            .get("id")
                            .and_then(|v| v.as_str())
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                            .or_else(|| state.social.find_reaction_id(actor, inner_ty, object_id, content).ok().flatten());
                        if let Some(rid) = rid {
                            let _ = state.social.remove_reaction(&rid);
                        }
                    }
                }
            }
        }
    }

    // Add our own public posts to the federated feed (what our "instance" has seen).
    if is_public_activity(&activity) {
        let _ = state
            .social
            .insert_federated_feed_item(&activity_id, Some(&expected_actor), activity_bytes.clone());

        // Also add to the DHT/global feed locally so a single peer sees its own public notes even
        // before any gossip round-trips.
        let _ = state
            .social
            .insert_global_feed_item(&activity_id, Some(&expected_actor), activity_bytes.clone());
    }

    let ty = activity.get("type").and_then(|v| v.as_str()).map(|s| s.to_string());
    let _ = state
        .ui_events
        .send(UiEvent::new("timeline", ty, Some(activity_id.clone())));

    // Index replies for UI: if this Create Note is a reply (inReplyTo), link it to the parent note id.
    if activity.get("type").and_then(|v| v.as_str()) == Some("Create") {
        if let Some(obj) = activity.get("object").and_then(|v| v.as_object()) {
            if obj.get("type").and_then(|v| v.as_str()) == Some("Note") {
                if let Some(in_reply_to) = obj.get("inReplyTo").and_then(|v| v.as_str()) {
                    let in_reply_to = in_reply_to.trim();
                    if !in_reply_to.is_empty() {
                        let _ = state.social.upsert_note_reply(in_reply_to, &activity_id, now_ms());
                    }
                }
            }
        }
    }

    // Enqueue per delivery affidabile (retry/backoff).
    let mut recipients = extract_recipients(&activity);
    if recipients.is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing recipients (to/cc)");
    }
    recipients.retain(|r| !is_blocked_actor(state, r));
    recipients.sort();
    recipients.dedup();

    let mut pending = 0u64;
    let has_followers = recipients.iter().any(|r| r == &local_followers);
    if has_followers {
        recipients.retain(|r| r != &local_followers);
        let base_recipients = recipients.clone();

        let mut cursor: Option<i64> = None;
        loop {
            let page = match state.social.list_followers(1000, cursor) {
                Ok(p) => p,
                Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
            };
            if page.items.is_empty() {
                break;
            }

            let mut batch = base_recipients.clone();
            batch.extend(page.items.into_iter().filter(|r| !is_blocked_actor(state, r)));
            batch.sort();
            batch.dedup();
            if !batch.is_empty() {
                pending = pending.saturating_add(match state.queue.enqueue_activity(activity_bytes.clone(), batch).await {
                    Ok(v) => v,
                    Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("enqueue failed: {e}")),
                });
            }

            cursor = page.next.as_deref().and_then(|s| s.parse::<i64>().ok());
            if cursor.is_none() {
                break;
            }
        }
    } else {
        if !recipients.is_empty() {
            pending = match state.queue.enqueue_activity(activity_bytes, recipients).await {
                Ok(v) => v,
                Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("enqueue failed: {e}")),
            };
        }
    }

    // Global timeline gossip (best-effort): publish only Public activities.
    if is_public_activity(&activity) {
        let _ = state
            .delivery
            .publish_gossip("/fedi3/global/1", serde_json::to_vec(&activity).unwrap_or_default())
            .await;
    }

    // Mastodon spesso accetta 202 per outbox processing.
    let _ = parts;
    simple(StatusCode::ACCEPTED, &format!("accepted (pending={pending})"))
}

fn activity_has_recipient(activity: &serde_json::Value, target: &str) -> bool {
    fn has(v: &serde_json::Value, target: &str) -> bool {
        match v {
            serde_json::Value::String(s) => s == target,
            serde_json::Value::Array(arr) => arr.iter().any(|it| has(it, target)),
            _ => false,
        }
    }
    activity.get("to").map(|v| has(v, target)).unwrap_or(false)
        || activity.get("cc").map(|v| has(v, target)).unwrap_or(false)
}

async fn ui_stream_get(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }

    let filter_kind = parts
        .uri
        .query()
        .unwrap_or("")
        .split('&')
        .find(|p| p.starts_with("kind="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_default();

    let rx = state.ui_events.subscribe();
    let stream = unfold(rx, move |mut rx| {
        let filter_kind = filter_kind.clone();
        async move {
            loop {
                match rx.recv().await {
                    Ok(ev) => {
                        if !filter_kind.is_empty() && ev.kind != filter_kind {
                            continue;
                        }
                        let data = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".to_string());
                        let evt = Event::default().event("fedi3").data(data);
                        return Some((Ok::<Event, Infallible>(evt), rx));
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => return None,
                }
            }
        }
    });

    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("keepalive"))
        .into_response()
}

fn activity_add_cc(activity: &mut serde_json::Value, target: &str) {
    let Some(map) = activity.as_object_mut() else { return };
    match map.get_mut("cc") {
        Some(serde_json::Value::Array(arr)) => arr.push(serde_json::Value::String(target.to_string())),
        Some(serde_json::Value::String(_)) => {
            map.insert(
                "cc".to_string(),
                serde_json::Value::Array(vec![serde_json::Value::String(target.to_string())]),
            );
        }
        _ => {
            map.insert(
                "cc".to_string(),
                serde_json::Value::Array(vec![serde_json::Value::String(target.to_string())]),
            );
        }
    }
}

fn expand_media_attachments(state: &ApState, obj_map: &mut serde_json::Map<String, serde_json::Value>) {
    // Support either `fedi3Media: ["<id>", ...]` or `attachment: ["<id>", ...]` (strings).
    let mut ids: Vec<String> = Vec::new();

    if let Some(v) = obj_map.remove("fedi3Media") {
        if let serde_json::Value::Array(arr) = v {
            for it in arr {
                if let Some(s) = it.as_str() {
                    let s = s.trim();
                    if !s.is_empty() {
                        ids.push(s.to_string());
                    }
                }
            }
        }
    }

    // If attachment is string-array, treat strings that are not URLs as media ids.
    let mut existing_objects: Vec<serde_json::Value> = Vec::new();
    if let Some(v) = obj_map.get("attachment") {
        match v {
            serde_json::Value::Array(arr) => {
                for it in arr {
                    match it {
                        serde_json::Value::String(s) => {
                            let s = s.trim();
                            if s.starts_with("http://") || s.starts_with("https://") {
                                // Keep explicit URLs as-is (string form).
                                existing_objects.push(serde_json::Value::String(s.to_string()));
                            } else if !s.is_empty() {
                                ids.push(s.to_string());
                            }
                        }
                        serde_json::Value::Object(_) => existing_objects.push(it.clone()),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    if ids.is_empty() {
        if !existing_objects.is_empty() {
            obj_map.insert("attachment".to_string(), serde_json::Value::Array(existing_objects));
        }
        return;
    }

    let mut out = existing_objects;
    for id in ids {
        let Ok(Some(item)) = state.social.get_media(&id) else { continue };
        let ty = if item.media_type.to_ascii_lowercase().starts_with("image/") {
            "Image"
        } else if item.media_type.to_ascii_lowercase().starts_with("video/") {
            "Video"
        } else if item.media_type.to_ascii_lowercase().starts_with("audio/") {
            "Audio"
        } else {
            "Document"
        };

        let mut doc = serde_json::Map::new();
        doc.insert("type".to_string(), serde_json::Value::String(ty.to_string()));
        doc.insert(
            "mediaType".to_string(),
            serde_json::Value::String(item.media_type.clone()),
        );
        doc.insert("url".to_string(), serde_json::Value::String(item.url.clone()));
        if let Some(bh) = item.blurhash.as_deref().filter(|s| !s.trim().is_empty()) {
            doc.insert("blurhash".to_string(), serde_json::Value::String(bh.to_string()));
        }
        if let Some(w) = item.width.and_then(|v| (v > 0).then_some(v as u64)) {
            doc.insert("width".to_string(), serde_json::Value::Number(w.into()));
        }
        if let Some(h) = item.height.and_then(|v| (v > 0).then_some(v as u64)) {
            doc.insert("height".to_string(), serde_json::Value::Number(h.into()));
        }
        out.push(serde_json::Value::Object(doc));
    }

    if !out.is_empty() {
        obj_map.insert("attachment".to_string(), serde_json::Value::Array(out));
    }
}

fn normalize_note_for_compat(
    cfg: &ApConfig,
    obj_map: &mut serde_json::Map<String, serde_json::Value>,
    activity_to: Option<&serde_json::Value>,
    activity_cc: Option<&serde_json::Value>,
) {
    let content = obj_map
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if !content.is_empty() {
        let is_html = looks_like_html(&content);
        if !is_html {
            if !obj_map.contains_key("source") {
                obj_map.insert(
                    "source".to_string(),
                    serde_json::json!({
                        "content": content,
                        "mediaType": "text/plain"
                    }),
                );
            }
            let html = format!("<p>{}</p>", html_escape_with_breaks(&content));
            obj_map.insert("content".to_string(), serde_json::Value::String(html));
        }
    }

    let mut tags = Vec::<serde_json::Value>::new();
    if let Some(existing) = obj_map.get("tag") {
        match existing {
            serde_json::Value::Array(arr) => tags.extend(arr.iter().cloned()),
            v => tags.push(v.clone()),
        }
    }

    let mut existing_names = std::collections::HashSet::<String>::new();
    for t in &tags {
        if let Some(name) = t.get("name").and_then(|v| v.as_str()) {
            existing_names.insert(name.to_string());
        }
    }

    let text_for_tags = obj_map
        .get("source")
        .and_then(|v| v.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or(&content);
    for tag in extract_hashtags(text_for_tags) {
        let name = format!("#{tag}");
        if existing_names.insert(name.clone()) {
            let href = format!(
                "{}/tags/{}",
                cfg.public_base_url.trim_end_matches('/'),
                urlencoding::encode(&tag)
            );
            tags.push(serde_json::json!({
                "type": "Hashtag",
                "name": name,
                "href": href,
            }));
        }
    }

    for actor in collect_recipient_actors(activity_to, activity_cc) {
        if let Some(name) = actor_mention_name(&actor) {
            if existing_names.insert(name.clone()) {
                tags.push(serde_json::json!({
                    "type": "Mention",
                    "name": name,
                    "href": actor,
                }));
            }
        }
    }

    if !tags.is_empty() {
        obj_map.insert("tag".to_string(), serde_json::Value::Array(tags));
    }
}

fn looks_like_html(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    t.contains('<') && t.contains('>')
}

fn html_escape_with_breaks(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\n' => out.push_str("<br>"),
            _ => out.push(ch),
        }
    }
    out
}

fn extract_hashtags(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut it = text.chars().peekable();
    while let Some(ch) = it.next() {
        if ch != '#' {
            continue;
        }
        let mut tag = String::new();
        while let Some(&c) = it.peek() {
            if c.is_alphanumeric() || c == '_' {
                tag.push(c);
                it.next();
            } else {
                break;
            }
        }
        if !tag.is_empty() && !out.contains(&tag) {
            out.push(tag);
        }
    }
    out
}

fn collect_recipient_actors(
    to: Option<&serde_json::Value>,
    cc: Option<&serde_json::Value>,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut add = |v: &serde_json::Value| {
        if let Some(s) = v.as_str() {
            if s.contains("activitystreams#Public") {
                return;
            }
            if s.ends_with("/followers") {
                return;
            }
            if s.starts_with("http://") || s.starts_with("https://") {
                out.push(s.to_string());
            }
        }
    };
    if let Some(v) = to {
        match v {
            serde_json::Value::Array(arr) => arr.iter().for_each(&mut add),
            _ => add(v),
        }
    }
    if let Some(v) = cc {
        match v {
            serde_json::Value::Array(arr) => arr.iter().for_each(&mut add),
            _ => add(v),
        }
    }
    out.sort();
    out.dedup();
    out
}

fn actor_mention_name(actor: &str) -> Option<String> {
    let uri = actor.parse::<Uri>().ok()?;
    let host = uri.host()?.trim();
    if host.is_empty() {
        return None;
    }
    let path = uri.path();
    let mut user = None;
    if let Some(rest) = path.strip_prefix("/users/") {
        user = rest.split('/').next().map(str::to_string);
    } else if let Some(rest) = path.strip_prefix("/@") {
        user = rest.split('/').next().map(str::to_string);
    }
    let user = user?;
    if user.is_empty() {
        return None;
    }
    Some(format!("@{user}@{host}"))
}

async fn inbox(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    let body_bytes = match axum::body::to_bytes(body, 2 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };
    if let Some(ip) = client_ip_from_headers(&parts.headers) {
        if let Err(resp) = inbox_rate_limit(state, &format!("ip:{ip}"), body_bytes.len() as u64).await {
            state.net.rate_limit_hit();
            let _ = state
                .social
                .insert_audit_event("rate_limit", None, None, None, false, Some("429"), Some("inbox ip"));
            return resp;
        }
    }

    let receipt_activity_id = crate::delivery_queue::activity_id_from_bytes(&body_bytes).unwrap_or_default();

    if let Err(e) = verify_digest_if_present(&parts.headers, &body_bytes) {
        let _ = state.social.insert_audit_event("inbox", None, None, None, false, Some("401"), Some("digest invalid"));
        return simple(StatusCode::UNAUTHORIZED, &format!("digest invalid: {e}"));
    }
    if let Err(e) = verify_date(&parts.headers, state.max_date_skew) {
        let _ = state.social.insert_audit_event("inbox", None, None, None, false, Some("401"), Some("date invalid"));
        return simple(StatusCode::UNAUTHORIZED, &format!("date invalid: {e}"));
    }

    let sig_header = parts
        .headers
        .get("Signature")
        .or_else(|| parts.headers.get("signature"))
        .and_then(|v| v.to_str().ok());

    let Some(sig_header) = sig_header else {
        let _ = state.social.insert_audit_event("inbox", None, None, None, false, Some("401"), Some("missing Signature"));
        return simple(StatusCode::UNAUTHORIZED, "missing Signature header");
    };

    let sig = match parse_signature_header(sig_header) {
        Ok(v) => v,
        Err(e) => {
            let _ = state.social.insert_audit_event("inbox", None, None, None, false, Some("401"), Some("bad Signature"));
            return simple(StatusCode::UNAUTHORIZED, &format!("bad Signature: {e}"));
        }
    };

    // Temp blocks (best-effort anti-abuse) on keyId, before expensive key resolution.
    if let Ok(Some(_until)) = state.social.abuse_check_blocked(&abuse_key_for_key_id(&sig.key_id)) {
        let _ = state.social.insert_audit_event("inbox", None, Some(&sig.key_id), None, false, Some("429"), Some("temp blocked keyId"));
        return simple(StatusCode::TOO_MANY_REQUESTS, "rate limited");
    }

    // Basic per-sender rate limiting keyed by Signature.keyId (best-effort).
    if let Err(resp) = inbox_rate_limit(state, &sig.key_id, body_bytes.len() as u64).await {
        return resp;
    }

    let signing_string = match build_signing_string(&parts.method, &parts.uri, &parts.headers, &sig.headers) {
        Ok(s) => s,
        Err(e) => return simple(StatusCode::UNAUTHORIZED, &format!("bad signed headers: {e}")),
    };

    let summary = match state.key_resolver.resolve_actor_summary_for_key_id(&sig.key_id).await {
        Ok(s) => s,
        Err(e) => {
            let _ = state.social.abuse_record_strike(&abuse_key_for_key_id(&sig.key_id), 1);
            let _ = state.social.insert_audit_event("inbox", None, Some(&sig.key_id), None, false, Some("401"), Some("key resolve failed"));
            return simple(StatusCode::UNAUTHORIZED, &format!("key resolve failed: {e}"));
        }
    };
    let pem = summary.public_key_pem.clone();

    if let Err(e) = verify_signature_rsa_sha256(&pem, &signing_string, &sig.signature) {
        let _ = state.social.abuse_record_strike(&abuse_key_for_key_id(&sig.key_id), 1);
        let _ = state.social.abuse_record_strike(&abuse_key_for_actor(&summary.actor_url), 1);
        let _ = state.social.insert_audit_event("inbox", Some(&summary.actor_url), Some(&sig.key_id), None, false, Some("401"), Some("signature invalid"));
        return simple(StatusCode::UNAUTHORIZED, &format!("signature invalid: {e}"));
    }

    // Post-auth temp block on actor id.
    if let Ok(Some(_until)) = state.social.abuse_check_blocked(&abuse_key_for_actor(&summary.actor_url)) {
        let _ = state.social.insert_audit_event("inbox", Some(&summary.actor_url), Some(&sig.key_id), None, false, Some("429"), Some("temp blocked actor"));
        return simple(StatusCode::TOO_MANY_REQUESTS, "rate limited");
    }

    if is_blocked_actor(state, &summary.actor_url) {
        let _ = state.social.insert_audit_event("inbox", Some(&summary.actor_url), Some(&sig.key_id), None, false, Some("403"), Some("blocked"));
        return simple(StatusCode::FORBIDDEN, "blocked");
    }

    // Secondary rate limiting on resolved actor url (post-auth).
    if let Err(resp) =
        inbox_rate_limit(state, &format!("actor:{}", summary.actor_url), body_bytes.len() as u64).await
    {
        return resp;
    }

    // Persistent quotas (best-effort) after authentication.
    let bytes = body_bytes.len() as u64;
    let key_quota = format!("day:key:{}", short_hash(&sig.key_id));
    if let Err(resp) = inbox_quota_limit(state, &key_quota, bytes) {
        let _ = state.social.insert_audit_event("inbox", Some(&summary.actor_url), Some(&sig.key_id), None, false, Some("429"), Some("quota keyId/day"));
        return resp;
    }
    let actor_quota = format!("day:actor:{}", short_hash(&summary.actor_url));
    if let Err(resp) = inbox_quota_limit(state, &actor_quota, bytes) {
        let _ = state.social.insert_audit_event("inbox", Some(&summary.actor_url), Some(&sig.key_id), None, false, Some("429"), Some("quota actor/day"));
        return resp;
    }
    if let Some(host) = host_from_url(&summary.actor_url) {
        let host_quota = format!("day:host:{}", short_hash(&host));
        if let Err(resp) = inbox_quota_limit(state, &host_quota, bytes) {
            let _ = state.social.insert_audit_event("inbox", Some(&summary.actor_url), Some(&sig.key_id), None, false, Some("429"), Some("quota host/day"));
            return resp;
        }
    }

    let _ = state.social.upsert_actor_meta(&summary.actor_url, summary.is_fedi3);

    // Processing minimale: Follow / Accept / Undo (per compatibilità).
    let activity: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(_) => {
            let _ = state.social.insert_audit_event("inbox", Some(&summary.actor_url), Some(&sig.key_id), None, false, Some("400"), Some("invalid json"));
            return simple(StatusCode::BAD_REQUEST, "invalid activity json");
        }
    };
    let dedup_id = activity_dedup_id(&activity);
    match state.social.mark_inbox_seen(&dedup_id) {
        Ok(true) => {}
        Ok(false) => {
            let _ = state.social.insert_audit_event("inbox", Some(&summary.actor_url), Some(&sig.key_id), Some(&dedup_id), true, Some("202"), Some("duplicate"));
            return simple(StatusCode::ACCEPTED, "duplicate");
        }
        Err(e) => {
            let _ = state.social.insert_audit_event("inbox", Some(&summary.actor_url), Some(&sig.key_id), Some(&dedup_id), false, Some("502"), Some("db error"));
            return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}"));
        }
    }

    if let Err(e) = process_inbox_activity(state, &activity).await {
        let _ = state.social.insert_audit_event("inbox", Some(&summary.actor_url), Some(&sig.key_id), Some(&dedup_id), false, Some("502"), Some("processing error"));
        return simple(StatusCode::BAD_GATEWAY, &format!("processing error: {e}"));
    }

    // Best-effort delivery receipt for Fedi3 peers: helps the sender's queue handle long offline periods.
    if summary.is_fedi3 && !receipt_activity_id.is_empty() {
        let _ = send_delivery_receipt_best_effort(state, &summary.actor_url, &summary.public_key_pem, &receipt_activity_id).await;
    }

    let _ = state.social.insert_audit_event("inbox", Some(&summary.actor_url), Some(&sig.key_id), Some(&dedup_id), true, Some("202"), None);
    simple(StatusCode::ACCEPTED, "accepted")
}

#[derive(Debug, serde::Deserialize)]
struct ReceiptReq {
    activity_id: String,
}

async fn receipt_post(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    let body_bytes = match axum::body::to_bytes(body, 64 * 1024).await {
        Ok(b) => b,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid body"),
    };

    if let Err(e) = verify_digest_if_present(&parts.headers, &body_bytes) {
        let _ = state.social.insert_audit_event("receipt", None, None, None, false, Some("401"), Some("digest invalid"));
        return simple(StatusCode::UNAUTHORIZED, &format!("digest invalid: {e}"));
    }
    if let Err(e) = verify_date(&parts.headers, state.max_date_skew) {
        let _ = state.social.insert_audit_event("receipt", None, None, None, false, Some("401"), Some("date invalid"));
        return simple(StatusCode::UNAUTHORIZED, &format!("date invalid: {e}"));
    }

    let sig_header = parts
        .headers
        .get("Signature")
        .or_else(|| parts.headers.get("signature"))
        .and_then(|v| v.to_str().ok());

    let Some(sig_header) = sig_header else {
        let _ = state.social.insert_audit_event("receipt", None, None, None, false, Some("401"), Some("missing Signature"));
        return simple(StatusCode::UNAUTHORIZED, "missing Signature header");
    };

    let sig = match parse_signature_header(sig_header) {
        Ok(v) => v,
        Err(e) => {
            let _ = state.social.insert_audit_event("receipt", None, None, None, false, Some("401"), Some("bad Signature"));
            return simple(StatusCode::UNAUTHORIZED, &format!("bad Signature: {e}"));
        }
    };

    if let Ok(Some(_until)) = state.social.abuse_check_blocked(&abuse_key_for_key_id(&sig.key_id)) {
        let _ = state.social.insert_audit_event("receipt", None, Some(&sig.key_id), None, false, Some("429"), Some("temp blocked keyId"));
        return simple(StatusCode::TOO_MANY_REQUESTS, "rate limited");
    }

    let signing_string = match build_signing_string(&parts.method, &parts.uri, &parts.headers, &sig.headers) {
        Ok(s) => s,
        Err(e) => return simple(StatusCode::UNAUTHORIZED, &format!("bad signed headers: {e}")),
    };

    let summary = match state.key_resolver.resolve_actor_summary_for_key_id(&sig.key_id).await {
        Ok(s) => s,
        Err(e) => {
            let _ = state.social.abuse_record_strike(&abuse_key_for_key_id(&sig.key_id), 1);
            let _ = state.social.insert_audit_event("receipt", None, Some(&sig.key_id), None, false, Some("401"), Some("key resolve failed"));
            return simple(StatusCode::UNAUTHORIZED, &format!("key resolve failed: {e}"));
        }
    };

    if let Err(e) = verify_signature_rsa_sha256(&summary.public_key_pem, &signing_string, &sig.signature) {
        let _ = state.social.abuse_record_strike(&abuse_key_for_key_id(&sig.key_id), 1);
        let _ = state.social.abuse_record_strike(&abuse_key_for_actor(&summary.actor_url), 1);
        let _ = state.social.insert_audit_event("receipt", Some(&summary.actor_url), Some(&sig.key_id), None, false, Some("401"), Some("signature invalid"));
        return simple(StatusCode::UNAUTHORIZED, &format!("signature invalid: {e}"));
    }

    if let Ok(Some(_until)) = state.social.abuse_check_blocked(&abuse_key_for_actor(&summary.actor_url)) {
        let _ = state.social.insert_audit_event("receipt", Some(&summary.actor_url), Some(&sig.key_id), None, false, Some("429"), Some("temp blocked actor"));
        return simple(StatusCode::TOO_MANY_REQUESTS, "rate limited");
    }

    let input: ReceiptReq = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(_) => return simple(StatusCode::BAD_REQUEST, "invalid json"),
    };

    let updated = match state
        .queue
        .mark_delivered_by_receipt(&input.activity_id, &summary.actor_url)
        .await
    {
        Ok(n) => n,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };

    let _ = state.social.insert_audit_event("receipt", Some(&summary.actor_url), Some(&sig.key_id), Some(&input.activity_id), true, Some("200"), Some(&format!("updated={updated}")));
    axum::Json(serde_json::json!({ "ok": true, "updated": updated })).into_response()
}

async fn send_delivery_receipt_best_effort(
    state: &ApState,
    sender_actor_url: &str,
    sender_public_key_pem: &str,
    activity_id: &str,
) -> anyhow::Result<()> {
    let sender_actor_url = sender_actor_url.trim();
    let activity_id = activity_id.trim();
    if sender_actor_url.is_empty() || activity_id.is_empty() {
        return Ok(());
    }

    let sender_uri: Uri = sender_actor_url.parse()?;
    let Some(scheme) = sender_uri.scheme_str() else { return Ok(()) };
    let Some(auth) = sender_uri.authority() else { return Ok(()) };
    let sender_base = format!("{scheme}://{auth}");
    let receipt_url = format!("{sender_base}/.fedi3/receipt");

    let base = state.cfg.public_base_url.trim_end_matches('/');
    let me = format!("{base}/users/{}", state.cfg.username);
    let my_key_id = format!("{me}#main-key");

    let body = serde_json::json!({
        "activity_id": activity_id,
    });
    let body_bytes = serde_json::to_vec(&body).unwrap_or_default();

    // Prefer P2P/WebRTC when available; fall back to HTTPS.
    let actor_info = state.delivery.resolve_actor_info(sender_actor_url).await.ok();
    if let Some(info) = actor_info {
        if let Some(peer_id) = info.p2p_peer_id.as_deref() {
            if !info.p2p_peer_addrs.is_empty() {
                let _ = state.delivery.p2p_add_peer_addrs(peer_id, info.p2p_peer_addrs).await;
            }
            if state
                .delivery
                .deliver_json_p2p(peer_id, &state.private_key_pem, &my_key_id, &receipt_url, Some(sender_public_key_pem), &body_bytes)
                .await
                .is_ok()
            {
                return Ok(());
            }
            if state
                .delivery
                .deliver_json_webrtc(sender_actor_url, peer_id, &state.private_key_pem, &my_key_id, &receipt_url, &body_bytes)
                .await
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    let _ = state
        .delivery
        .deliver_json(&state.private_key_pem, &my_key_id, &receipt_url, &body_bytes)
        .await;
    Ok(())
}

fn is_blocked_actor(state: &ApState, actor_url: &str) -> bool {
    if state.cfg.blocked_actors.iter().any(|a| a == actor_url) {
        return true;
    }
    if state.social.is_actor_blocked(actor_url).unwrap_or(false) {
        return true;
    }
    let Some(host) = host_from_url(actor_url) else { return false };
    state.cfg.blocked_domains.iter().any(|p| domain_matches(&host, p))
}

fn host_from_url(url: &str) -> Option<String> {
    let uri: Uri = url.parse().ok()?;
    uri.host().map(|h| h.to_ascii_lowercase())
}

fn client_ip_from_headers(headers: &HeaderMap) -> Option<String> {
    if let Some(v) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(ip) = v.split(',').next().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            return Some(ip.to_string());
        }
    }
    headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn domain_matches(host: &str, pattern: &str) -> bool {
    let host = host.trim().to_ascii_lowercase();
    let p = pattern.trim().trim_end_matches('.').to_ascii_lowercase();
    if p.is_empty() {
        return false;
    }
    if let Some(suffix) = p.strip_prefix("*.") {
        return host == suffix || host.ends_with(&format!(".{suffix}"));
    }
    if let Some(suffix) = p.strip_prefix('.') {
        return host == suffix || host.ends_with(&format!(".{suffix}"));
    }
    host == p
}

async fn inbox_rate_limit(state: &ApState, key_id: &str, bytes: u64) -> Result<(), Response<Body>> {
    let limits = state.inbox_limits.clone();
    let now = now_ms();
    let mut guard = state.inbox_limiter.lock().await;
    let st = guard.entry(key_id.to_string()).or_default();
    if now.saturating_sub(st.window_start_ms) >= 60_000 {
        st.window_start_ms = now;
        st.reqs = 0;
        st.bytes = 0;
    }
    if st.reqs.saturating_add(1) > limits.max_reqs_per_min.max(1) {
        return Err(simple(StatusCode::TOO_MANY_REQUESTS, "rate limited"));
    }
    if st.bytes.saturating_add(bytes) > limits.max_bytes_per_min.max(1024) {
        return Err(simple(StatusCode::TOO_MANY_REQUESTS, "rate limited"));
    }
    st.reqs = st.reqs.saturating_add(1);
    st.bytes = st.bytes.saturating_add(bytes);
    // Avoid unbounded growth.
    if guard.len() > 50_000 {
        guard.retain(|_, v| now.saturating_sub(v.window_start_ms) < 10 * 60_000);
    }
    Ok(())
}

fn inbox_quota_limit(state: &ApState, key: &str, bytes: u64) -> Result<(), Response<Body>> {
    let limits = state.inbox_limits.clone();
    let window_ms: i64 = 24 * 3600 * 1000;
    let ok = state
        .social
        .bump_inbox_quota(key, window_ms, limits.max_reqs_per_day, limits.max_bytes_per_day, bytes)
        .unwrap_or(true);
    if !ok {
        return Err(simple(StatusCode::TOO_MANY_REQUESTS, "rate limited"));
    }
    Ok(())
}

fn short_hash(s: &str) -> String {
    use sha2::Digest as _;
    let mut h = sha2::Sha256::new();
    h.update(s.as_bytes());
    hex::encode(&h.finalize()[..8])
}

fn abuse_key_for_key_id(key_id: &str) -> String {
    format!("key:{}", short_hash(key_id))
}

fn abuse_key_for_actor(actor_url: &str) -> String {
    format!("actor:{}", short_hash(actor_url))
}

fn activity_dedup_id(activity: &serde_json::Value) -> String {
    if let Some(id) = activity.get("id").and_then(|v| v.as_str()) {
        let id = id.trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }
    let bytes = canonical_json_bytes(activity);
    let mut hasher = sha2::Sha256::new();
    use sha2::Digest as _;
    hasher.update(&bytes);
    format!("urn:fedi3:inbox:{}", hex::encode(hasher.finalize()))
}

pub(crate) fn activity_dedup_id_public(activity: &serde_json::Value) -> String {
    activity_dedup_id(activity)
}

fn canonical_json_bytes(v: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&canonicalize_json(v)).unwrap_or_else(|_| b"null".to_vec())
}

fn canonicalize_json(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                if let Some(val) = map.get(&k) {
                    out.insert(k, canonicalize_json(val));
                }
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(canonicalize_json).collect())
        }
        _ => v.clone(),
    }
}

pub(crate) async fn process_inbox_activity(state: &ApState, activity: &serde_json::Value) -> anyhow::Result<()> {
    let ty = activity.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let base = state.cfg.public_base_url.trim_end_matches('/');
    let me = format!("{base}/users/{}", state.cfg.username);

    match ty {
        "Create" | "Update" | "Delete" | "Announce" | "Like" | "EmojiReact" => {
            store_generic_activity(state, activity)?;

            // Basic object/reaction storage to support timelines and interactions.
            let actor = activity.get("actor").and_then(|v| v.as_str()).unwrap_or("");
            match ty {
                "Create" | "Update" => {
                    if let Some((obj_id, obj_json)) = extract_object(activity) {
                        let actor_opt = if actor.is_empty() { None } else { Some(actor) };

                        // Index replies (Create Note with inReplyTo) so UI can show replies under posts.
                        if ty == "Create" {
                            if let Ok(obj_v) = serde_json::from_slice::<serde_json::Value>(&obj_json) {
                                if obj_v.get("type").and_then(|v| v.as_str()) == Some("Note") {
                                    if let Some(in_reply_to) = obj_v.get("inReplyTo").and_then(|v| v.as_str()) {
                                        let in_reply_to = in_reply_to.trim();
                                        if !in_reply_to.is_empty() {
                                            let act_store_id = activity_dedup_id(activity);
                                            let _ = state.social.upsert_note_reply(in_reply_to, &act_store_id, now_ms());
                                        }
                                    }
                                }
                            }
                        }

                        let _ = state.social.upsert_object_with_actor(&obj_id, actor_opt, obj_json);
                    } else if let Some(obj_id) = extract_object_id(activity) {
                        let _ = fetch_and_store_object(state, &obj_id).await;
                    }
                }
                "Delete" => {
                    if let Some(obj_id) = extract_object_id(activity) {
                        // Delete is terminal; store tombstone if provided, otherwise mark deleted and try fetch.
                        if let Some((oid, tombstone_json, is_tombstone)) = extract_object_or_tombstone(activity) {
                            if is_tombstone {
                                let actor_opt = if actor.is_empty() { None } else { Some(actor) };
                                let _ = state.social.upsert_object_with_actor(&oid, actor_opt, tombstone_json);
                                let _ = state.social.mark_object_deleted(&oid);
                            } else {
                                let _ = state.social.mark_object_deleted(&oid);
                            }
                        } else {
                            let _ = state.social.mark_object_deleted(&obj_id);
                            let _ = fetch_and_store_object(state, &obj_id).await;
                        }
                    }
                }
                "Announce" | "Like" | "EmojiReact" => {
                    if !actor.is_empty() {
                        if let Some(obj_id) = extract_object_id(activity) {
                            let act_id = activity.get("id").and_then(|v| v.as_str()).unwrap_or("");
                            if !act_id.is_empty() {
                                let content = activity.get("content").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty());
                                let _ = state
                                    .social
                                    .upsert_reaction_with_content(act_id, ty, actor, &obj_id, content);
                            }
                            // Ensure we have the object locally.
                            let _ = fetch_and_store_object(state, &obj_id).await;
                        }
                    }
                }
                _ => {}
            }

            let act_id = activity
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.trim().is_empty());
            let _ = state
                .ui_events
                .send(UiEvent::new("inbox", Some(ty.to_string()), act_id));
            Ok(())
        }
        "Follow" => {
            let actor = activity.get("actor").and_then(|v| v.as_str()).unwrap_or("");
            let object = activity.get("object").and_then(|v| v.as_str()).unwrap_or("");
            if actor.is_empty() || object != me {
                return Ok(());
            }
            let act_id = activity
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.trim().is_empty());
            let _ = state
                .ui_events
                .send(UiEvent::new("notification", Some("Follow".to_string()), act_id));
            state.social.add_follower(actor)?;
            if let Some(fid) = activity.get("id").and_then(|v| v.as_str()) {
                let _ = state.social.remember_inbox_follow(fid, actor);
            }

            // Invia Accept al follower.
            let accept_id = state.social.new_activity_id(&me);
            let accept = serde_json::json!({
              "@context": "https://www.w3.org/ns/activitystreams",
              "id": accept_id,
              "type": "Accept",
              "actor": me,
              "object": activity,
              "to": [actor]
            });
            let bytes = serde_json::to_vec(&accept)?;
            let id = accept.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if !id.is_empty() {
                let _ = state.social.store_outbox(id, bytes.clone());
            }
            state.queue.enqueue_activity(bytes, vec![actor.to_string()]).await?;
            let _ = state
                .ui_events
                .send(UiEvent::new("timeline", Some("Accept".to_string()), Some(accept_id.clone())));
            Ok(())
        }
        "Accept" => {
            // Se qualcuno accetta un nostro Follow, segna following accepted.
            let accept_actor = activity.get("actor").and_then(|v| v.as_str()).unwrap_or("");
            let obj = activity.get("object");
            let Some(obj) = obj else { return Ok(()) };

            // Some servers send object as the Follow activity id (string). Best-effort fetch.
            let follow_obj: serde_json::Value = match obj {
                serde_json::Value::String(id_or_url) => {
                    resolve_activity_ref(state, id_or_url).await.unwrap_or(serde_json::Value::Null)
                }
                _ => obj.clone(),
            };

            let obj_type = follow_obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if obj_type != "Follow" {
                if !accept_actor.is_empty() {
                    if let Ok(Some(FollowingStatus::Pending)) = state.social.get_following_status(accept_actor) {
                        state.social.set_following(accept_actor, FollowingStatus::Accepted)?;
                        let act_id = activity
                            .get("id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .filter(|s| !s.trim().is_empty());
                        let _ = state
                            .ui_events
                            .send(UiEvent::new("notification", Some("Accept".to_string()), act_id));
                    }
                }
                return Ok(());
            }
            let obj_actor = follow_obj.get("actor").and_then(|v| v.as_str()).unwrap_or("");
            let obj_object = follow_obj.get("object").and_then(|v| v.as_str()).unwrap_or("");
            if obj_actor != me || obj_object.is_empty() {
                return Ok(());
            }
            state.social.set_following(obj_object, FollowingStatus::Accepted)?;
            let act_id = activity
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.trim().is_empty());
            let _ = state
                .ui_events
                .send(UiEvent::new("notification", Some("Accept".to_string()), act_id));
            Ok(())
        }
        "Reject" => {
            // Reject of our Follow: remove following entry (best-effort).
            let reject_actor = activity.get("actor").and_then(|v| v.as_str()).unwrap_or("");
            let obj = activity.get("object");
            let Some(obj) = obj else { return Ok(()) };
            let follow_obj: serde_json::Value = match obj {
                serde_json::Value::String(id_or_url) => {
                    resolve_activity_ref(state, id_or_url).await.unwrap_or(serde_json::Value::Null)
                }
                _ => obj.clone(),
            };
            if follow_obj.get("type").and_then(|v| v.as_str()).unwrap_or("") != "Follow" {
                if !reject_actor.is_empty() {
                    if let Ok(Some(FollowingStatus::Pending)) = state.social.get_following_status(reject_actor) {
                        let _ = state.social.remove_following(reject_actor);
                        let act_id = activity
                            .get("id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .filter(|s| !s.trim().is_empty());
                        let _ = state
                            .ui_events
                            .send(UiEvent::new("notification", Some("Reject".to_string()), act_id));
                    }
                }
                return Ok(());
            }
            let obj_actor = follow_obj.get("actor").and_then(|v| v.as_str()).unwrap_or("");
            let obj_object = follow_obj.get("object").and_then(|v| v.as_str()).unwrap_or("");
            if obj_actor != me || obj_object.is_empty() {
                return Ok(());
            }
            let _ = state.social.remove_following(obj_object);
            let act_id = activity
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.trim().is_empty());
            let _ = state
                .ui_events
                .send(UiEvent::new("notification", Some("Reject".to_string()), act_id));
            Ok(())
        }
        "Undo" => {
            // Undo Follow/Like/Announce: rimuovi follower o reaction.
            let actor = activity.get("actor").and_then(|v| v.as_str()).unwrap_or("");
            let obj = activity.get("object");
            let Some(obj) = obj else { return Ok(()) };

            // Some servers send object as the original activity id (string). Best-effort fetch.
            let obj_val: serde_json::Value = match obj {
                serde_json::Value::String(id_or_url) => {
                    resolve_activity_ref(state, id_or_url)
                        .await
                        .unwrap_or_else(|| serde_json::Value::String(id_or_url.clone()))
                }
                _ => obj.clone(),
            };

            // If we still only have an id string, try to treat it as reaction/follow activity id.
            if let serde_json::Value::String(id) = &obj_val {
                let _ = state.social.remove_reaction(id);
                // If this was an Undo(Follow) with object=id only, use our inbox follow index.
                if !actor.is_empty() {
                    if let Ok(Some(follow_actor)) = state.social.get_inbox_follow_actor(id) {
                        if follow_actor == actor {
                            let _ = state.social.remove_follower(&follow_actor);
                            let _ = state.social.forget_inbox_follow(id);
                        }
                    }
                }
                return Ok(());
            }

            let obj_type = obj_val.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match obj_type {
                "Follow" => {
                    let obj_actor = obj_val.get("actor").and_then(|v| v.as_str()).unwrap_or("");
                    let obj_object = obj_val.get("object").and_then(|v| v.as_str()).unwrap_or("");
                    if actor.is_empty() || obj_actor != actor || obj_object != me {
                        return Ok(());
                    }
                    if let Some(fid) = obj_val.get("id").and_then(|v| v.as_str()) {
                        let _ = state.social.forget_inbox_follow(fid);
                    }
                    state.social.remove_follower(actor)?;
                    Ok(())
                }
                "Like" | "Announce" => {
                    // Many servers use object.id for the reaction activity id; fallback to embedded id.
                    let reaction_id = obj_val.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    if !reaction_id.is_empty() {
                        let _ = state.social.remove_reaction(reaction_id);
                    }
                    Ok(())
                }
                _ => Ok(()),
            }
        }
        _ => Ok(()),
    }
}

fn store_generic_activity(state: &ApState, activity: &serde_json::Value) -> anyhow::Result<()> {
    let actor = extract_actor(activity);
    let ty = activity.get("type").and_then(|v| v.as_str());
    let bytes = canonical_json_bytes(activity);
    let id = activity_dedup_id(activity);

    state.social.store_inbox_activity(&id, actor.as_deref(), ty, bytes.clone())?;
    if is_public_activity(activity) {
        let _ = state.social.insert_federated_feed_item(&id, actor.as_deref(), bytes);
    }
    Ok(())
}

async fn reactions_get(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let query = parts.uri.query().unwrap_or("");
    let object_id = query
        .split('&')
        .find(|p| p.starts_with("object="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_default();
    if object_id.trim().is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing object");
    }
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(50)
        .min(200);

    let rows = match state.social.list_reaction_counts(&object_id, limit) {
        Ok(v) => v,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };
    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(ty, content, count)| serde_json::json!({ "type": ty, "content": content, "count": count }))
        .collect();
    axum::Json(serde_json::json!({ "object": object_id, "items": items })).into_response()
}

async fn reactions_me_get(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let query = parts.uri.query().unwrap_or("");
    let object_id = query
        .split('&')
        .find(|p| p.starts_with("object="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_default();
    if object_id.trim().is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing object");
    }

    let base = state.cfg.public_base_url.trim_end_matches('/');
    let me = format!("{base}/users/{}", state.cfg.username);

    let rows = match state.social.list_reactions_for_actor_object(&me, &object_id, 100) {
        Ok(v) => v,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };

    let mut like_id: Option<String> = None;
    let mut announce_id: Option<String> = None;
    let mut emojis: Vec<serde_json::Value> = Vec::new();
    for (rid, ty, content) in rows {
        match ty.as_str() {
            "Like" => {
                if like_id.is_none() {
                    like_id = Some(rid);
                }
            }
            "Announce" => {
                if announce_id.is_none() {
                    announce_id = Some(rid);
                }
            }
            "EmojiReact" => {
                let c = content.unwrap_or_default();
                if c.trim().is_empty() {
                    continue;
                }
                emojis.push(serde_json::json!({ "id": rid, "content": c }));
            }
            _ => {}
        }
    }

    axum::Json(serde_json::json!({
      "object": object_id,
      "like": like_id,
      "announce": announce_id,
      "emojis": emojis,
    }))
    .into_response()
}

async fn reactions_actors_get(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let query = parts.uri.query().unwrap_or("");
    let object_id = query
        .split('&')
        .find(|p| p.starts_with("object="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_default();
    if object_id.trim().is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing object");
    }
    let ty = query
        .split('&')
        .find(|p| p.starts_with("type="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v))
        .unwrap_or_default();
    if ty.trim().is_empty() {
        return simple(StatusCode::BAD_REQUEST, "missing type");
    }
    let content = query
        .split('&')
        .find(|p| p.starts_with("content="))
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| url_decode(v));
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(50)
        .min(200);

    let rows = match state.social.list_reaction_actors_for_object(&object_id, &ty, content.as_deref(), limit) {
        Ok(v) => v,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };
    axum::Json(serde_json::json!({
      "object": object_id,
      "type": ty,
      "content": content,
      "items": rows,
    }))
    .into_response()
}

async fn notifications_get(state: &ApState, req: Request<Body>) -> Response<Body> {
    let (parts, _body) = req.into_parts();
    if let Err(resp) = require_internal(state, &parts.headers) {
        return resp;
    }
    let query = parts.uri.query().unwrap_or("");
    let limit = query
        .split('&')
        .find(|p| p.starts_with("limit="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(50)
        .min(200);
    let cursor = query
        .split('&')
        .find(|p| p.starts_with("cursor="))
        .and_then(|p| p.split_once('='))
        .and_then(|(_, v)| v.parse::<i64>().ok());

    let page = match state.social.list_inbox_notifications(limit.saturating_mul(3), cursor) {
        Ok(p) => p,
        Err(e) => return simple(StatusCode::BAD_GATEWAY, &format!("db error: {e}")),
    };

    let base = state.cfg.public_base_url.trim_end_matches('/');
    let me_actor = format!("{base}/users/{}", state.cfg.username);
    let my_handle = format!("@{}@{}", state.cfg.username, state.cfg.domain);

    let mut items = Vec::<serde_json::Value>::new();
    let mut cache: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
    for (raw, ts) in page.items {
        if items.len() as u32 >= limit {
            break;
        }
        if let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(&raw) {
            hydrate_activity(state, &mut v, &mut cache);
            if let Some(ty) = v.get("type").and_then(|v| v.as_str()) {
                if ty == "Create" || ty == "Announce" {
                    if let Some(serde_json::Value::String(obj_ref)) = v.get("object") {
                        if let Some(obj) = resolve_activity_ref(state, obj_ref).await {
                            if let Some(map) = v.as_object_mut() {
                                map.insert("object".to_string(), obj);
                            }
                        }
                    }
                }
            }
            if is_notification(&v, &me_actor, &my_handle) {
                items.push(serde_json::json!({ "ts": ts, "activity": v }));
            }
        }
    }

    axum::Json(serde_json::json!({
      "items": items,
      "next": page.next,
    }))
    .into_response()
}

fn is_notification(activity: &serde_json::Value, me_actor: &str, my_handle: &str) -> bool {
    let ty = activity.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match ty {
        "Follow" | "Accept" | "Reject" | "Announce" | "Like" | "EmojiReact" => true,
        "Create" => {
            let Some(obj) = activity.get("object").and_then(|v| v.as_object()) else {
                return false;
            };
            if obj.get("type").and_then(|v| v.as_str()) != Some("Note") {
                return false;
            }
            if let Some(serde_json::Value::String(in_reply_to)) = obj.get("inReplyTo") {
                if in_reply_to.contains("/users/") {
                    // Best-effort: treat replies to our local objects as notifications.
                    if in_reply_to.contains("/objects/") {
                        return true;
                    }
                }
            }
            // Mention tags or content contains our handle.
            if let Some(tags) = obj.get("tag").and_then(|v| v.as_array()) {
                for t in tags {
                    if t.get("type").and_then(|v| v.as_str()) == Some("Mention") {
                        if let Some(href) = t.get("href").and_then(|v| v.as_str()) {
                            if href == me_actor {
                                return true;
                            }
                        }
                        if let Some(name) = t.get("name").and_then(|v| v.as_str()) {
                            if name == my_handle || name.trim_start_matches('@') == my_handle.trim_start_matches('@') {
                                return true;
                            }
                        }
                    }
                }
            }
            if let Some(content) = obj.get("content").and_then(|v| v.as_str()) {
                if content.contains(my_handle) {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

fn hydrate_activity(
    state: &ApState,
    activity: &mut Value,
    cache: &mut std::collections::HashMap<String, Value>,
) {
    // Attach lightweight aggregated reaction stats (Like/Announce/EmojiReact) for UI.
    // This is served from the local DB and avoids per-note UI requests.
    if !activity.get("fedi3ReactionCounts").is_some() {
        let obj_id = match activity.get("object") {
            Some(Value::String(url)) => {
                let u = url.trim();
                if u.is_empty() { None } else { Some(u.to_string()) }
            }
            Some(v) => {
                // Support both `Create{object: Note}` and wrapper forms `Create{object: {object: Note}}`.
                v.get("id")
                    .and_then(|v| v.as_str())
                    .or_else(|| v.get("object").and_then(|o| o.get("id")).and_then(|v| v.as_str()))
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            }
            None => None,
        };
        if let Some(obj_id) = obj_id {
            if let Ok(rows) = state.social.list_reaction_counts(&obj_id, 12) {
                if !rows.is_empty() {
                    let mut out = Vec::<serde_json::Value>::new();
                    for (ty, content, count) in rows {
                        out.push(serde_json::json!({
                          "type": ty,
                          "content": content,
                          "count": count,
                        }));
                    }
                    activity
                        .as_object_mut()
                        .map(|m| m.insert("fedi3ReactionCounts".to_string(), serde_json::Value::Array(out)));
                }
            }
        }
    }

    let Some(obj) = activity.get_mut("object") else { return };

    // 1) Replace `object: "<url>"` with the fetched object json if available.
    if let Value::String(url) = obj {
        let url = url.trim();
        if url.starts_with("http://") || url.starts_with("https://") {
            if let Some(v) = cache.get(url).cloned() {
                *obj = v;
                return;
            }
            if let Ok(Some(bytes)) = state.social.get_object_json(url) {
                if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                    cache.insert(url.to_string(), v.clone());
                    *obj = v;
                    return;
                }
            }
        }
        return;
    }

    // 2) If `object` is embedded, attach best-effort context previews (reply/quote).
    let Some(obj_map) = obj.as_object_mut() else { return };
    hydrate_link_field(state, cache, obj_map, "inReplyTo", "fedi3InReplyToObject");
    hydrate_link_field(state, cache, obj_map, "quoteUrl", "fedi3QuoteObject");
    hydrate_link_field(state, cache, obj_map, "quoteUri", "fedi3QuoteObject");
    hydrate_link_field(state, cache, obj_map, "quote", "fedi3QuoteObject");
}

fn hydrate_link_field(
    state: &ApState,
    cache: &mut std::collections::HashMap<String, Value>,
    obj_map: &mut serde_json::Map<String, Value>,
    field: &str,
    out_field: &str,
) {
    if obj_map.contains_key(out_field) {
        return;
    }
    let Some(Value::String(url)) = obj_map.get(field) else { return };
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return;
    }
    if let Some(v) = cache.get(url).cloned() {
        obj_map.insert(out_field.to_string(), v);
        return;
    }
    if let Ok(Some(bytes)) = state.social.get_object_json(url) {
        if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
            cache.insert(url.to_string(), v.clone());
            obj_map.insert(out_field.to_string(), v);
        }
    }
}

async fn resolve_activity_ref(state: &ApState, id_or_url: &str) -> Option<serde_json::Value> {
    let s = id_or_url.trim();
    if s.is_empty() {
        return None;
    }
    if s.starts_with("http://") || s.starts_with("https://") {
        let _ = fetch_and_store_object(state, s).await;
        if let Ok(Some(bytes)) = state.social.get_object_json(s) {
            return serde_json::from_slice(&bytes).ok();
        }
        return None;
    }
    // Non-URL: try local outbox first (our own Follow id).
    if let Ok(Some(bytes)) = state.social.get_outbox(s) {
        return serde_json::from_slice(&bytes).ok();
    }
    // Fallback: maybe we stored it as a generic object.
    if let Ok(Some(bytes)) = state.social.get_object_json(s) {
        return serde_json::from_slice(&bytes).ok();
    }
    None
}

fn extract_actor(activity: &serde_json::Value) -> Option<String> {
    // activity.actor may be string or object; fallback to activity.object.attributedTo.
    if let Some(a) = activity.get("actor") {
        if let Some(s) = a.as_str() {
            return Some(s.to_string());
        }
        if let Some(id) = a.get("id").and_then(|v| v.as_str()) {
            return Some(id.to_string());
        }
    }
    let obj = activity.get("object")?;
    if let Some(at) = obj.get("attributedTo") {
        if let Some(s) = at.as_str() {
            return Some(s.to_string());
        }
        if let Some(id) = at.get("id").and_then(|v| v.as_str()) {
            return Some(id.to_string());
        }
        if let Some(arr) = at.as_array() {
            for item in arr {
                if let Some(s) = item.as_str() {
                    return Some(s.to_string());
                }
                if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                    return Some(id.to_string());
                }
            }
        }
    }
    None
}

fn extract_object_id(activity: &serde_json::Value) -> Option<String> {
    let obj = activity.get("object")?;
    match obj {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(map) => map.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()),
        serde_json::Value::Array(arr) => arr
            .iter()
            .find_map(|it| match it {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Object(map) => map.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()),
                _ => None,
            }),
        _ => None,
    }
}

fn extract_object(activity: &serde_json::Value) -> Option<(String, Vec<u8>)> {
    let obj = activity.get("object")?;
    match obj {
        serde_json::Value::Object(map) => {
            let id = map.get("id")?.as_str()?.to_string();
            let bytes = serde_json::to_vec(obj).ok()?;
            Some((id, bytes))
        }
        serde_json::Value::Array(arr) => {
            for it in arr {
                if let serde_json::Value::Object(map) = it {
                    if let Some(id) = map.get("id").and_then(|v| v.as_str()) {
                        let bytes = serde_json::to_vec(it).ok()?;
                        return Some((id.to_string(), bytes));
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn extract_object_or_tombstone(activity: &serde_json::Value) -> Option<(String, Vec<u8>, bool)> {
    let obj = activity.get("object")?;
    match obj {
        serde_json::Value::Object(map) => {
            let id = map.get("id")?.as_str()?.to_string();
            let ty = map.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let is_tombstone = ty == "Tombstone";
            let bytes = serde_json::to_vec(obj).ok()?;
            Some((id, bytes, is_tombstone))
        }
        _ => None,
    }
}

fn actor_from_object_url(object_url: &str) -> Option<String> {
    let uri: Uri = object_url.parse().ok()?;
    let scheme = uri.scheme_str()?;
    let authority = uri.authority()?;
    let base = format!("{scheme}://{authority}");
    let path = uri.path();
    let marker = "/users/";
    let idx = path.find(marker)?;
    let rest = &path[idx + marker.len()..];
    let mut parts = rest.split('/');
    let username = parts.next()?.trim();
    if username.is_empty() {
        return None;
    }
    if parts.next()? != "objects" {
        return None;
    }
    Some(format!("{base}/users/{username}"))
}

async fn fetch_object_from_peer(state: &ApState, object_url: &str) -> anyhow::Result<(bool, Option<Vec<u8>>)> {
    let Some(actor) = actor_from_object_url(object_url) else {
        return Ok((false, None));
    };
    let info = match state.delivery.resolve_actor_info(&actor).await {
        Ok(v) => v,
        Err(_) => return Ok((false, None)),
    };
    let Some(peer_id) = info.p2p_peer_id else {
        return Ok((false, None));
    };
    if !info.p2p_peer_addrs.is_empty() {
        let _ = state
            .delivery
            .p2p_add_peer_addrs(&peer_id, info.p2p_peer_addrs)
            .await;
    }

    let query = format!("?url={}", urlencoding::encode(object_url));
    let req = RelayHttpRequest {
        id: format!("obj-{}", now_ms()),
        method: "GET".to_string(),
        path: "/_fedi3/object".to_string(),
        query,
        headers: vec![(
            "accept".to_string(),
            "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\""
                .to_string(),
        )],
        body_b64: "".to_string(),
    };
    let resp = match state.delivery.p2p_request(&peer_id, req).await {
        Ok(v) => v,
        Err(_) => return Ok((true, None)),
    };
    if !(200..300).contains(&resp.status) {
        return Ok((true, None));
    }
    let bytes = B64.decode(resp.body_b64.as_bytes()).unwrap_or_default();
    if bytes.is_empty() {
        return Ok((true, None));
    }
    Ok((true, Some(bytes)))
}

pub(crate) async fn fetch_and_store_object(state: &ApState, object_url: &str) -> anyhow::Result<()> {
    // Avoid fetching local objects.
    if object_url.contains("/users/")
        && object_url.contains(state.cfg.public_base_url.trim_end_matches('/'))
    {
        return Ok(());
    }

    let (p2p_attempted, mut bytes) = fetch_object_from_peer(state, object_url).await?;
    if bytes.is_none() {
        if state.post_delivery_mode == PostDeliveryMode::P2pOnly {
            enqueue_object_fetch(state, object_url, "p2p-only: object not available")?;
            return Ok(());
        }
        if p2p_attempted && state.p2p_relay_fallback.as_secs() > 0 {
            sleep(state.p2p_relay_fallback).await;
        }
        let resp = {
            let uri: Uri = match object_url.parse() {
                Ok(u) => u,
                Err(e) => {
                    enqueue_object_fetch(state, object_url, &format!("bad url: {e:#}"))?;
                    return Ok(());
                }
            };
            let key_id = format!(
                "{}/users/{}#main-key",
                state.cfg.public_base_url.trim_end_matches('/'),
                state.cfg.username
            );
            let build_req = || {
                let mut headers = HeaderMap::new();
                headers.insert(
                    "Accept",
                    "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\""
                        .parse()
                        .expect("static header"),
                );
                // Authorized fetch (best-effort): sign the GET.
                let _ = sign_request_rsa_sha256(
                    &state.private_key_pem,
                    &key_id,
                    &Method::GET,
                    &uri,
                    &mut headers,
                    &[],
                    &["(request-target)", "host", "date"],
                );

                let mut req = state
                    .http
                    .get(object_url)
                    .header(
                        "Accept",
                        "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"",
                    );
                for (k, v) in headers.iter() {
                    req = req.header(k.as_str(), v.to_str().unwrap_or_default());
                }
                req
            };
            match send_with_retry_metrics(build_req, 3, &state.net).await {
                Ok(r) => r,
                Err(e) => {
                    enqueue_object_fetch(state, object_url, &format!("{e:#}"))?;
                    return Ok(());
                }
            }
        };

        if !resp.status().is_success() {
            enqueue_object_fetch(state, object_url, &format!("http {}", resp.status()))?;
            return Ok(());
        }
        bytes = match resp.bytes().await {
            Ok(b) => Some(b.to_vec()),
            Err(e) => {
                enqueue_object_fetch(state, object_url, &format!("{e:#}"))?;
                return Ok(());
            }
        };
    }

    let Some(bytes) = bytes else {
        enqueue_object_fetch(state, object_url, "empty response")?;
        return Ok(());
    };
    let v: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            enqueue_object_fetch(state, object_url, &format!("bad json: {e}"))?;
            return Ok(());
        }
    };
    let id = v.get("id").and_then(|vv| vv.as_str()).unwrap_or(object_url);
    let ty = v.get("type").and_then(|vv| vv.as_str()).unwrap_or("");
    let actor_opt = v
        .get("attributedTo")
        .and_then(|a| a.as_str())
        .or_else(|| v.get("actor").and_then(|a| a.as_str()));

    if ty == "Tombstone" {
        let _ = state.social.upsert_object_with_actor(id, actor_opt, bytes.to_vec());
        let _ = state.social.mark_object_deleted(id);
        return Ok(());
    }

    let _ = state.social.upsert_object_with_actor(id, actor_opt, bytes.to_vec());
    Ok(())
}

fn enqueue_object_fetch(state: &ApState, object_url: &str, err: &str) -> anyhow::Result<()> {
    let next = now_ms().saturating_add(10_000);
    state.social.enqueue_object_fetch(object_url, next, Some(err))?;
    state.object_fetch.notify();
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[derive(Clone, Copy)]
enum ActivityAccept {
    ActivityJson,
    LdJson,
}

fn accept_activity(headers: &http::HeaderMap) -> Option<ActivityAccept> {
    let accept = headers.get(header::ACCEPT)?.to_str().ok()?.to_ascii_lowercase();
    if accept.contains("application/ld+json") {
        Some(ActivityAccept::LdJson)
    } else if accept.contains("application/activity+json") || accept.contains("application/json") || accept.contains("*/*") {
        Some(ActivityAccept::ActivityJson)
    } else {
        None
    }
}

fn json_activity<T: Serialize>(status: StatusCode, accept: Option<ActivityAccept>, value: &T) -> Response<Body> {
    let body = serde_json::to_vec(value).unwrap_or_default();
    let mut resp = Response::new(Body::from(body));
    *resp.status_mut() = status;
    let headers = resp.headers_mut();
    let ct = match accept.unwrap_or(ActivityAccept::ActivityJson) {
        ActivityAccept::ActivityJson => "application/activity+json; charset=utf-8",
        ActivityAccept::LdJson => "application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"; charset=utf-8",
    };
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(ct).unwrap());
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp
}

fn jrd<T: Serialize>(status: StatusCode, value: &T) -> Response<Body> {
    let body = serde_json::to_vec(value).unwrap_or_default();
    let mut resp = Response::new(Body::from(body));
    *resp.status_mut() = status;
    let headers = resp.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/jrd+json; charset=utf-8"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp
}

fn is_actor_value(v: &serde_json::Value) -> bool {
    matches!(
        v.get("type").and_then(|t| t.as_str()),
        Some("Person") | Some("Service") | Some("Organization") | Some("Group") | Some("Application")
    )
}

fn activity_from_note(note: &serde_json::Value) -> Option<serde_json::Value> {
    let id = note.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.trim().is_empty() {
        return None;
    }
    let actor = note
        .get("attributedTo")
        .or_else(|| note.get("actor"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    Some(serde_json::json!({
        "id": format!("{id}#search"),
        "type": "Create",
        "actor": actor,
        "object": note.clone(),
    }))
}

fn note_id_from_activity(v: &serde_json::Value) -> Option<String> {
    v.get("object")
        .and_then(|o| o.get("id"))
        .and_then(|id| id.as_str())
        .map(|s| s.to_string())
}

fn set_search_source(v: &mut serde_json::Value, source: &str) {
    if let Some(obj) = v.as_object_mut() {
        obj.insert(
            "fedi3SearchSource".to_string(),
            serde_json::Value::String(source.to_string()),
        );
    }
}

fn normalize_search_source(input: &str) -> String {
    match input.trim().to_lowercase().as_str() {
        "local" => "local".to_string(),
        "relay" => "relay".to_string(),
        _ => "all".to_string(),
    }
}

fn normalize_search_consistency(input: &str) -> String {
    match input.trim().to_lowercase().as_str() {
        "best" => "best".to_string(),
        _ => "full".to_string(),
    }
}

async fn require_relay_search_coverage(state: &ApState) -> Result<(), Response<Body>> {
    let coverage = relay_search_coverage(state).await.map_err(|e| {
        simple(StatusCode::BAD_GATEWAY, &format!("relay coverage failed: {e}"))
    })?;
    let total_users = coverage.get("total_users").and_then(|v| v.as_u64()).unwrap_or(0);
    let indexed_users = coverage.get("indexed_users").and_then(|v| v.as_u64()).unwrap_or(0);
    let coverage_window_ms = coverage.get("coverage_window_ms").and_then(|v| v.as_i64()).unwrap_or(0);
    let last_index_ms = coverage.get("last_index_ms").and_then(|v| v.as_i64()).unwrap_or(0);
    let relays_total = coverage.get("relays_total").and_then(|v| v.as_u64()).unwrap_or(0);
    let relays_synced = coverage.get("relays_synced").and_then(|v| v.as_u64()).unwrap_or(0);
    let relay_sync_window_ms = coverage.get("relay_sync_window_ms").and_then(|v| v.as_i64()).unwrap_or(0);
    let relays_last_sync_ms = coverage.get("relays_last_sync_ms").and_then(|v| v.as_i64()).unwrap_or(0);
    let now = now_ms();

    let index_ok = indexed_users >= total_users && total_users > 0;
    let index_fresh = coverage_window_ms <= 0 || (now - last_index_ms) <= coverage_window_ms;
    let relays_ok = relays_total == 0 || relays_synced >= relays_total;
    let relays_fresh = relay_sync_window_ms <= 0 || (now - relays_last_sync_ms) <= relay_sync_window_ms;

    if index_ok && index_fresh && relays_ok && relays_fresh {
        Ok(())
    } else {
        Err(simple(
            StatusCode::SERVICE_UNAVAILABLE,
            "search coverage incomplete (relay index or relay sync not ready)",
        ))
    }
}

#[derive(Debug, serde::Deserialize)]
struct RelaySearchNotesResponse {
    total: Option<u64>,
    next: Option<String>,
    items: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, serde::Deserialize)]
struct RelayUpdateReq {
    add: Option<Vec<RelayItemInput>>,
    remove: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
struct RelayItemInput {
    #[serde(alias = "relay_url", alias = "base")]
    relay_base_url: String,
    #[serde(alias = "relay_ws", alias = "ws")]
    relay_ws_url: Option<String>,
}

async fn relay_search_notes(
    state: &ApState,
    q: &str,
    tag: &str,
    limit: u32,
    cursor: Option<i64>,
) -> Result<Option<crate::social_db::CollectionPage<serde_json::Value>>, anyhow::Error> {
    let base = state
        .cfg
        .relay_base_url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let Some(base) = base else { return Ok(None) };
    let token = state.cfg.relay_token.as_deref().unwrap_or("").trim().to_string();
    if token.is_empty() {
        return Ok(None);
    }
    let url = format!("{}/_fedi3/relay/search/notes", base.trim_end_matches('/'));
    let mut params = vec![
        ("username".to_string(), state.cfg.username.clone()),
        ("q".to_string(), q.to_string()),
        ("tag".to_string(), tag.to_string()),
        ("limit".to_string(), limit.to_string()),
    ];
    if let Some(cur) = cursor {
        params.push(("cursor".to_string(), cur.to_string()));
    }
    if tag.trim().is_empty() && q.trim().is_empty() {
        return Ok(None);
    }
    let resp = send_with_retry_metrics(
        || {
            state
                .http
                .get(&url)
                .query(&params)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
        },
        3,
        &state.net,
    )
    .await?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let data = resp.json::<RelaySearchNotesResponse>().await.unwrap_or(RelaySearchNotesResponse {
        total: Some(0),
        next: None,
        items: Some(Vec::new()),
    });
    let items = data.items.unwrap_or_default();
    let total = data.total.unwrap_or(items.len() as u64);
    Ok(Some(crate::social_db::CollectionPage { total, items, next: data.next }))
}

#[derive(Debug, serde::Deserialize)]
struct RelaySearchUsersResponse {
    total: Option<u64>,
    next: Option<String>,
    items: Option<Vec<serde_json::Value>>,
}

async fn relay_search_users(
    state: &ApState,
    q: &str,
    limit: u32,
    cursor: Option<i64>,
) -> Result<Option<crate::social_db::CollectionPage<serde_json::Value>>, anyhow::Error> {
    let base = state
        .cfg
        .relay_base_url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let Some(base) = base else { return Ok(None) };
    let token = state.cfg.relay_token.as_deref().unwrap_or("").trim().to_string();
    if token.is_empty() {
        return Ok(None);
    }
    let url = format!("{}/_fedi3/relay/search/users", base.trim_end_matches('/'));
    let mut params = vec![
        ("username".to_string(), state.cfg.username.clone()),
        ("q".to_string(), q.to_string()),
        ("limit".to_string(), limit.to_string()),
    ];
    if let Some(cur) = cursor {
        params.push(("cursor".to_string(), cur.to_string()));
    }
    let resp = send_with_retry_metrics(
        || {
            state
                .http
                .get(&url)
                .query(&params)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
        },
        3,
        &state.net,
    )
    .await?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let data = resp.json::<RelaySearchUsersResponse>().await.unwrap_or(RelaySearchUsersResponse {
        total: Some(0),
        next: None,
        items: Some(Vec::new()),
    });
    let items = data.items.unwrap_or_default();
    let total = data.total.unwrap_or(items.len() as u64);
    Ok(Some(crate::social_db::CollectionPage { total, items, next: data.next }))
}

#[derive(Debug, serde::Deserialize)]
struct RelaySearchTagsResponse {
    items: Option<Vec<RelayTagItem>>,
}

#[derive(Debug, serde::Deserialize)]
struct RelayTagItem {
    name: String,
    count: u64,
}

async fn relay_search_hashtags(
    state: &ApState,
    q: &str,
    limit: u32,
) -> Result<Option<Vec<(String, u64)>>, anyhow::Error> {
    let base = state
        .cfg
        .relay_base_url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let Some(base) = base else { return Ok(None) };
    let token = state.cfg.relay_token.as_deref().unwrap_or("").trim().to_string();
    if token.is_empty() {
        return Ok(None);
    }
    let url = format!("{}/_fedi3/relay/search/hashtags", base.trim_end_matches('/'));
    let resp = send_with_retry_metrics(
        || {
            state
                .http
                .get(&url)
                .query(&[
                    ("username", state.cfg.username.as_str()),
                    ("q", q),
                    ("limit", &limit.to_string()),
                ])
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
        },
        3,
        &state.net,
    )
    .await?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let data = resp.json::<RelaySearchTagsResponse>().await.unwrap_or(RelaySearchTagsResponse { items: None });
    let items = data.items.unwrap_or_default();
    Ok(Some(items.into_iter().map(|i| (i.name, i.count)).collect()))
}

async fn relay_search_coverage(state: &ApState) -> anyhow::Result<serde_json::Value> {
    let Some(base) = state
        .cfg
        .relay_base_url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    else {
        return Err(anyhow::anyhow!("missing relay_base_url"));
    };
    let token = state.cfg.relay_token.as_deref().unwrap_or("").trim().to_string();
    if token.is_empty() {
        return Err(anyhow::anyhow!("missing relay_token"));
    }
    let url = format!("{}/_fedi3/relay/search/coverage", base.trim_end_matches('/'));
    let resp = send_with_retry_metrics(
        || {
            state
                .http
                .get(&url)
                .query(&[("username", state.cfg.username.as_str())])
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
        },
        3,
        &state.net,
    )
    .await?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("relay coverage http {status}: {body}"));
    }
    let value = resp.json::<serde_json::Value>().await?;
    Ok(value)
}

fn simple(status: StatusCode, msg: &str) -> Response<Body> {
    let mut resp = Response::new(Body::from(msg.to_string()));
    *resp.status_mut() = status;
    resp
}

fn url_decode(s: &str) -> String {
    // Decodifica minimale per `resource=` (solo %XX e '+').
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h1 = bytes[i + 1];
                let h2 = bytes[i + 2];
                if let (Some(a), Some(b)) = (from_hex(h1), from_hex(h2)) {
                    out.push((a * 16 + b) as char);
                    i += 3;
                } else {
                    out.push('%');
                    i += 1;
                }
            }
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    out
}


fn from_hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}
