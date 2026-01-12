/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use anyhow::{Context, Result};
use rand::{rngs::OsRng, RngCore};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct SocialDb {
    path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct MediaItem {
    pub id: String,
    pub url: String,
    pub media_type: String,
    pub size: i64,
    pub created_at_ms: i64,
    pub local_name: Option<String>,
    pub actor_id: Option<String>,
    pub last_access_ms: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub blurhash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ActorMeta {
    pub actor_id: String,
    pub is_fedi3: bool,
    pub last_seen_ms: i64,
}

#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub id: i64,
    pub kind: String,
    pub created_at_ms: i64,
    pub actor_id: Option<String>,
    pub key_id: Option<String>,
    pub activity_id: Option<String>,
    pub ok: bool,
    pub status: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GlobalFeedItem {
    pub activity_id: String,
    pub created_at_ms: i64,
    pub actor_id: Option<String>,
    pub activity_json: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ObjectRow {
    pub object_id: String,
    pub created_at_ms: i64,
    pub actor_id: Option<String>,
    pub object_json: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct CollectionPage<T> {
    pub total: u64,
    pub items: Vec<T>,
    pub next: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatThread {
    pub thread_id: String,
    pub kind: String,
    pub title: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub last_message_ms: Option<i64>,
    pub last_message_preview: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RelayEntry {
    pub relay_base_url: String,
    pub relay_ws_url: Option<String>,
    pub last_seen_ms: i64,
    pub source: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatMessage {
    pub message_id: String,
    pub thread_id: String,
    pub sender_actor: String,
    pub sender_device: String,
    pub created_at_ms: i64,
    pub edited_at_ms: Option<i64>,
    pub deleted: bool,
    pub body_json: String,
}

impl SocialDb {
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self> {
        let path = db_path.as_ref().to_path_buf();
        let conn = Connection::open(&path).with_context(|| format!("open db: {}", path.display()))?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS inbox_seen (
              activity_id TEXT PRIMARY KEY,
              seen_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS inbox_items (
              activity_id TEXT PRIMARY KEY,
              created_at_ms INTEGER NOT NULL,
              actor_id TEXT NULL,
              type TEXT NULL,
              activity_json BLOB NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_inbox_created ON inbox_items(created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS objects (
              object_id TEXT PRIMARY KEY,
              created_at_ms INTEGER NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              deleted INTEGER NOT NULL,
              object_json BLOB NOT NULL
            );

            CREATE TABLE IF NOT EXISTS object_attachments (
              object_id TEXT NOT NULL,
              url TEXT NOT NULL,
              media_type TEXT NULL,
              name TEXT NULL,
              blurhash TEXT NULL,
              width INTEGER NULL,
              height INTEGER NULL,
              PRIMARY KEY(object_id, url)
            );
            CREATE INDEX IF NOT EXISTS idx_attach_object ON object_attachments(object_id);

            CREATE TABLE IF NOT EXISTS object_meta (
              object_id TEXT PRIMARY KEY,
              sensitive INTEGER NULL,
              summary TEXT NULL
            );

            CREATE TABLE IF NOT EXISTS object_tags (
              object_id TEXT NOT NULL,
              tag_type TEXT NOT NULL,
              name TEXT NOT NULL DEFAULT '',
              href TEXT NOT NULL DEFAULT '',
              PRIMARY KEY(object_id, tag_type, name, href)
            );
            CREATE INDEX IF NOT EXISTS idx_tags_object ON object_tags(object_id);

            CREATE TABLE IF NOT EXISTS object_fetch_jobs (
              object_url TEXT PRIMARY KEY,
              created_at_ms INTEGER NOT NULL,
              next_attempt_at_ms INTEGER NOT NULL,
              attempt INTEGER NOT NULL,
              status INTEGER NOT NULL,
              last_error TEXT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_fetch_due ON object_fetch_jobs(status, next_attempt_at_ms);

            CREATE TABLE IF NOT EXISTS reactions (
              reaction_id TEXT PRIMARY KEY,
              created_at_ms INTEGER NOT NULL,
              type TEXT NOT NULL,
              actor_id TEXT NOT NULL,
              object_id TEXT NOT NULL,
              content TEXT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_reactions_object ON reactions(object_id);

            -- Index of replies: which activities are replies to which Note id (inReplyTo).
            -- activity_id refers to either inbox_items.activity_id (dedup id) or outbox_items.id.
            CREATE TABLE IF NOT EXISTS note_replies (
              note_id TEXT NOT NULL,
              activity_id TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL,
              PRIMARY KEY(note_id, activity_id)
            );
            CREATE INDEX IF NOT EXISTS idx_note_replies_note_created ON note_replies(note_id, created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS followers (
              actor_id TEXT PRIMARY KEY,
              created_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS following (
              actor_id TEXT PRIMARY KEY,
              status INTEGER NOT NULL,
              created_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS outbox_items (
              id TEXT PRIMARY KEY,
              created_at_ms INTEGER NOT NULL,
              activity_json BLOB NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_outbox_created ON outbox_items(created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS media_items (
              id TEXT PRIMARY KEY,
              url TEXT NOT NULL,
              media_type TEXT NOT NULL,
              size INTEGER NOT NULL,
              created_at_ms INTEGER NOT NULL,
              local_name TEXT NULL,
              actor_id TEXT NULL
            );

            CREATE TABLE IF NOT EXISTS actor_meta (
              actor_id TEXT PRIMARY KEY,
              is_fedi3 INTEGER NOT NULL,
              last_seen_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS global_feed (
              activity_id TEXT PRIMARY KEY,
              created_at_ms INTEGER NOT NULL,
              actor_id TEXT NULL,
              size_bytes INTEGER NOT NULL,
              last_access_ms INTEGER NOT NULL,
              activity_json BLOB NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_global_feed_created ON global_feed(created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS federated_feed (
              activity_id TEXT PRIMARY KEY,
              created_at_ms INTEGER NOT NULL,
              actor_id TEXT NULL,
              size_bytes INTEGER NOT NULL,
              last_access_ms INTEGER NOT NULL,
              activity_json BLOB NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_federated_feed_created ON federated_feed(created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS p2p_sync_state (
              actor_id TEXT PRIMARY KEY,
              since_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS chat_threads (
              thread_id TEXT PRIMARY KEY,
              kind TEXT NOT NULL,
              title TEXT NULL,
              created_at_ms INTEGER NOT NULL,
              updated_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS chat_members (
              thread_id TEXT NOT NULL,
              actor_id TEXT NOT NULL,
              role TEXT NOT NULL,
              added_at_ms INTEGER NOT NULL,
              PRIMARY KEY(thread_id, actor_id)
            );
            CREATE INDEX IF NOT EXISTS idx_chat_members_actor ON chat_members(actor_id);
            CREATE TABLE IF NOT EXISTS chat_messages (
              message_id TEXT PRIMARY KEY,
              thread_id TEXT NOT NULL,
              sender_actor TEXT NOT NULL,
              sender_device TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL,
              edited_at_ms INTEGER NULL,
              deleted INTEGER NOT NULL DEFAULT 0,
              body_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_chat_messages_thread_created ON chat_messages(thread_id, created_at_ms DESC);
            CREATE TABLE IF NOT EXISTS chat_message_status (
              message_id TEXT NOT NULL,
              actor_id TEXT NOT NULL,
              status TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              PRIMARY KEY(message_id, actor_id)
            );
            CREATE TABLE IF NOT EXISTS chat_message_reactions (
              message_id TEXT NOT NULL,
              actor_id TEXT NOT NULL,
              reaction TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL,
              PRIMARY KEY(message_id, actor_id, reaction)
            );
            CREATE INDEX IF NOT EXISTS idx_chat_reactions_message ON chat_message_reactions(message_id);
            CREATE TABLE IF NOT EXISTS relay_registry (
              relay_base_url TEXT PRIMARY KEY,
              relay_ws_url TEXT NULL,
              last_seen_ms INTEGER NOT NULL,
              source TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_relay_registry_seen ON relay_registry(last_seen_ms DESC);
            CREATE TABLE IF NOT EXISTS chat_identity_keys (
              key_id TEXT PRIMARY KEY,
              public_b64 TEXT NOT NULL,
              secret_b64 TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS chat_prekeys (
              key_id TEXT PRIMARY KEY,
              public_b64 TEXT NOT NULL,
              secret_b64 TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL,
              used_at_ms INTEGER NULL
            );

            CREATE TABLE IF NOT EXISTS inbox_follows (
              activity_id TEXT PRIMARY KEY,
              actor_id TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS local_meta (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS blocked_actors (
              actor_id TEXT PRIMARY KEY,
              created_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS audit_events (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              kind TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL,
              actor_id TEXT NULL,
              key_id TEXT NULL,
              activity_id TEXT NULL,
              ok INTEGER NOT NULL,
              status TEXT NULL,
              detail TEXT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_events(created_at_ms DESC);

            -- Persistent quota windows for inbound inbox (anti-abuse).
            -- Key format is opaque to DB (e.g. "day:actor:<hash>").
            CREATE TABLE IF NOT EXISTS inbox_quota (
              quota_key TEXT PRIMARY KEY,
              window_start_ms INTEGER NOT NULL,
              reqs INTEGER NOT NULL,
              bytes INTEGER NOT NULL,
              updated_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_inbox_quota_updated ON inbox_quota(updated_at_ms DESC);

            -- Anti-abuse strikes + temporary blocks (best-effort).
            CREATE TABLE IF NOT EXISTS abuse_strikes (
              abuse_key TEXT PRIMARY KEY,
              strikes INTEGER NOT NULL,
              last_strike_ms INTEGER NOT NULL,
              block_until_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_abuse_block_until ON abuse_strikes(block_until_ms);

            "#,
        )?;
        ensure_columns(&conn, "object_attachments", &[
            ("blurhash", "TEXT"),
            ("width", "INTEGER"),
            ("height", "INTEGER"),
        ])?;
        ensure_columns(&conn, "media_items", &[
            ("last_access_ms", "INTEGER NOT NULL DEFAULT 0"),
            ("width", "INTEGER NULL"),
            ("height", "INTEGER NULL"),
            ("blurhash", "TEXT NULL"),
        ])?;
        ensure_columns(&conn, "objects", &[
            ("pinned", "INTEGER NOT NULL DEFAULT 0"),
            ("actor_id", "TEXT NULL"),
            ("size_bytes", "INTEGER NOT NULL DEFAULT 0"),
            ("last_access_ms", "INTEGER NOT NULL DEFAULT 0"),
        ])?;
        ensure_columns(&conn, "reactions", &[
            ("content", "TEXT NULL"),
        ])?;
        Ok(Self { path })
    }

    pub fn health_check(&self) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.query_row("SELECT 1", [], |_| Ok(()))?;
        Ok(())
    }

    pub fn abuse_check_blocked(&self, abuse_key: &str) -> Result<Option<i64>> {
        let abuse_key = abuse_key.trim();
        if abuse_key.is_empty() {
            return Ok(None);
        }
        let now = now_ms();
        let conn = Connection::open(&self.path)?;
        let until: Option<i64> = conn
            .query_row(
                "SELECT block_until_ms FROM abuse_strikes WHERE abuse_key=?1",
                params![abuse_key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(until.filter(|v| *v > now))
    }

    /// Records a strike and returns the new `block_until_ms` if the key is blocked.
    pub fn abuse_record_strike(&self, abuse_key: &str, weight: u32) -> Result<Option<i64>> {
        let abuse_key = abuse_key.trim();
        if abuse_key.is_empty() || weight == 0 {
            return Ok(None);
        }
        let now = now_ms();
        let weight = weight.min(10) as i64;

        // Policy: decay after 24h; block for 1h at >=10 strikes.
        const DECAY_MS: i64 = 24 * 3600 * 1000;
        const BLOCK_THRESHOLD: i64 = 10;
        const BLOCK_MS: i64 = 3600 * 1000;

        let mut conn = Connection::open(&self.path)?;
        let tx = conn.transaction()?;

        let row: Option<(i64, i64, i64)> = tx
            .query_row(
                "SELECT strikes, last_strike_ms, block_until_ms FROM abuse_strikes WHERE abuse_key=?1",
                params![abuse_key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;

        let (mut strikes, last_strike_ms, mut block_until_ms) = row.unwrap_or((0, 0, 0));
        if now.saturating_sub(last_strike_ms) > DECAY_MS {
            strikes = 0;
        }

        strikes = strikes.saturating_add(weight);
        if strikes >= BLOCK_THRESHOLD {
            block_until_ms = now.saturating_add(BLOCK_MS);
        }

        tx.execute(
            r#"
            INSERT INTO abuse_strikes(abuse_key, strikes, last_strike_ms, block_until_ms)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(abuse_key) DO UPDATE SET
              strikes=excluded.strikes,
              last_strike_ms=excluded.last_strike_ms,
              block_until_ms=(CASE WHEN excluded.block_until_ms > abuse_strikes.block_until_ms THEN excluded.block_until_ms ELSE abuse_strikes.block_until_ms END)
            "#,
            params![abuse_key, strikes, now, block_until_ms],
        )?;
        tx.commit()?;

        Ok(if block_until_ms > now { Some(block_until_ms) } else { None })
    }

    pub fn mark_inbox_seen(&self, activity_id: &str) -> Result<bool> {
        let conn = Connection::open(&self.path)?;
        let exists: Option<String> = conn
            .query_row(
                "SELECT activity_id FROM inbox_seen WHERE activity_id=?1",
                params![activity_id],
                |r| r.get(0),
            )
            .optional()?;
        if exists.is_some() {
            return Ok(false);
        }
        conn.execute(
            "INSERT INTO inbox_seen(activity_id, seen_at_ms) VALUES (?1, ?2)",
            params![activity_id, now_ms()],
        )?;
        Ok(true)
    }

    pub fn add_follower(&self, actor_id: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT OR IGNORE INTO followers(actor_id, created_at_ms) VALUES (?1, ?2)",
            params![actor_id, now_ms()],
        )?;
        Ok(())
    }

    pub fn remember_inbox_follow(&self, activity_id: &str, actor_id: &str) -> Result<()> {
        if activity_id.trim().is_empty() || actor_id.trim().is_empty() {
            return Ok(());
        }
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT OR REPLACE INTO inbox_follows(activity_id, actor_id, created_at_ms) VALUES (?1, ?2, ?3)",
            params![activity_id, actor_id, now_ms()],
        )?;
        Ok(())
    }

    pub fn get_inbox_follow_actor(&self, activity_id: &str) -> Result<Option<String>> {
        let conn = Connection::open(&self.path)?;
        conn.query_row(
            "SELECT actor_id FROM inbox_follows WHERE activity_id=?1",
            params![activity_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn forget_inbox_follow(&self, activity_id: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        let _ = conn.execute("DELETE FROM inbox_follows WHERE activity_id=?1", params![activity_id])?;
        Ok(())
    }

    pub fn block_actor(&self, actor_id: &str) -> Result<()> {
        let actor_id = actor_id.trim();
        if actor_id.is_empty() {
            return Ok(());
        }
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT OR REPLACE INTO blocked_actors(actor_id, created_at_ms) VALUES (?1, ?2)",
            params![actor_id, now_ms()],
        )?;
        Ok(())
    }

    pub fn unblock_actor(&self, actor_id: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        let _ = conn.execute("DELETE FROM blocked_actors WHERE actor_id=?1", params![actor_id])?;
        Ok(())
    }

    pub fn is_actor_blocked(&self, actor_id: &str) -> Result<bool> {
        let conn = Connection::open(&self.path)?;
        let v: Option<String> = conn
            .query_row(
                "SELECT actor_id FROM blocked_actors WHERE actor_id=?1",
                params![actor_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v.is_some())
    }

    pub fn list_blocked_actors(&self, limit: u32, offset: u32) -> Result<Vec<String>> {
        let conn = Connection::open(&self.path)?;
        let limit = limit.max(1).min(10_000) as i64;
        let offset = offset.min(100_000) as i64;
        let mut stmt = conn.prepare(
            "SELECT actor_id FROM blocked_actors ORDER BY created_at_ms DESC LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt
            .query_map(params![limit, offset], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn upsert_actor_meta(&self, actor_id: &str, is_fedi3: bool) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            r#"
            INSERT INTO actor_meta(actor_id, is_fedi3, last_seen_ms)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(actor_id) DO UPDATE SET
              is_fedi3=excluded.is_fedi3,
              last_seen_ms=excluded.last_seen_ms
            "#,
            params![actor_id, if is_fedi3 { 1 } else { 0 }, now_ms()],
        )?;
        Ok(())
    }

    pub fn get_actor_meta(&self, actor_id: &str) -> Result<Option<ActorMeta>> {
        let conn = Connection::open(&self.path)?;
        conn.query_row(
            "SELECT actor_id, is_fedi3, last_seen_ms FROM actor_meta WHERE actor_id=?1",
            params![actor_id],
            |r| {
                let is_fedi3: i64 = r.get(1)?;
                Ok(ActorMeta {
                    actor_id: r.get(0)?,
                    is_fedi3: is_fedi3 != 0,
                    last_seen_ms: r.get(2)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn store_inbox_activity(
        &self,
        activity_id: &str,
        actor_id: Option<&str>,
        ty: Option<&str>,
        activity_json: Vec<u8>,
    ) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT OR REPLACE INTO inbox_items(activity_id, created_at_ms, actor_id, type, activity_json) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![activity_id, now_ms(), actor_id, ty, activity_json],
        )?;
        Ok(())
    }

    pub fn list_inbox_notifications(&self, limit: u32, cursor: Option<i64>) -> Result<CollectionPage<(Vec<u8>, i64)>> {
        let limit = limit.min(200);
        let conn = Connection::open(&self.path)?;
        let before = cursor.unwrap_or(i64::MAX);
        let mut stmt = conn.prepare(
            r#"
            SELECT activity_json, created_at_ms
            FROM inbox_items
            WHERE created_at_ms < ?1
              AND type IN ('Follow','Accept','Reject','Announce','Like','EmojiReact','Create')
            ORDER BY created_at_ms DESC
            LIMIT ?2
            "#,
        )?;
        let rows = stmt.query_map(params![before, limit], |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?)))?;
        let items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        let next = items.last().map(|(_, ts)| ts.to_string());
        Ok(CollectionPage { total: 0, items, next })
    }

    pub fn store_inbox_activity_at(
        &self,
        activity_id: &str,
        created_at_ms: i64,
        actor_id: Option<&str>,
        ty: Option<&str>,
        activity_json: Vec<u8>,
    ) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT INTO inbox_items(activity_id, created_at_ms, actor_id, type, activity_json) VALUES (?1, ?2, ?3, ?4, ?5)\n             ON CONFLICT(activity_id) DO UPDATE SET\n               activity_json=excluded.activity_json,\n               actor_id=COALESCE(excluded.actor_id, inbox_items.actor_id),\n               type=COALESCE(excluded.type, inbox_items.type),\n               created_at_ms=(CASE WHEN excluded.created_at_ms < inbox_items.created_at_ms THEN excluded.created_at_ms ELSE inbox_items.created_at_ms END)",
            params![activity_id, created_at_ms, actor_id, ty, activity_json],
        )?;
        Ok(())
    }

    pub fn list_inbox_since_with_ts(&self, since_ms: i64, limit: u32) -> Result<(Vec<(Vec<u8>, i64)>, i64)> {
        let conn = Connection::open(&self.path)?;
        let limit = limit.max(1).min(500) as i64;
        let mut stmt = conn.prepare(
            "SELECT activity_json, created_at_ms FROM inbox_items WHERE created_at_ms > ?1 ORDER BY created_at_ms ASC LIMIT ?2",
        )?;
        let mut rows = stmt.query(params![since_ms, limit])?;
        let mut items: Vec<(Vec<u8>, i64)> = Vec::new();
        let mut latest: i64 = since_ms;
        while let Some(row) = rows.next()? {
            let json: Vec<u8> = row.get(0)?;
            let created_at_ms: i64 = row.get(1)?;
            if created_at_ms > latest {
                latest = created_at_ms;
            }
            items.push((json, created_at_ms));
        }
        Ok((items, latest))
    }

    pub fn insert_audit_event(
        &self,
        kind: &str,
        actor_id: Option<&str>,
        key_id: Option<&str>,
        activity_id: Option<&str>,
        ok: bool,
        status: Option<&str>,
        detail: Option<&str>,
    ) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT INTO audit_events(kind, created_at_ms, actor_id, key_id, activity_id, ok, status, detail) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                kind,
                now_ms(),
                actor_id,
                key_id,
                activity_id,
                if ok { 1 } else { 0 },
                status,
                detail
            ],
        )?;
        Ok(())
    }

    pub fn list_audit_events(&self, limit: u32) -> Result<Vec<AuditEvent>> {
        let limit = limit.max(1).min(500);
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, kind, created_at_ms, actor_id, key_id, activity_id, ok, status, detail
            FROM audit_events
            ORDER BY created_at_ms DESC
            LIMIT ?1
            "#,
        )?;
        let mut rows = stmt.query(params![limit])?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            out.push(AuditEvent {
                id: r.get(0)?,
                kind: r.get(1)?,
                created_at_ms: r.get(2)?,
                actor_id: r.get(3)?,
                key_id: r.get(4)?,
                activity_id: r.get(5)?,
                ok: (r.get::<_, i64>(6)? != 0),
                status: r.get(7)?,
                detail: r.get(8)?,
            });
        }
        Ok(out)
    }

    /// Persistent quota window (best-effort anti-abuse).
    /// Returns `Ok(true)` if allowed and the counter was incremented.
    pub fn bump_inbox_quota(
        &self,
        quota_key: &str,
        window_ms: i64,
        max_reqs: u32,
        max_bytes: u64,
        add_bytes: u64,
    ) -> Result<bool> {
        let quota_key = quota_key.trim();
        if quota_key.is_empty() || window_ms <= 0 {
            return Ok(true);
        }
        let max_reqs = max_reqs.max(1);
        let max_bytes = max_bytes.max(1024);
        let now = now_ms();

        let mut conn = Connection::open(&self.path)?;
        let tx = conn.transaction()?;

        let row: Option<(i64, u32, u64)> = tx
            .query_row(
                "SELECT window_start_ms, reqs, bytes FROM inbox_quota WHERE quota_key=?1",
                params![quota_key],
                |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u32, r.get::<_, i64>(2)? as u64)),
            )
            .optional()?;

        let (mut start_ms, mut reqs, mut bytes) = match row {
            Some((s, r, b)) => (s, r, b),
            None => (now, 0, 0),
        };

        if now.saturating_sub(start_ms) >= window_ms {
            start_ms = now;
            reqs = 0;
            bytes = 0;
        }

        if reqs.saturating_add(1) > max_reqs || bytes.saturating_add(add_bytes) > max_bytes {
            return Ok(false);
        }

        reqs = reqs.saturating_add(1);
        bytes = bytes.saturating_add(add_bytes);

        tx.execute(
            r#"
            INSERT INTO inbox_quota(quota_key, window_start_ms, reqs, bytes, updated_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(quota_key) DO UPDATE SET
              window_start_ms=excluded.window_start_ms,
              reqs=excluded.reqs,
              bytes=excluded.bytes,
              updated_at_ms=excluded.updated_at_ms
            "#,
            params![quota_key, start_ms, reqs as i64, bytes as i64, now],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn prune_inbox_quota_before(&self, cutoff_ms: i64) -> Result<u64> {
        let conn = Connection::open(&self.path)?;
        Ok(conn.execute("DELETE FROM inbox_quota WHERE updated_at_ms < ?1", params![cutoff_ms])? as u64)
    }

    pub fn prune_audit_events_before(&self, cutoff_ms: i64) -> Result<u64> {
        let conn = Connection::open(&self.path)?;
        Ok(conn.execute("DELETE FROM audit_events WHERE created_at_ms < ?1", params![cutoff_ms])? as u64)
    }

    pub fn insert_global_feed_item(
        &self,
        activity_id: &str,
        actor_id: Option<&str>,
        activity_json: Vec<u8>,
    ) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        let now = now_ms();
        let size_bytes: i64 = activity_json.len().try_into().unwrap_or(i64::MAX);
        conn.execute(
            r#"
            INSERT OR IGNORE INTO global_feed(activity_id, created_at_ms, actor_id, size_bytes, last_access_ms, activity_json)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![activity_id, now, actor_id, size_bytes, now, activity_json],
        )?;
        Ok(())
    }

    pub fn insert_federated_feed_item(
        &self,
        activity_id: &str,
        actor_id: Option<&str>,
        activity_json: Vec<u8>,
    ) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        let now = now_ms();
        let size_bytes: i64 = activity_json.len().try_into().unwrap_or(i64::MAX);
        conn.execute(
            r#"
            INSERT OR IGNORE INTO federated_feed(activity_id, created_at_ms, actor_id, size_bytes, last_access_ms, activity_json)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![activity_id, now, actor_id, size_bytes, now, activity_json],
        )?;
        Ok(())
    }

    pub fn global_feed_actor_stats_since(&self, actor_id: &str, since_ms: i64) -> Result<(u64, u64)> {
        let conn = Connection::open(&self.path)?;
        let (count, bytes): (u64, i64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(size_bytes), 0) FROM global_feed WHERE actor_id=?1 AND created_at_ms>=?2",
            params![actor_id, since_ms],
            |r| Ok((r.get::<_, u64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        Ok((count, bytes.max(0) as u64))
    }

    pub fn list_global_feed(&self, limit: u32, cursor_ms: Option<i64>) -> Result<CollectionPage<GlobalFeedItem>> {
        let conn = Connection::open(&self.path)?;
        let total: u64 = conn.query_row("SELECT COUNT(*) FROM global_feed", [], |r| r.get(0))?;
        let limit = limit.min(200).max(1);
        let (sql, params_vec): (String, Vec<rusqlite::types::Value>) = if let Some(c) = cursor_ms {
            (
                "SELECT activity_id, created_at_ms, actor_id, activity_json FROM global_feed WHERE created_at_ms < ?1 ORDER BY created_at_ms DESC LIMIT ?2"
                    .to_string(),
                vec![c.into(), (limit as i64).into()],
            )
        } else {
            (
                "SELECT activity_id, created_at_ms, actor_id, activity_json FROM global_feed ORDER BY created_at_ms DESC LIMIT ?1"
                    .to_string(),
                vec![(limit as i64).into()],
            )
        };

        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params_vec))?;
        let mut items = Vec::new();
        let mut last_created: Option<i64> = None;
        while let Some(row) = rows.next()? {
            let activity_id: String = row.get(0)?;
            let created_at_ms: i64 = row.get(1)?;
            let actor_id: Option<String> = row.get(2)?;
            let activity_json: Vec<u8> = row.get(3)?;
            last_created = Some(created_at_ms);
            items.push(GlobalFeedItem {
                activity_id,
                created_at_ms,
                actor_id,
                activity_json,
            });
        }

        if let Some(id0) = items.first().map(|i| i.activity_id.clone()) {
            let _ = conn.execute(
                "UPDATE global_feed SET last_access_ms=?2 WHERE activity_id=?1",
                params![id0, now_ms()],
            );
        }

        let next = if items.len() as u32 == limit {
            last_created.map(|v| v.to_string())
        } else {
            None
        };
        Ok(CollectionPage { total, items, next })
    }

    pub fn list_federated_feed(&self, limit: u32, cursor_ms: Option<i64>) -> Result<CollectionPage<GlobalFeedItem>> {
        let conn = Connection::open(&self.path)?;
        let total: u64 = conn.query_row("SELECT COUNT(*) FROM federated_feed", [], |r| r.get(0))?;
        let limit = limit.min(200).max(1);
        let (sql, params_vec): (String, Vec<rusqlite::types::Value>) = if let Some(c) = cursor_ms {
            (
                "SELECT activity_id, created_at_ms, actor_id, activity_json FROM federated_feed WHERE created_at_ms < ?1 ORDER BY created_at_ms DESC LIMIT ?2"
                    .to_string(),
                vec![c.into(), (limit as i64).into()],
            )
        } else {
            (
                "SELECT activity_id, created_at_ms, actor_id, activity_json FROM federated_feed ORDER BY created_at_ms DESC LIMIT ?1"
                    .to_string(),
                vec![(limit as i64).into()],
            )
        };

        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params_vec))?;
        let mut items = Vec::new();
        let mut last_created: Option<i64> = None;
        while let Some(row) = rows.next()? {
            let activity_id: String = row.get(0)?;
            let created_at_ms: i64 = row.get(1)?;
            let actor_id: Option<String> = row.get(2)?;
            let activity_json: Vec<u8> = row.get(3)?;
            last_created = Some(created_at_ms);
            items.push(GlobalFeedItem {
                activity_id,
                created_at_ms,
                actor_id,
                activity_json,
            });
        }

        if let Some(id0) = items.first().map(|i| i.activity_id.clone()) {
            let _ = conn.execute(
                "UPDATE federated_feed SET last_access_ms=?2 WHERE activity_id=?1",
                params![id0, now_ms()],
            );
        }

        let next = if items.len() as u32 == limit {
            last_created.map(|v| v.to_string())
        } else {
            None
        };
        Ok(CollectionPage { total, items, next })
    }

    pub fn list_home_feed(&self, limit: u32, cursor_ms: Option<i64>) -> Result<CollectionPage<GlobalFeedItem>> {
        let conn = Connection::open(&self.path)?;
        let accepted = FollowingStatus::Accepted as u32;
        let limit = limit.min(200).max(1) as i64;

        let total: u64 = conn.query_row(
            r#"
            SELECT COUNT(*) FROM (
              SELECT activity_id AS id, created_at_ms AS ts FROM inbox_items
              WHERE actor_id IN (SELECT actor_id FROM following WHERE status=?1)
              UNION ALL
              SELECT id AS id, created_at_ms AS ts FROM outbox_items
            )
            "#,
            params![accepted],
            |r| r.get(0),
        )?;

        let (sql, params_vec): (String, Vec<rusqlite::types::Value>) = if let Some(c) = cursor_ms {
            (
                r#"
                SELECT activity_id, created_at_ms, actor_id, activity_json
                FROM (
                  SELECT activity_id, created_at_ms, actor_id, activity_json
                  FROM inbox_items
                  WHERE actor_id IN (SELECT actor_id FROM following WHERE status=?1)
                  UNION ALL
                  SELECT id AS activity_id, created_at_ms, NULL AS actor_id, activity_json
                  FROM outbox_items
                )
                WHERE created_at_ms < ?2
                ORDER BY created_at_ms DESC
                LIMIT ?3
                "#
                .to_string(),
                vec![accepted.into(), c.into(), limit.into()],
            )
        } else {
            (
                r#"
                SELECT activity_id, created_at_ms, actor_id, activity_json
                FROM (
                  SELECT activity_id, created_at_ms, actor_id, activity_json
                  FROM inbox_items
                  WHERE actor_id IN (SELECT actor_id FROM following WHERE status=?1)
                  UNION ALL
                  SELECT id AS activity_id, created_at_ms, NULL AS actor_id, activity_json
                  FROM outbox_items
                )
                ORDER BY created_at_ms DESC
                LIMIT ?2
                "#
                .to_string(),
                vec![accepted.into(), limit.into()],
            )
        };

        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params_vec))?;
        let mut items = Vec::new();
        let mut last_created: Option<i64> = None;
        while let Some(row) = rows.next()? {
            let activity_id: String = row.get(0)?;
            let created_at_ms: i64 = row.get(1)?;
            let actor_id: Option<String> = row.get(2)?;
            let activity_json: Vec<u8> = row.get(3)?;
            last_created = Some(created_at_ms);
            items.push(GlobalFeedItem {
                activity_id,
                created_at_ms,
                actor_id,
                activity_json,
            });
        }

        let next = if items.len() as i64 == limit {
            last_created.map(|v| v.to_string())
        } else {
            None
        };
        Ok(CollectionPage { total, items, next })
    }

    pub fn list_unified_feed(&self, limit: u32, cursor_ms: Option<i64>) -> Result<CollectionPage<GlobalFeedItem>> {
        let conn = Connection::open(&self.path)?;
        let accepted = FollowingStatus::Accepted as u32;
        let limit = limit.min(200).max(1) as i64;

        let total: u64 = conn.query_row(
            r#"
            SELECT COUNT(*) FROM (
              SELECT activity_id AS id, created_at_ms AS ts FROM global_feed
              UNION ALL
              SELECT activity_id AS id, created_at_ms AS ts FROM federated_feed
              UNION ALL
              SELECT activity_id AS id, created_at_ms AS ts FROM inbox_items
              WHERE actor_id IN (SELECT actor_id FROM following WHERE status=?1)
              UNION ALL
              SELECT id AS id, created_at_ms AS ts FROM outbox_items
            )
            "#,
            params![accepted],
            |r| r.get(0),
        )?;

        let (sql, params_vec): (String, Vec<rusqlite::types::Value>) = if let Some(c) = cursor_ms {
            (
                r#"
                SELECT activity_id, created_at_ms, actor_id, activity_json
                FROM (
                  SELECT activity_id, created_at_ms, actor_id, activity_json FROM global_feed
                  UNION ALL
                  SELECT activity_id, created_at_ms, actor_id, activity_json FROM federated_feed
                  UNION ALL
                  SELECT activity_id, created_at_ms, actor_id, activity_json
                  FROM inbox_items
                  WHERE actor_id IN (SELECT actor_id FROM following WHERE status=?1)
                  UNION ALL
                  SELECT id AS activity_id, created_at_ms, NULL AS actor_id, activity_json
                  FROM outbox_items
                )
                WHERE created_at_ms < ?2
                ORDER BY created_at_ms DESC
                LIMIT ?3
                "#
                .to_string(),
                vec![accepted.into(), c.into(), limit.into()],
            )
        } else {
            (
                r#"
                SELECT activity_id, created_at_ms, actor_id, activity_json
                FROM (
                  SELECT activity_id, created_at_ms, actor_id, activity_json FROM global_feed
                  UNION ALL
                  SELECT activity_id, created_at_ms, actor_id, activity_json FROM federated_feed
                  UNION ALL
                  SELECT activity_id, created_at_ms, actor_id, activity_json
                  FROM inbox_items
                  WHERE actor_id IN (SELECT actor_id FROM following WHERE status=?1)
                  UNION ALL
                  SELECT id AS activity_id, created_at_ms, NULL AS actor_id, activity_json
                  FROM outbox_items
                )
                ORDER BY created_at_ms DESC
                LIMIT ?2
                "#
                .to_string(),
                vec![accepted.into(), limit.into()],
            )
        };

        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params_vec))?;
        let mut items = Vec::new();
        let mut last_created: Option<i64> = None;
        while let Some(row) = rows.next()? {
            let activity_id: String = row.get(0)?;
            let created_at_ms: i64 = row.get(1)?;
            let actor_id: Option<String> = row.get(2)?;
            let activity_json: Vec<u8> = row.get(3)?;
            last_created = Some(created_at_ms);
            items.push(GlobalFeedItem {
                activity_id,
                created_at_ms,
                actor_id,
                activity_json,
            });
        }

        let next = if items.len() as i64 == limit {
            last_created.map(|v| v.to_string())
        } else {
            None
        };
        Ok(CollectionPage { total, items, next })
    }

    /// Best-effort backfill so the local DHT/global timeline can show our own previously
    /// published public activities (useful after upgrades).
    pub fn backfill_global_feed_from_outbox(&self, limit: u32) -> Result<u64> {
        let conn = Connection::open(&self.path)?;
        let limit = limit.min(2000).max(1) as i64;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, activity_json
            FROM outbox_items
            ORDER BY created_at_ms DESC
            LIMIT ?1
            "#,
        )?;
        let mut rows = stmt.query(params![limit])?;
        let mut inserted = 0u64;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let bytes: Vec<u8> = row.get(1)?;
            let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else { continue };
            if !is_public_activity_value(&v) {
                continue;
            }
            if self.insert_global_feed_item(&id, None, bytes).is_ok() {
                inserted = inserted.saturating_add(1);
            }
        }
        Ok(inserted)
    }

    pub fn prune_global_feed_before(&self, cutoff_ms: i64, limit: u32) -> Result<u64> {
        let conn = Connection::open(&self.path)?;
        // SQLite compatibility: some builds don't support `DELETE ... LIMIT`.
        let deleted = conn.execute(
            r#"
            DELETE FROM global_feed
            WHERE activity_id IN (
              SELECT activity_id
              FROM global_feed
              WHERE created_at_ms < ?1
              ORDER BY created_at_ms ASC
              LIMIT ?2
            )
            "#,
            params![cutoff_ms, limit as i64],
        )?;
        Ok(deleted as u64)
    }

    pub fn prune_global_feed_to_max(&self, max_items: u32) -> Result<u64> {
        let conn = Connection::open(&self.path)?;
        let max_items = max_items.max(1) as i64;
        let deleted = conn.execute(
            r#"
            DELETE FROM global_feed
            WHERE activity_id IN (
              SELECT activity_id FROM global_feed
              ORDER BY created_at_ms DESC
              LIMIT -1 OFFSET ?1
            )
            "#,
            params![max_items],
        )?;
        Ok(deleted as u64)
    }

    pub fn prune_federated_feed_before(&self, cutoff_ms: i64, limit: u32) -> Result<u64> {
        let conn = Connection::open(&self.path)?;
        // SQLite compatibility: some builds don't support `DELETE ... LIMIT`.
        let deleted = conn.execute(
            r#"
            DELETE FROM federated_feed
            WHERE activity_id IN (
              SELECT activity_id
              FROM federated_feed
              WHERE created_at_ms < ?1
              ORDER BY created_at_ms ASC
              LIMIT ?2
            )
            "#,
            params![cutoff_ms, limit as i64],
        )?;
        Ok(deleted as u64)
    }

    pub fn prune_federated_feed_to_max(&self, max_items: u32) -> Result<u64> {
        let conn = Connection::open(&self.path)?;
        let max_items = max_items.max(1) as i64;
        let deleted = conn.execute(
            r#"
            DELETE FROM federated_feed
            WHERE activity_id IN (
              SELECT activity_id FROM federated_feed
              ORDER BY created_at_ms DESC
              LIMIT -1 OFFSET ?1
            )
            "#,
            params![max_items],
        )?;
        Ok(deleted as u64)
    }

    pub fn upsert_object(&self, object_id: &str, object_json: Vec<u8>) -> Result<()> {
        self.upsert_object_with_actor(object_id, None, object_json)
    }

    pub fn upsert_object_with_actor(
        &self,
        object_id: &str,
        actor_id: Option<&str>,
        object_json: Vec<u8>,
    ) -> Result<()> {
        let now = now_ms();
        let conn = Connection::open(&self.path)?;
        let size_bytes: i64 = object_json.len().try_into().unwrap_or(i64::MAX);
        conn.execute(
            r#"
            INSERT INTO objects(object_id, created_at_ms, updated_at_ms, deleted, object_json, actor_id, size_bytes, last_access_ms)
            VALUES (?1, ?2, ?2, 0, ?3, ?4, ?5, ?6)
            ON CONFLICT(object_id) DO UPDATE SET
              updated_at_ms=excluded.updated_at_ms,
              deleted=0,
              object_json=excluded.object_json,
              actor_id=COALESCE(excluded.actor_id, objects.actor_id),
              size_bytes=excluded.size_bytes,
              last_access_ms=excluded.last_access_ms
            "#,
            params![object_id, now, object_json, actor_id, size_bytes, now],
        )?;
        // Best-effort: store attachments metadata if parsable.
        let _ = self.store_attachments_from_object_json(object_id, &conn);
        let _ = self.store_meta_from_object_json(object_id, &conn);
        let _ = self.store_tags_from_object_json(object_id, &conn);
        Ok(())
    }

    pub fn mark_object_deleted(&self, object_id: &str) -> Result<()> {
        let now = now_ms();
        let conn = Connection::open(&self.path)?;
        conn.execute(
            r#"
            INSERT INTO objects(object_id, created_at_ms, updated_at_ms, deleted, object_json)
            VALUES (?1, ?2, ?2, 1, X'')
            ON CONFLICT(object_id) DO UPDATE SET updated_at_ms=excluded.updated_at_ms, deleted=1
            "#,
            params![object_id, now],
        )?;
        Ok(())
    }

    pub fn upsert_reaction(&self, reaction_id: &str, ty: &str, actor_id: &str, object_id: &str) -> Result<()> {
        self.upsert_reaction_with_content(reaction_id, ty, actor_id, object_id, None)
    }

    pub fn upsert_reaction_with_content(
        &self,
        reaction_id: &str,
        ty: &str,
        actor_id: &str,
        object_id: &str,
        content: Option<&str>,
    ) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT OR REPLACE INTO reactions(reaction_id, created_at_ms, type, actor_id, object_id, content) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![reaction_id, now_ms(), ty, actor_id, object_id, content],
        )?;
        Ok(())
    }

    pub fn list_reaction_counts(&self, object_id: &str, limit: u32) -> Result<Vec<(String, Option<String>, u64)>> {
        let object_id = object_id.trim();
        if object_id.is_empty() {
            return Ok(vec![]);
        }
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn.prepare(
            r#"
            SELECT type, content, COUNT(*) AS c
            FROM reactions
            WHERE object_id=?1
            GROUP BY type, content
            ORDER BY c DESC
            LIMIT ?2
            "#,
        )?;
        let rows = stmt.query_map(params![object_id, limit.min(200)], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, i64>(2)? as u64))
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn search_notes_by_text(&self, q: &str, limit: u32, cursor: Option<i64>) -> Result<CollectionPage<ObjectRow>> {
        let q = q.trim().to_lowercase();
        if q.is_empty() {
            return Ok(CollectionPage {
                total: 0,
                items: Vec::new(),
                next: None,
            });
        }
        let conn = Connection::open(&self.path)?;
        let limit = limit.min(200).max(1) as i64;
        let q_like = format!("%{}%", escape_like(&q));
        let total: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM objects WHERE deleted=0 AND lower(CAST(object_json AS TEXT)) LIKE ?1",
                params![q_like],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let mut stmt = if cursor.is_some() {
            conn.prepare(
                "SELECT object_id, created_at_ms, actor_id, object_json FROM objects WHERE deleted=0 AND lower(CAST(object_json AS TEXT)) LIKE ?1 AND created_at_ms < ?2 ORDER BY created_at_ms DESC LIMIT ?3",
            )?
        } else {
            conn.prepare(
                "SELECT object_id, created_at_ms, actor_id, object_json FROM objects WHERE deleted=0 AND lower(CAST(object_json AS TEXT)) LIKE ?1 ORDER BY created_at_ms DESC LIMIT ?2",
            )?
        };
        let mut rows = if let Some(cur) = cursor {
            stmt.query(params![q_like, cur, limit])?
        } else {
            stmt.query(params![q_like, limit])?
        };
        let mut items = Vec::<ObjectRow>::new();
        let mut last_created = None;
        while let Some(row) = rows.next()? {
            let object_id: String = row.get(0)?;
            let created_at_ms: i64 = row.get(1)?;
            let actor_id: Option<String> = row.get(2)?;
            let object_json: Vec<u8> = row.get(3)?;
            last_created = Some(created_at_ms);
            items.push(ObjectRow {
                object_id,
                created_at_ms,
                actor_id,
                object_json,
            });
        }
        let next = if items.len() as i64 == limit {
            last_created.map(|v| v.to_string())
        } else {
            None
        };
        Ok(CollectionPage { total, items, next })
    }

    pub fn search_notes_by_tag(&self, tag: &str, limit: u32, cursor: Option<i64>) -> Result<CollectionPage<ObjectRow>> {
        let tag = tag.trim().trim_start_matches('#').to_lowercase();
        if tag.is_empty() {
            return Ok(CollectionPage {
                total: 0,
                items: Vec::new(),
                next: None,
            });
        }
        let conn = Connection::open(&self.path)?;
        let limit = limit.min(200).max(1) as i64;
        let tag_hash = format!("#{tag}");
        let total: u64 = conn
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM object_tags t
                JOIN objects o ON o.object_id = t.object_id
                WHERE o.deleted=0
                  AND (lower(t.name)=?1 OR lower(t.name)=?2)
                  AND (t.tag_type='Hashtag' OR t.name LIKE '#%')
                "#,
                params![tag, tag_hash],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let mut stmt = if cursor.is_some() {
            conn.prepare(
                r#"
                SELECT o.object_id, o.created_at_ms, o.actor_id, o.object_json
                FROM object_tags t
                JOIN objects o ON o.object_id = t.object_id
                WHERE o.deleted=0
                  AND (lower(t.name)=?1 OR lower(t.name)=?2)
                  AND (t.tag_type='Hashtag' OR t.name LIKE '#%')
                  AND o.created_at_ms < ?3
                ORDER BY o.created_at_ms DESC
                LIMIT ?4
                "#,
            )?
        } else {
            conn.prepare(
                r#"
                SELECT o.object_id, o.created_at_ms, o.actor_id, o.object_json
                FROM object_tags t
                JOIN objects o ON o.object_id = t.object_id
                WHERE o.deleted=0
                  AND (lower(t.name)=?1 OR lower(t.name)=?2)
                  AND (t.tag_type='Hashtag' OR t.name LIKE '#%')
                ORDER BY o.created_at_ms DESC
                LIMIT ?3
                "#,
            )?
        };
        let mut rows = if let Some(cur) = cursor {
            stmt.query(params![tag, tag_hash, cur, limit])?
        } else {
            stmt.query(params![tag, tag_hash, limit])?
        };
        let mut items = Vec::<ObjectRow>::new();
        let mut last_created = None;
        while let Some(row) = rows.next()? {
            let object_id: String = row.get(0)?;
            let created_at_ms: i64 = row.get(1)?;
            let actor_id: Option<String> = row.get(2)?;
            let object_json: Vec<u8> = row.get(3)?;
            last_created = Some(created_at_ms);
            items.push(ObjectRow {
                object_id,
                created_at_ms,
                actor_id,
                object_json,
            });
        }
        let next = if items.len() as i64 == limit {
            last_created.map(|v| v.to_string())
        } else {
            None
        };
        Ok(CollectionPage { total, items, next })
    }

    pub fn search_actors_by_text(&self, q: &str, limit: u32, cursor: Option<i64>) -> Result<CollectionPage<ObjectRow>> {
        let q = q.trim().to_lowercase();
        if q.is_empty() {
            return Ok(CollectionPage {
                total: 0,
                items: Vec::new(),
                next: None,
            });
        }
        let conn = Connection::open(&self.path)?;
        let limit = limit.min(200).max(1) as i64;
        let q_like = format!("%{}%", escape_like(&q));
        let total: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM objects WHERE deleted=0 AND lower(CAST(object_json AS TEXT)) LIKE ?1",
                params![q_like],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let mut stmt = if cursor.is_some() {
            conn.prepare(
                "SELECT object_id, created_at_ms, actor_id, object_json FROM objects WHERE deleted=0 AND lower(CAST(object_json AS TEXT)) LIKE ?1 AND created_at_ms < ?2 ORDER BY created_at_ms DESC LIMIT ?3",
            )?
        } else {
            conn.prepare(
                "SELECT object_id, created_at_ms, actor_id, object_json FROM objects WHERE deleted=0 AND lower(CAST(object_json AS TEXT)) LIKE ?1 ORDER BY created_at_ms DESC LIMIT ?2",
            )?
        };
        let mut rows = if let Some(cur) = cursor {
            stmt.query(params![q_like, cur, limit])?
        } else {
            stmt.query(params![q_like, limit])?
        };
        let mut items = Vec::<ObjectRow>::new();
        let mut last_created = None;
        while let Some(row) = rows.next()? {
            let object_id: String = row.get(0)?;
            let created_at_ms: i64 = row.get(1)?;
            let actor_id: Option<String> = row.get(2)?;
            let object_json: Vec<u8> = row.get(3)?;
            last_created = Some(created_at_ms);
            items.push(ObjectRow {
                object_id,
                created_at_ms,
                actor_id,
                object_json,
            });
        }
        let next = if items.len() as i64 == limit {
            last_created.map(|v| v.to_string())
        } else {
            None
        };
        Ok(CollectionPage { total, items, next })
    }

    pub fn search_hashtags(&self, q: &str, limit: u32) -> Result<Vec<(String, u64)>> {
        let q = q.trim().trim_start_matches('#').to_lowercase();
        let conn = Connection::open(&self.path)?;
        let limit = limit.min(200).max(1) as i64;
        let q_like = if q.is_empty() {
            "%".to_string()
        } else {
            format!("%{}%", escape_like(&q))
        };
        let mut stmt = conn.prepare(
            r#"
            SELECT name, COUNT(*)
            FROM object_tags
            WHERE name != ''
              AND (tag_type='Hashtag' OR name LIKE '#%')
              AND lower(name) LIKE ?1
            GROUP BY name
            ORDER BY COUNT(*) DESC
            LIMIT ?2
            "#,
        )?;
        let mut rows = stmt.query(params![q_like, limit])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let name: String = row.get(0)?;
            let count: u64 = row.get(1)?;
            out.push((name, count));
        }
        Ok(out)
    }

    pub fn list_reactions_for_actor_object(
        &self,
        actor_id: &str,
        object_id: &str,
        limit: u32,
    ) -> Result<Vec<(String, String, Option<String>)>> {
        let actor_id = actor_id.trim().trim_end_matches('/');
        let object_id = object_id.trim();
        if actor_id.is_empty() || object_id.is_empty() {
            return Ok(vec![]);
        }
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn.prepare(
            r#"
            SELECT reaction_id, type, content
            FROM reactions
            WHERE actor_id=?1 AND object_id=?2
            ORDER BY created_at_ms DESC
            LIMIT ?3
            "#,
        )?;
        let rows = stmt.query_map(params![actor_id, object_id, limit.min(200)], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn list_reaction_actors_for_object(
        &self,
        object_id: &str,
        ty: &str,
        content: Option<&str>,
        limit: u32,
    ) -> Result<Vec<String>> {
        fn map_actor_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<String> {
            r.get::<_, String>(0)
        }

        let object_id = object_id.trim();
        let ty = ty.trim();
        if object_id.is_empty() || ty.is_empty() {
            return Ok(vec![]);
        }
        let conn = Connection::open(&self.path)?;
        let mut stmt = if content.unwrap_or_default().trim().is_empty() {
            conn.prepare(
                r#"
                SELECT actor_id
                FROM reactions
                WHERE object_id=?1 AND type=?2 AND (content IS NULL OR content='')
                ORDER BY created_at_ms DESC
                LIMIT ?3
                "#,
            )?
        } else {
            conn.prepare(
                r#"
                SELECT actor_id
                FROM reactions
                WHERE object_id=?1 AND type=?2 AND content=?3
                ORDER BY created_at_ms DESC
                LIMIT ?4
                "#,
            )?
        };

        let rows = if content.unwrap_or_default().trim().is_empty() {
            stmt.query_map(params![object_id, ty, limit.min(200)], map_actor_row)?
        } else {
            stmt.query_map(params![object_id, ty, content.unwrap(), limit.min(200)], map_actor_row)?
        };
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn find_reaction_id(
        &self,
        actor_id: &str,
        ty: &str,
        object_id: &str,
        content: Option<&str>,
    ) -> Result<Option<String>> {
        let actor_id = actor_id.trim().trim_end_matches('/');
        let object_id = object_id.trim();
        if actor_id.is_empty() || object_id.is_empty() {
            return Ok(None);
        }
        let ty = ty.trim();
        if ty.is_empty() {
            return Ok(None);
        }
        let content = content.map(str::trim).filter(|s| !s.is_empty());
        let conn = Connection::open(&self.path)?;
        let row: Option<String> = if let Some(c) = content {
            conn.query_row(
                "SELECT reaction_id FROM reactions WHERE actor_id=?1 AND object_id=?2 AND type=?3 AND content=?4 ORDER BY created_at_ms DESC LIMIT 1",
                params![actor_id, object_id, ty, c],
                |r| r.get(0),
            )
            .optional()?
        } else {
            conn.query_row(
                "SELECT reaction_id FROM reactions WHERE actor_id=?1 AND object_id=?2 AND type=?3 AND (content IS NULL OR content='') ORDER BY created_at_ms DESC LIMIT 1",
                params![actor_id, object_id, ty],
                |r| r.get(0),
            )
            .optional()?
        };
        Ok(row)
    }

    pub fn remove_reaction(&self, reaction_id: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute("DELETE FROM reactions WHERE reaction_id=?1", params![reaction_id])?;
        Ok(())
    }

    pub fn enqueue_object_fetch(&self, object_url: &str, next_attempt_at_ms: i64, err: Option<&str>) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        // status: 0=pending, 1=done, 2=dead
        conn.execute(
            r#"
            INSERT INTO object_fetch_jobs(object_url, created_at_ms, next_attempt_at_ms, attempt, status, last_error)
            VALUES (?1, ?2, ?3, 0, 0, ?4)
            ON CONFLICT(object_url) DO UPDATE SET
              next_attempt_at_ms=excluded.next_attempt_at_ms,
              status=0,
              last_error=excluded.last_error
            "#,
            params![object_url, now_ms(), next_attempt_at_ms, err],
        )?;
        Ok(())
    }

    pub fn try_mark_object_fetch_attempt(&self, object_url: &str, attempt: u32, next_attempt_at_ms: i64, err: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "UPDATE object_fetch_jobs SET attempt=?2, next_attempt_at_ms=?3, last_error=?4 WHERE object_url=?1",
            params![object_url, attempt, next_attempt_at_ms, err],
        )?;
        Ok(())
    }

    pub fn mark_object_fetch_done(&self, object_url: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "UPDATE object_fetch_jobs SET status=1, last_error=NULL WHERE object_url=?1",
            params![object_url],
        )?;
        Ok(())
    }

    pub fn mark_object_fetch_dead(&self, object_url: &str, err: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "UPDATE object_fetch_jobs SET status=2, last_error=?2 WHERE object_url=?1",
            params![object_url, err],
        )?;
        Ok(())
    }

    pub fn fetch_due_object_jobs(&self, limit: u32) -> Result<Vec<ObjectFetchJob>> {
        let conn = Connection::open(&self.path)?;
        let now = now_ms();
        let mut stmt = conn.prepare(
            r#"
            SELECT object_url, attempt
            FROM object_fetch_jobs
            WHERE status=0 AND next_attempt_at_ms <= ?1
            ORDER BY next_attempt_at_ms ASC
            LIMIT ?2
            "#,
        )?;
        let mut rows = stmt.query(params![now, limit])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(ObjectFetchJob {
                object_url: row.get(0)?,
                attempt: row.get(1)?,
            });
        }
        Ok(out)
    }

    pub fn remove_follower(&self, actor_id: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute("DELETE FROM followers WHERE actor_id=?1", params![actor_id])?;
        Ok(())
    }

    pub fn set_following(&self, actor_id: &str, status: FollowingStatus) -> Result<()> {
        let actor_id = actor_id.trim().trim_end_matches('/');
        if actor_id.is_empty() {
            return Ok(());
        }
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT INTO following(actor_id, status, created_at_ms) VALUES (?1, ?2, ?3)\n             ON CONFLICT(actor_id) DO UPDATE SET status=excluded.status",
            params![actor_id, status as u32, now_ms()],
        )?;
        Ok(())
    }

    pub fn get_following_status(&self, actor_id: &str) -> Result<Option<FollowingStatus>> {
        let actor_id = actor_id.trim().trim_end_matches('/');
        if actor_id.is_empty() {
            return Ok(None);
        }
        let conn = Connection::open(&self.path)?;
        let status_opt: Option<u32> = conn
            .query_row(
                "SELECT status FROM following WHERE actor_id=?1",
                params![actor_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(match status_opt {
            Some(0) => Some(FollowingStatus::Pending),
            Some(1) => Some(FollowingStatus::Accepted),
            Some(_) => None,
            None => None,
        })
    }

    pub fn remove_following(&self, actor_id: &str) -> Result<()> {
        let actor_id = actor_id.trim().trim_end_matches('/');
        if actor_id.is_empty() {
            return Ok(());
        }
        let conn = Connection::open(&self.path)?;
        conn.execute("DELETE FROM following WHERE actor_id=?1", params![actor_id])?;
        Ok(())
    }

    pub fn store_outbox(&self, id: &str, activity_json: Vec<u8>) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT OR REPLACE INTO outbox_items(id, created_at_ms, activity_json) VALUES (?1, ?2, ?3)",
            params![id, now_ms(), activity_json],
        )?;
        Ok(())
    }

    pub fn store_outbox_at(&self, id: &str, created_at_ms: i64, activity_json: Vec<u8>) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT INTO outbox_items(id, created_at_ms, activity_json) VALUES (?1, ?2, ?3)\n             ON CONFLICT(id) DO UPDATE SET\n               activity_json=excluded.activity_json,\n               created_at_ms=(CASE WHEN excluded.created_at_ms < outbox_items.created_at_ms THEN excluded.created_at_ms ELSE outbox_items.created_at_ms END)",
            params![id, created_at_ms, activity_json],
        )?;
        Ok(())
    }

    pub fn get_outbox(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let conn = Connection::open(&self.path)?;
        conn.query_row(
            "SELECT activity_json FROM outbox_items WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn get_inbox(&self, activity_id: &str) -> Result<Option<Vec<u8>>> {
        let conn = Connection::open(&self.path)?;
        conn.query_row(
            "SELECT activity_json FROM inbox_items WHERE activity_id=?1",
            params![activity_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn upsert_note_reply(&self, note_id: &str, activity_id: &str, created_at_ms: i64) -> Result<()> {
        let note_id = note_id.trim();
        let activity_id = activity_id.trim();
        if note_id.is_empty() || activity_id.is_empty() {
            return Ok(());
        }
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT OR REPLACE INTO note_replies(note_id, activity_id, created_at_ms) VALUES (?1, ?2, ?3)",
            params![note_id, activity_id, created_at_ms],
        )?;
        Ok(())
    }

    pub fn list_note_replies(&self, note_id: &str, limit: u32, cursor: Option<i64>) -> Result<CollectionPage<GlobalFeedItem>> {
        let note_id = note_id.trim();
        if note_id.is_empty() {
            return Ok(CollectionPage { total: 0, items: vec![], next: None });
        }
        let limit = limit.min(200);
        let conn = Connection::open(&self.path)?;
        let total: u64 = conn.query_row(
            "SELECT COUNT(*) FROM note_replies WHERE note_id=?1",
            params![note_id],
            |r| r.get(0),
        )?;

        let before = cursor.unwrap_or(i64::MAX);
        let mut stmt = conn.prepare(
            r#"
            SELECT r.activity_id, r.created_at_ms, COALESCE(o.activity_json, i.activity_json) AS activity_json
            FROM note_replies r
            LEFT JOIN outbox_items o ON o.id = r.activity_id
            LEFT JOIN inbox_items i ON i.activity_id = r.activity_id
            WHERE r.note_id=?1 AND r.created_at_ms < ?2
            ORDER BY r.created_at_ms DESC
            LIMIT ?3
            "#,
        )?;
        let mut items: Vec<GlobalFeedItem> = vec![];
        let mut rows = stmt.query(params![note_id, before, limit])?;
        while let Some(row) = rows.next()? {
            let activity_id: String = row.get(0)?;
            let created_at_ms: i64 = row.get(1)?;
            let activity_json: Option<Vec<u8>> = row.get(2)?;
            if let Some(activity_json) = activity_json {
                items.push(GlobalFeedItem {
                    activity_id,
                    created_at_ms,
                    actor_id: None,
                    activity_json,
                });
            }
        }

        let next = items.last().map(|it| it.created_at_ms.to_string());
        Ok(CollectionPage { total, items, next })
    }

    pub fn get_local_meta(&self, key: &str) -> Result<Option<String>> {
        let conn = Connection::open(&self.path)?;
        conn.query_row(
            "SELECT value FROM local_meta WHERE key=?1",
            params![key],
            |r| r.get(0),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn set_local_meta(&self, key: &str, value: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT OR REPLACE INTO local_meta(key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn list_followers(&self, limit: u32, cursor: Option<i64>) -> Result<CollectionPage<String>> {
        self.list_collection("followers", limit, cursor)
    }

    pub fn count_followers(&self) -> Result<u64> {
        let conn = Connection::open(&self.path)?;
        let total: u64 = conn.query_row("SELECT COUNT(*) FROM followers", [], |r| r.get(0))?;
        Ok(total)
    }

    /// Followers from legacy instances (best-effort).
    /// We treat unknown actors (missing `actor_meta`) as legacy for safety.
    pub fn count_legacy_followers(&self) -> Result<u64> {
        let conn = Connection::open(&self.path)?;
        let total: u64 = conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM followers f
            LEFT JOIN actor_meta m ON m.actor_id = f.actor_id
            WHERE m.is_fedi3 IS NULL OR m.is_fedi3 = 0
            "#,
            [],
            |r| r.get(0),
        )?;
        Ok(total)
    }

    pub fn list_following(&self, limit: u32, cursor: Option<i64>) -> Result<CollectionPage<String>> {
        self.list_collection("following", limit, cursor)
    }

    pub fn list_following_accepted_ids(&self, limit: u32) -> Result<Vec<String>> {
        let conn = Connection::open(&self.path)?;
        let limit = limit.max(1).min(10_000) as i64;
        let mut stmt = conn.prepare(
            "SELECT actor_id FROM following WHERE status=?1 ORDER BY created_at_ms DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![FollowingStatus::Accepted as u32, limit], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    fn list_collection(&self, table: &str, limit: u32, cursor: Option<i64>) -> Result<CollectionPage<String>> {
        let conn = Connection::open(&self.path)?;
        let total: u64 = conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;

        let (sql, params_vec): (String, Vec<rusqlite::types::Value>) = if let Some(c) = cursor {
            (
                format!(
                    "SELECT actor_id, created_at_ms FROM {table} WHERE created_at_ms < ?1 ORDER BY created_at_ms DESC LIMIT ?2"
                ),
                vec![c.into(), (limit as i64).into()],
            )
        } else {
            (
                format!(
                    "SELECT actor_id, created_at_ms FROM {table} ORDER BY created_at_ms DESC LIMIT ?1"
                ),
                vec![(limit as i64).into()],
            )
        };

        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params_vec))?;
        let mut items = Vec::new();
        let mut last_created: Option<i64> = None;
        while let Some(row) = rows.next()? {
            let actor_id: String = row.get(0)?;
            let created_at_ms: i64 = row.get(1)?;
            last_created = Some(created_at_ms);
            items.push(actor_id);
        }

        let next = if items.len() as u32 == limit {
            last_created.map(|v| v.to_string())
        } else {
            None
        };

        Ok(CollectionPage { total, items, next })
    }

    pub fn list_outbox(&self, limit: u32, cursor: Option<i64>) -> Result<CollectionPage<Vec<u8>>> {
        let conn = Connection::open(&self.path)?;
        let total: u64 = conn.query_row("SELECT COUNT(*) FROM outbox_items", [], |r| r.get(0))?;

        let (sql, params_vec): (String, Vec<rusqlite::types::Value>) = if let Some(c) = cursor {
            (
                "SELECT activity_json, created_at_ms FROM outbox_items WHERE created_at_ms < ?1 ORDER BY created_at_ms DESC LIMIT ?2"
                    .to_string(),
                vec![c.into(), (limit as i64).into()],
            )
        } else {
            (
                "SELECT activity_json, created_at_ms FROM outbox_items ORDER BY created_at_ms DESC LIMIT ?1"
                    .to_string(),
                vec![(limit as i64).into()],
            )
        };

        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params_vec))?;
        let mut items = Vec::new();
        let mut last_created: Option<i64> = None;
        while let Some(row) = rows.next()? {
            let json: Vec<u8> = row.get(0)?;
            let created_at_ms: i64 = row.get(1)?;
            last_created = Some(created_at_ms);
            items.push(json);
        }

        let next = if items.len() as u32 == limit {
            last_created.map(|v| v.to_string())
        } else {
            None
        };
        Ok(CollectionPage { total, items, next })
    }

    pub fn list_outbox_since(&self, since_ms: i64, limit: u32) -> Result<(Vec<Vec<u8>>, i64)> {
        let conn = Connection::open(&self.path)?;
        let limit = limit.max(1).min(500) as i64;
        let mut stmt = conn.prepare(
            "SELECT activity_json, created_at_ms FROM outbox_items WHERE created_at_ms > ?1 ORDER BY created_at_ms ASC LIMIT ?2",
        )?;
        let mut rows = stmt.query(params![since_ms, limit])?;
        let mut items = Vec::new();
        let mut latest: i64 = since_ms;
        while let Some(row) = rows.next()? {
            let json: Vec<u8> = row.get(0)?;
            let created_at_ms: i64 = row.get(1)?;
            if created_at_ms > latest {
                latest = created_at_ms;
            }
            items.push(json);
        }
        Ok((items, latest))
    }

    pub fn list_outbox_since_with_ts(&self, since_ms: i64, limit: u32) -> Result<(Vec<(Vec<u8>, i64)>, i64)> {
        let conn = Connection::open(&self.path)?;
        let limit = limit.max(1).min(500) as i64;
        let mut stmt = conn.prepare(
            "SELECT activity_json, created_at_ms FROM outbox_items WHERE created_at_ms > ?1 ORDER BY created_at_ms ASC LIMIT ?2",
        )?;
        let mut rows = stmt.query(params![since_ms, limit])?;
        let mut items: Vec<(Vec<u8>, i64)> = Vec::new();
        let mut latest: i64 = since_ms;
        while let Some(row) = rows.next()? {
            let json: Vec<u8> = row.get(0)?;
            let created_at_ms: i64 = row.get(1)?;
            if created_at_ms > latest {
                latest = created_at_ms;
            }
            items.push((json, created_at_ms));
        }
        Ok((items, latest))
    }

    pub fn get_p2p_sync_since(&self, actor_id: &str) -> Result<i64> {
        let conn = Connection::open(&self.path)?;
        let v: Option<i64> = conn
            .query_row(
                "SELECT since_ms FROM p2p_sync_state WHERE actor_id=?1",
                params![actor_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v.unwrap_or(0))
    }

    pub fn set_p2p_sync_since(&self, actor_id: &str, since_ms: i64) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT INTO p2p_sync_state(actor_id, since_ms) VALUES (?1, ?2)\n             ON CONFLICT(actor_id) DO UPDATE SET since_ms=excluded.since_ms",
            params![actor_id, since_ms],
        )?;
        Ok(())
    }


    pub fn new_activity_id(&self, base_actor: &str) -> String {
        let mut b = [0u8; 16];
        OsRng.fill_bytes(&mut b);
        let suffix: String = b.iter().map(|v| format!("{v:02x}")).collect();
        format!("{base_actor}/activities/{suffix}")
    }

    fn store_attachments_from_object_json(&self, object_id: &str, conn: &Connection) -> Result<()> {
        let object_json: Vec<u8> = conn.query_row(
            "SELECT object_json FROM objects WHERE object_id=?1",
            params![object_id],
            |r| r.get(0),
        )?;

        let v: serde_json::Value = match serde_json::from_slice(&object_json) {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };

        let attachments = v.get("attachment").or_else(|| v.get("attachments"));
        let Some(attachments) = attachments else { return Ok(()) };

        let mut list = Vec::<AttachmentRow>::new();
        match attachments {
            serde_json::Value::Array(arr) => {
                for item in arr {
                    list.extend(parse_attachment(item));
                }
            }
            _ => {
                list.extend(parse_attachment(attachments));
            }
        }

        if list.is_empty() {
            return Ok(());
        }

        let tx = conn.unchecked_transaction()?;
        for row in list {
            tx.execute(
                "INSERT OR REPLACE INTO object_attachments(object_id, url, media_type, name, blurhash, width, height) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    object_id,
                    row.url,
                    row.media_type,
                    row.name,
                    row.blurhash,
                    row.width,
                    row.height
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn store_meta_from_object_json(&self, object_id: &str, conn: &Connection) -> Result<()> {
        let object_json: Vec<u8> = conn.query_row(
            "SELECT object_json FROM objects WHERE object_id=?1",
            params![object_id],
            |r| r.get(0),
        )?;

        let v: serde_json::Value = match serde_json::from_slice(&object_json) {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };

        let sensitive = v
            .get("sensitive")
            .and_then(|s| s.as_bool())
            .map(|b| if b { 1 } else { 0 });
        let summary = v.get("summary").and_then(|s| s.as_str()).map(|s| s.to_string());
        if sensitive.is_none() && summary.is_none() {
            return Ok(());
        }
        conn.execute(
            "INSERT INTO object_meta(object_id, sensitive, summary) VALUES (?1, ?2, ?3)\n             ON CONFLICT(object_id) DO UPDATE SET sensitive=excluded.sensitive, summary=excluded.summary",
            params![object_id, sensitive, summary],
        )?;
        Ok(())
    }

    fn store_tags_from_object_json(&self, object_id: &str, conn: &Connection) -> Result<()> {
        let object_json: Vec<u8> = conn.query_row(
            "SELECT object_json FROM objects WHERE object_id=?1",
            params![object_id],
            |r| r.get(0),
        )?;
        let v: serde_json::Value = match serde_json::from_slice(&object_json) {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };

        let tags = v.get("tag");
        let Some(tags) = tags else { return Ok(()) };

        let mut list = Vec::<(String, String, String)>::new();
        match tags {
            serde_json::Value::Array(arr) => {
                for item in arr {
                    if let Some(entry) = parse_tag(item) {
                        list.push((entry.0, entry.1.unwrap_or_default(), entry.2.unwrap_or_default()));
                    }
                }
            }
            _ => {
                if let Some(entry) = parse_tag(tags) {
                    list.push((entry.0, entry.1.unwrap_or_default(), entry.2.unwrap_or_default()));
                }
            }
        }

        if list.is_empty() {
            return Ok(());
        }

        let tx = conn.unchecked_transaction()?;
        for (tag_type, name, href) in list {
            tx.execute(
                "INSERT OR REPLACE INTO object_tags(object_id, tag_type, name, href) VALUES (?1, ?2, ?3, ?4)",
                params![object_id, tag_type, name, href],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn upsert_media(
        &self,
        id: &str,
        url: &str,
        media_type: &str,
        size: i64,
        local_name: Option<&str>,
        actor_id: Option<&str>,
        width: Option<i64>,
        height: Option<i64>,
        blurhash: Option<&str>,
    ) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            r#"
            INSERT OR REPLACE INTO media_items(id, url, media_type, size, created_at_ms, local_name, actor_id, last_access_ms, width, height, blurhash)
            VALUES (?1, ?2, ?3, ?4, COALESCE((SELECT created_at_ms FROM media_items WHERE id=?1), ?5), ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                id,
                url,
                media_type,
                size,
                now_ms(),
                local_name,
                actor_id,
                now_ms(),
                width,
                height,
                blurhash,
            ],
        )?;
        Ok(())
    }

    pub fn get_media(&self, id: &str) -> Result<Option<MediaItem>> {
        let conn = Connection::open(&self.path)?;
        let item = conn
            .query_row(
                "SELECT id, url, media_type, size, created_at_ms, local_name, actor_id, last_access_ms, width, height, blurhash FROM media_items WHERE id=?1",
                params![id],
                |r| {
                    Ok(MediaItem {
                        id: r.get(0)?,
                        url: r.get(1)?,
                        media_type: r.get(2)?,
                        size: r.get(3)?,
                        created_at_ms: r.get(4)?,
                        local_name: r.get(5)?,
                        actor_id: r.get(6)?,
                        last_access_ms: r.get(7)?,
                        width: r.get(8)?,
                        height: r.get(9)?,
                        blurhash: r.get(10)?,
                    })
                },
            )
            .optional()?;
        if item.is_some() {
            let _ = conn.execute(
                "UPDATE media_items SET last_access_ms=?2 WHERE id=?1",
                params![id, now_ms()],
            );
        }
        Ok(item)
    }

    pub fn touch_object(&self, object_id: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        let _ = conn.execute(
            "UPDATE objects SET last_access_ms=?2 WHERE object_id=?1",
            params![object_id, now_ms()],
        )?;
        Ok(())
    }

    pub fn get_object_json(&self, object_id: &str) -> Result<Option<Vec<u8>>> {
        let conn = Connection::open(&self.path)?;
        let json: Option<Vec<u8>> = conn
            .query_row(
                "SELECT object_json FROM objects WHERE object_id=?1",
                params![object_id],
                |r| r.get(0),
            )
            .optional()?;
        if json.is_some() {
            let _ = conn.execute(
                "UPDATE objects SET last_access_ms=?2 WHERE object_id=?1",
                params![object_id, now_ms()],
            );
        }
        Ok(json)
    }

    pub fn delete_media(&self, id: &str) -> Result<u64> {
        let conn = Connection::open(&self.path)?;
        Ok(conn.execute("DELETE FROM media_items WHERE id=?1", params![id])? as u64)
    }

    pub fn prune_media_per_actor_other(&self, max_per_actor: u32, limit: u32) -> Result<Vec<String>> {
        let max_per_actor = max_per_actor.max(1) as i64;
        let limit = limit.max(1) as i64;
        let mut conn = Connection::open(&self.path)?;

        let ids = {
            let mut stmt = conn.prepare(
                r#"
                SELECT id FROM (
                  SELECT m.id,
                         ROW_NUMBER() OVER (PARTITION BY m.actor_id ORDER BY m.created_at_ms DESC) AS rn
                  FROM media_items m
                  LEFT JOIN actor_meta am ON m.actor_id = am.actor_id
                  LEFT JOIN following f ON m.actor_id = f.actor_id
                  WHERE m.actor_id IS NOT NULL
                    AND NOT (COALESCE(am.is_fedi3,0)=1 AND COALESCE(f.status,0)=1)
                )
                WHERE rn > ?1
                LIMIT ?2
                "#,
            )?;
            let rows = stmt.query_map(params![max_per_actor, limit], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut local_names: Vec<String> = Vec::new();
        for id in &ids {
            if let Ok(Some(l)) = conn
                .query_row("SELECT local_name FROM media_items WHERE id=?1", params![id], |r| r.get::<_, Option<String>>(0))
                .optional()
            {
                if let Some(n) = l {
                    local_names.push(n);
                }
            }
        }

        let tx = conn.transaction()?;
        for id in &ids {
            let _ = tx.execute("DELETE FROM media_items WHERE id=?1", params![id])?;
        }
        tx.commit()?;
        Ok(local_names)
    }

    pub fn prune_media_per_actor_followed_fedi3(&self, max_per_actor: u32, limit: u32) -> Result<Vec<String>> {
        let max_per_actor = max_per_actor.max(1) as i64;
        let limit = limit.max(1) as i64;
        let mut conn = Connection::open(&self.path)?;

        let ids = {
            let mut stmt = conn.prepare(
                r#"
                SELECT id FROM (
                  SELECT m.id,
                         ROW_NUMBER() OVER (PARTITION BY m.actor_id ORDER BY m.created_at_ms DESC) AS rn
                  FROM media_items m
                  JOIN actor_meta am ON m.actor_id = am.actor_id
                  JOIN following f ON m.actor_id = f.actor_id
                  WHERE m.actor_id IS NOT NULL
                    AND am.is_fedi3=1
                    AND f.status=1
                )
                WHERE rn > ?1
                LIMIT ?2
                "#,
            )?;
            let rows = stmt.query_map(params![max_per_actor, limit], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut local_names: Vec<String> = Vec::new();
        for id in &ids {
            if let Ok(Some(l)) = conn
                .query_row("SELECT local_name FROM media_items WHERE id=?1", params![id], |r| r.get::<_, Option<String>>(0))
                .optional()
            {
                if let Some(n) = l {
                    local_names.push(n);
                }
            }
        }

        let tx = conn.transaction()?;
        for id in &ids {
            let _ = tx.execute("DELETE FROM media_items WHERE id=?1", params![id])?;
        }
        tx.commit()?;
        Ok(local_names)
    }

    pub fn prune_inbox_items(&self, max_items: u32) -> Result<u64> {
        let conn = Connection::open(&self.path)?;
        let max_items = max_items.max(1) as i64;
        let deleted = conn.execute(
            r#"
            DELETE FROM inbox_items
            WHERE activity_id IN (
              SELECT activity_id FROM inbox_items
              ORDER BY created_at_ms DESC
              LIMIT -1 OFFSET ?1
            )
            "#,
            params![max_items],
        )?;
        Ok(deleted as u64)
    }

    pub fn prune_inbox_seen_before(&self, cutoff_ms: i64) -> Result<u64> {
        let conn = Connection::open(&self.path)?;
        Ok(conn.execute("DELETE FROM inbox_seen WHERE seen_at_ms < ?1", params![cutoff_ms])? as u64)
    }

    pub fn prune_objects_before(&self, cutoff_ms: i64, limit: u32) -> Result<u64> {
        let mut conn = Connection::open(&self.path)?;
        let tx = conn.transaction()?;
        let ids = {
            let mut stmt = tx.prepare(
                r#"
                SELECT object_id FROM objects
                WHERE updated_at_ms < ?1 AND COALESCE(pinned,0)=0
                LIMIT ?2
                "#,
            )?;
            let rows = stmt.query_map(params![cutoff_ms, limit], |r| r.get::<_, String>(0))?;
            let ids = rows.collect::<rusqlite::Result<Vec<_>>>()?;
            ids
        };
        if ids.is_empty() {
            return Ok(0);
        }
        for id in &ids {
            let _ = tx.execute("DELETE FROM object_attachments WHERE object_id=?1", params![id])?;
            let _ = tx.execute("DELETE FROM object_meta WHERE object_id=?1", params![id])?;
            let _ = tx.execute("DELETE FROM object_tags WHERE object_id=?1", params![id])?;
            let _ = tx.execute("DELETE FROM objects WHERE object_id=?1", params![id])?;
        }
        tx.commit()?;
        Ok(ids.len() as u64)
    }

    pub fn prune_objects_other_before(&self, cutoff_ms: i64, limit: u32) -> Result<u64> {
        let mut conn = Connection::open(&self.path)?;
        let tx = conn.transaction()?;
        let ids = {
            let mut stmt = tx.prepare(
                r#"
                SELECT o.object_id
                FROM objects o
                LEFT JOIN actor_meta am ON o.actor_id = am.actor_id
                LEFT JOIN following f ON o.actor_id = f.actor_id
                WHERE o.updated_at_ms < ?1
                  AND COALESCE(o.pinned,0)=0
                  AND NOT (COALESCE(am.is_fedi3,0)=1 AND COALESCE(f.status,0)=1)
                LIMIT ?2
                "#,
            )?;
            let rows = stmt.query_map(params![cutoff_ms, limit], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        if ids.is_empty() {
            return Ok(0);
        }
        for id in &ids {
            let _ = tx.execute("DELETE FROM object_attachments WHERE object_id=?1", params![id])?;
            let _ = tx.execute("DELETE FROM object_meta WHERE object_id=?1", params![id])?;
            let _ = tx.execute("DELETE FROM object_tags WHERE object_id=?1", params![id])?;
            let _ = tx.execute("DELETE FROM objects WHERE object_id=?1", params![id])?;
        }
        tx.commit()?;
        Ok(ids.len() as u64)
    }

    pub fn prune_objects_followed_fedi3_before(&self, cutoff_ms: i64, limit: u32) -> Result<u64> {
        let mut conn = Connection::open(&self.path)?;
        let tx = conn.transaction()?;
        let ids = {
            let mut stmt = tx.prepare(
                r#"
                SELECT o.object_id
                FROM objects o
                JOIN actor_meta am ON o.actor_id = am.actor_id
                JOIN following f ON o.actor_id = f.actor_id
                WHERE o.updated_at_ms < ?1
                  AND COALESCE(o.pinned,0)=0
                  AND am.is_fedi3=1
                  AND f.status=1
                LIMIT ?2
                "#,
            )?;
            let rows = stmt.query_map(params![cutoff_ms, limit], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        if ids.is_empty() {
            return Ok(0);
        }
        for id in &ids {
            let _ = tx.execute("DELETE FROM object_attachments WHERE object_id=?1", params![id])?;
            let _ = tx.execute("DELETE FROM object_meta WHERE object_id=?1", params![id])?;
            let _ = tx.execute("DELETE FROM object_tags WHERE object_id=?1", params![id])?;
            let _ = tx.execute("DELETE FROM objects WHERE object_id=?1", params![id])?;
        }
        tx.commit()?;
        Ok(ids.len() as u64)
    }

    pub fn prune_objects_per_actor_other(&self, max_per_actor: u32, limit: u32) -> Result<u64> {
        let max_per_actor = max_per_actor.max(1) as i64;
        let limit = limit.max(1) as i64;
        let mut conn = Connection::open(&self.path)?;
        let tx = conn.transaction()?;
        let ids = {
            let mut stmt = tx.prepare(
                r#"
                SELECT object_id FROM (
                  SELECT o.object_id,
                         ROW_NUMBER() OVER (PARTITION BY o.actor_id ORDER BY o.updated_at_ms DESC) AS rn
                  FROM objects o
                  LEFT JOIN actor_meta am ON o.actor_id = am.actor_id
                  LEFT JOIN following f ON o.actor_id = f.actor_id
                  WHERE o.actor_id IS NOT NULL
                    AND COALESCE(o.pinned,0)=0
                    AND NOT (COALESCE(am.is_fedi3,0)=1 AND COALESCE(f.status,0)=1)
                )
                WHERE rn > ?1
                LIMIT ?2
                "#,
            )?;
            let rows = stmt.query_map(params![max_per_actor, limit], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        if ids.is_empty() {
            return Ok(0);
        }
        for id in &ids {
            let _ = tx.execute("DELETE FROM object_attachments WHERE object_id=?1", params![id])?;
            let _ = tx.execute("DELETE FROM object_meta WHERE object_id=?1", params![id])?;
            let _ = tx.execute("DELETE FROM object_tags WHERE object_id=?1", params![id])?;
            let _ = tx.execute("DELETE FROM objects WHERE object_id=?1", params![id])?;
        }
        tx.commit()?;
        Ok(ids.len() as u64)
    }

    pub fn prune_objects_per_actor_followed_fedi3(&self, max_per_actor: u32, limit: u32) -> Result<u64> {
        let max_per_actor = max_per_actor.max(1) as i64;
        let limit = limit.max(1) as i64;
        let mut conn = Connection::open(&self.path)?;
        let tx = conn.transaction()?;
        let ids = {
            let mut stmt = tx.prepare(
                r#"
                SELECT object_id FROM (
                  SELECT o.object_id,
                         ROW_NUMBER() OVER (PARTITION BY o.actor_id ORDER BY o.updated_at_ms DESC) AS rn
                  FROM objects o
                  JOIN actor_meta am ON o.actor_id = am.actor_id
                  JOIN following f ON o.actor_id = f.actor_id
                  WHERE o.actor_id IS NOT NULL
                    AND COALESCE(o.pinned,0)=0
                    AND am.is_fedi3=1
                    AND f.status=1
                )
                WHERE rn > ?1
                LIMIT ?2
                "#,
            )?;
            let rows = stmt.query_map(params![max_per_actor, limit], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        if ids.is_empty() {
            return Ok(0);
        }
        for id in &ids {
            let _ = tx.execute("DELETE FROM object_attachments WHERE object_id=?1", params![id])?;
            let _ = tx.execute("DELETE FROM object_meta WHERE object_id=?1", params![id])?;
            let _ = tx.execute("DELETE FROM object_tags WHERE object_id=?1", params![id])?;
            let _ = tx.execute("DELETE FROM objects WHERE object_id=?1", params![id])?;
        }
        tx.commit()?;
        Ok(ids.len() as u64)
    }

    pub fn prune_object_bytes_per_actor_other(&self, max_bytes: u64, max_actors: u32, max_deletes: u32) -> Result<u64> {
        let max_actors = max_actors.max(1) as i64;
        let max_deletes = max_deletes.max(1) as i64;
        let mut conn = Connection::open(&self.path)?;

        // Pick actors that exceed the budget (best-effort).
        let actor_ids: Vec<String> = {
            let mut stmt = conn.prepare(
                r#"
                SELECT o.actor_id
                FROM objects o
                LEFT JOIN actor_meta am ON o.actor_id = am.actor_id
                LEFT JOIN following f ON o.actor_id = f.actor_id
                WHERE o.actor_id IS NOT NULL AND COALESCE(o.pinned,0)=0
                  AND NOT (COALESCE(am.is_fedi3,0)=1 AND COALESCE(f.status,0)=1)
                GROUP BY o.actor_id
                HAVING SUM(o.size_bytes) > ?1
                LIMIT ?2
                "#,
            )?;
            let rows = stmt.query_map(params![max_bytes as i64, max_actors], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

        if actor_ids.is_empty() {
            return Ok(0);
        }

        let tx = conn.transaction()?;
        let mut deleted: u64 = 0;
        for actor_id in actor_ids {
            let mut total: i64 = tx
                .query_row(
                    "SELECT COALESCE(SUM(size_bytes),0) FROM objects WHERE actor_id=?1 AND COALESCE(pinned,0)=0",
                    params![actor_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if total as u64 <= max_bytes {
                continue;
            }

            let mut stmt = tx.prepare(
                r#"
                SELECT object_id, size_bytes
                FROM objects
                WHERE actor_id=?1 AND COALESCE(pinned,0)=0
                ORDER BY last_access_ms ASC, updated_at_ms ASC
                LIMIT ?2
                "#,
            )?;
            let rows = stmt
                .query_map(params![actor_id, max_deletes], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            for (obj_id, sz) in rows {
                if total as u64 <= max_bytes {
                    break;
                }
                let _ = tx.execute("DELETE FROM object_attachments WHERE object_id=?1", params![obj_id])?;
                let _ = tx.execute("DELETE FROM object_meta WHERE object_id=?1", params![obj_id])?;
                let _ = tx.execute("DELETE FROM object_tags WHERE object_id=?1", params![obj_id])?;
                let _ = tx.execute("DELETE FROM objects WHERE object_id=?1", params![obj_id])?;
                deleted += 1;
                total = total.saturating_sub(sz.max(0));
            }
        }
        tx.commit()?;
        Ok(deleted)
    }

    pub fn prune_object_bytes_per_actor_followed_fedi3(&self, max_bytes: u64, max_actors: u32, max_deletes: u32) -> Result<u64> {
        let max_actors = max_actors.max(1) as i64;
        let max_deletes = max_deletes.max(1) as i64;
        let mut conn = Connection::open(&self.path)?;

        let actor_ids: Vec<String> = {
            let mut stmt = conn.prepare(
                r#"
                SELECT o.actor_id
                FROM objects o
                JOIN actor_meta am ON o.actor_id = am.actor_id
                JOIN following f ON o.actor_id = f.actor_id
                WHERE o.actor_id IS NOT NULL AND COALESCE(o.pinned,0)=0
                  AND am.is_fedi3=1 AND f.status=1
                GROUP BY o.actor_id
                HAVING SUM(o.size_bytes) > ?1
                LIMIT ?2
                "#,
            )?;
            let rows = stmt.query_map(params![max_bytes as i64, max_actors], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

        if actor_ids.is_empty() {
            return Ok(0);
        }

        let tx = conn.transaction()?;
        let mut deleted: u64 = 0;
        for actor_id in actor_ids {
            let mut total: i64 = tx
                .query_row(
                    "SELECT COALESCE(SUM(size_bytes),0) FROM objects WHERE actor_id=?1 AND COALESCE(pinned,0)=0",
                    params![actor_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if total as u64 <= max_bytes {
                continue;
            }

            let mut stmt = tx.prepare(
                r#"
                SELECT object_id, size_bytes
                FROM objects
                WHERE actor_id=?1 AND COALESCE(pinned,0)=0
                ORDER BY last_access_ms ASC, updated_at_ms ASC
                LIMIT ?2
                "#,
            )?;
            let rows = stmt
                .query_map(params![actor_id, max_deletes], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            for (obj_id, sz) in rows {
                if total as u64 <= max_bytes {
                    break;
                }
                let _ = tx.execute("DELETE FROM object_attachments WHERE object_id=?1", params![obj_id])?;
                let _ = tx.execute("DELETE FROM object_meta WHERE object_id=?1", params![obj_id])?;
                let _ = tx.execute("DELETE FROM object_tags WHERE object_id=?1", params![obj_id])?;
                let _ = tx.execute("DELETE FROM objects WHERE object_id=?1", params![obj_id])?;
                deleted += 1;
                total = total.saturating_sub(sz.max(0));
            }
        }
        tx.commit()?;
        Ok(deleted)
    }

    pub fn prune_media_bytes_per_actor_other(&self, max_bytes: u64, max_actors: u32, max_deletes: u32) -> Result<Vec<String>> {
        let max_actors = max_actors.max(1) as i64;
        let max_deletes = max_deletes.max(1) as i64;
        let mut conn = Connection::open(&self.path)?;

        let actor_ids: Vec<String> = {
            let mut stmt = conn.prepare(
                r#"
                SELECT m.actor_id
                FROM media_items m
                LEFT JOIN actor_meta am ON m.actor_id = am.actor_id
                LEFT JOIN following f ON m.actor_id = f.actor_id
                WHERE m.actor_id IS NOT NULL AND m.local_name IS NOT NULL
                  AND NOT (COALESCE(am.is_fedi3,0)=1 AND COALESCE(f.status,0)=1)
                GROUP BY m.actor_id
                HAVING SUM(m.size) > ?1
                LIMIT ?2
                "#,
            )?;
            let rows = stmt.query_map(params![max_bytes as i64, max_actors], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        if actor_ids.is_empty() {
            return Ok(Vec::new());
        }

        let tx = conn.transaction()?;
        let mut local_names: Vec<String> = Vec::new();
        for actor_id in actor_ids {
            let mut total: i64 = tx
                .query_row(
                    "SELECT COALESCE(SUM(size),0) FROM media_items WHERE actor_id=?1 AND local_name IS NOT NULL",
                    params![actor_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if total as u64 <= max_bytes {
                continue;
            }

            let mut stmt = tx.prepare(
                r#"
                SELECT id, size, local_name
                FROM media_items
                WHERE actor_id=?1 AND local_name IS NOT NULL
                ORDER BY last_access_ms ASC, created_at_ms ASC
                LIMIT ?2
                "#,
            )?;
            let rows = stmt
                .query_map(params![actor_id, max_deletes], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            for (id, sz, local) in rows {
                if total as u64 <= max_bytes {
                    break;
                }
                let _ = tx.execute("DELETE FROM media_items WHERE id=?1", params![id])?;
                local_names.push(local);
                total = total.saturating_sub(sz.max(0));
            }
        }
        tx.commit()?;
        Ok(local_names)
    }

    pub fn prune_media_bytes_per_actor_followed_fedi3(&self, max_bytes: u64, max_actors: u32, max_deletes: u32) -> Result<Vec<String>> {
        let max_actors = max_actors.max(1) as i64;
        let max_deletes = max_deletes.max(1) as i64;
        let mut conn = Connection::open(&self.path)?;

        let actor_ids: Vec<String> = {
            let mut stmt = conn.prepare(
                r#"
                SELECT m.actor_id
                FROM media_items m
                JOIN actor_meta am ON m.actor_id = am.actor_id
                JOIN following f ON m.actor_id = f.actor_id
                WHERE m.actor_id IS NOT NULL AND m.local_name IS NOT NULL
                  AND am.is_fedi3=1 AND f.status=1
                GROUP BY m.actor_id
                HAVING SUM(m.size) > ?1
                LIMIT ?2
                "#,
            )?;
            let rows = stmt.query_map(params![max_bytes as i64, max_actors], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        if actor_ids.is_empty() {
            return Ok(Vec::new());
        }

        let tx = conn.transaction()?;
        let mut local_names: Vec<String> = Vec::new();
        for actor_id in actor_ids {
            let mut total: i64 = tx
                .query_row(
                    "SELECT COALESCE(SUM(size),0) FROM media_items WHERE actor_id=?1 AND local_name IS NOT NULL",
                    params![actor_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if total as u64 <= max_bytes {
                continue;
            }

            let mut stmt = tx.prepare(
                r#"
                SELECT id, size, local_name
                FROM media_items
                WHERE actor_id=?1 AND local_name IS NOT NULL
                ORDER BY last_access_ms ASC, created_at_ms ASC
                LIMIT ?2
                "#,
            )?;
            let rows = stmt
                .query_map(params![actor_id, max_deletes], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            for (id, sz, local) in rows {
                if total as u64 <= max_bytes {
                    break;
                }
                let _ = tx.execute("DELETE FROM media_items WHERE id=?1", params![id])?;
                local_names.push(local);
                total = total.saturating_sub(sz.max(0));
            }
        }
        tx.commit()?;
        Ok(local_names)
    }

    pub fn get_chat_identity_key(&self) -> Result<Option<(String, String)>> {
        let conn = Connection::open(&self.path)?;
        conn.query_row(
            "SELECT public_b64, secret_b64 FROM chat_identity_keys ORDER BY created_at_ms DESC LIMIT 1",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn set_chat_identity_key(&self, public_b64: &str, secret_b64: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT OR REPLACE INTO chat_identity_keys (key_id, public_b64, secret_b64, created_at_ms) VALUES (?1, ?2, ?3, ?4)",
            params!["identity", public_b64, secret_b64, now_ms()],
        )?;
        Ok(())
    }

    pub fn insert_chat_prekey(&self, key_id: &str, public_b64: &str, secret_b64: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT OR REPLACE INTO chat_prekeys (key_id, public_b64, secret_b64, created_at_ms, used_at_ms) VALUES (?1, ?2, ?3, ?4, NULL)",
            params![key_id, public_b64, secret_b64, now_ms()],
        )?;
        Ok(())
    }

    pub fn count_unused_chat_prekeys(&self) -> Result<u32> {
        let conn = Connection::open(&self.path)?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM chat_prekeys WHERE used_at_ms IS NULL", [], |r| r.get(0))?;
        Ok(count.max(0) as u32)
    }

    pub fn list_chat_prekeys(&self, limit: u32) -> Result<Vec<(String, String, String)>> {
        let conn = Connection::open(&self.path)?;
        let limit = limit.min(200).max(1) as i64;
        let mut stmt = conn.prepare(
            "SELECT key_id, public_b64, secret_b64 FROM chat_prekeys WHERE used_at_ms IS NULL ORDER BY created_at_ms ASC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn get_chat_prekey_secret(&self, key_id: &str) -> Result<Option<String>> {
        let conn = Connection::open(&self.path)?;
        conn.query_row(
            "SELECT secret_b64 FROM chat_prekeys WHERE key_id=?1",
            params![key_id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn mark_chat_prekey_used(&self, key_id: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "UPDATE chat_prekeys SET used_at_ms=?2 WHERE key_id=?1 AND used_at_ms IS NULL",
            params![key_id, now_ms()],
        )?;
        Ok(())
    }

    pub fn create_chat_thread(&self, thread_id: &str, kind: &str, title: Option<&str>) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        let now = now_ms();
        conn.execute(
            "INSERT OR IGNORE INTO chat_threads (thread_id, kind, title, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?4)",
            params![thread_id, kind, title, now],
        )?;
        if let Some(t) = title {
            conn.execute(
                "UPDATE chat_threads SET title=?2, updated_at_ms=?3 WHERE thread_id=?1",
                params![thread_id, t, now],
            )?;
        } else {
            conn.execute(
                "UPDATE chat_threads SET updated_at_ms=?2 WHERE thread_id=?1",
                params![thread_id, now],
            )?;
        }
        Ok(())
    }

    pub fn touch_chat_thread(&self, thread_id: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "UPDATE chat_threads SET updated_at_ms=?2 WHERE thread_id=?1",
            params![thread_id, now_ms()],
        )?;
        Ok(())
    }

    pub fn upsert_chat_member(&self, thread_id: &str, actor_id: &str, role: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        let now = now_ms();
        conn.execute(
            r#"
            INSERT INTO chat_members (thread_id, actor_id, role, added_at_ms)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(thread_id, actor_id)
            DO UPDATE SET role=excluded.role
            "#,
            params![thread_id, actor_id, role, now],
        )?;
        Ok(())
    }

    pub fn remove_chat_member(&self, thread_id: &str, actor_id: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "DELETE FROM chat_members WHERE thread_id=?1 AND actor_id=?2",
            params![thread_id, actor_id],
        )?;
        Ok(())
    }

    pub fn set_chat_members(&self, thread_id: &str, members: &[String]) -> Result<()> {
        let mut conn = Connection::open(&self.path)?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM chat_members WHERE thread_id=?1", params![thread_id])?;
        let now = now_ms();
        for actor in members {
            if actor.trim().is_empty() {
                continue;
            }
            tx.execute(
                "INSERT OR REPLACE INTO chat_members (thread_id, actor_id, role, added_at_ms) VALUES (?1, ?2, ?3, ?4)",
                params![thread_id, actor, "member", now],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_chat_members(&self, thread_id: &str) -> Result<Vec<(String, String)>> {
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn.prepare(
            "SELECT actor_id, role FROM chat_members WHERE thread_id=?1 ORDER BY added_at_ms ASC",
        )?;
        let rows = stmt.query_map(params![thread_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn update_chat_thread_title(&self, thread_id: &str, title: Option<&str>) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "UPDATE chat_threads SET title=?2, updated_at_ms=?3 WHERE thread_id=?1",
            params![thread_id, title, now_ms()],
        )?;
        Ok(())
    }

    pub fn list_chat_threads(&self, limit: u32, cursor: Option<i64>) -> Result<CollectionPage<ChatThread>> {
        let conn = Connection::open(&self.path)?;
        let total: u64 = conn.query_row("SELECT COUNT(*) FROM chat_threads", [], |r| r.get(0))?;
        let limit = limit.min(200).max(1);
        let (sql, params_vec): (String, Vec<rusqlite::types::Value>) = if let Some(c) = cursor {
            (
                r#"
                SELECT t.thread_id, t.kind, t.title, t.created_at_ms, t.updated_at_ms,
                       m.created_at_ms, m.body_json
                FROM chat_threads t
                LEFT JOIN chat_messages m ON m.message_id = (
                    SELECT message_id FROM chat_messages WHERE thread_id=t.thread_id ORDER BY created_at_ms DESC LIMIT 1
                )
                WHERE t.updated_at_ms < ?1
                ORDER BY t.updated_at_ms DESC
                LIMIT ?2
                "#
                .to_string(),
                vec![c.into(), (limit as i64).into()],
            )
        } else {
            (
                r#"
                SELECT t.thread_id, t.kind, t.title, t.created_at_ms, t.updated_at_ms,
                       m.created_at_ms, m.body_json
                FROM chat_threads t
                LEFT JOIN chat_messages m ON m.message_id = (
                    SELECT message_id FROM chat_messages WHERE thread_id=t.thread_id ORDER BY created_at_ms DESC LIMIT 1
                )
                ORDER BY t.updated_at_ms DESC
                LIMIT ?1
                "#
                .to_string(),
                vec![(limit as i64).into()],
            )
        };
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params_vec))?;
        let mut items = Vec::new();
        let mut last_updated: Option<i64> = None;
        while let Some(row) = rows.next()? {
            let thread_id: String = row.get(0)?;
            let kind: String = row.get(1)?;
            let title: Option<String> = row.get(2)?;
            let created_at_ms: i64 = row.get(3)?;
            let updated_at_ms: i64 = row.get(4)?;
            let last_message_ms: Option<i64> = row.get(5)?;
            let last_body: Option<String> = row.get(6)?;
            last_updated = Some(updated_at_ms);
            let preview = last_body
                .and_then(|body| serde_json::from_str::<serde_json::Value>(&body).ok())
                .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(|s| s.to_string()));
            items.push(ChatThread {
                thread_id,
                kind,
                title,
                created_at_ms,
                updated_at_ms,
                last_message_ms,
                last_message_preview: preview,
            });
        }
        Ok(CollectionPage {
            total,
            items,
            next: last_updated.map(|v| v.to_string()),
        })
    }

    pub fn insert_chat_message(&self, msg: &ChatMessage) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            r#"
            INSERT OR IGNORE INTO chat_messages
              (message_id, thread_id, sender_actor, sender_device, created_at_ms, edited_at_ms, deleted, body_json)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                msg.message_id,
                msg.thread_id,
                msg.sender_actor,
                msg.sender_device,
                msg.created_at_ms,
                msg.edited_at_ms,
                if msg.deleted { 1 } else { 0 },
                msg.body_json
            ],
        )?;
        Ok(())
    }

    pub fn list_chat_messages(&self, thread_id: &str, limit: u32, cursor: Option<i64>) -> Result<CollectionPage<ChatMessage>> {
        let conn = Connection::open(&self.path)?;
        let total: u64 = conn.query_row(
            "SELECT COUNT(*) FROM chat_messages WHERE thread_id=?1",
            params![thread_id],
            |r| r.get(0),
        )?;
        let limit = limit.min(200).max(1);
        let (sql, params_vec): (String, Vec<rusqlite::types::Value>) = if let Some(c) = cursor {
            (
                "SELECT message_id, thread_id, sender_actor, sender_device, created_at_ms, edited_at_ms, deleted, body_json FROM chat_messages WHERE thread_id=?1 AND created_at_ms < ?2 ORDER BY created_at_ms DESC LIMIT ?3".to_string(),
                vec![thread_id.to_string().into(), c.into(), (limit as i64).into()],
            )
        } else {
            (
                "SELECT message_id, thread_id, sender_actor, sender_device, created_at_ms, edited_at_ms, deleted, body_json FROM chat_messages WHERE thread_id=?1 ORDER BY created_at_ms DESC LIMIT ?2".to_string(),
                vec![thread_id.to_string().into(), (limit as i64).into()],
            )
        };
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params_vec))?;
        let mut items = Vec::new();
        let mut last_created: Option<i64> = None;
        while let Some(row) = rows.next()? {
            let message_id: String = row.get(0)?;
            let thread_id: String = row.get(1)?;
            let sender_actor: String = row.get(2)?;
            let sender_device: String = row.get(3)?;
            let created_at_ms: i64 = row.get(4)?;
            let edited_at_ms: Option<i64> = row.get(5)?;
            let deleted: i64 = row.get(6)?;
            let body_json: String = row.get(7)?;
            last_created = Some(created_at_ms);
            items.push(ChatMessage {
                message_id,
                thread_id,
                sender_actor,
                sender_device,
                created_at_ms,
                edited_at_ms,
                deleted: deleted != 0,
                body_json,
            });
        }
        Ok(CollectionPage {
            total,
            items,
            next: last_created.map(|v| v.to_string()),
        })
    }

    pub fn update_chat_message_edit(&self, message_id: &str, body_json: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "UPDATE chat_messages SET body_json=?2, edited_at_ms=?3 WHERE message_id=?1",
            params![message_id, body_json, now_ms()],
        )?;
        Ok(())
    }

    pub fn mark_chat_message_deleted(&self, message_id: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "UPDATE chat_messages SET deleted=1, edited_at_ms=?2 WHERE message_id=?1",
            params![message_id, now_ms()],
        )?;
        Ok(())
    }

    pub fn upsert_chat_message_status(&self, message_id: &str, actor_id: &str, status: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            r#"
            INSERT INTO chat_message_status (message_id, actor_id, status, updated_at_ms)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(message_id, actor_id)
            DO UPDATE SET status=excluded.status, updated_at_ms=excluded.updated_at_ms
            "#,
            params![message_id, actor_id, status, now_ms()],
        )?;
        Ok(())
    }

    pub fn list_chat_message_statuses(&self, message_ids: &[String]) -> Result<Vec<(String, String, String, i64)>> {
        if message_ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = Connection::open(&self.path)?;
        let placeholders = message_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT message_id, actor_id, status, updated_at_ms FROM chat_message_status WHERE message_id IN ({})",
            placeholders
        );
        let params_vec = message_ids
            .iter()
            .cloned()
            .map(|v| rusqlite::types::Value::from(v))
            .collect::<Vec<_>>();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params_vec), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn upsert_relay_entry(&self, base_url: &str, ws_url: Option<&str>, source: &str) -> Result<()> {
        let base = base_url.trim().trim_end_matches('/').to_string();
        if base.is_empty() {
            return Ok(());
        }
        let ws = ws_url.map(|s| s.trim().trim_end_matches('/').to_string());
        let src = source.trim();
        let conn = Connection::open(&self.path)?;
        conn.execute(
            r#"
            INSERT INTO relay_registry (relay_base_url, relay_ws_url, last_seen_ms, source)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(relay_base_url) DO UPDATE SET
              relay_ws_url=excluded.relay_ws_url,
              last_seen_ms=excluded.last_seen_ms,
              source=excluded.source
            "#,
            params![base, ws, now_ms(), src],
        )?;
        Ok(())
    }

    pub fn list_relay_entries(&self, limit: u32) -> Result<Vec<RelayEntry>> {
        let conn = Connection::open(&self.path)?;
        let limit = limit.max(1).min(1000) as i64;
        let mut stmt = conn.prepare(
            "SELECT relay_base_url, relay_ws_url, last_seen_ms, source FROM relay_registry ORDER BY last_seen_ms DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok(RelayEntry {
                relay_base_url: r.get::<_, String>(0)?,
                relay_ws_url: r.get::<_, Option<String>>(1)?,
                last_seen_ms: r.get::<_, i64>(2)?,
                source: r.get::<_, String>(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn remove_relay_entry(&self, base_url: &str) -> Result<()> {
        let base = base_url.trim().trim_end_matches('/');
        if base.is_empty() {
            return Ok(());
        }
        let conn = Connection::open(&self.path)?;
        conn.execute("DELETE FROM relay_registry WHERE relay_base_url=?1", params![base])?;
        Ok(())
    }

    pub fn list_actor_meta_fedi3(&self, limit: u32) -> Result<Vec<String>> {
        let conn = Connection::open(&self.path)?;
        let limit = limit.max(1).min(1000) as i64;
        let mut stmt = conn.prepare(
            "SELECT actor_id FROM actor_meta WHERE is_fedi3=1 ORDER BY last_seen_ms DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn get_chat_message_thread_id(&self, message_id: &str) -> Result<Option<String>> {
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn.prepare("SELECT thread_id FROM chat_messages WHERE message_id=?1")?;
        let mut rows = stmt.query(params![message_id])?;
        if let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            Ok(Some(id))
        } else {
            Ok(None)
        }
    }

    pub fn get_chat_message_meta(&self, message_id: &str) -> Result<Option<(String, bool)>> {
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn.prepare("SELECT sender_actor, deleted FROM chat_messages WHERE message_id=?1")?;
        let mut rows = stmt.query(params![message_id])?;
        if let Some(row) = rows.next()? {
            let sender: String = row.get(0)?;
            let deleted: bool = row.get::<_, i64>(1)? != 0;
            Ok(Some((sender, deleted)))
        } else {
            Ok(None)
        }
    }

    pub fn chat_message_seen_by_others(&self, message_id: &str, self_actor: &str) -> Result<bool> {
        let conn = Connection::open(&self.path)?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM chat_message_status WHERE message_id=?1 AND status='seen' AND actor_id != ?2",
            params![message_id, self_actor],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn get_chat_member_role(&self, thread_id: &str, actor_id: &str) -> Result<Option<String>> {
        let conn = Connection::open(&self.path)?;
        conn.query_row(
            "SELECT role FROM chat_members WHERE thread_id=?1 AND actor_id=?2",
            params![thread_id, actor_id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn delete_chat_thread(&self, thread_id: &str) -> Result<()> {
        let mut conn = Connection::open(&self.path)?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM chat_message_status WHERE message_id IN (SELECT message_id FROM chat_messages WHERE thread_id=?1)",
            params![thread_id],
        )?;
        tx.execute(
            "DELETE FROM chat_message_reactions WHERE message_id IN (SELECT message_id FROM chat_messages WHERE thread_id=?1)",
            params![thread_id],
        )?;
        tx.execute("DELETE FROM chat_messages WHERE thread_id=?1", params![thread_id])?;
        tx.execute("DELETE FROM chat_members WHERE thread_id=?1", params![thread_id])?;
        tx.execute("DELETE FROM chat_threads WHERE thread_id=?1", params![thread_id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn add_chat_reaction(&self, message_id: &str, actor_id: &str, reaction: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT OR REPLACE INTO chat_message_reactions (message_id, actor_id, reaction, created_at_ms) VALUES (?1, ?2, ?3, ?4)",
            params![message_id, actor_id, reaction, now_ms()],
        )?;
        Ok(())
    }

    pub fn remove_chat_reaction(&self, message_id: &str, actor_id: &str, reaction: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "DELETE FROM chat_message_reactions WHERE message_id=?1 AND actor_id=?2 AND reaction=?3",
            params![message_id, actor_id, reaction],
        )?;
        Ok(())
    }

    pub fn list_chat_reactions(&self, message_ids: &[String], self_actor: &str) -> Result<Vec<(String, String, i64, bool)>> {
        if message_ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = Connection::open(&self.path)?;
        let placeholders = (0..message_ids.len())
            .map(|idx| format!("?{}", idx + 1))
            .collect::<Vec<_>>()
            .join(",");
        let mut params_vec = Vec::with_capacity(message_ids.len() + 1);
        for id in message_ids {
            params_vec.push(rusqlite::types::Value::from(id.clone()));
        }
        params_vec.push(rusqlite::types::Value::from(self_actor.to_string()));
        let sql = format!(
            "SELECT message_id, reaction, COUNT(*) as cnt, SUM(CASE WHEN actor_id=?{} THEN 1 ELSE 0 END) as me
             FROM chat_message_reactions
             WHERE message_id IN ({})
             GROUP BY message_id, reaction",
            message_ids.len() + 1,
            placeholders
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params_vec), |r| {
            let me_count: i64 = r.get(3)?;
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                me_count > 0,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum FollowingStatus {
    Pending = 0,
    Accepted = 1,
}

#[derive(Debug, Clone)]
pub struct ObjectFetchJob {
    pub object_url: String,
    pub attempt: u32,
}

#[derive(Debug, Clone)]
struct AttachmentRow {
    url: String,
    media_type: Option<String>,
    name: Option<String>,
    blurhash: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
}

fn parse_attachment(v: &serde_json::Value) -> Vec<AttachmentRow> {
    match v {
        serde_json::Value::String(url) => vec![AttachmentRow {
            url: url.clone(),
            media_type: None,
            name: None,
            blurhash: None,
            width: None,
            height: None,
        }],
        serde_json::Value::Object(map) => {
            let attachment_media_type = map
                .get("mediaType")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string());
            let name = map.get("name").and_then(|n| n.as_str()).map(|s| s.to_string());
            let blurhash = map
                .get("blurhash")
                .and_then(|b| b.as_str())
                .map(|s| s.to_string());
            let width = map.get("width").and_then(|w| w.as_i64());
            let height = map.get("height").and_then(|h| h.as_i64());

            let Some(url_val) = map.get("url").or_else(|| map.get("href")) else { return vec![] };
            parse_url_field(url_val, &attachment_media_type, &name, &blurhash, width, height)
        }
        _ => vec![],
    }
}

fn is_public_activity_value(activity: &serde_json::Value) -> bool {
    fn has_public(v: &serde_json::Value) -> bool {
        match v {
            serde_json::Value::String(s) => s == "https://www.w3.org/ns/activitystreams#Public",
            serde_json::Value::Array(arr) => arr.iter().any(has_public),
            _ => false,
        }
    }
    activity.get("to").map(has_public).unwrap_or(false) || activity.get("cc").map(has_public).unwrap_or(false)
}

fn parse_url_field(
    v: &serde_json::Value,
    default_media_type: &Option<String>,
    default_name: &Option<String>,
    default_blurhash: &Option<String>,
    default_width: Option<i64>,
    default_height: Option<i64>,
) -> Vec<AttachmentRow> {
    match v {
        serde_json::Value::String(url) => vec![AttachmentRow {
            url: url.clone(),
            media_type: default_media_type.clone(),
            name: default_name.clone(),
            blurhash: default_blurhash.clone(),
            width: default_width,
            height: default_height,
        }],
        serde_json::Value::Object(map) => {
            let url = map
                .get("href")
                .and_then(|u| u.as_str())
                .or_else(|| map.get("url").and_then(|u| u.as_str()))
                .map(|s| s.to_string());
            let Some(url) = url else { return vec![] };

            let media_type = map
                .get("mediaType")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
                .or_else(|| default_media_type.clone());

            vec![AttachmentRow {
                url,
                media_type,
                name: default_name.clone(),
                blurhash: default_blurhash.clone(),
                width: default_width,
                height: default_height,
            }]
        }
        serde_json::Value::Array(arr) => {
            let mut out = Vec::new();
            for item in arr {
                out.extend(parse_url_field(
                    item,
                    default_media_type,
                    default_name,
                    default_blurhash,
                    default_width,
                    default_height,
                ));
            }
            out
        }
        _ => vec![],
    }
}

fn ensure_columns(conn: &Connection, table: &str, cols: &[(&str, &str)]) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut existing = std::collections::HashSet::new();
    for r in rows {
        existing.insert(r?);
    }
    for (name, ty) in cols {
        if !existing.contains(*name) {
            conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {name} {ty}"), [])?;
        }
    }
    Ok(())
}

fn parse_tag(v: &serde_json::Value) -> Option<(String, Option<String>, Option<String>)> {
    match v {
        serde_json::Value::String(s) => Some(("Tag".to_string(), Some(s.clone()), None)),
        serde_json::Value::Object(map) => {
            let tag_type = map
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("Tag")
                .to_string();
            let name = map.get("name").and_then(|n| n.as_str()).map(|s| s.to_string());
            let href = map.get("href").and_then(|h| h.as_str()).map(|s| s.to_string());
            if name.is_none() && href.is_none() {
                return None;
            }
            Some((tag_type, name, href))
        }
        _ => None,
    }
}

fn escape_like(input: &str) -> String {
    input.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
