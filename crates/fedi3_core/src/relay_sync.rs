/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

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
