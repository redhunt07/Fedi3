/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

//! Public landing page served at the relay root.
//!
//! Before this module existed, `GET /` fell through to the wildcard forwarder
//! and answered 404 on every relay, so anyone following a link from the
//! fediverse landed on an empty page. The page is embedded in the binary on
//! purpose: it must work on Caddy, YunoHost and bare deployments alike, with
//! no extra volume or static-file mount.
//!
//! The page is HTML and CSS; the live mesh view adds one same-origin script
//! (`/assets/landing/live.js`) that polls `/_fedi3/relay/live`. That keeps the
//! page working under the CSP documented in `.env.example`, because
//! `script-src` falls back to `default-src 'self'`. Without JavaScript the page
//! still renders and links the raw JSON snapshot.
//! Screenshots are reused from `docs/assets` instead of being duplicated.

use std::net::SocketAddr;
use std::sync::OnceLock;

use axum::{
    body::Bytes,
    extract::{ConnectInfo, Path, RawQuery, State},
    http::{header, HeaderMap, Method, StatusCode},
    response::{IntoResponse, Redirect, Response},
};

use super::{
    client_ip, forward_to_user, maybe_redirect_canonical, origin_for_links_with_cfg,
    user_from_host, AppState,
};

const PAGE_EN: &str = include_str!("landing/index_en.html");
const PAGE_IT: &str = include_str!("landing/index_it.html");
const STYLE: &str = include_str!("landing/style.css");
const LIVE_JS: &str = include_str!("landing/live.js");
const BANNER: &[u8] = include_bytes!("../../../docs/assets/header.svg");
const FAVICON: &[u8] = include_bytes!("../../../app/web/favicon.png");
const SHOT_TIMELINE: &[u8] =
    include_bytes!("../../../docs/assets/screenshots/desktop-timeline.png");
const SHOT_CHAT: &[u8] = include_bytes!("../../../docs/assets/screenshots/desktop-chat.png");
const SHOT_PROFILE: &[u8] = include_bytes!("../../../docs/assets/screenshots/desktop-profile.png");

#[derive(Clone, Copy)]
enum Lang {
    En,
    It,
}

/// `GET /` — English landing page, or the forwarded root of a user subdomain.
pub(super) async fn root(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    method: Method,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    serve(
        RootRequest {
            state,
            peer,
            method,
            headers,
            raw_query,
            body,
        },
        Lang::En,
        "/",
    )
    .await
}

/// `GET /it` — Italian landing page.
pub(super) async fn root_it(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    method: Method,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    serve(
        RootRequest {
            state,
            peer,
            method,
            headers,
            raw_query,
            body,
        },
        Lang::It,
        "/it",
    )
    .await
}

/// `GET /en` — stable alias for the English page.
pub(super) async fn root_en() -> Response {
    Redirect::permanent("/").into_response()
}

/// `GET /assets/landing/:name` — page assets embedded in the binary.
pub(super) async fn asset(Path(name): Path<String>) -> Response {
    let (body, content_type): (&[u8], &'static str) = match name.as_str() {
        "style.css" => (STYLE.as_bytes(), "text/css; charset=utf-8"),
        "live.js" => (LIVE_JS.as_bytes(), "text/javascript; charset=utf-8"),
        "banner.svg" => (BANNER, "image/svg+xml"),
        "favicon.png" => (FAVICON, "image/png"),
        "timeline.png" => (SHOT_TIMELINE, "image/png"),
        "chat.png" => (SHOT_CHAT, "image/png"),
        "profile.png" => (SHOT_PROFILE, "image/png"),
        _ => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        body.to_vec(),
    )
        .into_response()
}

/// Everything a root request carries. Grouped in a struct so the shared
/// handler stays within clippy's argument limit.
struct RootRequest {
    state: AppState,
    peer: SocketAddr,
    method: Method,
    headers: HeaderMap,
    raw_query: Option<String>,
    body: Bytes,
}

async fn serve(req: RootRequest, lang: Lang, path: &'static str) -> Response {
    let RootRequest {
        state,
        peer,
        method,
        headers,
        raw_query,
        body,
    } = req;
    if let Some(resp) =
        maybe_redirect_canonical(&state.cfg, &headers, &method, path, raw_query.as_deref())
    {
        return resp;
    }

    // In base-domain mode a user subdomain owns its root and every other path:
    // keep forwarding there instead of serving the landing page.
    if let Some(user) = user_from_host(&state.cfg, &headers) {
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
        let query = raw_query.map(|q| format!("?{q}")).unwrap_or_default();
        return forward_to_user(state, user, method, path, query, headers, body).await;
    }

    if method != Method::GET && method != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }

    let (scheme, host) = origin_for_links_with_cfg(&state.cfg, &headers);
    // The Host header is client controlled; escape before interpolating it
    // into attributes so a crafted authority cannot inject markup.
    let origin = escape_html(&format!("{scheme}://{host}"));
    let users = state.db.count_users().await.unwrap_or(0);
    let html = render(lang, &origin, users);
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=300"),
        ],
        html,
    )
        .into_response()
}

/// Content hash of the embedded landing assets: cache busting must follow the
/// bytes, not the release version, or a relay upgraded without a version bump
/// keeps serving the previous stylesheet and script for a day.
fn asset_tag() -> &'static str {
    static TAG: OnceLock<String> = OnceLock::new();
    TAG.get_or_init(|| {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in STYLE.bytes().chain(LIVE_JS.bytes()) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        format!("{hash:012x}")
    })
}

fn render(lang: Lang, origin: &str, users: u64) -> String {
    let template = match lang {
        Lang::En => PAGE_EN,
        Lang::It => PAGE_IT,
    };
    template
        .replace("{{origin}}", origin)
        .replace("{{version}}", env!("CARGO_PKG_VERSION"))
        .replace("{{asset_tag}}", asset_tag())
        .replace("{{users}}", &users.to_string())
}

fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_both_languages_with_placeholders_replaced() {
        for lang in [Lang::En, Lang::It] {
            let html = render(lang, "https://relay.example", 7);
            assert!(html.contains("https://relay.example/"));
            assert!(!html.contains("{{origin}}"));
            assert!(!html.contains("{{version}}"));
            assert!(!html.contains("{{asset_tag}}"));
            assert!(html.contains("style.css?v="));
            assert!(!html.contains("{{users}}"));
            assert!(html.contains(": 7"));
        }
    }

    #[test]
    fn escapes_host_injection() {
        assert_eq!(
            escape_html("https://relay.example\"><script>"),
            "https://relay.example&quot;&gt;&lt;script&gt;"
        );
    }

    #[test]
    fn assets_are_embedded_and_non_empty() {
        assert!(!STYLE.is_empty());
        assert!(!LIVE_JS.is_empty());
        assert!(!BANNER.is_empty());
        assert!(!SHOT_TIMELINE.is_empty());
    }
}
