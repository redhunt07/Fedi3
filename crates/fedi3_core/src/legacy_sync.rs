/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use crate::ap;
use crate::delivery::Delivery;
use crate::social_db::SocialDb;
use anyhow::{Context, Result};
use reqwest::header::USER_AGENT;
use reqwest::header::{ACCEPT, AUTHORIZATION};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::time::sleep;
use tracing::{debug, warn};

#[derive(Debug, Deserialize)]
struct ActorDoc {
    outbox: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrderedCollection {
    first: Option<String>,
    #[serde(rename = "orderedItems")]
    ordered_items: Option<Vec<Value>>,
}

#[derive(Debug, Deserialize)]
struct OrderedCollectionPage {
    next: Option<String>,
    #[serde(rename = "orderedItems")]
    ordered_items: Option<Vec<Value>>,
}

#[derive(Debug, Deserialize)]
struct RelayLegacySyncResponse {
    #[serde(default)]
    schema_version: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    checkpoint_ms: Option<i64>,
    #[serde(default)]
    items: Vec<RelayLegacyItem>,
    #[serde(default)]
    next: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RelayLegacyItem {
    note: Value,
    created_at_ms: i64,
}

const LEGACY_STREAMS: [&str; 4] = ["home", "social", "local", "federated"];
const LEGACY_RETRY_ATTEMPTS: u32 = 4;
const LEGACY_RETRY_BASE_MS: u64 = 250;
const LEGACY_RETRY_MAX_MS: u64 = 4_000;

pub fn start_legacy_sync_worker(
    state: ap::ApState,
    delivery: Arc<Delivery>,
    social: Arc<SocialDb>,
    http: reqwest::Client,
    mut shutdown: watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        // Misskey-like "home enrichment" requires that the instance keeps pulling what it follows
        // even if it missed inbox deliveries while offline.
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(300));
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
            if let Err(e) = run_once(&state, &delivery, &social, &http).await {
                warn!("legacy sync error: {e:#}");
            }
        }
    });
}

pub async fn run_legacy_sync_now(
    state: &ap::ApState,
    max_pages: usize,
    max_items_per_actor: usize,
    include_fedi3: bool,
) -> Result<()> {
    let delivery = state.delivery.clone();
    let social = state.social.clone();
    let http = state.http.clone();
    run_once_with_limits(
        state,
        &delivery,
        &social,
        &http,
        max_pages,
        max_items_per_actor,
        include_fedi3,
    )
    .await
}

async fn run_once(
    state: &ap::ApState,
    delivery: &Delivery,
    social: &SocialDb,
    http: &reqwest::Client,
) -> Result<()> {
    run_once_with_limits(state, delivery, social, http, 2, 200, false).await
}

