/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use fedi3_protocol::RelayHttpRequest;
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

pub fn start_relay_sync_worker(state: ApState, mut shutdown: watch::Receiver<bool>) {
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
    });
}

pub async fn sync_once(state: &ApState) -> anyhow::Result<()> {
    // Always store current relay.
    if let Some(base) = state.cfg.relay_base_url.as_ref() {
        state.social.upsert_relay_entry(base, None, "self").ok();
    }

    // Fetch relay list from current relay (best-effort).
    if let Some(base) = state.cfg.relay_base_url.as_ref() {
        let url = format!("{}/_fedi3/relay/relays", base.trim_end_matches('/'));
        let mut req = state.http.get(&url);
        if let Some(tok) = state.cfg.relay_token.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            req = req.header("Authorization", format!("Bearer {}", tok));
        }
        if let Ok(resp) = send_with_retry_metrics(|| req.try_clone().unwrap(), 3, &state.net).await {
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
            if let Some(tok) = state.cfg.relay_token.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                req = req.header("Authorization", format!("Bearer {}", tok));
            }
            let _ = send_with_retry_metrics(|| req.try_clone().unwrap(), 3, &state.net).await;
        }
    }

    Ok(())
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
            let _ = state.social.upsert_relay_entry(base_url, ws_url.as_deref(), source);
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
