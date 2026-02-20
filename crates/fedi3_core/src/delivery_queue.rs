/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use crate::delivery::Delivery;
use crate::ui_events::UiEvent;
use anyhow::{Context, Result};
use rand::{rngs::OsRng, RngCore};
use rusqlite::{params, Connection};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{broadcast, watch, Notify};
use tracing::{info, warn};

#[derive(Clone)]
pub struct DeliveryQueue {
    db_path: PathBuf,
    notify: Arc<Notify>,
}

#[derive(Clone, Copy)]
pub struct QueueSettings {
    pub max_attempts: u32,
    pub base_backoff_secs: u64,
    pub max_backoff_secs: u64,
    pub post_delivery_mode: PostDeliveryMode,
    pub p2p_relay_fallback_secs: u64,
    pub p2p_cache_ttl_secs: u64,
}

impl Default for QueueSettings {
    fn default() -> Self {
        Self {
            max_attempts: 10,
            base_backoff_secs: 5,
            max_backoff_secs: 3600,
            post_delivery_mode: PostDeliveryMode::P2pRelay,
            p2p_relay_fallback_secs: 5,
            p2p_cache_ttl_secs: 7 * 24 * 3600,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostDeliveryMode {
    P2pOnly,
    P2pRelay,
}

impl PostDeliveryMode {
    pub fn from_str(input: &str) -> Option<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "p2p_only" | "p2ponly" | "p2p-only" => Some(Self::P2pOnly),
            "p2p_relay" | "p2prelay" | "p2p-relay" | "p2p_first" | "p2p-first" => {
                Some(Self::P2pRelay)
            }
            _ => None,
        }
    }
}

impl DeliveryQueue {
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self> {
        let db_path = db_path.as_ref().to_path_buf();
        init_db(&db_path)?;
        Ok(Self {
            db_path,
            notify: Arc::new(Notify::new()),
        })
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub async fn enqueue_activity(
        &self,
        activity_json: Vec<u8>,
        targets: Vec<String>,
    ) -> Result<u64> {
        self.enqueue_activity_with_key_id(activity_json, targets, None)
            .await
    }

    pub async fn enqueue_activity_with_key_id(
        &self,
        activity_json: Vec<u8>,
        targets: Vec<String>,
        key_id: Option<String>,
    ) -> Result<u64> {
        let created_at = now_ms();
        let activity_id = activity_id_from_bytes(&activity_json).unwrap_or_default();
        let count = tokio::task::spawn_blocking({
            let db_path = self.db_path.clone();
            let activity_json = activity_json.clone();
            let activity_id = activity_id.clone();
            move || -> Result<u64> {
                let mut conn = Connection::open(db_path)?;
                let tx = conn.transaction()?;
                for t in targets {
                    let job_id = new_job_id();
                    tx.execute(
                        r#"
                        INSERT INTO delivery_jobs (
                          id, created_at_ms, next_attempt_at_ms, attempt, status, target, activity_json, key_id, activity_id, last_error
                        ) VALUES (?1, ?2, ?3, 0, 0, ?4, ?5, ?6, ?7, NULL)
                        "#,
                        params![job_id, created_at, created_at, t, activity_json, key_id, activity_id],
                    )?;
                }
                tx.commit()?;
                let pending: u64 = conn.query_row(
                    "SELECT COUNT(*) FROM delivery_jobs WHERE status = 0",
                    [],
                    |r| r.get(0),
                )?;
                Ok(pending)
            }
        })
        .await??;

        self.notify.notify_one();
        Ok(count)
    }

    pub fn start_worker(
        &self,
        shutdown: watch::Receiver<bool>,
        delivery: Arc<Delivery>,
        private_key_pem: String,
        key_id: String,
        settings: QueueSettings,
        ui_events: broadcast::Sender<UiEvent>,
    ) {
        let queue = self.clone();
        tokio::spawn(async move {
            if let Err(e) = queue
                .run_loop(
                    shutdown,
                    delivery,
                    private_key_pem,
                    key_id,
                    settings,
                    ui_events.clone(),
                )
                .await
            {
                warn!("delivery worker stopped: {e:#}");
            }
        });
    }

    async fn run_loop(
        &self,
        mut shutdown: watch::Receiver<bool>,
        delivery: Arc<Delivery>,
        private_key_pem: String,
        key_id: String,
        settings: QueueSettings,
        ui_events: broadcast::Sender<UiEvent>,
    ) -> Result<()> {
        info!("delivery queue db: {}", self.db_path.display());

        let tick = Duration::from_secs(2);
        loop {
            if *shutdown.borrow() {
                break;
            }

            let jobs = self.fetch_due_jobs(40).await?;
            if jobs.is_empty() {
                tokio::select! {
                    _ = self.notify.notified() => {}
                    _ = tokio::time::sleep(tick) => {}
                    _ = shutdown.changed() => {}
                }
                continue;
            }

            // Dedup deliveries to sharedInbox by grouping on resolved inbox URL.
            let mut actor_cache: HashMap<String, crate::delivery::ActorInfo> = HashMap::new();
            let mut groups: HashMap<(String, String, String), Vec<Job>> = HashMap::new();
            let mut passthrough = Vec::<Job>::new();

            for job in jobs {
                if *shutdown.borrow() {
                    break;
                }

                // Keep P2P-capable jobs as-is: no sharedInbox concern and may use mailbox fallback.
                if !job.target.contains("/inbox") {
                    match resolve_actor_cached(
                        &delivery,
                        &private_key_pem,
                        job.key_id.as_deref().unwrap_or(&key_id),
                        &job.target,
                        &mut actor_cache,
                    )
                    .await
                    {
                        Ok(info) if info.p2p_peer_id.is_some() => {
                            passthrough.push(job);
                            continue;
                        }
                        Ok(_) => {
                            // Actor resolved but no P2P peer, fall through to HTTP delivery
                        }
                        Err(e) => {
                            let err_str = e.to_string();
                            if err_str.contains("404")
                                || err_str.contains("410")
                                || err_str.contains("429")
                                || err_str.contains("peer deleted")
                            {
                                let _ =
                                    self.mark_dead(&job.id, "peer deleted or unreachable").await;
                                continue;
                            } else {
                                // For other errors, skip this job for now
                                continue;
                            }
                        }
                    }
                }

                let effective_key_id = job.key_id.as_deref().unwrap_or(&key_id).to_string();
                let inbox_url = if job.target.contains("/inbox") {
                    job.target.clone()
                } else {
                    match resolve_actor_cached(
                        &delivery,
                        &private_key_pem,
                        &effective_key_id,
                        &job.target,
                        &mut actor_cache,
                    )
                    .await
                    {
                        Ok(info) => info.inbox,
                        Err(e) => {
                            let err_str = e.to_string();
                            if err_str.contains("404")
                                || err_str.contains("410")
                                || err_str.contains("429")
                                || err_str.contains("peer deleted")
                            {
                                let _ =
                                    self.mark_dead(&job.id, "peer deleted or unreachable").await;
                                continue;
                            } else {
                                warn!("delivery resolve error: {e:#}");
                                continue;
                            }
                        }
                    }
                };

                let body_hash = short_body_hash(&job.activity_json);
                groups
                    .entry((effective_key_id, inbox_url, body_hash))
                    .or_default()
                    .push(job);
            }

            // Process grouped HTTP jobs first.
            for ((effective_key_id, inbox_url, _), jobs) in groups {
                if *shutdown.borrow() {
                    break;
                }
                if let Err(e) = self
                    .process_group_http(
                        &delivery,
                        &private_key_pem,
                        &effective_key_id,
                        &settings,
                        &inbox_url,
                        jobs,
                    )
                    .await
                {
                    warn!("delivery group error: {e:#}");
                }
            }

            // Then process passthrough jobs with the full P2P pipeline.
            for job in passthrough {
                if *shutdown.borrow() {
                    break;
                }
                let res = self
                    .process_one(
                        &delivery,
                        &private_key_pem,
                        &key_id,
                        &settings,
                        job,
                        &ui_events,
                    )
                    .await;
                if let Err(e) = res {
                    warn!("delivery job error: {e:#}");
                }
            }
        }
        Ok(())
    }

    async fn fetch_due_jobs(&self, limit: u32) -> Result<Vec<Job>> {
        tokio::task::spawn_blocking({
            let db_path = self.db_path.clone();
            move || -> Result<Vec<Job>> {
                let conn = Connection::open(db_path)?;
                let now = now_ms();
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, attempt, target, activity_json, key_id, activity_id
                    FROM delivery_jobs
                    WHERE status IN (0, 3) AND next_attempt_at_ms <= ?1
                    ORDER BY next_attempt_at_ms ASC
                    LIMIT ?2
                    "#,
                )?;
                let mut rows = stmt.query(params![now, limit])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    out.push(Job {
                        id: row.get(0)?,
                        attempt: row.get(1)?,
                        target: row.get(2)?,
                        activity_json: row.get(3)?,
                        key_id: row.get(4)?,
                        activity_id: row.get(5)?,
                    });
                }
                Ok(out)
            }
        })
        .await?
    }

