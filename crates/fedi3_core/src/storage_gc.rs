/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use crate::social_db::SocialDb;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::watch;
use tracing::{info, warn};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct StorageConfig {
    pub gc_interval_secs: Option<u64>,

    pub inbox_max_items: Option<u32>,
    pub inbox_seen_ttl_days: Option<u32>,

    pub global_feed_max_items: Option<u32>,
    pub global_feed_ttl_days: Option<u32>,
    pub global_feed_prune_batch: Option<u32>,

    pub federated_feed_max_items: Option<u32>,
    pub federated_feed_ttl_days: Option<u32>,
    pub federated_feed_prune_batch: Option<u32>,

    /// Se true: mantieni cache "lunga" solo per attori Fedi3 seguiti (following accepted),
    /// tutto il resto ha TTL aggressivo.
    pub cache_only_followed_fedi3: Option<bool>,
    pub followed_fedi3_object_ttl_days: Option<u32>,
    pub other_object_ttl_days: Option<u32>,
    pub object_prune_batch: Option<u32>,
    pub followed_fedi3_max_objects_per_actor: Option<u32>,
    pub other_max_objects_per_actor: Option<u32>,
    pub max_objects_per_actor_batch: Option<u32>,
    pub followed_fedi3_max_object_bytes_per_actor: Option<u64>,
    pub other_max_object_bytes_per_actor: Option<u64>,
    pub max_object_bytes_actors_per_run: Option<u32>,
    pub max_object_bytes_deletes_per_actor: Option<u32>,

    /// Limite massimo per la cache media locale (directory `media/`).
    pub media_max_local_cache_bytes: Option<u64>,
    pub followed_fedi3_max_media_items_per_actor: Option<u32>,
    pub other_max_media_items_per_actor: Option<u32>,
    pub max_media_items_per_actor_batch: Option<u32>,
    pub followed_fedi3_max_media_bytes_per_actor: Option<u64>,
    pub other_max_media_bytes_per_actor: Option<u64>,
    pub max_media_bytes_actors_per_run: Option<u32>,
    pub max_media_bytes_deletes_per_actor: Option<u32>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            gc_interval_secs: Some(300),
            inbox_max_items: Some(2000),
            inbox_seen_ttl_days: Some(30),
            global_feed_max_items: Some(20000),
            global_feed_ttl_days: Some(120),
            global_feed_prune_batch: Some(1000),
            federated_feed_max_items: Some(20000),
            federated_feed_ttl_days: Some(180),
            federated_feed_prune_batch: Some(1000),
            cache_only_followed_fedi3: Some(false),
            followed_fedi3_object_ttl_days: Some(365),
            other_object_ttl_days: Some(90),
            object_prune_batch: Some(1000),
            media_max_local_cache_bytes: Some(50 * 1024 * 1024 * 1024),
            followed_fedi3_max_objects_per_actor: Some(5000),
            other_max_objects_per_actor: Some(1000),
            max_objects_per_actor_batch: Some(3000),
            followed_fedi3_max_media_items_per_actor: Some(500),
            other_max_media_items_per_actor: Some(50),
            max_media_items_per_actor_batch: Some(500),
            followed_fedi3_max_object_bytes_per_actor: Some(50 * 1024 * 1024),
            other_max_object_bytes_per_actor: Some(5 * 1024 * 1024),
            max_object_bytes_actors_per_run: Some(25),
            max_object_bytes_deletes_per_actor: Some(500),
            followed_fedi3_max_media_bytes_per_actor: Some(2 * 1024 * 1024 * 1024),
            other_max_media_bytes_per_actor: Some(200 * 1024 * 1024),
            max_media_bytes_actors_per_run: Some(25),
            max_media_bytes_deletes_per_actor: Some(200),
        }
    }
}

pub fn start_storage_gc_worker(
    cfg: StorageConfig,
    social: std::sync::Arc<SocialDb>,
    data_dir: PathBuf,
    mut shutdown: watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        let interval = cfg.gc_interval_secs.unwrap_or(300).max(30);
        let mut tick = tokio::time::interval(Duration::from_secs(interval));
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

            if let Err(e) = run_once(&cfg, &social, &data_dir).await {
                warn!("storage gc error: {e:#}");
            }
        }
    });
}

