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
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Misskey-like "home enrichment" requires that the instance keeps pulling what it follows
        // even if it missed inbox deliveries while offline.
        loop {
            tokio::select! {
                _ = crate::power::sleep(300, 900, 1800) => {}
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() { break; }
                    continue;
                }
            }
            if *shutdown.borrow() {
                break;
            }
            if let Err(e) = run_once(&state, &delivery, &social, &http).await {
                warn!("legacy sync error: {e:#}");
            }
        }
    })
}

pub async fn run_legacy_sync_now(
    state: &ap::ApState,
    max_pages: usize,
    max_items_per_actor: usize,
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
    )
    .await
}

async fn run_once(
    state: &ap::ApState,
    delivery: &Delivery,
    social: &SocialDb,
    http: &reqwest::Client,
) -> Result<()> {
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
    // Pending follows are polled too: their public posts belong in the
    // federated timeline even before the follow request is accepted.
    let following = social.list_following_ids(5000).unwrap_or_default();
    if following.is_empty() {
        return Ok(());
    }

    for actor_url in following {
        // Fedi3 peers used to be skipped here because they have p2p sync and direct
        // delivery. That left a fresh follow with an empty timeline until the peer
        // posted again, so we poll every followed actor. Ingest dedupes by activity
        // id, so p2p deliveries and this poll never create duplicates.
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

    // H-03: followed actors and their outbox/page URLs come from remote servers.
    crate::net_policy::check_url_allowed(actor_url).await?;

    let actor_doc: ActorDoc =
        fetch_legacy_json(http, actor_url, crate::net_policy::MAX_ACTOR_DOC_BYTES).await?;

    let Some(outbox_url) = actor_doc.outbox else {
        return Ok(());
    };
    crate::net_policy::check_url_allowed(&outbox_url).await?;

    let col: OrderedCollection =
        fetch_legacy_json(http, &outbox_url, crate::net_policy::MAX_OUTBOX_PAGE_BYTES).await?;

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

        let page_json =
            fetch_legacy_bytes(http, &page_url, crate::net_policy::MAX_OUTBOX_PAGE_BYTES).await?;

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

async fn fetch_legacy_bytes(_http: &reqwest::Client, url: &str, cap: usize) -> Result<Vec<u8>> {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        ACCEPT,
        http::HeaderValue::from_static("application/activity+json"),
    );
    headers.insert(
        USER_AGENT,
        http::HeaderValue::from_static(concat!("fedi3/", env!("CARGO_PKG_VERSION"))),
    );
    let response = crate::net_policy::get_checked(url, headers)
        .await?
        .error_for_status()?;
    crate::net_policy::read_capped(response, cap).await
}

async fn fetch_legacy_json<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    url: &str,
    cap: usize,
) -> Result<T> {
    serde_json::from_slice(&fetch_legacy_bytes(http, url, cap).await?)
        .context("invalid legacy JSON")
}

#[cfg(test)]
mod bounded_fetch_tests {
    use super::*;
    #[tokio::test]
    async fn actor_collection_and_page_fetches_are_capped_before_parsing() {
        // The escape hatch is process-wide: hold the shared lock so the
        // net_policy tests cannot flip it while this fetch is in flight.
        let _guard = crate::net_policy::ENV_LOCK.lock().await;
        std::env::set_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH", "true");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/large", listener.local_addr().unwrap());
        let app =
            axum::Router::new().route("/large", axum::routing::get(|| async { vec![b' '; 4096] }));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        assert!(fetch_legacy_json::<ActorDoc>(&client, &url, 1024)
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("too large"));
        assert!(fetch_legacy_json::<OrderedCollection>(&client, &url, 1024)
            .await
            .is_err());
        assert!(fetch_legacy_bytes(&client, &url, 1024).await.is_err());
        server.abort();
        std::env::remove_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH");
    }
}