    async fn process_one(
        &self,
        delivery: &Delivery,
        private_key_pem: &str,
        key_id: &str,
        settings: &QueueSettings,
        job: Job,
        ui_events: &broadcast::Sender<UiEvent>,
    ) -> Result<()> {
        let effective_key_id = job.key_id.as_deref().unwrap_or(key_id);

        let (inbox_url, _p2p_peer_id, _p2p_peer_addrs, _public_key_pem) =
            if job.target.contains("/inbox") {
                (job.target.clone(), None, Vec::new(), None)
            } else {
                let info = delivery
                    .resolve_actor_info_signed(private_key_pem, effective_key_id, &job.target)
                    .await;
                let info = if info.is_ok() {
                    info
                } else {
                    delivery.resolve_actor_info(&job.target).await
                };
                match info {
                    Ok(v) => (v.inbox, v.p2p_peer_id, v.p2p_peer_addrs, v.public_key_pem),
                    Err(e) => {
                        let err_str = e.to_string();
                        if err_str.contains("404")
                            || err_str.contains("410")
                            || err_str.contains("429")
                            || err_str.contains("peer deleted")
                        {
                            self.mark_dead(&job.id, "peer deleted or unreachable")
                                .await?;
                            return Ok(());
                        } else {
                            return Err(e);
                        }
                    }
                }
        };

        let attempt_no: u32 = job.attempt.saturating_add(1);

        self.deliver_via_relay(
            &job,
            private_key_pem,
            effective_key_id,
            &inbox_url,
            &job.activity_json,
            attempt_no,
            None,
            delivery,
            settings,
            &ui_events,
        )
        .await?;
        Ok(())
    }