async fn run_once(cfg: &StorageConfig, social: &SocialDb, data_dir: &Path) -> Result<()> {
    let now = now_ms();

    if let Some(max) = cfg.inbox_max_items {
        let deleted = tokio::task::spawn_blocking({
            let s = social.clone();
            move || s.prune_inbox_items(max)
        })
        .await??;
        if deleted > 0 {
            info!(deleted, "gc pruned inbox_items");
        }
    }

    if let Some(days) = cfg.inbox_seen_ttl_days {
        let cutoff = now.saturating_sub((days as i64).saturating_mul(24 * 3600 * 1000));
        let deleted = tokio::task::spawn_blocking({
            let s = social.clone();
            move || s.prune_inbox_seen_before(cutoff)
        })
        .await??;
        if deleted > 0 {
            info!(deleted, "gc pruned inbox_seen");
        }
    }

    // Always prune persistent anti-abuse windows and audit logs to avoid unbounded growth.
    {
        let cutoff = now.saturating_sub(3_i64.saturating_mul(24 * 3600 * 1000));
        let deleted = tokio::task::spawn_blocking({
            let s = social.clone();
            move || s.prune_inbox_quota_before(cutoff)
        })
        .await??;
        if deleted > 0 {
            info!(deleted, "gc pruned inbox_quota");
        }
    }
    {
        let cutoff = now.saturating_sub(30_i64.saturating_mul(24 * 3600 * 1000));
        let deleted = tokio::task::spawn_blocking({
            let s = social.clone();
            move || s.prune_audit_events_before(cutoff)
        })
        .await??;
        if deleted > 0 {
            info!(deleted, "gc pruned audit_events");
        }
    }

    if let Some(max) = cfg.global_feed_max_items {
        let deleted = tokio::task::spawn_blocking({
            let s = social.clone();
            move || s.prune_global_feed_to_max(max)
        })
        .await??;
        if deleted > 0 {
            info!(deleted, "gc pruned global_feed max");
        }
    }
    if let Some(days) = cfg.global_feed_ttl_days {
        let cutoff = now.saturating_sub((days as i64).saturating_mul(24 * 3600 * 1000));
        let batch = cfg.global_feed_prune_batch.unwrap_or(1000).max(100);
        loop {
            let deleted = tokio::task::spawn_blocking({
                let s = social.clone();
                move || s.prune_global_feed_before(cutoff, batch)
            })
            .await??;
            if deleted == 0 {
                break;
            }
            info!(deleted, "gc pruned global_feed ttl");
            if deleted < batch as u64 {
                break;
            }
        }
    }

    if let Some(max) = cfg.federated_feed_max_items {
        let deleted = tokio::task::spawn_blocking({
            let s = social.clone();
            move || s.prune_federated_feed_to_max(max)
        })
        .await??;
        if deleted > 0 {
            info!(deleted, "gc pruned federated_feed max");
        }
    }
    if let Some(days) = cfg.federated_feed_ttl_days {
        let cutoff = now.saturating_sub((days as i64).saturating_mul(24 * 3600 * 1000));
        let batch = cfg.federated_feed_prune_batch.unwrap_or(1000).max(100);
        loop {
            let deleted = tokio::task::spawn_blocking({
                let s = social.clone();
                move || s.prune_federated_feed_before(cutoff, batch)
            })
            .await??;
            if deleted == 0 {
                break;
            }
            info!(deleted, "gc pruned federated_feed ttl");
            if deleted < batch as u64 {
                break;
            }
        }
    }

    let batch = cfg.object_prune_batch.unwrap_or(1000).max(100);
    if cfg.cache_only_followed_fedi3.unwrap_or(false) {
        // Per-actor quotas first (to cap growth deterministically).
        let obj_batch = cfg.max_objects_per_actor_batch.unwrap_or(2000).max(100);
        let obj_bytes_actors = cfg.max_object_bytes_actors_per_run.unwrap_or(25).max(1);
        let obj_bytes_deletes = cfg
            .max_object_bytes_deletes_per_actor
            .unwrap_or(500)
            .max(10);

        if let Some(max_bytes) = cfg.other_max_object_bytes_per_actor {
            let deleted = tokio::task::spawn_blocking({
                let s = social.clone();
                move || {
                    s.prune_object_bytes_per_actor_other(
                        max_bytes,
                        obj_bytes_actors,
                        obj_bytes_deletes,
                    )
                }
            })
            .await??;
            if deleted > 0 {
                info!(deleted, "gc pruned objects per-actor bytes (other)");
            }
        }
        if let Some(max_bytes) = cfg.followed_fedi3_max_object_bytes_per_actor {
            let deleted = tokio::task::spawn_blocking({
                let s = social.clone();
                move || {
                    s.prune_object_bytes_per_actor_followed_fedi3(
                        max_bytes,
                        obj_bytes_actors,
                        obj_bytes_deletes,
                    )
                }
            })
            .await??;
            if deleted > 0 {
                info!(
                    deleted,
                    "gc pruned objects per-actor bytes (followed fedi3)"
                );
            }
        }

        if let Some(max) = cfg.other_max_objects_per_actor {
            loop {
                let deleted = tokio::task::spawn_blocking({
                    let s = social.clone();
                    move || s.prune_objects_per_actor_other(max, obj_batch)
                })
                .await??;
                if deleted == 0 {
                    break;
                }
                info!(deleted, "gc pruned objects per-actor (other)");
                if deleted < obj_batch as u64 {
                    break;
                }
            }
        }
        if let Some(max) = cfg.followed_fedi3_max_objects_per_actor {
            loop {
                let deleted = tokio::task::spawn_blocking({
                    let s = social.clone();
                    move || s.prune_objects_per_actor_followed_fedi3(max, obj_batch)
                })
                .await??;
                if deleted == 0 {
                    break;
                }
                info!(deleted, "gc pruned objects per-actor (followed fedi3)");
                if deleted < obj_batch as u64 {
                    break;
                }
            }
        }

        if let Some(days) = cfg.other_object_ttl_days {
            let cutoff = now.saturating_sub((days as i64).saturating_mul(24 * 3600 * 1000));
            loop {
                let deleted = tokio::task::spawn_blocking({
                    let s = social.clone();
                    move || s.prune_objects_other_before(cutoff, batch)
                })
                .await??;
                if deleted == 0 {
                    break;
                }
                info!(deleted, "gc pruned objects (other)");
                if deleted < batch as u64 {
                    break;
                }
            }
        }
        if let Some(days) = cfg.followed_fedi3_object_ttl_days {
            let cutoff = now.saturating_sub((days as i64).saturating_mul(24 * 3600 * 1000));
            loop {
                let deleted = tokio::task::spawn_blocking({
                    let s = social.clone();
                    move || s.prune_objects_followed_fedi3_before(cutoff, batch)
                })
                .await??;
                if deleted == 0 {
                    break;
                }
                info!(deleted, "gc pruned objects (followed fedi3)");
                if deleted < batch as u64 {
                    break;
                }
            }
        }
    } else if let Some(days) = cfg.followed_fedi3_object_ttl_days {
        let cutoff = now.saturating_sub((days as i64).saturating_mul(24 * 3600 * 1000));
        loop {
            let deleted = tokio::task::spawn_blocking({
                let s = social.clone();
                move || s.prune_objects_before(cutoff, batch)
            })
            .await??;
            if deleted == 0 {
                break;
            }
            info!(deleted, "gc pruned objects");
            if deleted < batch as u64 {
                break;
            }
        }
    }

    // Per-actor media quotas (DB + files).
    let media_batch = cfg.max_media_items_per_actor_batch.unwrap_or(500).max(50);
    let media_bytes_actors = cfg.max_media_bytes_actors_per_run.unwrap_or(25).max(1);
    let media_bytes_deletes = cfg.max_media_bytes_deletes_per_actor.unwrap_or(200).max(10);

    if let Some(max_bytes) = cfg.other_max_media_bytes_per_actor {
        let names = tokio::task::spawn_blocking({
            let s = social.clone();
            move || {
                s.prune_media_bytes_per_actor_other(
                    max_bytes,
                    media_bytes_actors,
                    media_bytes_deletes,
                )
            }
        })
        .await??;
        if !names.is_empty() {
            delete_local_media_files(&data_dir.join("media"), &names)?;
            info!(
                deleted = names.len(),
                "gc pruned media per-actor bytes (other)"
            );
        }
    }
    if let Some(max_bytes) = cfg.followed_fedi3_max_media_bytes_per_actor {
        let names = tokio::task::spawn_blocking({
            let s = social.clone();
            move || {
                s.prune_media_bytes_per_actor_followed_fedi3(
                    max_bytes,
                    media_bytes_actors,
                    media_bytes_deletes,
                )
            }
        })
        .await??;
        if !names.is_empty() {
            delete_local_media_files(&data_dir.join("media"), &names)?;
            info!(
                deleted = names.len(),
                "gc pruned media per-actor bytes (followed fedi3)"
            );
        }
    }

    if cfg.cache_only_followed_fedi3.unwrap_or(false) {
        if let Some(max) = cfg.other_max_media_items_per_actor {
            loop {
                let local_names = tokio::task::spawn_blocking({
                    let s = social.clone();
                    move || s.prune_media_per_actor_other(max, media_batch)
                })
                .await??;
                if local_names.is_empty() {
                    break;
                }
                delete_local_media_files(&data_dir.join("media"), &local_names)?;
                info!(
                    deleted = local_names.len(),
                    "gc pruned media per-actor (other)"
                );
                if local_names.len() < media_batch as usize {
                    break;
                }
            }
        }
        if let Some(max) = cfg.followed_fedi3_max_media_items_per_actor {
            loop {
                let local_names = tokio::task::spawn_blocking({
                    let s = social.clone();
                    move || s.prune_media_per_actor_followed_fedi3(max, media_batch)
                })
                .await??;
                if local_names.is_empty() {
                    break;
                }
                delete_local_media_files(&data_dir.join("media"), &local_names)?;
                info!(
                    deleted = local_names.len(),
                    "gc pruned media per-actor (followed fedi3)"
                );
                if local_names.len() < media_batch as usize {
                    break;
                }
            }
        }
    }

    if let Some(max_bytes) = cfg.media_max_local_cache_bytes {
        prune_media_dir(&data_dir.join("media"), max_bytes, social).await?;
    }

    Ok(())
}

