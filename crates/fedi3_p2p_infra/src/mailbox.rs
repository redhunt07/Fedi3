/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use anyhow::{Context, Result};
use fedi3_protocol::RelayHttpRequest;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct MailboxStore {
    db_path: PathBuf,
    max_per_peer: u32,
    max_bytes_per_peer: u64,
    max_ttl_secs: u64,
}

impl MailboxStore {
    pub fn open(
        db_path: impl AsRef<Path>,
        max_per_peer: u32,
        max_bytes_per_peer: u64,
        max_ttl_secs: u64,
    ) -> Result<Self> {
        let db_path = db_path.as_ref().to_path_buf();
        init_db(&db_path)?;
        Ok(Self {
            db_path,
            max_per_peer: max_per_peer.max(1),
            max_bytes_per_peer: max_bytes_per_peer.max(1024),
            max_ttl_secs: max_ttl_secs.max(60).min(30 * 24 * 3600),
        })
    }

    pub async fn put(
        &self,
        from_peer_id: &str,
        to_peer_id: &str,
        msg_id: &str,
        req: &RelayHttpRequest,
        ttl_secs: u64,
    ) -> Result<()> {
        let db_path = self.db_path.clone();
        let from_peer_id = from_peer_id.to_string();
        let to_peer_id = to_peer_id.to_string();
        let msg_id = msg_id.to_string();
        let req_json = serde_json::to_vec(req).context("serialize req")?;
        let size_bytes: u64 = req_json.len().try_into().unwrap_or(u64::MAX);
        let ttl_secs = ttl_secs
            .max(60)
            .min(self.max_ttl_secs)
            .min(30 * 24 * 3600);
        let max_per_peer = self.max_per_peer;
        let max_bytes_per_peer = self.max_bytes_per_peer;
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = Connection::open(db_path)?;
            let now = now_ms();
            let expires_at_ms = now.saturating_add((ttl_secs as i64).saturating_mul(1000));
            conn.execute(
                r#"
                INSERT OR REPLACE INTO mailbox_messages (id, to_peer_id, from_peer_id, created_at_ms, expires_at_ms, size_bytes, req_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![msg_id, to_peer_id, from_peer_id, now, expires_at_ms, size_bytes as i64, req_json],
            )?;

            // Cleanup expired.
            let _ = conn.execute("DELETE FROM mailbox_messages WHERE expires_at_ms <= ?1", params![now]);

            // Enforce max_per_peer by deleting oldest.
            let count: u64 = conn.query_row(
                "SELECT COUNT(*) FROM mailbox_messages WHERE to_peer_id = ?1",
                params![to_peer_id],
                |r| r.get(0),
            )?;
            if count > max_per_peer as u64 {
                let to_delete = count - max_per_peer as u64;
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id FROM mailbox_messages
                    WHERE to_peer_id = ?1
                    ORDER BY created_at_ms ASC
                    LIMIT ?2
                    "#,
                )?;
                let rows = stmt
                    .query_map(params![to_peer_id, to_delete], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                for id in rows {
                    let _ = conn.execute(
                        "DELETE FROM mailbox_messages WHERE id = ?1 AND to_peer_id = ?2",
                        params![id, to_peer_id],
                    );
                }
            }

            // Enforce max_bytes_per_peer by deleting oldest until under limit.
            let total_bytes: i64 = conn.query_row(
                "SELECT COALESCE(SUM(size_bytes), 0) FROM mailbox_messages WHERE to_peer_id = ?1",
                params![to_peer_id],
                |r| r.get(0),
            )?;
            let mut over: i128 = (total_bytes as i128).saturating_sub(max_bytes_per_peer as i128);
            if over > 0 {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, size_bytes FROM mailbox_messages
                    WHERE to_peer_id = ?1
                    ORDER BY created_at_ms ASC
                    "#,
                )?;
                let mut rows = stmt.query(params![to_peer_id])?;
                while over > 0 {
                    let Some(row) = rows.next()? else { break };
                    let id: String = row.get(0)?;
                    let sz: i64 = row.get(1)?;
                    let _ = conn.execute(
                        "DELETE FROM mailbox_messages WHERE id = ?1 AND to_peer_id = ?2",
                        params![id, to_peer_id],
                    );
                    over = over.saturating_sub(sz as i128);
                }
            }
            Ok(())
        })
        .await??;
        Ok(())
    }

    pub async fn poll(&self, for_peer_id: &str, limit: u32) -> Result<Vec<MailboxMessage>> {
        let db_path = self.db_path.clone();
        let for_peer_id = for_peer_id.to_string();
        let limit = limit.max(1).min(200);
        let out = tokio::task::spawn_blocking(move || -> Result<Vec<MailboxMessage>> {
            let conn = Connection::open(db_path)?;
            let now = now_ms();
            let _ = conn.execute("DELETE FROM mailbox_messages WHERE expires_at_ms <= ?1", params![now]);

            let mut stmt = conn.prepare(
                r#"
                SELECT id, req_json
                FROM mailbox_messages
                WHERE to_peer_id = ?1
                ORDER BY created_at_ms ASC
                LIMIT ?2
                "#,
            )?;
            let mut rows = stmt.query(params![for_peer_id, limit])?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                let id: String = row.get(0)?;
                let req_json: Vec<u8> = row.get(1)?;
                let req: RelayHttpRequest = serde_json::from_slice(&req_json).context("parse req_json")?;
                out.push(MailboxMessage { id, req });
            }
            Ok(out)
        })
        .await??;
        Ok(out)
    }

    pub async fn ack(&self, for_peer_id: &str, ids: &[String]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let db_path = self.db_path.clone();
        let for_peer_id = for_peer_id.to_string();
        let ids = ids.to_vec();
        let deleted = tokio::task::spawn_blocking(move || -> Result<u64> {
            let mut conn = Connection::open(db_path)?;
            let tx = conn.transaction()?;
            let mut deleted: u64 = 0;
            for id in ids {
                deleted += tx.execute(
                    "DELETE FROM mailbox_messages WHERE id = ?1 AND to_peer_id = ?2",
                    params![id, for_peer_id],
                )? as u64;
            }
            tx.commit()?;
            Ok(deleted)
        })
        .await??;
        Ok(deleted)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MailboxMessage {
    pub id: String,
    pub req: RelayHttpRequest,
}

fn init_db(path: &Path) -> Result<()> {
    let conn = Connection::open(path).with_context(|| format!("open db: {}", path.display()))?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS mailbox_messages (
          id TEXT PRIMARY KEY,
          to_peer_id TEXT NOT NULL,
          from_peer_id TEXT NOT NULL DEFAULT '',
          created_at_ms INTEGER NOT NULL,
          expires_at_ms INTEGER NOT NULL,
          size_bytes INTEGER NOT NULL DEFAULT 0,
          req_json BLOB NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_mailbox_peer ON mailbox_messages(to_peer_id, created_at_ms);
        CREATE INDEX IF NOT EXISTS idx_mailbox_exp ON mailbox_messages(expires_at_ms);
        "#,
    )?;
    ensure_column(&conn, "mailbox_messages", "from_peer_id", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(&conn, "mailbox_messages", "size_bytes", "INTEGER NOT NULL DEFAULT 0")?;
    Ok(())
}

fn ensure_column(conn: &Connection, table: &str, column: &str, decl: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(());
        }
    }
    conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"), [])?;
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
