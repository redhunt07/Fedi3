/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use crate::http_retry::send_with_retry;
use crate::http_sig::sign_request_rsa_sha256;
use crate::social_db::{ObjectFetchJob, SocialDb};
use anyhow::Result;
use http::{HeaderMap, Method, Uri};
use rand::{rngs::OsRng, RngCore};
use std::{sync::Arc, time::Duration};
use tokio::sync::{watch, Notify};
use tracing::warn;

#[derive(Clone)]
pub struct ObjectFetchWorker {
    notify: Arc<Notify>,
    max_attempts: u32,
    base_backoff_secs: u64,
    max_backoff_secs: u64,
}

impl Default for ObjectFetchWorker {
    fn default() -> Self {
        Self {
            notify: Arc::new(Notify::new()),
            max_attempts: 10,
            base_backoff_secs: 10,
            max_backoff_secs: 3600,
        }
    }
}

impl ObjectFetchWorker {
    pub fn notify(&self) {
        self.notify.notify_one();
    }

    pub fn start_with_signing(
        &self,
        shutdown: watch::Receiver<bool>,
        db: Arc<SocialDb>,
        http: reqwest::Client,
        signed: Option<SignedFetchConfig>,
    ) {
        let this = self.clone();
        tokio::spawn(async move {
            if let Err(e) = this.run_loop(shutdown, db, http, signed).await {
                warn!("object fetch worker stopped: {e:#}");
            }
        });
    }

    pub fn start(&self, shutdown: watch::Receiver<bool>, db: Arc<SocialDb>, http: reqwest::Client) {
        self.start_with_signing(shutdown, db, http, None);
    }

    async fn run_loop(
        &self,
        mut shutdown: watch::Receiver<bool>,
        db: Arc<SocialDb>,
        http: reqwest::Client,
        signed: Option<SignedFetchConfig>,
    ) -> Result<()> {
        let tick = Duration::from_secs(3);
        loop {
            if *shutdown.borrow() {
                break;
            }

            let jobs = tokio::task::spawn_blocking({
                let db = db.clone();
                move || db.fetch_due_object_jobs(20)
            })
            .await??;

            if jobs.is_empty() {
                tokio::select! {
                    _ = self.notify.notified() => {}
                    _ = tokio::time::sleep(tick) => {}
                    _ = shutdown.changed() => {}
                }
                continue;
            }

            for job in jobs {
                if *shutdown.borrow() {
                    break;
                }
                if let Err(e) = self.process_one(&db, &http, &signed, job).await {
                    warn!("object fetch job error: {e:#}");
                }
            }
        }
        Ok(())
    }

    async fn process_one(
        &self,
        db: &SocialDb,
        http: &reqwest::Client,
        signed: &Option<SignedFetchConfig>,
        job: ObjectFetchJob,
    ) -> Result<()> {
        let attempt_no = job.attempt.saturating_add(1);
        let res = fetch_one(http, signed, &job.object_url).await;

        match res {
            Ok(Some((id, json_bytes, is_tombstone))) => {
                let _ = tokio::task::spawn_blocking({
                    let db = db.clone();
                    let url = job.object_url.clone();
                    let actor_id = extract_actor_id_from_object_json(&json_bytes);
                    move || -> Result<()> {
                        let _ = db.upsert_object_with_actor(
                            &id,
                            actor_id.as_deref(),
                            json_bytes.clone(),
                        );
                        if is_tombstone {
                            let _ = db.mark_object_deleted(&id);
                        }
                        db.mark_object_fetch_done(&url)?;
                        Ok(())
                    }
                })
                .await??;
                Ok(())
            }
            Ok(None) => {
                // Not fetchable (yet): schedule retry.
                self.reschedule(db, &job.object_url, attempt_no, "fetch failed")
                    .await
            }
            Err(e) => {
                self.reschedule(db, &job.object_url, attempt_no, &format!("{e:#}"))
                    .await
            }
        }
    }

    async fn reschedule(&self, db: &SocialDb, url: &str, attempt_no: u32, err: &str) -> Result<()> {
        let next = now_ms().saturating_add(
            next_backoff(attempt_no, self.base_backoff_secs, self.max_backoff_secs).as_millis()
                as i64,
        );
        if attempt_no >= self.max_attempts {
            tokio::task::spawn_blocking({
                let db = db.clone();
                let url = url.to_string();
                let err = err.to_string();
                move || db.mark_object_fetch_dead(&url, &err)
            })
            .await??;
            return Ok(());
        }

        tokio::task::spawn_blocking({
            let db = db.clone();
            let url = url.to_string();
            let err = err.to_string();
            move || db.try_mark_object_fetch_attempt(&url, attempt_no, next, &err)
        })
        .await??;
        Ok(())
    }
}

#[derive(Clone)]
pub struct SignedFetchConfig {
    pub private_key_pem: String,
    pub key_id: String,
}

async fn fetch_one(
    http: &reqwest::Client,
    signed: &Option<SignedFetchConfig>,
    url: &str,
) -> Result<Option<(String, Vec<u8>, bool)>> {
    let accept = "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"";

    let resp = if let Some(s) = signed {
        if let Ok(uri) = url.parse::<Uri>() {
            let mut headers = HeaderMap::new();
            headers.insert("Accept", accept.parse().expect("static header"));
            if sign_request_rsa_sha256(
                &s.private_key_pem,
                &s.key_id,
                &Method::GET,
                &uri,
                &mut headers,
                &[],
                &["(request-target)", "host", "date"],
            )
            .is_ok()
            {
                let mut req = http.get(url).header("Accept", accept);
                for (k, v) in headers.iter() {
                    req = req.header(k.as_str(), v.to_str().unwrap_or_default());
                }
                match send_with_retry(|| req.try_clone().unwrap(), 3).await {
                    Ok(r) => r,
                    Err(_) => send_with_retry(|| http.get(url).header("Accept", accept), 3).await?,
                }
            } else {
                send_with_retry(|| http.get(url).header("Accept", accept), 3).await?
            }
        } else {
            send_with_retry(|| http.get(url).header("Accept", accept), 3).await?
        }
    } else {
        send_with_retry(|| http.get(url).header("Accept", accept), 3).await?
    };
    if !resp.status().is_success() {
        return Ok(None);
    }
    let bytes = resp.bytes().await?;
    let v: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let id = v
        .get("id")
        .and_then(|vv| vv.as_str())
        .unwrap_or(url)
        .to_string();
    let ty = v.get("type").and_then(|vv| vv.as_str()).unwrap_or("");
    let is_tombstone = ty == "Tombstone";
    Ok(Some((id, bytes.to_vec(), is_tombstone)))
}

fn extract_actor_id_from_object_json(bytes: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    v.get("attributedTo")
        .and_then(|a| a.as_str())
        .or_else(|| v.get("actor").and_then(|a| a.as_str()))
        .map(|s| s.to_string())
}

fn next_backoff(attempt: u32, base_secs: u64, max_secs: u64) -> Duration {
    let pow = attempt.saturating_sub(1).min(20);
    let mut secs = base_secs.saturating_mul(1u64 << pow);
    if secs > max_secs {
        secs = max_secs;
    }
    let mut b = [0u8; 2];
    OsRng.fill_bytes(&mut b);
    let jitter_ms = u16::from_le_bytes(b) as u64 % 1000;
    Duration::from_secs(secs) + Duration::from_millis(jitter_ms)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