    async fn deliver_via_relay(
        &self,
        job: &Job,
        private_key_pem: &str,
        key_id: &str,
        inbox_url: &str,
        body: &[u8],
        attempt_no: u32,
        relay_reason: Option<String>,
        delivery: &Delivery,
        settings: &QueueSettings,
        ui_events: &broadcast::Sender<UiEvent>,
    ) -> Result<()> {
        if let Some(reason) = relay_reason {
            let summary = reason
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .replace('\n', " ");
            let label = if summary.is_empty() {
                "relay_fallback".to_string()
            } else {
                summary.chars().take(140).collect()
            };
            let detail = format!("relay_fallback: {label}");
            let _ = ui_events.send(UiEvent::new(
                "delivery",
                Some(detail),
                job.activity_id.clone(),
            ));
            tracing::info!(
                "relay_fallback job_id={} target={} activity_id={:?} reason={}",
                job.id,
                job.target,
                job.activity_id,
                label
            );
        }

        match delivery
            .deliver_json(private_key_pem, key_id, inbox_url, body)
            .await
        {
            Ok(()) => {
                self.mark_delivered(&job.id).await?;
            }
            Err(e) => {
                if attempt_no >= settings.max_attempts {
                    self.mark_dead(&job.id, &format!("{e:#}")).await?;
                    return Ok(());
                }
                let delay = next_backoff(
                    attempt_no,
                    settings.base_backoff_secs,
                    settings.max_backoff_secs,
                );
                self.reschedule(&job.id, attempt_no, delay, &format!("{e:#}"))
                    .await?;
            }
        }
        Ok(())
    }

