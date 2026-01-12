/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use fedi3_protocol::RelayHttpRequest;
use tokio::sync::watch;
use tracing::info;

use crate::ap::ApState;
use crate::delivery::is_public_activity;
use crate::p2p::P2pConfig;

#[derive(Debug, serde::Deserialize)]
struct SyncOutboxResp {
    items: Vec<serde_json::Value>,
    latest_ms: i64,
}

pub fn start_p2p_sync_worker(state: ApState, cfg: P2pConfig, mut shutdown: watch::Receiver<bool>) {
    if !cfg.enable || !cfg.sync_enable.unwrap_or(true) {
        return;
    }
    let interval_secs = cfg.sync_poll_secs.unwrap_or(30).max(5).min(3600);
    let batch_limit = cfg.sync_batch_limit.unwrap_or(50).max(1).min(200);

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

            let following = match state.social.list_following_accepted_ids(1000) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if following.is_empty() {
                continue;
            }

            for actor_url in following {
                if *shutdown.borrow() {
                    break;
                }

                let since = state.social.get_p2p_sync_since(&actor_url).unwrap_or(0);

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
                    id: format!("sync-{}-{}", short_hash(&actor_url), now_ms()),
                    method: "GET".to_string(),
                    path: "/.fedi3/sync/outbox".to_string(),
                    query: format!("?since={since}&limit={batch_limit}"),
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
                let sync: SyncOutboxResp = match serde_json::from_slice(&body) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let mut stored: u32 = 0;
                for activity in sync.items {
                    if !is_public_activity(&activity) {
                        continue;
                    }
                    let ty = activity.get("type").and_then(|v| v.as_str());
                    let actor = activity.get("actor").and_then(|v| v.as_str());

                    let activity_id = activity_dedup_id(&activity);
                    if !state.social.mark_inbox_seen(&activity_id).unwrap_or(false) {
                        continue;
                    }
                    let bytes = canonical_json_bytes(&activity);
                    let _ = state.social.store_inbox_activity(&activity_id, actor, ty, bytes.clone());
                    let _ = state.social.insert_federated_feed_item(&activity_id, actor, bytes);
                    if let Some(a) = actor {
                        let _ = state.social.upsert_actor_meta(a, true);
                    }
                    stored = stored.saturating_add(1);
                }

                if sync.latest_ms > since {
                    let _ = state.social.set_p2p_sync_since(&actor_url, sync.latest_ms);
                }
                if stored > 0 {
                    info!(peer=%peer_id, actor=%actor_url, stored, "p2p sync stored");
                }
            }
        }
    });
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn short_hash(s: &str) -> String {
    use sha2::Digest as _;
    let mut h = sha2::Sha256::new();
    h.update(s.as_bytes());
    hex::encode(&h.finalize()[..8])
}

fn activity_dedup_id(activity: &serde_json::Value) -> String {
    if let Some(id) = activity.get("id").and_then(|v| v.as_str()) {
        let id = id.trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }
    let bytes = canonical_json_bytes(activity);
    format!("urn:fedi3:sync:{}", short_hash(&String::from_utf8_lossy(&bytes)))
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
        serde_json::Value::Array(arr) => serde_json::Value::Array(arr.iter().map(canonicalize_json).collect()),
        _ => v.clone(),
    }
}
