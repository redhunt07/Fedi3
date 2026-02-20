/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use fedi3_protocol::RelayHttpRequest;
use http::{HeaderMap, Method, Uri};
use tokio::sync::watch;
use tracing::info;

use crate::ap::ApState;
use crate::http_sig::sign_request_rsa_sha256;
use crate::p2p::{DidDiscoveryRecord, P2pConfig};

#[derive(Debug, serde::Deserialize)]
struct DeviceOutboxItem {
    id: String,
    created_at_ms: i64,
    activity: serde_json::Value,
}

#[derive(Debug, serde::Deserialize)]
struct DeviceOutboxResp {
    did: Option<String>,
    items: Vec<DeviceOutboxItem>,
    latest_ms: i64,
}

#[derive(Debug, serde::Deserialize)]
struct DeviceInboxItem {
    id: String,
    created_at_ms: i64,
    activity: serde_json::Value,
}

#[derive(Debug, serde::Deserialize)]
struct DeviceInboxResp {
    did: Option<String>,
    items: Vec<DeviceInboxItem>,
    latest_ms: i64,
}

pub fn start_device_sync_worker(
    state: ApState,
    cfg: P2pConfig,
    mut shutdown: watch::Receiver<bool>,
) {
    if !cfg.enable || !cfg.device_sync_enable.unwrap_or(false) {
        return;
    }
    let interval_secs = cfg.device_sync_poll_secs.unwrap_or(30).max(5).min(3600);
    let batch_limit = cfg.sync_batch_limit.unwrap_or(50).max(1).min(200);

    let did = state
        .cfg
        .also_known_as
        .iter()
        .find(|s| s.starts_with("did:fedi3:"))
        .cloned();
    let Some(did) = did else {
        return;
    };

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

            let Ok(Some(rec)) = state.delivery.p2p_resolve_did(&did).await else {
                continue;
            };
            if let Err(e) = sync_with_did_record(&state, &did, &rec, batch_limit).await {
                info!("device sync failed: {e:#}");
            }
        }
    });
}

