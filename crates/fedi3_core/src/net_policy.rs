/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

//! Outbound fetch policy for federation traffic (H-03/H-04).
//!
//! Every URL that comes from a remote peer (actor documents, key ids, objects, outbox pages,
//! inboxes) is attacker-controlled. This module centralizes the rules that apply to those
//! fetches:
//!
//! * only `http`/`https`;
//! * no private, loopback, link-local, unique-local or unspecified addresses (SSRF guard),
//!   unless the operator explicitly opts in with `FEDI3_ALLOW_PRIVATE_ACTOR_FETCH=true`;
//! * bounded redirects;
//! * bounded response bodies, streamed instead of buffered blindly.
//!
//! The connector validates the exact DNS answers it uses on every connection,
//! including automatic redirects. Literal redirect targets are checked by the policy.

use anyhow::{anyhow, bail, Result};
use http::Uri;
use std::net::IpAddr;
use std::time::Duration;

/// Actor documents and key metadata are small; anything bigger is treated as hostile.
pub const MAX_ACTOR_DOC_BYTES: usize = 256 * 1024;
/// ActivityStreams objects can be larger, but still bounded.
pub const MAX_OBJECT_DOC_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_MEDIA_BYTES: usize = 32 * 1024 * 1024;
/// Outbox pages fetched by the legacy sync worker.
pub const MAX_OUTBOX_PAGE_BYTES: usize = 2 * 1024 * 1024;
/// Hard cap on followed redirects for federation fetches.
pub const MAX_REDIRECTS: usize = 5;

/// `true` when the operator allowed fetches to private addresses (LAN-only federation, tests).
pub fn allow_private_fetch() -> bool {
    matches!(
        std::env::var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes")
    )
}

/// Whether the connector may contact this address.
pub fn is_public_ip(ip: &IpAddr) -> bool {
    fedi3_protocol::net::is_public_ip(ip)
}

fn parse_ip_literal(host: &str) -> Option<IpAddr> {
    let trimmed = host.trim_start_matches('[').trim_end_matches(']');
    trimmed.parse::<IpAddr>().ok()
}

/// Validate a federation URL before issuing a request.
///
/// Rejects non-http(s) schemes and hosts that resolve exclusively to private addresses.
/// `FEDI3_ALLOW_PRIVATE_ACTOR_FETCH=true` disables only the address check.
pub async fn check_url_allowed(raw_url: &str) -> Result<()> {
    let url = raw_url.trim();
    if url.is_empty() {
        bail!("empty URL");
    }
    let uri: Uri = url
        .parse()
        .map_err(|e| anyhow!("invalid URL {url:?}: {e}"))?;
    let scheme = uri
        .scheme_str()
        .ok_or_else(|| anyhow!("URL without scheme: {url}"))?;
    if scheme != "http" && scheme != "https" {
        bail!("unsupported scheme {scheme:?} in {url}");
    }
    let host = uri
        .host()
        .ok_or_else(|| anyhow!("URL without host: {url}"))?
        .to_string();
    if host.is_empty() {
        bail!("empty host in {url}");
    }
    if allow_private_fetch() {
        return Ok(());
    }

    if let Some(ip) = parse_ip_literal(&host) {
        if !is_public_ip(&ip) {
            bail!("URL {url} points to non-public address {ip}");
        }
        return Ok(());
    }

    let port = uri
        .port_u16()
        .unwrap_or(if scheme == "https" { 443 } else { 80 });
    let addrs: Vec<_> = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|e| anyhow!("DNS resolution failed for {host}: {e}"))?
        .collect();
    if addrs.is_empty() {
        bail!("DNS returned no address for {host}");
    }
    // Reject when any resolved address is private: a hostname with mixed records could
    // otherwise be used to bounce requests into the local network.
    for addr in addrs {
        if !is_public_ip(&addr.ip()) {
            bail!("host {host} resolves to non-public address {}", addr.ip());
        }
    }
    Ok(())
}

/// Redirect policy: follow at most [`MAX_REDIRECTS`] hops and only across http(s).
pub fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            attempt.stop()
        } else {
            match attempt.url().scheme() {
                "http" | "https" => {
                    if !allow_private_fetch()
                        && attempt
                            .url()
                            .host_str()
                            .and_then(parse_ip_literal)
                            .is_some_and(|ip| !is_public_ip(&ip))
                    {
                        attempt.error("redirect to non-public address")
                    } else {
                        attempt.follow()
                    }
                }
                _ => attempt.stop(),
            }
        }
    })
}