    async fn mark_delivered(&self, id: &str) -> Result<()> {
        tokio::task::spawn_blocking({
            let db_path = self.db_path.clone();
            let id = id.to_string();
            move || -> Result<()> {
                let conn = Connection::open(db_path)?;
                conn.execute(
                    "UPDATE delivery_jobs SET status = 1, last_error = NULL WHERE id = ?1",
                    params![id],
                )?;
                Ok(())
            }
        })
        .await??;
        Ok(())
    }

    async fn mark_dead(&self, id: &str, err: &str) -> Result<()> {
        tokio::task::spawn_blocking({
            let db_path = self.db_path.clone();
            let id = id.to_string();
            let err = err.to_string();
            move || -> Result<()> {
                let conn = Connection::open(db_path)?;
                conn.execute(
                    "UPDATE delivery_jobs SET status = 2, last_error = ?2 WHERE id = ?1",
                    params![id, err],
                )?;
                Ok(())
            }
        })
        .await??;
        Ok(())
    }

    async fn reschedule(&self, id: &str, attempt: u32, delay: Duration, err: &str) -> Result<()> {
        let next = now_ms().saturating_add(delay.as_millis() as i64);
        tokio::task::spawn_blocking({
            let db_path = self.db_path.clone();
            let id = id.to_string();
            let err = err.to_string();
            move || -> Result<()> {
                let conn = Connection::open(db_path)?;
                conn.execute(
                    "UPDATE delivery_jobs SET attempt = ?2, next_attempt_at_ms = ?3, last_error = ?4 WHERE id = ?1",
                    params![id, attempt, next, err],
                )?;
                Ok(())
            }
        })
        .await??;
        Ok(())
    }

    pub async fn mark_delivered_by_receipt(
        &self,
        activity_id: &str,
        from_actor: &str,
    ) -> Result<u64> {
        let activity_id = activity_id.trim().to_string();
        let from_actor = from_actor.trim().to_string();
        if activity_id.is_empty() || from_actor.is_empty() {
            return Ok(0);
        }

        let from_inbox = format!("{from_actor}/inbox");
        let updated = tokio::task::spawn_blocking({
            let db_path = self.db_path.clone();
            move || -> Result<u64> {
                let conn = Connection::open(db_path)?;
                let changed = conn.execute(
                    "UPDATE delivery_jobs SET status = 1, last_error = NULL WHERE status = 3 AND activity_id = ?1 AND (target = ?2 OR target = ?3)",
                    params![activity_id, from_actor, from_inbox],
                )?;
                Ok(changed as u64)
            }
        })
        .await??;
        Ok(updated)
    }

    pub async fn stats(&self) -> Result<QueueStats> {
        tokio::task::spawn_blocking({
            let db_path = self.db_path.clone();
            move || -> Result<QueueStats> {
                let conn = Connection::open(db_path)?;
                let pending: u64 = conn.query_row(
                    "SELECT COUNT(*) FROM delivery_jobs WHERE status = 0",
                    [],
                    |r| r.get(0),
                )?;
                let delivered: u64 = conn.query_row(
                    "SELECT COUNT(*) FROM delivery_jobs WHERE status = 1",
                    [],
                    |r| r.get(0),
                )?;
                let dead: u64 = conn.query_row(
                    "SELECT COUNT(*) FROM delivery_jobs WHERE status = 2",
                    [],
                    |r| r.get(0),
                )?;
                Ok(QueueStats {
                    pending,
                    delivered,
                    dead,
                })
            }
        })
        .await?
    }