async fn sync_with_did_record(
    state: &ApState,
    did: &str,
    rec: &DidDiscoveryRecord,
    limit: u32,
) -> Result<()> {
    let self_peer = state.cfg.p2p_peer_id.as_deref().unwrap_or("");
    let mut peers = rec.peers.clone();
    peers.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
    peers.dedup_by(|a, b| a.peer_id == b.peer_id);

    for p in peers {
        if p.peer_id == self_peer || p.peer_id.trim().is_empty() {
            continue;
        }
        if !p.addrs.is_empty() {
            let _ = state
                .delivery
                .p2p_add_peer_addrs(&p.peer_id, p.addrs.clone())
                .await;
        }

        let since_key = format!("device_sync_since:{}", p.peer_id);
        let since = state
            .social
            .get_local_meta(&since_key)
            .ok()
            .flatten()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);

        let query = format!("?since={since}&limit={limit}");
        let path = "/.fedi3/device/outbox".to_string();
        let uri: Uri = format!("http://localhost{path}{query}").parse()?;

        let mut headers = HeaderMap::new();
        headers.insert("Accept", "application/json".parse()?);
        headers.insert("Host", "localhost".parse()?);
        headers.insert("X-Fedi3-Did", did.parse()?);

        let key_id = format!(
            "{}/users/{}#main-key",
            state.cfg.public_base_url.trim_end_matches('/'),
            state.cfg.username
        );
        sign_request_rsa_sha256(
            &state.private_key_pem,
            &key_id,
            &Method::GET,
            &uri,
            &mut headers,
            &[],
            &["(request-target)", "host", "date"],
        )?;

        let req = RelayHttpRequest {
            id: format!("devsync-{}-{}", p.peer_id, now_ms()),
            method: "GET".to_string(),
            path,
            query,
            headers: headers_to_vec(&headers),
            body_b64: "".to_string(),
        };

        let resp = match state.delivery.p2p_request(&p.peer_id, req).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        if !(200..300).contains(&resp.status) {
            continue;
        }
        let body = B64.decode(resp.body_b64.as_bytes()).unwrap_or_default();
        let out: DeviceOutboxResp = match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if out.did.as_deref().filter(|v| *v == did).is_none() {
            continue;
        }

        let mut stored: u32 = 0;
        for it in out.items {
            let id = it.id.trim().to_string();
            if id.is_empty() {
                continue;
            }
            let bytes = serde_json::to_vec(&it.activity).unwrap_or_default();
            if bytes.is_empty() {
                continue;
            }
            let _ = state.social.store_outbox_at(&id, it.created_at_ms, bytes);
            stored = stored.saturating_add(1);
        }

        if out.latest_ms > since {
            let _ = state
                .social
                .set_local_meta(&since_key, &out.latest_ms.to_string());
        }
        if stored > 0 {
            info!(peer=%p.peer_id, stored, "device sync stored");
        }

        // Inbox sync (same DID).
        let since_key = format!("device_sync_inbox_since:{}", p.peer_id);
        let since = state
            .social
            .get_local_meta(&since_key)
            .ok()
            .flatten()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);

        let query = format!("?since={since}&limit={limit}");
        let path = "/.fedi3/device/inbox".to_string();
        let uri: Uri = format!("http://localhost{path}{query}").parse()?;

        let mut headers = HeaderMap::new();
        headers.insert("Accept", "application/json".parse()?);
        headers.insert("Host", "localhost".parse()?);
        headers.insert("X-Fedi3-Did", did.parse()?);

        let key_id = format!(
            "{}/users/{}#main-key",
            state.cfg.public_base_url.trim_end_matches('/'),
            state.cfg.username
        );
        sign_request_rsa_sha256(
            &state.private_key_pem,
            &key_id,
            &Method::GET,
            &uri,
            &mut headers,
            &[],
            &["(request-target)", "host", "date"],
        )?;

        let req = RelayHttpRequest {
            id: format!("devsync-inbox-{}-{}", p.peer_id, now_ms()),
            method: "GET".to_string(),
            path,
            query,
            headers: headers_to_vec(&headers),
            body_b64: "".to_string(),
        };

        let resp = match state.delivery.p2p_request(&p.peer_id, req).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        if !(200..300).contains(&resp.status) {
            continue;
        }
        let body = B64.decode(resp.body_b64.as_bytes()).unwrap_or_default();
        let inbox: DeviceInboxResp = match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if inbox.did.as_deref().filter(|v| *v == did).is_none() {
            continue;
        }

        let mut stored_inbox: u32 = 0;
        for it in inbox.items {
            let id = it.id.trim().to_string();
            if id.is_empty() {
                continue;
            }
            // Dedup using the shared inbox_seen table.
            if !state.social.mark_inbox_seen(&id).unwrap_or(false) {
                continue;
            }
            let bytes = serde_json::to_vec(&it.activity).unwrap_or_default();
            if bytes.is_empty() {
                continue;
            }
            let actor = it.activity.get("actor").and_then(|v| v.as_str());
            let ty = it.activity.get("type").and_then(|v| v.as_str());
            let _ = state.social.store_inbox_activity_at(
                &id,
                it.created_at_ms,
                actor,
                ty,
                bytes.clone(),
            );
            let _ = state.social.insert_federated_feed_item(&id, actor, bytes);
            stored_inbox = stored_inbox.saturating_add(1);
        }

        if inbox.latest_ms > since {
            let _ = state
                .social
                .set_local_meta(&since_key, &inbox.latest_ms.to_string());
        }
        if stored_inbox > 0 {
            info!(peer=%p.peer_id, stored=stored_inbox, "device inbox sync stored");
        }
    }
    Ok(())
}

fn headers_to_vec(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|vs| (k.to_string(), vs.to_string())))
        .collect()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
