/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use fedi3_protocol::RelayHttpRequest;
use std::time::Duration;
use tokio::sync::watch;
use tracing::info;

use crate::ap::ApState;
use crate::http_retry::send_with_retry_metrics;

#[derive(Debug, serde::Deserialize)]
struct RelayListResponse {
    relays: Option<Vec<RelayListItem>>,
    items: Option<Vec<RelayListItem>>,
}

#[derive(Debug, serde::Deserialize)]
struct RelayListItem {
    relay_url: Option<String>,
    relay_ws: Option<String>,
    relay_base_url: Option<String>,
    relay_ws_url: Option<String>,
    base: Option<String>,
    ws: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct PeerRelayListResponse {
    items: Option<Vec<RelayListItem>>,
}

/// Shared relay directory maintained in the Fedi3 repository. Clients and the
/// core read the same file, so relay discovery does not depend on a single
/// relay being reachable.
pub const DEFAULT_RELAY_DIRECTORY_URLS: &[&str] = &[
    "https://raw.githubusercontent.com/redhunt07/Fedi3/main/relays/relays.json",
    "https://cdn.jsdelivr.net/gh/redhunt07/Fedi3@main/relays/relays.json",
];

#[derive(Debug, serde::Deserialize)]
struct RelayDirectoryResponse {
    relays: Option<Vec<RelayDirectoryItem>>,
}

#[derive(Debug, serde::Deserialize)]
struct RelayDirectoryItem {
    url: String,
    ws: Option<String>,
}

pub fn start_relay_sync_worker(
    state: ApState,
    mut shutdown: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    let interval_secs = 300;
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() { break; }
                }
                _ = tick.tick() => {}
            }
            if *shutdown.borrow() {
                break;
            }
            if let Err(e) = sync_once(&state).await {
                info!("relay sync error: {e:#}");
            }
        }
    })
}

pub async fn sync_once(state: &ApState) -> anyhow::Result<()> {
    // Always store current relay.
    if let Some(base) = state.cfg.relay_base_url.as_ref() {
        state.social.upsert_relay_entry(base, None, "self").ok();
    }

    // Shared directory: bootstrap discovery even when the current relay is down.
    fetch_directory_once(state).await;

    // Fetch relay list from current relay (best-effort).
    if let Some(base) = state.cfg.relay_base_url.as_ref() {
        let url = format!("{}/_fedi3/relay/relays", base.trim_end_matches('/'));
        let mut req = state.http.get(&url);
        if let Some(tok) = state
            .cfg
            .relay_token
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            req = req.header("Authorization", format!("Bearer {}", tok));
        }
        if let Ok(resp) = send_with_retry_metrics(|| req.try_clone().unwrap(), 3, &state.net).await
        {
            if resp.status().is_success() {
                if let Ok(body) = resp.json::<RelayListResponse>().await {
                    if let Some(items) = body.relays.or(body.items) {
                        store_relay_items(state, items, "relay");
                    }
                }
            }
        }
    }

    // Fetch relay lists from known peers (best-effort).
    let actors = state.social.list_actor_meta_fedi3(50).unwrap_or_default();
    for actor_url in actors {
        let info = match state.delivery.resolve_actor_info(&actor_url).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let Some(peer_id) = info.p2p_peer_id else {
            continue;
        };
        if !info.p2p_peer_addrs.is_empty() {
            let _ = state
                .delivery
                .p2p_add_peer_addrs(&peer_id, info.p2p_peer_addrs)
                .await;
        }
        let req = RelayHttpRequest {
            id: format!("relay-list-{}", now_ms()),
            method: "GET".to_string(),
            path: "/.fedi3/relays".to_string(),
            query: "".to_string(),
            headers: vec![("accept".to_string(), "application/json".to_string())],
            body_b64: "".to_string(),
        };
        let resp = match state.delivery.p2p_request(&peer_id, req).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        if !(200..300).contains(&resp.status) {
            continue;
        }
        let body = B64.decode(resp.body_b64.as_bytes()).unwrap_or_default();
        if let Ok(list) = serde_json::from_slice::<PeerRelayListResponse>(&body) {
            if let Some(items) = list.items {
                store_relay_items(state, items, "peer");
            }
        }
    }

    // Best-effort: push our known relays to current relay (if supported).
    if let Some(base) = state.cfg.relay_base_url.as_ref() {
        let items = state.social.list_relay_entries(200).unwrap_or_default();
        if !items.is_empty() {
            let url = format!("{}/_fedi3/relay/relays", base.trim_end_matches('/'));
            let mut req = state.http.post(&url).json(&serde_json::json!({
                "relays": items
                    .into_iter()
                    .map(|r| {
                        serde_json::json!({
                            "relay_url": r.relay_base_url,
                            "relay_ws": r.relay_ws_url
                        })
                    })
                    .collect::<Vec<_>>()
            }));
            if let Some(tok) = state
                .cfg
                .relay_token
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                req = req.header("Authorization", format!("Bearer {}", tok));
            }
            let _ = send_with_retry_metrics(|| req.try_clone().unwrap(), 3, &state.net).await;
        }
    }

    Ok(())
}