async fn run_once_with_limits(
    state: &ap::ApState,
    delivery: &Delivery,
    social: &SocialDb,
    http: &reqwest::Client,
    max_pages: usize,
    max_items_per_actor: usize,
    include_fedi3: bool,
) -> Result<()> {
    let mut relay_ingested_total = 0usize;
    let mut relay_ok = false;
    let mut relay_last_err = String::new();
    for stream in LEGACY_STREAMS {
        match sync_from_relay_legacy_stream(
            state,
            social,
            http,
            max_pages,
            max_items_per_actor,
            stream,
        )
        .await
        {
            Ok(ingested) => {
                relay_ok = true;
                relay_ingested_total += ingested;
                let _ = social.set_local_meta(&format!("relay_legacy_last_error_{stream}"), "");
                if ingested > 0 {
                    debug!("relay legacy sync stream={stream} ingested {ingested} items");
                }
            }
            Err(e) => {
                let _ = social.set_local_meta(
                    &format!("relay_legacy_last_error_{stream}"),
                    &e.to_string(),
                );
                relay_last_err = e.to_string();
                debug!("relay legacy sync stream={stream} unavailable: {e:#}");
            }
        }
    }
    if relay_ok {
        let _ = social.set_local_meta("relay_legacy_last_error", "");
        let _ = social.set_local_meta("relay_legacy_last_items", &relay_ingested_total.to_string());
        return Ok(());
    }
    if !relay_last_err.is_empty() {
        let _ = social.set_local_meta("relay_legacy_sync_phase", "error");
        let _ = social.set_local_meta("relay_legacy_last_error", &relay_last_err);
    }

    let following = social.list_following_accepted_ids(5000).unwrap_or_default();
    if following.is_empty() {
        return Ok(());
    }

    for actor_url in following {
        // Skip Fedi3 peers (they have p2p sync + direct delivery mechanisms already).
        if let Ok(Some(meta)) = social.get_actor_meta(&actor_url) {
            if meta.is_fedi3 && !include_fedi3 {
                continue;
            }
        }
        if let Err(e) = poll_actor_outbox(
            state,
            delivery,
            social,
            http,
            &actor_url,
            max_pages,
            max_items_per_actor,
        )
        .await
        {
            debug!("legacy poll failed for {actor_url}: {e:#}");
        }
    }
    let _ = social.set_local_meta("relay_legacy_sync_phase", "ready");
    Ok(())
}

async fn poll_actor_outbox(
    state: &ap::ApState,
    _delivery: &Delivery,
    social: &SocialDb,
    http: &reqwest::Client,
    actor_url: &str,
    max_pages: usize,
    max_items_per_actor: usize,
) -> Result<()> {
    let actor_url = actor_url.trim();
    if actor_url.is_empty() {
        return Ok(());
    }

    // Best-effort: resolve actor and fetch outbox URL.
    let actor_doc = http
        .get(actor_url)
        .header(
            ACCEPT,
            "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"",
        )
        .header(USER_AGENT, format!("fedi3/{}", env!("CARGO_PKG_VERSION")))
        .send()
        .await
        .with_context(|| format!("fetch actor: {actor_url}"))?
        .error_for_status()
        .with_context(|| format!("actor not ok: {actor_url}"))?
        .json::<ActorDoc>()
        .await
        .with_context(|| format!("parse actor json: {actor_url}"))?;

    let Some(outbox_url) = actor_doc.outbox else {
        return Ok(());
    };

    // Step 1: fetch outbox collection to find its first page.
    let col = http
        .get(&outbox_url)
        .header(ACCEPT, "application/activity+json")
        .header(USER_AGENT, format!("fedi3/{}", env!("CARGO_PKG_VERSION")))
        .send()
        .await
        .with_context(|| format!("fetch outbox: {outbox_url}"))?
        .error_for_status()
        .with_context(|| format!("outbox not ok: {outbox_url}"))?
        .json::<OrderedCollection>()
        .await
        .with_context(|| format!("parse outbox json: {outbox_url}"))?;

    // Some servers return the items directly on the collection (rare). Prefer first page when present.
    let first_page = col.first.clone().unwrap_or(outbox_url.clone());

    // Step 2: fetch first page (newest items).
    let mut next_page: Option<String> = Some(first_page);
    let mut pages = 0usize;
    let mut ingested = 0usize;
    let mut dup_streak = 0usize;

    while let Some(page_url) = next_page.take() {
        if pages >= max_pages {
            break;
        }
        if ingested >= max_items_per_actor {
            break;
        }

        let page_json = http
            .get(&page_url)
            .header(ACCEPT, "application/activity+json")
            .header(USER_AGENT, format!("fedi3/{}", env!("CARGO_PKG_VERSION")))
            .send()
            .await
            .with_context(|| format!("fetch outbox page: {page_url}"))?
            .error_for_status()
            .with_context(|| format!("outbox page not ok: {page_url}"))?
            .bytes()
            .await?;

        let parsed_page = serde_json::from_slice::<OrderedCollectionPage>(&page_json).ok();
        let items = parsed_page
            .as_ref()
            .and_then(|p| p.ordered_items.clone())
            .or_else(|| col.ordered_items.clone())
            .unwrap_or_default();
        next_page = parsed_page.and_then(|p| p.next);

        if items.is_empty() {
            break;
        }

        for activity in items.into_iter() {
            if ingested >= max_items_per_actor {
                break;
            }
            if !activity.is_object() {
                continue;
            }
            let dedup_id = ap::activity_dedup_id_public(&activity);
            match social.mark_inbox_seen(&dedup_id) {
                Ok(true) => {
                    dup_streak = 0;
                }
                Ok(false) => {
                    dup_streak += 1;
                    if dup_streak >= 25 {
                        next_page = None;
                        break;
                    }
                    continue;
                }
                Err(_) => continue,
            }

            // Best-effort: store and process as if it arrived via inbox.
            if let Err(e) = ap::process_inbox_activity(state, &activity).await {
                debug!("process pulled activity failed ({dedup_id}): {e:#}");
                continue;
            }

            // Best-effort: warm object cache if activity references a remote object URL.
            if let Some(obj_id) = activity
                .get("object")
                .and_then(|o| o.as_str())
                .map(str::trim)
                .filter(|s| s.starts_with("http://") || s.starts_with("https://"))
            {
                let _ = crate::ap::fetch_and_store_object(state, obj_id).await;
            }

            ingested += 1;
        }

        pages += 1;
    }

    debug!("legacy poll ingested {ingested} items from {actor_url}");
    Ok(())
}

