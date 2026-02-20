/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use fedi3_protocol::RelayHttpRequest;
use http::{HeaderMap, Method, Uri};
use tokio::sync::watch;
use tracing::info;

use crate::ap::ApState;
use crate::p2p::P2pConfig;
use crate::http_sig::sign_request_rsa_sha256;

#[derive(Debug, serde::Serialize)]
struct P2pSyncRequest {
    clock: std::collections::HashMap<String, i64>,
    limit: u32,
}

#[derive(Debug, serde::Deserialize)]
struct P2pSyncItem {
    actor_id: String,
    lamport: i64,
    activity: serde_json::Value,
}

#[derive(Debug, serde::Deserialize)]
struct P2pSyncResponse {
    items: Vec<P2pSyncItem>,
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

            let peers = match state.social.list_actor_meta_fedi3(1000) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if peers.is_empty() {
                continue;
            }

            let mut clock = std::collections::HashMap::new();
            if let Ok(entries) = state.social.list_p2p_actor_clock(5000) {
                for (actor_id, lamport) in entries {
                    clock.insert(actor_id, lamport);
                }
            }

            for actor_url in peers {
                if *shutdown.borrow() {
                    break;
                }

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

                let req_body = serde_json::to_vec(&P2pSyncRequest {
                    clock: clock.clone(),
                    limit: batch_limit,
                })
                .unwrap_or_default();

                let mut headers = HeaderMap::new();
                headers.insert("accept", "application/json".parse().unwrap());
                headers.insert("content-type", "application/json".parse().unwrap());

                let uri: Uri = "http://localhost/.fedi3/sync/activities"
                    .parse()
                    .unwrap_or_else(|_| Uri::from_static("http://localhost/"));
                let key_id = format!(
                    "{}/users/{}#main-key",
                    state.cfg.public_base_url.trim_end_matches('/'),
                    state.cfg.username
                );
                if sign_request_rsa_sha256(
                    &state.private_key_pem,
                    &key_id,
                    &Method::POST,
                    &uri,
                    &mut headers,
                    &req_body,
                    &["(request-target)", "host", "date", "digest"],
                )
                .is_err()
                {
                    continue;
                }

                let mut header_vec = Vec::new();
                for (k, v) in headers.iter() {
                    if let Ok(val) = v.to_str() {
                        header_vec.push((k.to_string(), val.to_string()));
                    }
                }
                let req = RelayHttpRequest {
                    id: format!("sync-{}-{}", short_hash(&actor_url), now_ms()),
                    method: "POST".to_string(),
                    path: "/.fedi3/sync/activities".to_string(),
                    query: "".to_string(),
                    headers: header_vec,
                    body_b64: B64.encode(req_body),
                };

                let resp = match state.delivery.p2p_request(&peer_id, req).await {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if !(200..300).contains(&resp.status) {
                    continue;
                }

                let body = B64.decode(resp.body_b64.as_bytes()).unwrap_or_default();
                let sync: P2pSyncResponse = match serde_json::from_slice(&body) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let mut stored: u32 = 0;
                for item in sync.items {
                    let ty = item.activity.get("type").and_then(|v| v.as_str());
                    let actor = if !item.actor_id.trim().is_empty() {
                        Some(item.actor_id.as_str())
                    } else {
                        item.activity.get("actor").and_then(|v| v.as_str())
                    };
                    let Some(actor) = actor else {
                        continue;
                    };

                    let activity_id = activity_dedup_id(&item.activity);
                    let bytes = canonical_json_bytes(&item.activity);
                    let _ = state
                        .social
                        .upsert_p2p_activity(&activity_id, actor, item.lamport, bytes.clone());
                    if !state.social.mark_inbox_seen(&activity_id).unwrap_or(false) {
                        continue;
                    }
                    let _ =
                        state
                            .social
                            .store_inbox_activity(&activity_id, Some(actor), ty, bytes.clone());
                    if crate::delivery::is_public_activity(&item.activity) {
                        let _ = state
                            .social
                            .insert_federated_feed_item(&activity_id, Some(actor), bytes);
                    }
                    let _ = state.social.upsert_actor_meta(actor, true);
                    stored = stored.saturating_add(1);
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
    format!(
        "urn:fedi3:sync:{}",
        short_hash(&String::from_utf8_lossy(&bytes))
    )
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