/// Directory sources: `FEDI3_RELAY_DIRECTORY_URLS` overrides the defaults; an
/// explicitly empty value disables directory discovery.
pub fn relay_directory_urls() -> Vec<String> {
    match std::env::var("FEDI3_RELAY_DIRECTORY_URLS") {
        Ok(value) => value
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        Err(_) => DEFAULT_RELAY_DIRECTORY_URLS
            .iter()
            .map(|s| s.to_string())
            .collect(),
    }
}

/// Parses a relay directory document into `(base, ws)` pairs, dropping
/// malformed or duplicate entries.
pub fn parse_directory_items(body: &[u8]) -> Vec<(String, Option<String>)> {
    let Ok(doc) = serde_json::from_slice::<RelayDirectoryResponse>(body) else {
        return Vec::new();
    };
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    for item in doc.relays.unwrap_or_default() {
        let base = item.url.trim().trim_end_matches('/');
        if base.is_empty() || !(base.starts_with("https://") || base.starts_with("http://")) {
            continue;
        }
        if out.iter().any(|(b, _)| b == base) {
            continue;
        }
        let ws = item
            .ws
            .map(|w| w.trim().trim_end_matches('/').to_string())
            .filter(|w| !w.is_empty());
        out.push((base.to_string(), ws));
    }
    out
}

async fn fetch_directory_once(state: &ApState) {
    let urls = relay_directory_urls();
    for url in urls {
        let req = state.http.get(&url).timeout(Duration::from_secs(10));
        let Ok(resp) = req.send().await else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let Ok(body) = resp.bytes().await else {
            continue;
        };
        let items = parse_directory_items(&body);
        if items.is_empty() {
            continue;
        }
        let count = items.len();
        for (base, ws) in items {
            let inferred = infer_ws_from_base(&base);
            let ws_ref = ws.as_deref().or(inferred.as_deref());
            let _ = state.social.upsert_relay_entry(&base, ws_ref, "directory");
        }
        info!("relay directory: imported {count} relays from {url}");
        return;
    }
}

fn store_relay_items(state: &ApState, items: Vec<RelayListItem>, source: &str) {
    for item in items {
        let base = item
            .relay_base_url
            .as_deref()
            .or(item.relay_url.as_deref())
            .or(item.base.as_deref());
        let ws = item
            .relay_ws_url
            .as_deref()
            .or(item.relay_ws.as_deref())
            .or(item.ws.as_deref());
        if let Some(base_url) = base {
            let ws_url = ws
                .map(|v| v.to_string())
                .or_else(|| infer_ws_from_base(base_url));
            let _ = state
                .social
                .upsert_relay_entry(base_url, ws_url.as_deref(), source);
        }
    }
}

fn infer_ws_from_base(base: &str) -> Option<String> {
    let base = base.trim();
    if base.is_empty() {
        return None;
    }
    if let Some(rest) = base.strip_prefix("https://") {
        return Some(format!("wss://{}", rest.trim_end_matches('/')));
    }
    if let Some(rest) = base.strip_prefix("http://") {
        return Some(format!("ws://{}", rest.trim_end_matches('/')));
    }
    None
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_directory_items_and_skips_invalid_entries() {
        let body = br#"{
            "schema": "fedi3-relay-directory/v1",
            "updated_at": "2026-11-07T12:00:00Z",
            "relays": [
                {"url": "https://relay-a.example.test/", "ws": "wss://relay-a.example.test"},
                {"url": "ftp://ignored.example.test"},
                {"url": "https://relay-a.example.test"},
                {"url": "https://relay-b.example.test"}
            ]
        }"#;
        let items = parse_directory_items(body);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].0, "https://relay-a.example.test");
        assert_eq!(items[0].1.as_deref(), Some("wss://relay-a.example.test"));
        assert_eq!(items[1].0, "https://relay-b.example.test");
        assert_eq!(items[1].1, None);
    }

    #[test]
    fn invalid_directory_body_yields_no_items() {
        assert!(parse_directory_items(b"not json").is_empty());
        assert!(parse_directory_items(br#"{"relays": []}"#).is_empty());
    }

    #[test]
    fn default_directory_sources_are_https() {
        assert!(!DEFAULT_RELAY_DIRECTORY_URLS.is_empty());
        for url in DEFAULT_RELAY_DIRECTORY_URLS {
            assert!(url.starts_with("https://"));
        }
    }
}