async fn sync_from_relay_legacy_stream(
    state: &ap::ApState,
    social: &SocialDb,
    http: &reqwest::Client,
    max_pages: usize,
    max_items_per_actor: usize,
    stream: &str,
) -> Result<usize> {
    let checkpoint_key = format!("relay_legacy_checkpoint_ms_{stream}");
    let relay_base = state
        .cfg
        .relay_base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .context("missing relay_base_url")?;
    let token = state
        .cfg
        .relay_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .context("missing relay_token")?;
    let username = state.cfg.username.trim();
    if username.is_empty() {
        anyhow::bail!("missing username");
    }

    let checkpoint = social
        .get_local_meta(&checkpoint_key)
        .ok()
        .flatten()
        .or_else(|| {
            social
                .get_local_meta("relay_legacy_checkpoint_ms")
                .ok()
                .flatten()
        })
        .and_then(|v| v.parse::<i64>().ok());
    let mut cursor = None::<String>;
    let mut pages = 0usize;
    let mut total_ingested = 0usize;
    let mode_bootstrap = checkpoint.is_none();
    let mut latest_checkpoint = checkpoint.unwrap_or(0);

    let _ = social.set_local_meta(
        "relay_legacy_sync_phase",
        if mode_bootstrap {
            "bootstrap_download"
        } else {
            "delta_catchup"
        },
    );

    loop {
        if pages >= max_pages.max(1) {
            break;
        }
        if total_ingested >= max_items_per_actor.max(1) {
            break;
        }
        let page_limit = if mode_bootstrap { 1000 } else { 200 };
        let url = if mode_bootstrap {
            format!(
                "{}/_fedi3/relay/legacy/bootstrap?v=1&username={}&stream={}&limit={}&gzip=true{}",
                relay_base.trim_end_matches('/'),
                urlencoding::encode(username),
                stream,
                page_limit,
                cursor
                    .as_ref()
                    .map(|c| format!("&cursor={}", urlencoding::encode(c)))
                    .unwrap_or_default()
            )
        } else {
            format!(
                "{}/_fedi3/relay/legacy/sync?v=1&username={}&stream={}&limit={}&since={}{}",
                relay_base.trim_end_matches('/'),
                urlencoding::encode(username),
                stream,
                page_limit,
                checkpoint.unwrap_or(0),
                cursor
                    .as_ref()
                    .map(|c| format!("&cursor={}", urlencoding::encode(c)))
                    .unwrap_or_default()
            )
        };

        let data = fetch_relay_page_with_retry(http, &url, token, stream).await?;
        if data.schema_version.as_deref() != Some("1") {
            anyhow::bail!("unsupported relay legacy schema version");
        }
        if mode_bootstrap && data.mode.as_deref() != Some("bootstrap") {
            anyhow::bail!("unexpected relay mode");
        }
        if !mode_bootstrap && data.mode.as_deref() != Some("delta") {
            anyhow::bail!("unexpected relay mode");
        }
        if let Some(cp) = data.checkpoint_ms {
            if cp > latest_checkpoint {
                latest_checkpoint = cp;
            }
        }

        if data.items.is_empty() {
            break;
        }
        let _ = social.set_local_meta("relay_legacy_sync_phase", "apply");
        for item in data.items {
            if total_ingested >= max_items_per_actor.max(1) {
                break;
            }
            if let Some(cp) = Some(item.created_at_ms) {
                if cp > latest_checkpoint {
                    latest_checkpoint = cp;
                }
            }
            if ingest_relay_note_as_activity(
                state,
                social,
                stream,
                item.note,
                item.created_at_ms,
            )
            .await?
            {
                total_ingested += 1;
            }
        }

        cursor = data.next;
        pages += 1;
        if cursor.is_none() {
            break;
        }
    }

    if latest_checkpoint <= 0 {
        latest_checkpoint = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
    }
    let _ = social.set_local_meta(&checkpoint_key, &latest_checkpoint.to_string());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let _ = social.set_local_meta(
        &format!("relay_legacy_last_ok_ms_{stream}"),
        &now.to_string(),
    );
    let _ = social.set_local_meta(
        &format!("relay_legacy_last_items_{stream}"),
        &total_ingested.to_string(),
    );
    let _ = social.set_local_meta(&format!("relay_legacy_last_error_{stream}"), "");
    let _ = social.set_local_meta("relay_legacy_last_error", "");
    if mode_bootstrap {
        let _ = social.set_local_meta("relay_legacy_bootstrap_done", "1");
    }
    let _ = social.set_local_meta("relay_legacy_sync_phase", "ready");
    Ok(total_ingested)
}

