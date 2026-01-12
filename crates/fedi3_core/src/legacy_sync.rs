/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use crate::ap;
use crate::delivery::Delivery;
use crate::social_db::SocialDb;
use anyhow::{Context, Result};
use reqwest::header::ACCEPT;
use reqwest::header::USER_AGENT;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::watch;
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

pub async fn run_legacy_sync_now(state: &ap::ApState, max_pages: usize, max_items_per_actor: usize) -> Result<()> {
    let delivery = state.delivery.clone();
    let social = state.social.clone();
    let http = state.http.clone();
    run_once_with_limits(state, &delivery, &social, &http, max_pages, max_items_per_actor).await
}

async fn run_once(state: &ap::ApState, delivery: &Delivery, social: &SocialDb, http: &reqwest::Client) -> Result<()> {
    run_once_with_limits(state, delivery, social, http, 2, 200).await
}

async fn run_once_with_limits(
    state: &ap::ApState,
    delivery: &Delivery,
    social: &SocialDb,
    http: &reqwest::Client,
    max_pages: usize,
    max_items_per_actor: usize,
) -> Result<()> {
    let following = social.list_following_accepted_ids(5000).unwrap_or_default();
    if following.is_empty() {
        return Ok(());
    }

    for actor_url in following {
        // Skip Fedi3 peers (they have p2p sync + direct delivery mechanisms already).
        if let Ok(Some(meta)) = social.get_actor_meta(&actor_url) {
            if meta.is_fedi3 {
                continue;
            }
        }
        if let Err(e) = poll_actor_outbox(state, delivery, social, http, &actor_url, max_pages, max_items_per_actor).await {
            debug!("legacy poll failed for {actor_url}: {e:#}");
        }
    }
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