/// Client used for federation fetches: bounded timeouts, bounded redirects, explicit UA.
pub fn federation_client() -> reqwest::Client {
    fedi3_protocol::net::guard_client(reqwest::Client::builder(), allow_private_fetch())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .redirect(redirect_policy())
        .user_agent(format!("fedi3/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("valid federation HTTP configuration")
}

/// Read a response body, refusing to buffer more than `max` bytes.
pub async fn read_capped(mut resp: reqwest::Response, max: usize) -> Result<Vec<u8>> {
    if let Some(len) = resp.content_length() {
        if len as usize > max {
            bail!("response too large: {len} bytes (limit {max})");
        }
    }
    let mut out: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        if out.len().saturating_add(chunk.len()) > max {
            bail!("response exceeds {max} bytes");
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

/// GET with policy validation before *every* hop. Checking response.url() after
/// an automatic redirect is too late to prevent a request into a private LAN.
pub async fn get_checked(
    raw_url: &str,
    mut headers: reqwest::header::HeaderMap,
) -> Result<reqwest::Response> {
    static PUBLIC_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    static LAN_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    let allow_private = allow_private_fetch();
    let slot = if allow_private {
        &LAN_CLIENT
    } else {
        &PUBLIC_CLIENT
    };
    let client = slot.get_or_init(|| {
        fedi3_protocol::net::guard_client(reqwest::Client::builder(), allow_private)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("fedi3/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("valid HTTP client configuration")
    });
    let mut url = reqwest::Url::parse(raw_url)?;
    for hop in 0..=MAX_REDIRECTS {
        check_url_allowed(url.as_str()).await?;
        let response = client
            .get(url.clone())
            .headers(headers.clone())
            .send()
            .await?;
        if !matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
            return Ok(response);
        }
        anyhow::ensure!(hop < MAX_REDIRECTS, "too many redirects");
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .ok_or_else(|| anyhow!("redirect without Location"))?
            .to_str()?;
        url = url.join(location)?;
        // Original HTTP signatures are bound to the old request target; never
        // leak authentication or cookies to the redirect destination.
        for name in [
            "authorization",
            "proxy-authorization",
            "cookie",
            "signature",
            "host",
            "digest",
            "date",
        ] {
            headers.remove(name);
        }
    }
    unreachable!("redirect loop is bounded")
}

/// Process-wide escape hatch: tests that toggle `FEDI3_ALLOW_PRIVATE_ACTOR_FETCH`
/// must hold this lock so they cannot race each other.
#[cfg(test)]
pub(crate) static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_v4_addresses_are_rejected() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.254",
            "192.168.1.10",
            "169.254.169.254",
            "0.0.0.0",
            "100.64.0.1",
            "198.18.0.1",
        ] {
            let ip: IpAddr = ip.parse().unwrap();
            assert!(!is_public_ip(&ip), "{ip} must not be public");
        }
    }

    #[test]
    fn public_v4_addresses_are_accepted() {
        for ip in ["8.8.8.8", "1.1.1.1", "93.184.216.34", "172.32.0.1"] {
            let ip: IpAddr = ip.parse().unwrap();
            assert!(is_public_ip(&ip), "{ip} must be public");
        }
    }

    #[test]
    fn private_v6_addresses_are_rejected() {
        for ip in [
            "::1",
            "::",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "2001:db8::1",
        ] {
            let ip: IpAddr = ip.parse().unwrap();
            assert!(!is_public_ip(&ip), "{ip} must not be public");
        }
    }

    #[test]
    fn public_v6_addresses_are_accepted() {
        for ip in ["2001:4860:4860::8888", "2606:4700:4700::1111"] {
            let ip: IpAddr = ip.parse().unwrap();
            assert!(is_public_ip(&ip), "{ip} must be public");
        }
    }

    #[test]
    fn ipv4_mapped_v6_follows_v4_rules() {
        let ip: IpAddr = "::ffff:127.0.0.1".parse().unwrap();
        assert!(!is_public_ip(&ip));
        let ip: IpAddr = "::ffff:8.8.8.8".parse().unwrap();
        assert!(is_public_ip(&ip));
    }

    #[tokio::test]
    async fn checked_redirect_is_rejected_before_private_target_is_contacted() {
        use axum::{response::IntoResponse, routing::get, Router};
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        let _guard = ENV_LOCK.lock().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let target = format!("{base}/secret");
        let hits = Arc::new(AtomicUsize::new(0));
        let secret_hits = hits.clone();
        let router = Router::new()
            .route(
                "/start",
                get(move || {
                    let target = target.clone();
                    async move {
                        std::env::remove_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH");
                        (axum::http::StatusCode::FOUND, [("Location", target)]).into_response()
                    }
                }),
            )
            .route(
                "/secret",
                get(move || {
                    let hits = secret_hits.clone();
                    async move {
                        hits.fetch_add(1, Ordering::Relaxed);
                        "secret"
                    }
                }),
            );
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        std::env::set_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH", "true");
        let response = get_checked(&format!("{base}/start"), Default::default()).await;
        std::env::remove_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH");
        server.abort();
        assert!(response.is_err());
        assert_eq!(hits.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn checked_fetch_refuses_an_oversized_response() {
        use axum::{routing::get, Router};
        let _guard = ENV_LOCK.lock().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/large", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route("/large", get(|| async { vec![0u8; 4096] })),
            )
            .await
            .unwrap();
        });
        std::env::set_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH", "true");
        let response = get_checked(&url, Default::default()).await.unwrap();
        let result = read_capped(response, 1024).await;
        std::env::remove_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH");
        server.abort();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn loopback_literal_is_rejected_without_dns() {
        let _guard = ENV_LOCK.lock().await;
        std::env::remove_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH");
        assert!(check_url_allowed("http://127.0.0.1:8787/users/alice")
            .await
            .is_err());
        assert!(check_url_allowed("http://[::1]:8787/").await.is_err());
        assert!(check_url_allowed("http://169.254.169.254/latest/meta-data")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn bad_scheme_is_rejected() {
        assert!(check_url_allowed("ftp://example.com/file").await.is_err());
        assert!(check_url_allowed("file:///etc/passwd").await.is_err());
        assert!(check_url_allowed("").await.is_err());
    }

    #[tokio::test]
    async fn escape_hatch_allows_loopback() {
        let _guard = ENV_LOCK.lock().await;
        std::env::set_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH", "true");
        assert!(check_url_allowed("http://127.0.0.1:1/").await.is_ok());
        std::env::remove_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH");
    }
}