fn retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn retry_delay_ms(stream: &str, attempt: u32) -> u64 {
    let exp = LEGACY_RETRY_BASE_MS
        .saturating_mul(1u64 << attempt.min(6))
        .min(LEGACY_RETRY_MAX_MS);
    let stream_bias: u64 = stream.bytes().fold(0u64, |acc, b| acc + b as u64) % 120;
    exp.saturating_add(stream_bias)
}

async fn fetch_relay_page_with_retry(
    http: &reqwest::Client,
    url: &str,
    token: &str,
    stream: &str,
) -> Result<RelayLegacySyncResponse> {
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..LEGACY_RETRY_ATTEMPTS {
        let resp = http
            .get(url)
            .header(
                ACCEPT,
                "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\", application/json",
            )
            .header(USER_AGENT, format!("fedi3/{}", env!("CARGO_PKG_VERSION")))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .send()
            .await;

        match resp {
            Ok(resp) => {
                if resp.status().is_success() {
                    return resp
                        .json::<RelayLegacySyncResponse>()
                        .await
                        .with_context(|| format!("invalid relay legacy sync payload: {url}"));
                }
                if retryable_status(resp.status()) && attempt + 1 < LEGACY_RETRY_ATTEMPTS {
                    sleep(std::time::Duration::from_millis(retry_delay_ms(stream, attempt))).await;
                    continue;
                }
                anyhow::bail!("relay legacy sync status {} for {url}", resp.status());
            }
            Err(e) => {
                last_err = Some(anyhow::anyhow!(e).context(format!("relay legacy sync request failed: {url}")));
                if attempt + 1 < LEGACY_RETRY_ATTEMPTS {
                    sleep(std::time::Duration::from_millis(retry_delay_ms(stream, attempt))).await;
                    continue;
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("relay legacy sync failed")))
}

async fn ingest_relay_note_as_activity(
    state: &ap::ApState,
    social: &SocialDb,
    stream: &str,
    mut note: Value,
    created_at_ms: i64,
) -> Result<bool> {
    if !note.is_object() {
        return Ok(false);
    }
    // Relay legacy feed can carry Page/Article wrappers: normalize to a Note-like object
    // so inbox processing and UI rendering stay deterministic.
    if let Some(obj) = note.as_object_mut() {
        match obj.get("type").and_then(|v| v.as_str()) {
            Some("Page") | Some("Article") => {
                obj.insert("type".to_string(), Value::String("Note".to_string()));
            }
            _ => {}
        }
        if obj.get("content").and_then(|v| v.as_str()).is_none() {
            if let Some(summary) = obj.get("name").and_then(|v| v.as_str()) {
                obj.insert("content".to_string(), Value::String(summary.to_string()));
            }
        }
    }
    let Some(note_id) = note
        .get("id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .map(|s| s.to_string())
    else {
        return Ok(false);
    };
    if note_id.is_empty() {
        return Ok(false);
    }
    let actor = note
        .get("attributedTo")
        .or_else(|| note.get("actor"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if actor.is_empty() {
        return Ok(false);
    }
    let relay_seen_ms = if created_at_ms > 0 {
        created_at_ms
    } else {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    };
    if let Some(obj) = note.as_object_mut() {
        obj.entry("created_at_ms".to_string())
            .or_insert_with(|| Value::Number(relay_seen_ms.into()));
    }
    let relay_seen_published = {
        let secs = relay_seen_ms.div_euclid(1000);
        let rem_ms = relay_seen_ms.rem_euclid(1000) as i128;
        time::OffsetDateTime::from_unix_timestamp(secs)
            .ok()
            .and_then(|dt| {
                dt.checked_add(time::Duration::milliseconds(rem_ms as i64))
                    .and_then(|x| x.format(&time::format_description::well_known::Rfc3339).ok())
            })
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
    };
    // Keep activity published aligned to relay-seen time for stable recency ordering.
    // Original note published timestamp is preserved inside `note` and used by UI labels.
    let published = relay_seen_published;
    let activity_id = format!("{note_id}#fedi3-relay-sync:{relay_seen_ms}");
    if !social.mark_inbox_seen(&activity_id).unwrap_or(false) {
        return Ok(false);
    }
    let activity = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": activity_id,
        "type": "Create",
        "actor": actor,
        "object": note,
        "published": published,
        "created_at_ms": relay_seen_ms,
        "fedi3RelaySync": true,
        "to": ["https://www.w3.org/ns/activitystreams#Public"]
    });
    let activity_bytes = serde_json::to_vec(&activity).unwrap_or_default();
    if let Err(e) = ap::process_inbox_activity(state, &activity).await {
        debug!("relay legacy activity ingest failed ({note_id}): {e:#}");
        return Ok(false);
    }
    if stream == "social" && !activity_bytes.is_empty() {
        let _ = social.insert_global_feed_item(&activity_id, Some(&actor), activity_bytes);
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_increases_and_caps() {
        let a = retry_delay_ms("home", 0);
        let b = retry_delay_ms("home", 1);
        let c = retry_delay_ms("home", 6);
        assert!(b > a);
        assert!(c <= LEGACY_RETRY_MAX_MS + 120);
    }

    #[test]
    fn retryable_status_policy() {
        assert!(retryable_status(reqwest::StatusCode::TOO_MANY_REQUESTS));
        assert!(retryable_status(reqwest::StatusCode::BAD_GATEWAY));
        assert!(!retryable_status(reqwest::StatusCode::BAD_REQUEST));
    }
}