fn delete_local_media_files(dir: &Path, names: &[String]) -> Result<()> {
    for n in names {
        if n.contains("..") || n.contains('/') || n.contains('\\') {
            continue;
        }
        let path = dir.join(n);
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

async fn prune_media_dir(dir: &Path, max_bytes: u64, social: &SocialDb) -> Result<()> {
    if max_bytes == 0 {
        return Ok(());
    }
    if !dir.exists() {
        return Ok(());
    }

    let dir = dir.to_path_buf();
    let social = social.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut entries: Vec<(PathBuf, u64, std::time::SystemTime)> = Vec::new();
        let mut total: u64 = 0;

        for ent in std::fs::read_dir(&dir).context("read media dir")? {
            let ent = ent?;
            let path = ent.path();
            if !path.is_file() {
                continue;
            }
            let meta = ent.metadata()?;
            let len = meta.len();
            let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            total = total.saturating_add(len);
            entries.push((path, len, mtime));
        }

        if total <= max_bytes {
            return Ok(());
        }

        entries.sort_by_key(|(_, _, t)| *t);
        for (path, len, _) in entries {
            if total <= max_bytes {
                break;
            }
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let _ = std::fs::remove_file(&path);
            total = total.saturating_sub(len);
            if !name.is_empty() {
                let _ = social.delete_media(&name);
            }
        }
        Ok(())
    })
    .await??;

    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