    async fn process_group_http(
        &self,
        delivery: &Delivery,
        private_key_pem: &str,
        effective_key_id: &str,
        settings: &QueueSettings,
        inbox_url: &str,
        jobs: Vec<Job>,
    ) -> Result<()> {
        if jobs.is_empty() {
            return Ok(());
        }
        let body = jobs[0].activity_json.clone();
        match delivery
            .deliver_json(private_key_pem, effective_key_id, inbox_url, &body)
            .await
        {
            Ok(()) => {
                for j in jobs {
                    let _ = self.mark_delivered(&j.id).await;
                }
                Ok(())
            }
            Err(e) => {
                for j in jobs {
                    let attempt_no: u32 = j.attempt.saturating_add(1);
                    if attempt_no >= settings.max_attempts {
                        let _ = self.mark_dead(&j.id, &format!("{e:#}")).await;
                        continue;
                    }
                    let delay = next_backoff(
                        attempt_no,
                        settings.base_backoff_secs,
                        settings.max_backoff_secs,
                    );
                    let _ = self
                        .reschedule(&j.id, attempt_no, delay, &format!("{e:#}"))
                        .await;
                }
                Ok(())
            }
        }
    }
}

async fn resolve_actor_cached(
    delivery: &Delivery,
    private_key_pem: &str,
    key_id: &str,
    actor_url: &str,
    cache: &mut HashMap<String, crate::delivery::ActorInfo>,
) -> Result<crate::delivery::ActorInfo> {
    if let Some(v) = cache.get(actor_url).cloned() {
        return Ok(v);
    }
    let info = match delivery
        .resolve_actor_info_signed(private_key_pem, key_id, actor_url)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("404")
                || err_str.contains("410")
                || err_str.contains("429")
                || err_str.contains("peer deleted")
            {
                return Err(e);
            } else {
                delivery.resolve_actor_info(actor_url).await?
            }
        }
    };
    cache.insert(actor_url.to_string(), info.clone());
    Ok(info)
}

fn short_body_hash(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    let mut h = sha2::Sha256::new();
    h.update(bytes);
    hex::encode(&h.finalize()[..8])
}

#[derive(Debug, Clone)]
struct Job {
    id: String,
    attempt: u32,
    target: String,
    activity_json: Vec<u8>,
    key_id: Option<String>,
    #[allow(dead_code)]
    activity_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct QueueStats {
    pub pending: u64,
    pub delivered: u64,
    pub dead: u64,
}

fn init_db(path: &Path) -> Result<()> {
    let conn = Connection::open(path).with_context(|| format!("open db: {}", path.display()))?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS delivery_jobs (
          id TEXT PRIMARY KEY,
          created_at_ms INTEGER NOT NULL,
          next_attempt_at_ms INTEGER NOT NULL,
          attempt INTEGER NOT NULL,
          status INTEGER NOT NULL,
          target TEXT NOT NULL,
          activity_json BLOB NOT NULL,
          key_id TEXT NULL,
          activity_id TEXT NULL,
          last_error TEXT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_delivery_due ON delivery_jobs(status, next_attempt_at_ms);
        "#,
    )?;
    // Migrate existing dbs.
    let _ = conn.execute("ALTER TABLE delivery_jobs ADD COLUMN key_id TEXT NULL", []);
    let _ = conn.execute(
        "ALTER TABLE delivery_jobs ADD COLUMN activity_id TEXT NULL",
        [],
    );
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn new_job_id() -> String {
    // 16 random bytes -> 32 hex chars
    let mut b = [0u8; 16];
    OsRng.fill_bytes(&mut b);
    b.iter().map(|v| format!("{v:02x}")).collect()
}

fn next_backoff(attempt: u32, base_secs: u64, max_secs: u64) -> Duration {
    let pow = attempt.saturating_sub(1).min(20);
    let mut secs = base_secs.saturating_mul(1u64 << pow);
    if secs > max_secs {
        secs = max_secs;
    }
    // jitter 0..1000ms
    let mut b = [0u8; 2];
    OsRng.fill_bytes(&mut b);
    let jitter_ms = u16::from_le_bytes(b) as u64 % 1000;
    Duration::from_secs(secs) + Duration::from_millis(jitter_ms)
}

pub(crate) fn activity_id_from_bytes(bytes: &[u8]) -> Option<String> {
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) {
        if let Some(id) = v.get("id").and_then(|v| v.as_str()) {
            let id = id.trim();
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
    }
    if bytes.is_empty() {
        return None;
    }
    use sha2::Digest as _;
    let mut h = sha2::Sha256::new();
    h.update(bytes);
    Some(format!(
        "urn:fedi3:activity:sha256:{}",
        hex::encode(h.finalize())
    ))
}
