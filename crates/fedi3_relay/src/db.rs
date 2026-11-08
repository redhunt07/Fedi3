//! Synchronous SQL implementation. Runtime callers use DbDispatch.
use super::*;

impl Db {
    pub(super) fn open_sqlite_conn(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path)?;
        self.apply_pragmas(&conn)?;
        Ok(conn)
    }

    pub(super) fn open_pg_conn(&self) -> Result<PgConn> {
        let pool = self
            .pg_pool
            .get()
            .ok_or_else(|| anyhow::anyhow!("postgres pool not initialized"))?;
        let client = block_on_result(pool.get())?;
        Ok(PgConn { client })
    }

    pub(super) fn apply_pragmas(&self, conn: &Connection) -> rusqlite::Result<()> {
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", self.db_synchronous.as_str());
        let _ = conn.pragma_update(None, "temp_store", "MEMORY");
        let _ = conn.pragma_update(None, "cache_size", self.db_cache_kb);
        let _ = conn.busy_timeout(Duration::from_millis(self.db_busy_timeout_ms));
        Ok(())
    }

    pub(super) fn init(&self) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute_batch(
                    r#"
            PRAGMA journal_mode=WAL;
CREATE TABLE IF NOT EXISTS forward_receipts (
 seq BIGINT PRIMARY KEY, username TEXT NOT NULL, created_at_ms BIGINT NOT NULL,
 previous_hash TEXT NOT NULL, entry_hash TEXT NOT NULL, payload_hash TEXT NOT NULL, envelope_json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS forward_receipts_user_seq ON forward_receipts(username,seq);
CREATE INDEX IF NOT EXISTS forward_receipts_created ON forward_receipts(created_at_ms);
            CREATE TABLE IF NOT EXISTS users (
              username TEXT PRIMARY KEY,
              token_sha256 TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL,
              disabled INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_users_username_lower ON users(lower(username));
            CREATE TABLE IF NOT EXISTS user_cache (
              username TEXT PRIMARY KEY,
              actor_json TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              actor_id TEXT NULL,
              actor_url TEXT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_user_cache_updated ON user_cache(updated_at_ms DESC);
            CREATE INDEX IF NOT EXISTS idx_user_cache_username_lower ON user_cache(lower(username));
            CREATE INDEX IF NOT EXISTS idx_user_cache_actor_id_lower ON user_cache(lower(actor_id));
            CREATE INDEX IF NOT EXISTS idx_user_cache_actor_url_lower ON user_cache(lower(actor_url));
            CREATE TABLE IF NOT EXISTS user_collection_cache (
              username TEXT NOT NULL,
              kind TEXT NOT NULL,
              json TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              PRIMARY KEY(username, kind)
            );
            CREATE TABLE IF NOT EXISTS inbox_spool (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              username TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL,
              method TEXT NOT NULL,
              path TEXT NOT NULL,
              query TEXT NOT NULL,
              headers_json TEXT NOT NULL,
              body_b64 TEXT NOT NULL,
              body_len INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS inbox_spool_user_created ON inbox_spool(username, created_at_ms);

            CREATE TABLE IF NOT EXISTS relay_registry (
              relay_url TEXT PRIMARY KEY,
              base_domain TEXT NULL,
              last_seen_ms INTEGER NOT NULL,
              last_telemetry_json TEXT NULL,
              sign_pubkey_b64 TEXT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_relay_registry_seen ON relay_registry(last_seen_ms DESC);

            CREATE TABLE IF NOT EXISTS relay_meta (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS peer_registry (
              peer_id TEXT PRIMARY KEY,
              last_seen_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS relay_peers (
              relay_url TEXT PRIMARY KEY,
              added_at_ms INTEGER NOT NULL,
              last_seen_ms INTEGER NULL
            );
            CREATE TABLE IF NOT EXISTS remote_users (
              username TEXT PRIMARY KEY,
              relay_url TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_remote_users_relay ON remote_users(relay_url);
            CREATE TABLE IF NOT EXISTS relay_blocks (
              kind TEXT NOT NULL,
              value TEXT NOT NULL,
              reason TEXT NULL,
              created_at_ms INTEGER NOT NULL,
              created_by TEXT NULL,
              expires_at_ms INTEGER NULL,
              PRIMARY KEY(kind, value)
            );
            CREATE TABLE IF NOT EXISTS user_blocks (
              username TEXT NOT NULL,
              kind TEXT NOT NULL,
              value TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              PRIMARY KEY(username, kind, value)
            );
            CREATE TABLE IF NOT EXISTS peer_directory (
              peer_id TEXT PRIMARY KEY,
              username TEXT NOT NULL,
              actor_url TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_peer_directory_user ON peer_directory(username);
            CREATE INDEX IF NOT EXISTS idx_peer_directory_actor ON peer_directory(actor_url);
            CREATE INDEX IF NOT EXISTS idx_peer_directory_user_lower ON peer_directory(lower(username));
            CREATE INDEX IF NOT EXISTS idx_peer_directory_actor_lower ON peer_directory(lower(actor_url));
            CREATE INDEX IF NOT EXISTS idx_peer_directory_updated ON peer_directory(updated_at_ms DESC);

            CREATE TABLE IF NOT EXISTS relay_user_directory (
              actor_url TEXT PRIMARY KEY,
              username TEXT NOT NULL,
              relay_url TEXT NOT NULL,
              updated_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_relay_user_dir_name ON relay_user_directory(username);
            CREATE INDEX IF NOT EXISTS idx_relay_user_dir_relay ON relay_user_directory(relay_url);
            CREATE INDEX IF NOT EXISTS idx_relay_user_dir_name_lower ON relay_user_directory(lower(username));
            CREATE INDEX IF NOT EXISTS idx_relay_user_dir_actor_lower ON relay_user_directory(lower(actor_url));
            CREATE INDEX IF NOT EXISTS idx_relay_user_dir_updated ON relay_user_directory(updated_at_ms DESC);

            CREATE TABLE IF NOT EXISTS user_moves (
              username TEXT PRIMARY KEY,
              moved_to_actor TEXT NOT NULL,
              moved_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS move_notices (
              notice_id TEXT PRIMARY KEY,
              notice_json TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_move_notices_created ON move_notices(created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS move_notice_fanout (
              notice_id TEXT NOT NULL,
              relay_url TEXT NOT NULL,
              tries INTEGER NOT NULL,
              last_try_ms INTEGER NOT NULL,
              sent_ok INTEGER NOT NULL,
              PRIMARY KEY(notice_id, relay_url)
            );

            CREATE TABLE IF NOT EXISTS webrtc_signals (
              signal_id TEXT PRIMARY KEY,
              to_peer_id TEXT NOT NULL,
              from_actor TEXT NOT NULL,
              session_id TEXT NOT NULL,
              kind TEXT NOT NULL,
              payload_json TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_webrtc_to_created ON webrtc_signals(to_peer_id, created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS media_items (
              id TEXT PRIMARY KEY,
              username TEXT NOT NULL,
              backend TEXT NOT NULL,
              storage_key TEXT NOT NULL,
              media_type TEXT NOT NULL,
              size INTEGER NOT NULL,
              created_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_media_user_created ON media_items(username, created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS relay_notes (
              note_id TEXT PRIMARY KEY,
              actor_id TEXT NULL,
              published_ms INTEGER NULL,
              content_text TEXT NOT NULL,
              content_html TEXT NOT NULL,
              note_json TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_relay_notes_created ON relay_notes(created_at_ms DESC);
            CREATE INDEX IF NOT EXISTS idx_relay_notes_actor ON relay_notes(actor_id);
            CREATE INDEX IF NOT EXISTS idx_relay_notes_published ON relay_notes(published_ms DESC);

            CREATE TABLE IF NOT EXISTS relay_note_tags (
              note_id TEXT NOT NULL,
              tag TEXT NOT NULL,
              PRIMARY KEY(note_id, tag)
            );
            CREATE INDEX IF NOT EXISTS idx_relay_note_tags_tag ON relay_note_tags(tag);
            CREATE INDEX IF NOT EXISTS idx_relay_note_tags_tag_lower ON relay_note_tags(lower(tag));

            CREATE TABLE IF NOT EXISTS relay_tag_counts (
              tag TEXT PRIMARY KEY,
              count INTEGER NOT NULL
            );
            INSERT OR IGNORE INTO relay_tag_counts(tag, count)
              SELECT tag, COUNT(*) FROM relay_note_tags GROUP BY tag;
            CREATE TRIGGER IF NOT EXISTS relay_tag_counts_insert
              AFTER INSERT ON relay_note_tags
            BEGIN
              INSERT INTO relay_tag_counts(tag, count) VALUES (NEW.tag, 1)
              ON CONFLICT(tag) DO UPDATE SET count = count + 1;
            END;
            CREATE TRIGGER IF NOT EXISTS relay_tag_counts_delete
              AFTER DELETE ON relay_note_tags
            BEGIN
              UPDATE relay_tag_counts SET count = count - 1 WHERE tag = OLD.tag;
              DELETE FROM relay_tag_counts WHERE tag = OLD.tag AND count <= 0;
            END;

            CREATE TABLE IF NOT EXISTS relay_notes_count (
              id INTEGER PRIMARY KEY CHECK(id = 1),
              count INTEGER NOT NULL
            );
            INSERT OR IGNORE INTO relay_notes_count(id, count)
              VALUES (1, (SELECT COUNT(*) FROM relay_notes));
            CREATE TRIGGER IF NOT EXISTS relay_notes_count_insert
              AFTER INSERT ON relay_notes
            BEGIN
              UPDATE relay_notes_count SET count = count + 1 WHERE id = 1;
            END;
            CREATE TRIGGER IF NOT EXISTS relay_notes_count_delete
              AFTER DELETE ON relay_notes
            BEGIN
              UPDATE relay_notes_count SET count = count - 1 WHERE id = 1;
            END;

            CREATE TABLE IF NOT EXISTS relay_outbox_index (
              username TEXT PRIMARY KEY,
              last_index_ms INTEGER NOT NULL,
              last_ok INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS admin_audit (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              action TEXT NOT NULL,
              username TEXT NULL,
              actor TEXT NULL,
              ip TEXT NULL,
              ok INTEGER NOT NULL,
              detail TEXT NULL,
              created_at_ms INTEGER NOT NULL,
              request_id TEXT NULL,
              correlation_id TEXT NULL,
              user_agent TEXT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_admin_audit_created ON admin_audit(created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS schema_version (
              id INTEGER PRIMARY KEY CHECK(id = 1),
              version INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS inbox_dedupe (
              activity_id TEXT NOT NULL,
              recipient TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL,
              PRIMARY KEY(activity_id, recipient)
            );
            CREATE INDEX IF NOT EXISTS idx_inbox_dedupe_created ON inbox_dedupe(created_at_ms);
            "#,
                )?;
                migrate_sqlite_schema(&conn)?;
                Ok(())
            }
            DbDriver::Postgres => {
                let url = self.db_url.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("FEDI3_RELAY_DB_URL is required for postgres")
                })?;
                let mut cfg = deadpool_postgres::Config::new();
                cfg.url = Some(url.to_string());
                cfg.manager = Some(ManagerConfig {
                    recycling_method: RecyclingMethod::Fast,
                });
                let mut pool_cfg = PoolConfig::new(self.pg_pool_max_size);
                pool_cfg.queue_mode = self.pg_pool_queue_mode;
                pool_cfg.timeouts = Timeouts {
                    wait: self.pg_pool_wait_ms.map(Duration::from_millis),
                    create: self.pg_pool_create_timeout_ms.map(Duration::from_millis),
                    recycle: self.pg_pool_recycle_timeout_ms.map(Duration::from_millis),
                };
                cfg.pool = Some(pool_cfg);
                let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;
                let _ = self.pg_pool.set(pool);
                let max_retries = self.pg_init_retries;
                let mut last_err: Option<anyhow::Error> = None;
                for attempt in 1..=max_retries {
                    match self.open_pg_conn() {
                        Ok(mut conn) => {
                            conn.batch_execute(include_str!("../sql/postgres_schema.sql"))?;
                            migrate_postgres_search_tsv(&mut conn)?;
                            // Reject databases written by a newer binary instead of
                            // silently running against an unknown schema (RD-03).
                            if let Some(row) = conn
                                .query_opt("SELECT version FROM schema_version WHERE id = 1", &[])?
                            {
                                let version: i64 = row.get(0);
                                if version > SCHEMA_VERSION {
                                    anyhow::bail!(
                                        "relay database schema version {version} is newer than this binary supports ({SCHEMA_VERSION})"
                                    );
                                }
                            }
                            conn.execute(
                                "INSERT INTO schema_version(id, version) VALUES (1, $1) ON CONFLICT (id) DO UPDATE SET version = EXCLUDED.version",
                                &[&SCHEMA_VERSION],
                            )?;
                            return Ok(());
                        }
                        Err(err) => {
                            last_err = Some(err);
                            let backoff_ms = (attempt as u64 * self.pg_init_backoff_ms).min(30_000);
                            warn!(
                                "postgres not ready (attempt {attempt}/{max_retries}); retrying in {backoff_ms}ms"
                            );
                            std::thread::sleep(Duration::from_millis(backoff_ms));
                        }
                    }
                }
                Err(anyhow::anyhow!(
                    "db init: Error occurred while creating a new object: {:#}",
                    last_err.unwrap()
                ))
            }
        }
    }

    pub(super) fn health_check(&self) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row("SELECT 1", [], |_| Ok(()))?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_one("SELECT 1", &[])?;
                let _: i32 = row.get(0);
                Ok(())
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn insert_admin_audit(
        &self,
        action: &str,
        username: Option<&str>,
        actor: Option<&str>,
        ip: Option<&str>,
        ok: bool,
        detail: Option<&str>,
        meta: &AuditMeta,
    ) -> Result<()> {
        let ts = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO admin_audit(action, username, actor, ip, ok, detail, created_at_ms, request_id, correlation_id, user_agent) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        action,
                        username,
                        actor,
                        ip,
                        if ok { 1 } else { 0 },
                        detail,
                        ts,
                        meta.request_id,
                        meta.correlation_id,
                        meta.user_agent
                    ],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO admin_audit(action, username, actor, ip, ok, detail, created_at_ms, request_id, correlation_id, user_agent) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                    &[
                        &action,
                        &username,
                        &actor,
                        &ip,
                        &ok,
                        &detail,
                        &ts,
                        &meta.request_id,
                        &meta.correlation_id,
                        &meta.user_agent,
                    ],
                )?;
                Ok(())
            }
        }
    }

    #[allow(clippy::type_complexity)]
    pub(super) fn list_admin_audit(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<
        Vec<(
            i64,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            bool,
            Option<String>,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
        )>,
    > {
        let limit = limit.clamp(1, 500) as i64;
        let offset = offset as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT id, action, username, actor, ip, ok, detail, created_at_ms, request_id, correlation_id, user_agent FROM admin_audit ORDER BY created_at_ms DESC LIMIT ?1 OFFSET ?2",
                )?;
                let mut rows = stmt.query(params![limit, offset])?;
                let mut out = Vec::new();
                while let Some(r) = rows.next()? {
                    let ok_int: i64 = r.get(5)?;
                    out.push((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        ok_int != 0,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                        r.get(9)?,
                        r.get(10)?,
                    ));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT id, action, username, actor, ip, ok, detail, created_at_ms, request_id, correlation_id, user_agent FROM admin_audit ORDER BY created_at_ms DESC LIMIT $1 OFFSET $2",
                    &[&limit, &offset],
                )?;
                let mut out = Vec::new();
                for r in rows {
                    out.push((
                        r.get(0),
                        r.get(1),
                        r.get(2),
                        r.get(3),
                        r.get(4),
                        r.get(5),
                        r.get(6),
                        r.get(7),
                        r.get(8),
                        r.get(9),
                        r.get(10),
                    ));
                }
                Ok(out)
            }
        }
    }

    pub(super) fn upsert_relay(
        &self,
        relay_url: &str,
        base_domain: Option<String>,
        telemetry_json: Option<String>,
        sign_pubkey_b64: Option<String>,
    ) -> Result<()> {
        let relay_url = relay_url.trim_end_matches('/');
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    r#"
            INSERT INTO relay_registry(relay_url, base_domain, last_seen_ms, last_telemetry_json, sign_pubkey_b64)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(relay_url) DO UPDATE SET
              base_domain=COALESCE(excluded.base_domain, relay_registry.base_domain),
              last_seen_ms=excluded.last_seen_ms,
              last_telemetry_json=COALESCE(excluded.last_telemetry_json, relay_registry.last_telemetry_json),
              sign_pubkey_b64=COALESCE(relay_registry.sign_pubkey_b64, excluded.sign_pubkey_b64)
            "#,
                    params![relay_url, base_domain, now, telemetry_json, sign_pubkey_b64],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    r#"
            INSERT INTO relay_registry(relay_url, base_domain, last_seen_ms, last_telemetry_json, sign_pubkey_b64)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT(relay_url) DO UPDATE SET
              base_domain=COALESCE(EXCLUDED.base_domain, relay_registry.base_domain),
              last_seen_ms=EXCLUDED.last_seen_ms,
              last_telemetry_json=COALESCE(EXCLUDED.last_telemetry_json, relay_registry.last_telemetry_json),
              sign_pubkey_b64=COALESCE(relay_registry.sign_pubkey_b64, EXCLUDED.sign_pubkey_b64)
            "#,
                    &[&relay_url, &base_domain, &now, &telemetry_json, &sign_pubkey_b64],
                )?;
                Ok(())
            }
        }
    }

    #[allow(clippy::type_complexity)]
    pub(super) fn list_relays(
        &self,
        limit: u32,
    ) -> Result<Vec<(String, Option<String>, i64, Option<String>, Option<String>)>> {
        let limit = limit.min(500) as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT relay_url, base_domain, last_seen_ms, last_telemetry_json, sign_pubkey_b64 FROM relay_registry ORDER BY last_seen_ms DESC LIMIT ?1",
                )?;
                let mut rows = stmt.query(params![limit])?;
                let mut out = Vec::new();
                while let Some(r) = rows.next()? {
                    out.push((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT relay_url, base_domain, last_seen_ms, last_telemetry_json, sign_pubkey_b64 FROM relay_registry ORDER BY last_seen_ms DESC LIMIT $1",
                    &[&limit],
                )?;
                let mut out = Vec::new();
                for r in rows {
                    out.push((r.get(0), r.get(1), r.get(2), r.get(3), r.get(4)));
                }
                Ok(out)
            }
        }
    }

    pub(super) fn get_relay_pubkey_b64(&self, relay_url: &str) -> Result<Option<String>> {
        let relay_url = relay_url.trim_end_matches('/');
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT sign_pubkey_b64 FROM relay_registry WHERE relay_url=?1",
                    params![relay_url],
                    |r| r.get::<_, Option<String>>(0),
                )
                .optional()
                .map(|v| v.flatten())
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT sign_pubkey_b64 FROM relay_registry WHERE relay_url=$1",
                    &[&relay_url],
                )?;
                Ok(row.and_then(|r| r.get::<_, Option<String>>(0)))
            }
        }
    }

    pub(super) fn relay_meta_get(&self, key: &str) -> Result<Option<String>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT value FROM relay_meta WHERE key=?1",
                    params![key],
                    |r| r.get(0),
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt("SELECT value FROM relay_meta WHERE key=$1", &[&key])?;
                Ok(row.map(|r| r.get(0)))
            }
        }
    }

    pub(super) fn relay_meta_set(&self, key: &str, value: &str) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT OR REPLACE INTO relay_meta(key,value) VALUES (?1,?2)",
                    params![key, value],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO relay_meta(key, value) VALUES ($1, $2) ON CONFLICT(key) DO UPDATE SET value=EXCLUDED.value",
                    &[&key, &value],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn list_relay_sync_state(&self) -> Result<Vec<(String, i64)>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare("SELECT key, value FROM relay_meta WHERE key LIKE 'relay_sync_last_ms:%' ESCAPE '\\'")?;
                let mut rows = stmt.query([])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    let key: String = row.get(0)?;
                    let value: String = row.get(1)?;
                    let relay_url = key.trim_start_matches("relay_sync_last_ms:").to_string();
                    let last_ms = value.parse::<i64>().unwrap_or(0);
                    out.push((relay_url, last_ms));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT key, value FROM relay_meta WHERE key LIKE 'relay_sync_last_ms:%' ESCAPE '\\'",
                    &[],
                )?;
                let mut out = Vec::new();
                for row in rows {
                    let key: String = row.get(0);
                    let value: String = row.get(1);
                    let relay_url = key.trim_start_matches("relay_sync_last_ms:").to_string();
                    let last_ms = value.parse::<i64>().unwrap_or(0);
                    out.push((relay_url, last_ms));
                }
                Ok(out)
            }
        }
    }

    pub(super) fn load_or_create_signing_keypair_b64(&self) -> Result<(String, String)> {
        let generate = || {
            let signing = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
            (
                B64.encode(signing.verifying_key().to_bytes()),
                B64.encode(signing.to_bytes()),
            )
        };
        let validate = |pk: String, sk: String| -> Result<(String, String)> {
            let bytes: [u8; 32] = B64
                .decode(&sk)?
                .try_into()
                .map_err(|_| anyhow::anyhow!("invalid signing key"))?;
            anyhow::ensure!(
                B64.encode(
                    ed25519_dalek::SigningKey::from_bytes(&bytes)
                        .verifying_key()
                        .to_bytes()
                ) == pk,
                "signing keypair mismatch"
            );
            Ok((pk, sk))
        };
        match self.driver {
            DbDriver::Sqlite => {
                let mut conn = self.open_sqlite_conn()?;
                let tx =
                    conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let pk: Option<String> = tx
                    .query_row(
                        "SELECT value FROM relay_meta WHERE key='sign_pk_b64'",
                        [],
                        |r| r.get(0),
                    )
                    .optional()?;
                let sk: Option<String> = tx
                    .query_row(
                        "SELECT value FROM relay_meta WHERE key='sign_sk_b64'",
                        [],
                        |r| r.get(0),
                    )
                    .optional()?;
                let pair = match (pk, sk) {
                    (Some(pk), Some(sk)) => validate(pk, sk)?,
                    (None, None) => {
                        let (pk, sk) = generate();
                        tx.execute("INSERT INTO relay_meta(key,value) VALUES ('sign_pk_b64',?1),('sign_sk_b64',?2)",params![pk,sk])?;
                        (pk, sk)
                    }
                    _ => anyhow::bail!("incomplete signing keypair"),
                };
                tx.commit()?;
                Ok(pair)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let mut tx = conn.transaction()?;
                tx.execute("INSERT INTO relay_meta(key,value) VALUES ('sign_key_lock','1') ON CONFLICT(key) DO NOTHING",&[])?;
                tx.query_one(
                    "SELECT value FROM relay_meta WHERE key='sign_key_lock' FOR UPDATE",
                    &[],
                )?;
                let rows = tx.query(
                    "SELECT key,value FROM relay_meta WHERE key IN ('sign_pk_b64','sign_sk_b64')",
                    &[],
                )?;
                let mut pk = None;
                let mut sk = None;
                for row in rows {
                    let key: String = row.get(0);
                    if key == "sign_pk_b64" {
                        pk = Some(row.get::<_, String>(1));
                    } else {
                        sk = Some(row.get::<_, String>(1));
                    }
                }
                let pair = match (pk, sk) {
                    (Some(pk), Some(sk)) => validate(pk, sk)?,
                    (None, None) => {
                        let (pk, sk) = generate();
                        tx.execute("INSERT INTO relay_meta(key,value) VALUES ('sign_pk_b64',$1),('sign_sk_b64',$2)",&[&pk,&sk])?;
                        (pk, sk)
                    }
                    _ => anyhow::bail!("incomplete signing keypair"),
                };
                tx.commit()?;
                Ok(pair)
            }
        }
    }

    pub(super) fn upsert_peer_seen(&self, peer_id: &str) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    r#"
            INSERT INTO peer_registry(peer_id, last_seen_ms) VALUES (?1, ?2)
            ON CONFLICT(peer_id) DO UPDATE SET last_seen_ms=excluded.last_seen_ms
            "#,
                    params![peer_id, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    r#"
            INSERT INTO peer_registry(peer_id, last_seen_ms) VALUES ($1, $2)
            ON CONFLICT(peer_id) DO UPDATE SET last_seen_ms=EXCLUDED.last_seen_ms
            "#,
                    &[&peer_id, &now],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn count_peers_seen_since(&self, cutoff_ms: i64) -> Result<u64> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let n: u64 = conn.query_row(
                    "SELECT COUNT(*) FROM peer_registry WHERE last_seen_ms >= ?1",
                    params![cutoff_ms],
                    |r| r.get(0),
                )?;
                Ok(n)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_one(
                    "SELECT COUNT(*) FROM peer_registry WHERE last_seen_ms >= $1",
                    &[&cutoff_ms],
                )?;
                let n: i64 = row.get(0);
                Ok(n.max(0) as u64)
            }
        }
    }

    pub(super) fn count_users_total(&self) -> Result<u64> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let n: u64 =
                    conn.query_row("SELECT COUNT(*) FROM users WHERE disabled=0", [], |r| {
                        r.get(0)
                    })?;
                Ok(n)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_one("SELECT COUNT(*) FROM users WHERE disabled=false", &[])?;
                let n: i64 = row.get(0);
                Ok(n.max(0) as u64)
            }
        }
    }

    pub(super) fn create_user(&self, username: &str, token: &str) -> Result<bool> {
        let hash = token_hash_hex(token);
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let exists: Option<String> = conn
                    .query_row(
                        "SELECT username FROM users WHERE username = ?1",
                        params![username],
                        |r| r.get(0),
                    )
                    .optional()?;
                if exists.is_some() {
                    return Ok(false);
                }
                conn.execute(
                    "INSERT INTO users(username, token_sha256, created_at_ms) VALUES (?1, ?2, ?3)",
                    params![username, hash, now],
                )?;
                Ok(true)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let exists = conn.query_opt(
                    "SELECT username FROM users WHERE username = $1",
                    &[&username],
                )?;
                if exists.is_some() {
                    return Ok(false);
                }
                conn.execute(
                    "INSERT INTO users(username, token_sha256, created_at_ms) VALUES ($1, $2, $3)",
                    &[&username, &hash, &now],
                )?;
                Ok(true)
            }
        }
    }

    pub(super) fn update_user_token(&self, username: &str, token: &str) -> Result<()> {
        let hash = token_hash_hex(token);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "UPDATE users SET token_sha256=?2 WHERE username=?1",
                    params![username, hash],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "UPDATE users SET token_sha256=$2 WHERE username=$1",
                    &[&username, &hash],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn verify_user_token(&self, username: &str, token: &str) -> Result<bool> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let row: Option<(String, i64)> = conn
                    .query_row(
                        "SELECT token_sha256, disabled FROM users WHERE username = ?1",
                        params![username],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                match row {
                    Some((stored, disabled)) => {
                        if disabled != 0 {
                            return Ok(false);
                        }
                        Ok(ct_eq_str(&stored, &token_hash_hex(token)))
                    }
                    None => Ok(false),
                }
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT token_sha256, disabled FROM users WHERE username = $1",
                    &[&username],
                )?;
                match row {
                    Some(r) => {
                        let stored: String = r.get(0);
                        let disabled: bool = r.get(1);
                        if disabled {
                            return Ok(false);
                        }
                        Ok(ct_eq_str(&stored, &token_hash_hex(token)))
                    }
                    None => Ok(false),
                }
            }
        }
    }

    pub(super) fn user_exists(&self, username: &str) -> Result<bool> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let exists: Option<String> = conn
                    .query_row(
                        "SELECT username FROM users WHERE username = ?1",
                        params![username],
                        |r| r.get(0),
                    )
                    .optional()?;
                Ok(exists.is_some())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let exists = conn.query_opt(
                    "SELECT username FROM users WHERE username = $1",
                    &[&username],
                )?;
                Ok(exists.is_some())
            }
        }
    }

    pub(super) fn upsert_user_token(
        &self,
        cfg: &RelayConfig,
        headers: &HeaderMap,
        username: &str,
        new_token: &str,
    ) -> Result<UpsertUserResult> {
        if !self.user_exists(username)? {
            let created = self.create_user(username, new_token)?;
            return Ok(if created {
                UpsertUserResult::Created
            } else {
                UpsertUserResult::Exists
            });
        }

        // User exists: only admin can change token unless self-register is enabled
        // and the caller proves ownership by presenting the current token.
        if is_authorized_admin(cfg, headers) {
            self.update_user_token(username, new_token)?;
            return Ok(UpsertUserResult::Updated);
        }
        if cfg.allow_self_register {
            if let Some(old) = bearer_token(headers) {
                if self.verify_user_token(username, &old)? {
                    self.update_user_token(username, new_token)?;
                    return Ok(UpsertUserResult::Updated);
                }
                return Ok(UpsertUserResult::Unauthorized);
            }
            return Ok(UpsertUserResult::Exists);
        }

        Ok(UpsertUserResult::Unauthorized)
    }

    pub(super) fn verify_or_register(
        &self,
        cfg: &RelayConfig,
        username: &str,
        token: &str,
    ) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let row: Option<(String, i64)> = conn
                    .query_row(
                        "SELECT token_sha256, disabled FROM users WHERE username = ?1",
                        params![username],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                match row {
                    Some((stored, disabled)) => {
                        if disabled != 0 {
                            return Err(anyhow::anyhow!("user disabled"));
                        }
                        let got = token_hash_hex(token);
                        if ct_eq_str(&stored, &got) {
                            Ok(())
                        } else {
                            Err(anyhow::anyhow!("invalid token"))
                        }
                    }
                    None => {
                        if !cfg.allow_self_register {
                            return Err(anyhow::anyhow!("unknown user (registration disabled)"));
                        }
                        drop(conn);
                        let created = self.create_user(username, token)?;
                        if created {
                            Ok(())
                        } else {
                            Err(anyhow::anyhow!("user exists"))
                        }
                    }
                }
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT token_sha256, disabled FROM users WHERE username = $1",
                    &[&username],
                )?;
                match row {
                    Some(r) => {
                        let stored: String = r.get(0);
                        let disabled: bool = r.get(1);
                        if disabled {
                            return Err(anyhow::anyhow!("user disabled"));
                        }
                        let got = token_hash_hex(token);
                        if ct_eq_str(&stored, &got) {
                            Ok(())
                        } else {
                            Err(anyhow::anyhow!("invalid token"))
                        }
                    }
                    None => {
                        if !cfg.allow_self_register {
                            return Err(anyhow::anyhow!("unknown user (registration disabled)"));
                        }
                        drop(conn);
                        let created = self.create_user(username, token)?;
                        if created {
                            Ok(())
                        } else {
                            Err(anyhow::anyhow!("user exists"))
                        }
                    }
                }
            }
        }
    }

    pub(super) fn verify_token(&self, username: &str, token: &str) -> Result<bool> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let row: Option<(String, i64)> = conn
                    .query_row(
                        "SELECT token_sha256, disabled FROM users WHERE username = ?1",
                        params![username],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                let Some((stored, disabled)) = row else {
                    return Ok(false);
                };
                if disabled != 0 {
                    return Ok(false);
                }
                Ok(ct_eq_str(&stored, &token_hash_hex(token)))
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT token_sha256, disabled FROM users WHERE username = $1",
                    &[&username],
                )?;
                let Some(r) = row else { return Ok(false) };
                let stored: String = r.get(0);
                let disabled: bool = r.get(1);
                if disabled {
                    return Ok(false);
                }
                Ok(ct_eq_str(&stored, &token_hash_hex(token)))
            }
        }
    }

    pub(super) fn set_user_move(&self, username: &str, moved_to_actor: &str) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO user_moves(username, moved_to_actor, moved_at_ms) VALUES (?1, ?2, ?3)\n             ON CONFLICT(username) DO UPDATE SET moved_to_actor=excluded.moved_to_actor, moved_at_ms=excluded.moved_at_ms",
                    params![username, moved_to_actor, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO user_moves(username, moved_to_actor, moved_at_ms) VALUES ($1, $2, $3)\n             ON CONFLICT(username) DO UPDATE SET moved_to_actor=EXCLUDED.moved_to_actor, moved_at_ms=EXCLUDED.moved_at_ms",
                    &[&username, &moved_to_actor, &now],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn clear_user_move(&self, username: &str) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let _ = conn.execute(
                    "DELETE FROM user_moves WHERE username=?1",
                    params![username],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let _ = conn.execute("DELETE FROM user_moves WHERE username=$1", &[&username])?;
                Ok(())
            }
        }
    }

    pub(super) fn get_user_move(&self, username: &str) -> Result<Option<(String, i64)>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT moved_to_actor, moved_at_ms FROM user_moves WHERE username=?1",
                    params![username],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT moved_to_actor, moved_at_ms FROM user_moves WHERE username=$1",
                    &[&username],
                )?;
                Ok(row.map(|r| (r.get(0), r.get(1))))
            }
        }
    }

    pub(super) fn has_move_notice(&self, notice_id: &str) -> Result<bool> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let v: Option<String> = conn
                    .query_row(
                        "SELECT notice_id FROM move_notices WHERE notice_id=?1",
                        params![notice_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                Ok(v.is_some())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT notice_id FROM move_notices WHERE notice_id=$1",
                    &[&notice_id],
                )?;
                Ok(row.is_some())
            }
        }
    }

    pub(super) fn upsert_move_notice(&self, notice_id: &str, notice_json: &str) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO move_notices(notice_id, notice_json, created_at_ms) VALUES (?1, ?2, ?3)\n             ON CONFLICT(notice_id) DO UPDATE SET notice_json=excluded.notice_json, created_at_ms=excluded.created_at_ms",
                    params![notice_id, notice_json, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO move_notices(notice_id, notice_json, created_at_ms) VALUES ($1, $2, $3)\n             ON CONFLICT(notice_id) DO UPDATE SET notice_json=EXCLUDED.notice_json, created_at_ms=EXCLUDED.created_at_ms",
                    &[&notice_id, &notice_json, &now],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn list_recent_move_notices(
        &self,
        cutoff_ms: i64,
        limit: u32,
    ) -> Result<Vec<(String, String, i64)>> {
        let limit = limit.clamp(1, 1000) as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT notice_id, notice_json, created_at_ms FROM move_notices WHERE created_at_ms >= ?1 ORDER BY created_at_ms DESC LIMIT ?2",
                )?;
                let mut rows = stmt.query(params![cutoff_ms, limit])?;
                let mut out = Vec::new();
                while let Some(r) = rows.next()? {
                    out.push((r.get(0)?, r.get(1)?, r.get(2)?));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT notice_id, notice_json, created_at_ms FROM move_notices WHERE created_at_ms >= $1 ORDER BY created_at_ms DESC LIMIT $2",
                    &[&cutoff_ms, &limit],
                )?;
                let mut out = Vec::new();
                for r in rows {
                    out.push((r.get(0), r.get(1), r.get(2)));
                }
                Ok(out)
            }
        }
    }

    pub(super) fn get_fanout_status(
        &self,
        notice_id: &str,
        relay_url: &str,
    ) -> Result<Option<(i64, i64, i64)>> {
        // returns (tries, last_try_ms, sent_ok)
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT tries, last_try_ms, sent_ok FROM move_notice_fanout WHERE notice_id=?1 AND relay_url=?2",
                    params![notice_id, relay_url],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT tries, last_try_ms, CASE WHEN sent_ok THEN 1 ELSE 0 END::bigint FROM move_notice_fanout WHERE notice_id=$1 AND relay_url=$2",
                    &[&notice_id, &relay_url],
                )?;
                Ok(row.map(|r| (r.get(0), r.get(1), r.get(2))))
            }
        }
    }

    pub(super) fn record_fanout_attempt(
        &self,
        notice_id: &str,
        relay_url: &str,
        sent_ok: bool,
    ) -> Result<()> {
        let relay_url = relay_url.trim_end_matches('/');
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO move_notice_fanout(notice_id, relay_url, tries, last_try_ms, sent_ok) VALUES (?1, ?2, 1, ?3, ?4)\n             ON CONFLICT(notice_id, relay_url) DO UPDATE SET tries=move_notice_fanout.tries+1, last_try_ms=excluded.last_try_ms, sent_ok=excluded.sent_ok",
                    params![notice_id, relay_url, now, if sent_ok { 1 } else { 0 }],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO move_notice_fanout(notice_id, relay_url, tries, last_try_ms, sent_ok) VALUES ($1, $2, 1, $3, $4)\n             ON CONFLICT(notice_id, relay_url) DO UPDATE SET tries=move_notice_fanout.tries+1, last_try_ms=EXCLUDED.last_try_ms, sent_ok=EXCLUDED.sent_ok",
                    &[&notice_id, &relay_url, &now, &sent_ok],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn count_users(&self) -> Result<u64> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let n: u64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
                Ok(n)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_one("SELECT COUNT(*) FROM users", &[])?;
                let n: i64 = row.get(0);
                Ok(n.max(0) as u64)
            }
        }
    }

    pub(super) fn list_users(&self, limit: u32, offset: u32) -> Result<Vec<(String, i64, i64)>> {
        let limit = limit.clamp(1, 500) as i64;
        let offset = offset as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT username, created_at_ms, disabled FROM users ORDER BY created_at_ms DESC LIMIT ?1 OFFSET ?2",
                )?;
                let mut rows = stmt.query(params![limit, offset])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    out.push((row.get(0)?, row.get(1)?, row.get(2)?));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT username, created_at_ms, disabled FROM users ORDER BY created_at_ms DESC LIMIT $1 OFFSET $2",
                    &[&limit, &offset],
                )?;
                let mut out = Vec::new();
                for row in rows {
                    let disabled: bool = row.get(2);
                    out.push((row.get(0), row.get(1), if disabled { 1 } else { 0 }));
                }
                Ok(out)
            }
        }
    }

    pub(super) fn set_disabled(&self, username: &str, disabled: bool) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "UPDATE users SET disabled=?2 WHERE username=?1",
                    params![username, if disabled { 1 } else { 0 }],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "UPDATE users SET disabled=$2 WHERE username=$1",
                    &[&username, &disabled],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn rotate_token(&self, username: &str, new_token: &str) -> Result<()> {
        let hash = token_hash_hex(new_token);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "UPDATE users SET token_sha256=?2 WHERE username=?1",
                    params![username, hash],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "UPDATE users SET token_sha256=$2 WHERE username=$1",
                    &[&username, &hash],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn get_user(&self, username: &str) -> Result<Option<(i64, i64)>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT created_at_ms, disabled FROM users WHERE username=?1",
                    params![username],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT created_at_ms, disabled FROM users WHERE username=$1",
                    &[&username],
                )?;
                Ok(row.map(|r| {
                    let disabled: bool = r.get(1);
                    (r.get(0), if disabled { 1 } else { 0 })
                }))
            }
        }
    }

    pub(super) fn delete_user(&self, username: &str) -> Result<bool> {
        // Blob deletion is performed first by the handler while the account is
        // disabled. Never discard the remaining retry metadata on storage error.
        let actors = "SELECT actor_id FROM user_cache WHERE username=?1 UNION SELECT actor_url FROM user_cache WHERE username=?1 UNION SELECT actor_url FROM peer_directory WHERE username=?1";
        let mut statements = vec![
            "UPDATE forward_receipts SET username='',envelope_json='' WHERE username=?1".to_string(),
            "DELETE FROM relay_meta WHERE key='push:' || ?1".to_string(),
            format!("DELETE FROM webrtc_signals WHERE from_actor IN ({actors}) OR to_peer_id IN (SELECT peer_id FROM peer_directory WHERE username=?1)"),
            format!("DELETE FROM relay_note_tags WHERE note_id IN (SELECT note_id FROM relay_notes WHERE actor_id IN ({actors}))"),
            format!("DELETE FROM relay_notes WHERE actor_id IN ({actors})"),
            format!("DELETE FROM relay_user_directory WHERE actor_url IN ({actors})"),
        ];
        for table in [
            "inbox_spool",
            "user_collection_cache",
            "user_blocks",
            "user_moves",
            "relay_outbox_index",
            "peer_directory",
            "user_cache",
        ] {
            statements.push(format!("DELETE FROM {table} WHERE username=?1"));
        }
        match self.driver {
            DbDriver::Sqlite => {
                let mut conn = self.open_sqlite_conn()?;
                let tx =
                    conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let remaining: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM media_items WHERE username=?1",
                    [username],
                    |r| r.get(0),
                )?;
                anyhow::ensure!(remaining == 0, "media cleanup pending");
                for sql in statements {
                    tx.execute(&sql, [username])?;
                }
                let changed = tx.execute("DELETE FROM users WHERE username=?1", [username])?;
                tx.commit()?;
                Ok(changed > 0)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let mut tx = conn.transaction()?;
                // Serialize deletion before any late journal append for this account.
                tx.query(
                    "SELECT username FROM users WHERE username=$1 FOR UPDATE",
                    &[&username],
                )?;
                let remaining: i64 = tx
                    .query_one(
                        "SELECT COUNT(*) FROM media_items WHERE username=$1",
                        &[&username],
                    )?
                    .get(0);
                anyhow::ensure!(remaining == 0, "media cleanup pending");
                for sql in statements {
                    tx.execute(&sql.replace("?1", "$1"), &[&username])?;
                }
                let changed = tx.execute("DELETE FROM users WHERE username=$1", &[&username])?;
                tx.commit()?;
                Ok(changed > 0)
            }
        }
    }

    pub(super) fn list_user_media(&self, username: &str) -> Result<Vec<MediaItem>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare("SELECT id,username,backend,storage_key,media_type,size,created_at_ms FROM media_items WHERE username=?1 ORDER BY id LIMIT 500")?;
                let rows = stmt.query_map([username], |r| {
                    Ok(MediaItem {
                        id: r.get(0)?,
                        username: r.get(1)?,
                        backend: r.get(2)?,
                        storage_key: r.get(3)?,
                        media_type: r.get(4)?,
                        size: r.get(5)?,
                        created_at_ms: r.get(6)?,
                    })
                })?;
                Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                Ok(conn.query("SELECT id,username,backend,storage_key,media_type,size,created_at_ms FROM media_items WHERE username=$1 ORDER BY id LIMIT 500", &[&username])?.into_iter().map(|r| MediaItem{id:r.get(0),username:r.get(1),backend:r.get(2),storage_key:r.get(3),media_type:r.get(4),size:r.get(5),created_at_ms:r.get(6)}).collect())
            }
        }
    }

    pub(super) fn expired_media_reservations(
        &self,
        backend: &str,
        cutoff: i64,
    ) -> Result<Vec<MediaItem>> {
        let pending = format!("pending:{backend}");
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt=conn.prepare("SELECT id,username,backend,storage_key,media_type,size,created_at_ms FROM media_items WHERE backend=?1 AND created_at_ms<?2 LIMIT 500")?;
                let rows = stmt.query_map(params![pending, cutoff], |r| {
                    Ok(MediaItem {
                        id: r.get(0)?,
                        username: r.get(1)?,
                        backend: r.get(2)?,
                        storage_key: r.get(3)?,
                        media_type: r.get(4)?,
                        size: r.get(5)?,
                        created_at_ms: r.get(6)?,
                    })
                })?;
                Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                Ok(conn.query("SELECT id,username,backend,storage_key,media_type,size,created_at_ms FROM media_items WHERE backend=$1 AND created_at_ms<$2 LIMIT 500",&[&pending,&cutoff])?.into_iter().map(|r| MediaItem{id:r.get(0),username:r.get(1),backend:r.get(2),storage_key:r.get(3),media_type:r.get(4),size:r.get(5),created_at_ms:r.get(6)}).collect())
            }
        }
    }

    pub(super) fn reserve_media_item(
        &self,
        item: &MediaItem,
        quota: u64,
    ) -> Result<MediaReservation> {
        anyhow::ensure!(item.size >= 0, "invalid media size");
        let pending_backend = format!("pending:{}", item.backend);
        match self.driver {
            DbDriver::Sqlite => {
                let mut conn = self.open_sqlite_conn()?;
                let tx =
                    conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let enabled: Option<i64> = tx
                    .query_row(
                        "SELECT disabled FROM users WHERE username=?1",
                        [&item.username],
                        |r| r.get(0),
                    )
                    .optional()?;
                if enabled != Some(0) {
                    return Ok(MediaReservation::UserUnavailable);
                }
                let used: i64 = tx.query_row(
                    "SELECT COALESCE(SUM(size),0) FROM media_items WHERE username=?1",
                    [&item.username],
                    |r| r.get(0),
                )?;
                if quota > 0 && (used.max(0) as u64).saturating_add(item.size as u64) > quota {
                    return Ok(MediaReservation::QuotaExceeded);
                }
                tx.execute("INSERT INTO media_items(id,username,backend,storage_key,media_type,size,created_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![item.id,item.username,pending_backend,item.storage_key,item.media_type,item.size,item.created_at_ms])?;
                tx.commit()?;
                Ok(MediaReservation::Reserved)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let mut tx = conn.transaction()?;
                let row = tx.query_opt(
                    "SELECT disabled FROM users WHERE username=$1 FOR UPDATE",
                    &[&item.username],
                )?;
                if row.map(|r| r.get::<_, bool>(0)) != Some(false) {
                    return Ok(MediaReservation::UserUnavailable);
                }
                let used: i64 = tx
                    .query_one(
                        "SELECT COALESCE(SUM(size),0)::BIGINT FROM media_items WHERE username=$1",
                        &[&item.username],
                    )?
                    .get(0);
                if quota > 0 && (used.max(0) as u64).saturating_add(item.size as u64) > quota {
                    return Ok(MediaReservation::QuotaExceeded);
                }
                tx.execute("INSERT INTO media_items(id,username,backend,storage_key,media_type,size,created_at_ms) VALUES ($1,$2,$3,$4,$5,$6,$7)", &[&item.id,&item.username,&pending_backend,&item.storage_key,&item.media_type,&item.size,&item.created_at_ms])?;
                tx.commit()?;
                Ok(MediaReservation::Reserved)
            }
        }
    }

    pub(super) fn finish_media_upload(&self, item: &MediaItem) -> Result<bool> {
        // Do not resurrect an upload racing with account deletion.
        let sql = "UPDATE media_items SET size=?3, backend=?4 WHERE id=?1 AND username=?2 AND EXISTS(SELECT 1 FROM users WHERE username=?2 AND disabled=0)";
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                Ok(conn.execute(
                    sql,
                    params![item.id, item.username, item.size, item.backend],
                )? > 0)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                Ok(conn.execute("WITH active AS (SELECT username FROM users WHERE username=$2 AND disabled=false FOR UPDATE) UPDATE media_items SET size=$3, backend=$4 WHERE id=$1 AND username IN (SELECT username FROM active)", &[&item.id,&item.username,&item.size,&item.backend])? > 0)
            }
        }
    }

    pub(super) fn is_user_enabled(&self, username: &str) -> Result<bool> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let row: Option<i64> = conn
                    .query_row(
                        "SELECT disabled FROM users WHERE username=?1",
                        params![username],
                        |r| r.get(0),
                    )
                    .optional()?;
                Ok(matches!(row, Some(v) if v == 0))
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row =
                    conn.query_opt("SELECT disabled FROM users WHERE username=$1", &[&username])?;
                Ok(row.map(|r| !r.get::<_, bool>(0)).unwrap_or(false))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn enqueue_spool(
        &self,
        cfg: &RelayConfig,
        username: &str,
        method: &str,
        path: &str,
        query: &str,
        headers: &[(String, String)],
        body_b64: &str,
        body_len: i64,
    ) -> Result<()> {
        anyhow::ensure!(body_len >= 0, "invalid spool body length");
        let cutoff = now_ms().saturating_sub((cfg.spool_ttl_secs.min(30 * 86400) as i64) * 1000);
        let headers_json = serde_json::to_string(headers).unwrap_or_else(|_| "[]".to_string());
        let now = now_ms();
        let cap = cfg.spool_max_rows_per_user as i64;
        let byte_cap = cfg.spool_max_bytes_per_user as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let mut db_conn = self.open_sqlite_conn()?;
                let conn =
                    db_conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let enabled: bool = conn.query_row(
                    "SELECT disabled=0 FROM users WHERE username=?1",
                    [username],
                    |r| r.get(0),
                )?;
                anyhow::ensure!(enabled, "account disabled");
                conn.execute(
                    "DELETE FROM inbox_spool WHERE username=?1 AND created_at_ms < ?2",
                    params![username, cutoff],
                )?;
                if byte_cap > 0 {
                    if body_len > byte_cap {
                        anyhow::bail!("spool body exceeds per-user byte cap");
                    }
                    let used: i64 = conn.query_row(
                        "SELECT COALESCE(SUM(body_len), 0) FROM inbox_spool WHERE username=?1",
                        params![username],
                        |r| r.get(0),
                    )?;
                    let mut excess = used.saturating_add(body_len) - byte_cap;
                    if excess > 0 {
                        let mut stmt = conn.prepare(
                            "SELECT id, body_len FROM inbox_spool WHERE username=?1 ORDER BY created_at_ms ASC",
                        )?;
                        let mut rows = stmt.query(params![username])?;
                        let mut to_delete = Vec::new();
                        while let Some(row) = rows.next()? {
                            if excess <= 0 {
                                break;
                            }
                            let id: i64 = row.get(0)?;
                            let len: i64 = row.get(1)?;
                            to_delete.push(id);
                            excess -= len.max(0);
                        }
                        drop(rows);
                        drop(stmt);
                        for id in to_delete {
                            conn.execute("DELETE FROM inbox_spool WHERE id=?1", params![id])?;
                        }
                    }
                }
                conn.execute(
                    "INSERT INTO inbox_spool(username, created_at_ms, method, path, query, headers_json, body_b64, body_len) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![username, now, method, path, query, headers_json, body_b64, body_len],
                )?;

                let count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM inbox_spool WHERE username=?1",
                    params![username],
                    |r| r.get(0),
                )?;
                if cap > 0 && count > cap {
                    let excess = count - cap;
                    let _ = conn.execute(
                        "DELETE FROM inbox_spool WHERE id IN (SELECT id FROM inbox_spool WHERE username=?1 ORDER BY created_at_ms ASC LIMIT ?2)",
                        params![username, excess],
                    )?;
                }
                conn.commit()?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut db_conn = self.open_pg_conn()?;
                let mut conn = db_conn.transaction()?;
                let row = conn.query_one(
                    "SELECT disabled FROM users WHERE username=$1 FOR UPDATE",
                    &[&username],
                )?;
                let disabled: bool = row.get(0);
                anyhow::ensure!(!disabled, "account disabled");
                conn.execute(
                    "DELETE FROM inbox_spool WHERE username=$1 AND created_at_ms < $2",
                    &[&username, &cutoff],
                )?;
                if byte_cap > 0 {
                    if body_len > byte_cap {
                        anyhow::bail!("spool body exceeds per-user byte cap");
                    }
                    let row = conn.query_one(
                        "SELECT COALESCE(SUM(body_len)::bigint, 0) FROM inbox_spool WHERE username=$1",
                        &[&username],
                    )?;
                    let used: i64 = row.get(0);
                    let mut excess = used.saturating_add(body_len) - byte_cap;
                    if excess > 0 {
                        let rows = conn.query(
                            "SELECT id, body_len FROM inbox_spool WHERE username=$1 ORDER BY created_at_ms ASC",
                            &[&username],
                        )?;
                        let mut to_delete = Vec::new();
                        for r in rows {
                            if excess <= 0 {
                                break;
                            }
                            let id: i64 = r.get(0);
                            let len: i64 = r.get(1);
                            to_delete.push(id);
                            excess -= len.max(0);
                        }
                        for id in to_delete {
                            conn.execute("DELETE FROM inbox_spool WHERE id=$1", &[&id])?;
                        }
                    }
                }
                conn.execute(
                    "INSERT INTO inbox_spool(username, created_at_ms, method, path, query, headers_json, body_b64, body_len) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                    &[&username, &now, &method, &path, &query, &headers_json, &body_b64, &body_len],
                )?;
                let row = conn.query_one(
                    "SELECT COUNT(*) FROM inbox_spool WHERE username=$1",
                    &[&username],
                )?;
                let count: i64 = row.get(0);
                if cap > 0 && count > cap {
                    let excess = count - cap;
                    let _ = conn.execute(
                        "DELETE FROM inbox_spool WHERE id IN (SELECT id FROM inbox_spool WHERE username=$1 ORDER BY created_at_ms ASC LIMIT $2)",
                        &[&username, &excess],
                    )?;
                }
                conn.commit()?;
                Ok(())
            }
        }
    }

    pub(super) fn list_spool(&self, username: &str, limit: usize) -> Result<Vec<SpoolItem>> {
        let limit = limit.min(1000) as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT id, method, path, query, headers_json, body_b64, created_at_ms FROM inbox_spool WHERE username=?1 ORDER BY created_at_ms ASC LIMIT ?2",
                )?;
                let mut rows = stmt.query(params![username, limit])?;
                let mut out = Vec::new();
                while let Some(r) = rows.next()? {
                    out.push(SpoolItem {
                        id: r.get(0)?,
                        method: r.get(1)?,
                        path: r.get(2)?,
                        query: r.get(3)?,
                        headers_json: r.get(4)?,
                        body_b64: r.get(5)?,
                        created_at_ms: r.get(6)?,
                    });
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT id, method, path, query, headers_json, body_b64, created_at_ms FROM inbox_spool WHERE username=$1 ORDER BY created_at_ms ASC LIMIT $2",
                    &[&username, &limit],
                )?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(SpoolItem {
                        id: r.get(0),
                        method: r.get(1),
                        path: r.get(2),
                        query: r.get(3),
                        headers_json: r.get(4),
                        body_b64: r.get(5),
                        created_at_ms: r.get(6),
                    });
                }
                Ok(out)
            }
        }
    }

    pub(super) fn delete_spool_ids(&self, ids: &[i64]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        match self.driver {
            DbDriver::Sqlite => {
                let mut conn = self.open_sqlite_conn()?;
                let tx = conn.transaction()?;
                for chunk in ids.chunks(DB_BATCH_DELETE_MAX) {
                    let placeholders = std::iter::repeat_n("?", chunk.len())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let sql = format!("DELETE FROM inbox_spool WHERE id IN ({placeholders})");
                    let params: Vec<&dyn rusqlite::ToSql> =
                        chunk.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
                    let _ = tx.execute(&sql, rusqlite::params_from_iter(params))?;
                }
                tx.commit()?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                for chunk in ids.chunks(DB_BATCH_DELETE_MAX) {
                    let _ =
                        conn.execute("DELETE FROM inbox_spool WHERE id = ANY($1)", &[&chunk])?;
                }
                Ok(())
            }
        }
    }

    pub(super) fn cleanup_spool(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms().saturating_sub((ttl_secs.min(30 * 86400) as i64) * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM inbox_spool WHERE created_at_ms < ?1",
                    params![cutoff],
                )?;
                Ok(deleted as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM inbox_spool WHERE created_at_ms < $1",
                    &[&cutoff],
                )?;
                Ok(deleted)
            }
        }
    }

    pub(super) fn cleanup_move_notices(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms().saturating_sub((ttl_secs.min(30 * 86400) as i64) * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM move_notices WHERE created_at_ms < ?1",
                    params![cutoff],
                )?;
                let _ = conn.execute(
                    "DELETE FROM move_notice_fanout WHERE notice_id NOT IN (SELECT notice_id FROM move_notices)",
                    [],
                )?;
                Ok(deleted as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM move_notices WHERE created_at_ms < $1",
                    &[&cutoff],
                )?;
                let _ = conn.execute(
                    "DELETE FROM move_notice_fanout WHERE notice_id NOT IN (SELECT notice_id FROM move_notices)",
                    &[],
                )?;
                Ok(deleted)
            }
        }
    }

    pub(super) fn spool_stats(&self, username: &str) -> Result<(u64, u64)> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let (count, bytes): (i64, i64) = conn.query_row(
                    "SELECT COUNT(*), COALESCE(SUM(body_len), 0) FROM inbox_spool WHERE username=?1",
                    params![username],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                Ok((count.max(0) as u64, bytes.max(0) as u64))
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_one(
                    "SELECT COUNT(*), COALESCE(SUM(body_len)::bigint, 0) FROM inbox_spool WHERE username=$1",
                    &[&username],
                )?;
                let count: i64 = row.get(0);
                let bytes: i64 = row.get(1);
                Ok((count.max(0) as u64, bytes.max(0) as u64))
            }
        }
    }

    pub(super) fn upsert_peer(&self, relay_url: &str) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO relay_peers(relay_url, added_at_ms, last_seen_ms) VALUES (?1, ?2, NULL) ON CONFLICT(relay_url) DO NOTHING",
                    params![relay_url, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO relay_peers(relay_url, added_at_ms, last_seen_ms) VALUES ($1, $2, NULL) ON CONFLICT(relay_url) DO NOTHING",
                    &[&relay_url, &now],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn list_peers(&self) -> Result<Vec<(String, Option<i64>)>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT relay_url, last_seen_ms FROM relay_peers ORDER BY added_at_ms ASC",
                )?;
                let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
                Ok(rows.flatten().collect())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT relay_url, last_seen_ms FROM relay_peers ORDER BY added_at_ms ASC",
                    &[],
                )?;
                Ok(rows
                    .iter()
                    .map(|r| (r.get::<_, String>(0), r.get::<_, Option<i64>>(1)))
                    .collect())
            }
        }
    }

    pub(super) fn mark_peer_seen(&self, relay_url: &str) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "UPDATE relay_peers SET last_seen_ms=?2 WHERE relay_url=?1",
                    params![relay_url, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "UPDATE relay_peers SET last_seen_ms=$2 WHERE relay_url=$1",
                    &[&relay_url, &now],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn delete_peer(&self, relay_url: &str) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "DELETE FROM relay_peers WHERE relay_url=?1",
                    params![relay_url],
                )?;
                conn.execute(
                    "DELETE FROM remote_users WHERE relay_url=?1",
                    params![relay_url],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute("DELETE FROM relay_peers WHERE relay_url=$1", &[&relay_url])?;
                conn.execute("DELETE FROM remote_users WHERE relay_url=$1", &[&relay_url])?;
                Ok(())
            }
        }
    }

    pub(super) fn upsert_remote_user(&self, username: &str, relay_url: &str) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO remote_users(username, relay_url, updated_at_ms) VALUES (?1, ?2, ?3)
                     ON CONFLICT(username) DO UPDATE SET relay_url=excluded.relay_url, updated_at_ms=excluded.updated_at_ms",
                    params![username, relay_url, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO remote_users(username, relay_url, updated_at_ms) VALUES ($1, $2, $3)
                     ON CONFLICT(username) DO UPDATE SET relay_url=EXCLUDED.relay_url, updated_at_ms=EXCLUDED.updated_at_ms",
                    &[&username, &relay_url, &now],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn get_remote_user_relay(&self, username: &str) -> Result<Option<String>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT relay_url FROM remote_users WHERE username=?1",
                    params![username],
                    |r| r.get(0),
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT relay_url FROM remote_users WHERE username=$1",
                    &[&username],
                )?;
                Ok(row.map(|r| r.get(0)))
            }
        }
    }

    pub(super) fn prune_remote_users(&self, older_than_ms: i64) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "DELETE FROM remote_users WHERE updated_at_ms < ?1",
                    params![older_than_ms],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "DELETE FROM remote_users WHERE updated_at_ms < $1",
                    &[&older_than_ms],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn list_local_usernames(&self) -> Result<Vec<String>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT username FROM users WHERE disabled=0 ORDER BY username ASC LIMIT 5000",
                )?;
                let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
                Ok(rows.flatten().collect())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT username FROM users WHERE disabled=FALSE ORDER BY username ASC LIMIT 5000",
                    &[],
                )?;
                Ok(rows.iter().map(|r| r.get::<_, String>(0)).collect())
            }
        }
    }

    pub(super) fn user_token_matches(&self, username: &str, token: &str) -> Result<bool> {
        let hash = token_hash_hex(token);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let row: Option<(String, i64)> = conn
                    .query_row(
                        "SELECT token_sha256, disabled FROM users WHERE username=?1",
                        params![username],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                Ok(row
                    .map(|(h, d)| ct_eq_str(&h, &hash) && d == 0)
                    .unwrap_or(false))
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT token_sha256, disabled FROM users WHERE username=$1",
                    &[&username],
                )?;
                Ok(row
                    .map(|r| ct_eq_str(&r.get::<_, String>(0), &hash) && !r.get::<_, bool>(1))
                    .unwrap_or(false))
            }
        }
    }

    pub(super) fn upsert_relay_block(
        &self,
        kind: &str,
        value: &str,
        reason: Option<&str>,
        created_by: Option<&str>,
        expires_at_ms: Option<i64>,
    ) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO relay_blocks(kind, value, reason, created_at_ms, created_by, expires_at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(kind, value) DO UPDATE SET reason=excluded.reason, created_at_ms=excluded.created_at_ms,
                       created_by=excluded.created_by, expires_at_ms=excluded.expires_at_ms",
                    params![kind, value, reason, now, created_by, expires_at_ms],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO relay_blocks(kind, value, reason, created_at_ms, created_by, expires_at_ms)
                     VALUES ($1, $2, $3, $4, $5, $6)
                     ON CONFLICT(kind, value) DO UPDATE SET reason=EXCLUDED.reason, created_at_ms=EXCLUDED.created_at_ms,
                       created_by=EXCLUDED.created_by, expires_at_ms=EXCLUDED.expires_at_ms",
                    &[&kind, &value, &reason, &now, &created_by, &expires_at_ms],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn delete_relay_block(&self, kind: &str, value: &str) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "DELETE FROM relay_blocks WHERE kind=?1 AND value=?2",
                    params![kind, value],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "DELETE FROM relay_blocks WHERE kind=$1 AND value=$2",
                    &[&kind, &value],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn list_relay_blocks(&self) -> Result<Vec<RelayBlockRow>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT kind, value, reason, expires_at_ms FROM relay_blocks ORDER BY created_at_ms DESC LIMIT 5000",
                )?;
                let rows = stmt.query_map([], |r| {
                    Ok(RelayBlockRow {
                        kind: r.get(0)?,
                        value: r.get(1)?,
                        reason: r.get(2)?,
                        expires_at_ms: r.get(3)?,
                    })
                })?;
                Ok(rows.flatten().collect())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT kind, value, reason, expires_at_ms FROM relay_blocks ORDER BY created_at_ms DESC LIMIT 5000",
                    &[],
                )?;
                Ok(rows
                    .iter()
                    .map(|r| RelayBlockRow {
                        kind: r.get::<_, String>(0),
                        value: r.get::<_, String>(1),
                        reason: r.get::<_, Option<String>>(2),
                        expires_at_ms: r.get::<_, Option<i64>>(3),
                    })
                    .collect())
            }
        }
    }

    pub(super) fn replace_user_blocks(
        &self,
        username: &str,
        actors: &[String],
        domains: &[String],
    ) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let mut conn = self.open_sqlite_conn()?;
                let tx = conn.transaction()?;
                tx.execute(
                    "DELETE FROM user_blocks WHERE username=?1",
                    params![username],
                )?;
                for a in actors {
                    tx.execute(
                        "INSERT INTO user_blocks(username, kind, value, updated_at_ms) VALUES (?1, 'actor', ?2, ?3) ON CONFLICT DO NOTHING",
                        params![username, a, now],
                    )?;
                }
                for d in domains {
                    tx.execute(
                        "INSERT INTO user_blocks(username, kind, value, updated_at_ms) VALUES (?1, 'domain', ?2, ?3) ON CONFLICT DO NOTHING",
                        params![username, d, now],
                    )?;
                }
                tx.commit()?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let mut tx = conn.transaction()?;
                tx.execute("DELETE FROM user_blocks WHERE username=$1", &[&username])?;
                for a in actors {
                    tx.execute(
                        "INSERT INTO user_blocks(username, kind, value, updated_at_ms) VALUES ($1, 'actor', $2, $3) ON CONFLICT DO NOTHING",
                        &[&username, &a, &now],
                    )?;
                }
                for d in domains {
                    tx.execute(
                        "INSERT INTO user_blocks(username, kind, value, updated_at_ms) VALUES ($1, 'domain', $2, $3) ON CONFLICT DO NOTHING",
                        &[&username, &d, &now],
                    )?;
                }
                tx.commit()?;
                Ok(())
            }
        }
    }

    pub(super) fn list_user_blocks(&self, username: &str) -> Result<Vec<(String, String)>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt =
                    conn.prepare("SELECT kind, value FROM user_blocks WHERE username=?1")?;
                let rows = stmt.query_map(params![username], |r| Ok((r.get(0)?, r.get(1)?)))?;
                Ok(rows.flatten().collect())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT kind, value FROM user_blocks WHERE username=$1",
                    &[&username],
                )?;
                Ok(rows
                    .iter()
                    .map(|r| (r.get::<_, String>(0), r.get::<_, String>(1)))
                    .collect())
            }
        }
    }

    /// Relay-wide or per-user block match (actor exact, domain exact/suffix), honoring expiry.
    pub(super) fn is_blocked(&self, username: &str, sender_actor: &str) -> Result<bool> {
        let now = now_ms();
        let domain = origin_host(sender_actor).unwrap_or_default();
        let mut rules: Vec<(String, String, Option<i64>)> = Vec::new();
        for row in self.list_relay_blocks()? {
            rules.push((row.kind, row.value, row.expires_at_ms));
        }
        for (kind, value) in self.list_user_blocks(username)? {
            rules.push((kind, value, None));
        }
        for (kind, value, expires) in rules {
            if expires.map(|e| e <= now).unwrap_or(false) {
                continue;
            }
            let value = value.trim().trim_end_matches('/');
            if value.is_empty() {
                continue;
            }
            match kind.as_str() {
                "actor" if value == sender_actor.trim().trim_end_matches('/') => return Ok(true),
                "domain" if value == domain || domain.ends_with(&format!(".{value}")) => {
                    return Ok(true)
                }
                _ => {}
            }
        }
        Ok(false)
    }

    pub(super) fn upsert_actor_cache(&self, username: &str, actor_json: &str) -> Result<()> {
        let now = now_ms();
        let (actor_id, actor_url) = extract_actor_ids_from_json(actor_json);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO user_cache(username, actor_json, updated_at_ms, actor_id, actor_url) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(username) DO UPDATE SET actor_json=excluded.actor_json, updated_at_ms=excluded.updated_at_ms, actor_id=excluded.actor_id, actor_url=excluded.actor_url",
                    params![username, actor_json, now, actor_id, actor_url],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO user_cache(username, actor_json, updated_at_ms, actor_id, actor_url) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT(username) DO UPDATE SET actor_json=EXCLUDED.actor_json, updated_at_ms=EXCLUDED.updated_at_ms, actor_id=EXCLUDED.actor_id, actor_url=EXCLUDED.actor_url",
                    &[&username, &actor_json, &now, &actor_id, &actor_url],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn get_actor_cache(&self, username: &str) -> Result<Option<String>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT actor_json FROM user_cache WHERE username=?1",
                    params![username],
                    |r| r.get(0),
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT actor_json FROM user_cache WHERE username=$1",
                    &[&username],
                )?;
                Ok(row.map(|r| r.get(0)))
            }
        }
    }

    pub(super) fn upsert_collection_cache(
        &self,
        username: &str,
        kind: &str,
        json: &str,
    ) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO user_collection_cache(username, kind, json, updated_at_ms) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(username, kind) DO UPDATE SET json=excluded.json, updated_at_ms=excluded.updated_at_ms",
                    params![username, kind, json, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO user_collection_cache(username, kind, json, updated_at_ms) VALUES ($1, $2, $3, $4)
             ON CONFLICT(username, kind) DO UPDATE SET json=EXCLUDED.json, updated_at_ms=EXCLUDED.updated_at_ms",
                    &[&username, &kind, &json, &now],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn get_collection_cache(
        &self,
        username: &str,
        kind: &str,
    ) -> Result<Option<String>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT json FROM user_collection_cache WHERE username=?1 AND kind=?2",
                    params![username, kind],
                    |r| r.get(0),
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT json FROM user_collection_cache WHERE username=$1 AND kind=$2",
                    &[&username, &kind],
                )?;
                Ok(row.map(|r| r.get(0)))
            }
        }
    }

    pub(super) fn cleanup_peer_directory(&self, ttl_days: u32) -> Result<u64> {
        if ttl_days == 0 {
            return Ok(0);
        }
        let cutoff = now_ms().saturating_sub((ttl_days as i64) * 24 * 60 * 60 * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM peer_directory WHERE updated_at_ms < ?1",
                    params![cutoff],
                )?;
                Ok(deleted as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM peer_directory WHERE updated_at_ms < $1",
                    &[&cutoff],
                )?;
                Ok(deleted)
            }
        }
    }

    pub(super) fn cleanup_webrtc_signals(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms().saturating_sub((ttl_secs as i64) * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                Ok(conn.execute(
                    "DELETE FROM webrtc_signals WHERE created_at_ms < ?1",
                    params![cutoff],
                )? as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let deleted = conn.execute(
                    "DELETE FROM webrtc_signals WHERE created_at_ms < $1",
                    &[&cutoff],
                )?;
                Ok(deleted)
            }
        }
    }

    /// Retention for append-only tables that used to grow forever (M-02 / RD-05).
    pub(super) fn cleanup_admin_audit(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms().saturating_sub((ttl_secs as i64) * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                Ok(conn.execute(
                    "DELETE FROM admin_audit WHERE created_at_ms < ?1",
                    params![cutoff],
                )? as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                Ok(conn.execute(
                    "DELETE FROM admin_audit WHERE created_at_ms < $1",
                    &[&cutoff],
                )?)
            }
        }
    }

    pub(super) fn cleanup_relay_notes(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms().saturating_sub((ttl_secs as i64) * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "DELETE FROM relay_note_tags WHERE note_id IN (SELECT note_id FROM relay_notes WHERE created_at_ms < ?1)",
                    params![cutoff],
                )?;
                Ok(conn.execute(
                    "DELETE FROM relay_notes WHERE created_at_ms < ?1",
                    params![cutoff],
                )? as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "DELETE FROM relay_note_tags WHERE note_id IN (SELECT note_id FROM relay_notes WHERE created_at_ms < $1)",
                    &[&cutoff],
                )?;
                Ok(conn.execute(
                    "DELETE FROM relay_notes WHERE created_at_ms < $1",
                    &[&cutoff],
                )?)
            }
        }
    }

    pub(super) fn cleanup_peer_registry(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms().saturating_sub((ttl_secs as i64) * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                Ok(conn.execute(
                    "DELETE FROM peer_registry WHERE last_seen_ms < ?1",
                    params![cutoff],
                )? as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                Ok(conn.execute(
                    "DELETE FROM peer_registry WHERE last_seen_ms < $1",
                    &[&cutoff],
                )?)
            }
        }
    }

    pub(super) fn cleanup_relay_registry(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms().saturating_sub((ttl_secs as i64) * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                Ok(conn.execute(
                    "DELETE FROM relay_registry WHERE last_seen_ms < ?1",
                    params![cutoff],
                )? as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                Ok(conn.execute(
                    "DELETE FROM relay_registry WHERE last_seen_ms < $1",
                    &[&cutoff],
                )?)
            }
        }
    }

    pub(super) fn cleanup_relay_user_directory(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms().saturating_sub((ttl_secs as i64) * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                Ok(conn.execute(
                    "DELETE FROM relay_user_directory WHERE updated_at_ms < ?1",
                    params![cutoff],
                )? as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                Ok(conn.execute(
                    "DELETE FROM relay_user_directory WHERE updated_at_ms < $1",
                    &[&cutoff],
                )?)
            }
        }
    }

    /// Dedupe activities per (activity_id, recipient) with a short TTL (H-20 / RI-06).
    pub(super) fn insert_inbox_dedupe(&self, activity_id: &str, recipient: &str) -> Result<bool> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let changed = conn.execute(
                    "INSERT OR IGNORE INTO inbox_dedupe(activity_id, recipient, created_at_ms) VALUES (?1, ?2, ?3)",
                    params![activity_id, recipient, now],
                )?;
                Ok(changed > 0)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let changed = conn.execute(
                    "INSERT INTO inbox_dedupe(activity_id, recipient, created_at_ms) VALUES ($1, $2, $3) ON CONFLICT (activity_id, recipient) DO NOTHING",
                    &[&activity_id, &recipient, &now],
                )?;
                Ok(changed > 0)
            }
        }
    }

    pub(super) fn delete_inbox_dedupe(&self, activity_id: &str, recipient: &str) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "DELETE FROM inbox_dedupe WHERE activity_id = ?1 AND recipient = ?2",
                    params![activity_id, recipient],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "DELETE FROM inbox_dedupe WHERE activity_id = $1 AND recipient = $2",
                    &[&activity_id, &recipient],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn cleanup_inbox_dedupe(&self, ttl_secs: u64) -> Result<u64> {
        let cutoff = now_ms().saturating_sub((ttl_secs as i64) * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                Ok(conn.execute(
                    "DELETE FROM inbox_dedupe WHERE created_at_ms < ?1",
                    params![cutoff],
                )? as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                Ok(conn.execute(
                    "DELETE FROM inbox_dedupe WHERE created_at_ms < $1",
                    &[&cutoff],
                )?)
            }
        }
    }

    #[cfg(test)]
    pub(super) fn user_media_bytes(&self, username: &str) -> Result<u64> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let used: i64 = conn.query_row(
                    "SELECT COALESCE(SUM(size), 0) FROM media_items WHERE username=?1",
                    params![username],
                    |r| r.get(0),
                )?;
                Ok(used.max(0) as u64)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_one(
                    "SELECT COALESCE(SUM(size)::bigint, 0) FROM media_items WHERE username=$1",
                    &[&username],
                )?;
                let used: i64 = row.get(0);
                Ok(used.max(0) as u64)
            }
        }
    }

    pub(super) fn delete_media_item(&self, username: &str, id: &str) -> Result<bool> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let changed = conn.execute(
                    "DELETE FROM media_items WHERE username=?1 AND id=?2",
                    params![username, id],
                )?;
                Ok(changed > 0)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let changed = conn.execute(
                    "DELETE FROM media_items WHERE username=$1 AND id=$2",
                    &[&username, &id],
                )?;
                Ok(changed > 0)
            }
        }
    }

    pub(super) fn get_media_item(&self, username: &str, id: &str) -> Result<Option<MediaItem>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT id, username, backend, storage_key, media_type, size, created_at_ms FROM media_items WHERE username=?1 AND id=?2",
                    params![username, id],
                    |r| {
                        Ok(MediaItem {
                            id: r.get(0)?,
                            username: r.get(1)?,
                            backend: r.get(2)?,
                            storage_key: r.get(3)?,
                            media_type: r.get(4)?,
                            size: r.get(5)?,
                            created_at_ms: r.get(6)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT id, username, backend, storage_key, media_type, size, created_at_ms FROM media_items WHERE username=$1 AND id=$2",
                    &[&username, &id],
                )?;
                Ok(row.map(|r| MediaItem {
                    id: r.get(0),
                    username: r.get(1),
                    backend: r.get(2),
                    storage_key: r.get(3),
                    media_type: r.get(4),
                    size: r.get(5),
                    created_at_ms: r.get(6),
                }))
            }
        }
    }

    pub(super) fn upsert_relay_note(&self, note: &RelayNoteIndex) -> Result<()> {
        let published_ms = note.published_ms.unwrap_or(note.created_at_ms);
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO relay_notes(note_id, actor_id, published_ms, content_text, content_html, note_json, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)\n             ON CONFLICT(note_id) DO UPDATE SET\n               actor_id=excluded.actor_id,\n               published_ms=excluded.published_ms,\n               content_text=excluded.content_text,\n               content_html=excluded.content_html,\n               note_json=excluded.note_json",
                    params![
                        note.note_id,
                        note.actor_id,
                        published_ms,
                        note.content_text,
                        note.content_html,
                        note.note_json,
                        note.created_at_ms
                    ],
                )?;
                let tx = conn.unchecked_transaction()?;
                tx.execute(
                    "DELETE FROM relay_note_tags WHERE note_id=?1",
                    params![note.note_id],
                )?;
                for tag in &note.tags {
                    tx.execute(
                        "INSERT OR IGNORE INTO relay_note_tags(note_id, tag) VALUES (?1, ?2)",
                        params![note.note_id, tag],
                    )?;
                }
                tx.commit()?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let mut tx = conn.transaction()?;
                let params: &[&(dyn ToSql + Sync)] = &[
                    &note.note_id,
                    &note.actor_id,
                    &published_ms,
                    &note.content_text,
                    &note.content_html,
                    &note.note_json,
                    &note.created_at_ms,
                ];
                tx.execute(
                    "INSERT INTO relay_notes(note_id, actor_id, published_ms, content_text, content_html, note_json, created_at_ms) VALUES ($1, $2, $3, $4, $5, $6, $7)\n             ON CONFLICT(note_id) DO UPDATE SET\n               actor_id=EXCLUDED.actor_id,\n               published_ms=EXCLUDED.published_ms,\n               content_text=EXCLUDED.content_text,\n               content_html=EXCLUDED.content_html,\n               note_json=EXCLUDED.note_json",
                    params,
                )?;
                tx.execute(
                    "DELETE FROM relay_note_tags WHERE note_id=$1",
                    &[&note.note_id],
                )?;
                for tag in &note.tags {
                    tx.execute(
                        "INSERT INTO relay_note_tags(note_id, tag) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                        &[&note.note_id, &tag],
                    )?;
                }
                tx.commit()?;
                Ok(())
            }
        }
    }

    pub(super) fn search_relay_notes(
        &self,
        q: &str,
        tag: &str,
        limit: u32,
        cursor: Option<i64>,
        since: Option<i64>,
        total_mode: SearchTotalMode,
    ) -> Result<CollectionPage<String>> {
        let limit = limit.clamp(1, 200) as i64;
        let q_norm = q.trim().to_lowercase();
        let tag_norm = tag.trim().trim_start_matches('#').to_lowercase();
        let q_like = if q_norm.is_empty() {
            "%".to_string()
        } else {
            format!("%{}%", escape_like(&q_norm))
        };
        let tag_like = if tag_norm.is_empty() {
            "%".to_string()
        } else {
            format!("%{}%", escape_like(&tag_norm))
        };

        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let total_exact: u64 = if total_mode == SearchTotalMode::Exact {
                    if !tag_norm.is_empty() {
                        conn.query_row(
                            r#"
                SELECT COUNT(*)
                FROM relay_note_tags t
                JOIN relay_notes n ON n.note_id = t.note_id
                WHERE lower(t.tag) LIKE ?1 ESCAPE '\\'
                "#,
                            params![tag_like],
                            |r| r.get(0),
                        )
                        .unwrap_or(0)
                    } else {
                        conn.query_row(
                            "SELECT COUNT(*) FROM relay_notes WHERE lower(content_text) LIKE ?1 ESCAPE '\\'",
                            params![q_like],
                            |r| r.get(0),
                        )
                        .unwrap_or(0)
                    }
                } else {
                    0
                };

                let mut stmt;
                let mut rows;
                if !tag_norm.is_empty() {
                    if let Some(since) = since {
                        stmt = conn.prepare(
                            r#"
                    SELECT n.note_json, n.created_at_ms
                    FROM relay_note_tags t
                    JOIN relay_notes n ON n.note_id = t.note_id
                    WHERE lower(t.tag) LIKE ?1 ESCAPE '\\' AND n.created_at_ms > ?2
                    ORDER BY n.created_at_ms DESC
                    LIMIT ?3
                    "#,
                        )?;
                        rows = stmt.query(params![tag_like, since, limit])?;
                    } else if let Some(cur) = cursor {
                        stmt = conn.prepare(
                            r#"
                    SELECT n.note_json, n.created_at_ms
                    FROM relay_note_tags t
                    JOIN relay_notes n ON n.note_id = t.note_id
                    WHERE lower(t.tag) LIKE ?1 ESCAPE '\\' AND n.created_at_ms < ?2
                    ORDER BY n.created_at_ms DESC
                    LIMIT ?3
                    "#,
                        )?;
                        rows = stmt.query(params![tag_like, cur, limit])?;
                    } else {
                        stmt = conn.prepare(
                            r#"
                    SELECT n.note_json, n.created_at_ms
                    FROM relay_note_tags t
                    JOIN relay_notes n ON n.note_id = t.note_id
                    WHERE lower(t.tag) LIKE ?1 ESCAPE '\\'
                    ORDER BY n.created_at_ms DESC
                    LIMIT ?2
                    "#,
                        )?;
                        rows = stmt.query(params![tag_like, limit])?;
                    }
                } else if let Some(since) = since {
                    stmt = conn.prepare(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE (lower(content_text) LIKE ?1 ESCAPE '\\') AND created_at_ms > ?2 ORDER BY created_at_ms DESC LIMIT ?3",
                    )?;
                    rows = stmt.query(params![q_like, since, limit])?;
                } else if let Some(cur) = cursor {
                    stmt = conn.prepare(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE (lower(content_text) LIKE ?1 ESCAPE '\\') AND created_at_ms < ?2 ORDER BY created_at_ms DESC LIMIT ?3",
                    )?;
                    rows = stmt.query(params![q_like, cur, limit])?;
                } else {
                    stmt = conn.prepare(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE lower(content_text) LIKE ?1 ESCAPE '\\' ORDER BY created_at_ms DESC LIMIT ?2",
                    )?;
                    rows = stmt.query(params![q_like, limit])?;
                }
                let mut items = Vec::<String>::new();
                let mut last_created = None;
                while let Some(row) = rows.next()? {
                    let note_json: String = row.get(0)?;
                    let created_at_ms: i64 = row.get(1)?;
                    last_created = Some(created_at_ms);
                    items.push(note_json);
                }
                let next = if items.len() as i64 == limit {
                    last_created.map(|v| v.to_string())
                } else {
                    None
                };
                let total = match total_mode {
                    SearchTotalMode::Exact => total_exact,
                    SearchTotalMode::Approx => {
                        if !tag_norm.is_empty() {
                            let n: i64 = conn
                                .query_row(
                                    "SELECT COALESCE(SUM(count), 0) FROM relay_tag_counts WHERE lower(tag) LIKE ?1 ESCAPE '\\'",
                                    params![tag_like],
                                    |r| r.get(0),
                                )
                                .unwrap_or(0);
                            n.max(0) as u64
                        } else if q_norm.is_empty() {
                            let n: i64 = conn
                                .query_row(
                                    "SELECT count FROM relay_notes_count WHERE id = 1",
                                    [],
                                    |r| r.get(0),
                                )
                                .unwrap_or(0);
                            n.max(0) as u64
                        } else {
                            items.len() as u64
                        }
                    }
                    SearchTotalMode::None => 0,
                };
                Ok(CollectionPage { total, items, next })
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let total_exact: u64 = if total_mode == SearchTotalMode::Exact {
                    if !tag_norm.is_empty() {
                        let row = conn.query_one(
                            r#"
                SELECT COUNT(*)
                FROM relay_note_tags t
                JOIN relay_notes n ON n.note_id = t.note_id
                WHERE t.tag_tsv @@ plainto_tsquery('simple', $1)
                "#,
                            &[&tag_norm],
                        )?;
                        let n: i64 = row.get(0);
                        n.max(0) as u64
                    } else if !q_norm.is_empty() {
                        let row = conn.query_one(
                            "SELECT COUNT(*) FROM relay_notes WHERE search_tsv @@ plainto_tsquery('simple', $1)",
                            &[&q_norm],
                        )?;
                        let n: i64 = row.get(0);
                        n.max(0) as u64
                    } else {
                        let row = conn.query_one("SELECT COUNT(*) FROM relay_notes", &[])?;
                        let n: i64 = row.get(0);
                        n.max(0) as u64
                    }
                } else {
                    0
                };

                let rows = if !tag_norm.is_empty() {
                    if let Some(since) = since {
                        conn.query(
                            r#"
                    SELECT n.note_json, n.created_at_ms
                    FROM relay_note_tags t
                    JOIN relay_notes n ON n.note_id = t.note_id
                    WHERE t.tag_tsv @@ plainto_tsquery('simple', $1) AND n.created_at_ms > $2
                    ORDER BY n.created_at_ms DESC
                    LIMIT $3
                    "#,
                            &[&tag_norm, &since, &limit],
                        )?
                    } else if let Some(cur) = cursor {
                        conn.query(
                            r#"
                    SELECT n.note_json, n.created_at_ms
                    FROM relay_note_tags t
                    JOIN relay_notes n ON n.note_id = t.note_id
                    WHERE t.tag_tsv @@ plainto_tsquery('simple', $1) AND n.created_at_ms < $2
                    ORDER BY n.created_at_ms DESC
                    LIMIT $3
                    "#,
                            &[&tag_norm, &cur, &limit],
                        )?
                    } else {
                        conn.query(
                            r#"
                    SELECT n.note_json, n.created_at_ms
                    FROM relay_note_tags t
                    JOIN relay_notes n ON n.note_id = t.note_id
                    WHERE t.tag_tsv @@ plainto_tsquery('simple', $1)
                    ORDER BY n.created_at_ms DESC
                    LIMIT $2
                    "#,
                            &[&tag_norm, &limit],
                        )?
                    }
                } else if let (false, Some(since_val)) = (q_norm.is_empty(), since) {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE search_tsv @@ plainto_tsquery('simple', $1) AND created_at_ms > $2 ORDER BY created_at_ms DESC LIMIT $3",
                        &[&q_norm, &since_val, &limit],
                    )?
                } else if let (false, Some(cur)) = (q_norm.is_empty(), cursor) {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE search_tsv @@ plainto_tsquery('simple', $1) AND created_at_ms < $2 ORDER BY created_at_ms DESC LIMIT $3",
                        &[&q_norm, &cur, &limit],
                    )?
                } else if !q_norm.is_empty() {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE search_tsv @@ plainto_tsquery('simple', $1) ORDER BY created_at_ms DESC LIMIT $2",
                        &[&q_norm, &limit],
                    )?
                } else if let Some(since) = since {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE created_at_ms > $1 ORDER BY created_at_ms DESC LIMIT $2",
                        &[&since, &limit],
                    )?
                } else if let Some(cur) = cursor {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE created_at_ms < $1 ORDER BY created_at_ms DESC LIMIT $2",
                        &[&cur, &limit],
                    )?
                } else {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes ORDER BY created_at_ms DESC LIMIT $1",
                        &[&limit],
                    )?
                };

                let mut items = Vec::<String>::new();
                let mut last_created = None;
                for row in rows {
                    let note_json: String = row.get(0);
                    let created_at_ms: i64 = row.get(1);
                    last_created = Some(created_at_ms);
                    items.push(note_json);
                }
                let next = if items.len() as i64 == limit {
                    last_created.map(|v| v.to_string())
                } else {
                    None
                };
                let total = match total_mode {
                    SearchTotalMode::Exact => total_exact,
                    SearchTotalMode::Approx => {
                        if !tag_norm.is_empty() {
                            let row = conn.query_one(
                                "SELECT COALESCE(SUM(count)::bigint, 0) FROM relay_tag_counts WHERE lower(tag) LIKE $1 ESCAPE '\\'",
                                &[&tag_like],
                            )?;
                            let n: i64 = row.get(0);
                            n.max(0) as u64
                        } else if q_norm.is_empty() {
                            let row = conn.query_one(
                                "SELECT count FROM relay_notes_count WHERE id = 1",
                                &[],
                            )?;
                            let n: i64 = row.get(0);
                            n.max(0) as u64
                        } else {
                            items.len() as u64
                        }
                    }
                    SearchTotalMode::None => 0,
                };
                Ok(CollectionPage { total, items, next })
            }
        }
    }

    pub(super) fn list_relay_notes_sync(
        &self,
        limit: u32,
        since: Option<i64>,
        cursor: Option<i64>,
    ) -> Result<CollectionPage<(String, i64)>> {
        let limit = limit.clamp(1, 200) as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt;
                let mut rows;
                if let Some(since) = since {
                    stmt = conn.prepare(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE created_at_ms > ?1 ORDER BY created_at_ms DESC LIMIT ?2",
                    )?;
                    rows = stmt.query(params![since, limit])?;
                } else if let Some(cur) = cursor {
                    stmt = conn.prepare(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE created_at_ms < ?1 ORDER BY created_at_ms DESC LIMIT ?2",
                    )?;
                    rows = stmt.query(params![cur, limit])?;
                } else {
                    stmt = conn.prepare(
                        "SELECT note_json, created_at_ms FROM relay_notes ORDER BY created_at_ms DESC LIMIT ?1",
                    )?;
                    rows = stmt.query(params![limit])?;
                }
                let mut items = Vec::<(String, i64)>::new();
                let mut last_created = None;
                while let Some(row) = rows.next()? {
                    let note_json: String = row.get(0)?;
                    let created_at_ms: i64 = row.get(1)?;
                    last_created = Some(created_at_ms);
                    items.push((note_json, created_at_ms));
                }
                let next = if items.len() as i64 == limit {
                    last_created.map(|v| v.to_string())
                } else {
                    None
                };
                Ok(CollectionPage {
                    total: items.len() as u64,
                    items,
                    next,
                })
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = if let Some(since) = since {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE created_at_ms > $1 ORDER BY created_at_ms DESC LIMIT $2",
                        &[&since, &limit],
                    )?
                } else if let Some(cur) = cursor {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes WHERE created_at_ms < $1 ORDER BY created_at_ms DESC LIMIT $2",
                        &[&cur, &limit],
                    )?
                } else {
                    conn.query(
                        "SELECT note_json, created_at_ms FROM relay_notes ORDER BY created_at_ms DESC LIMIT $1",
                        &[&limit],
                    )?
                };
                let mut items = Vec::<(String, i64)>::new();
                let mut last_created = None;
                for row in rows {
                    let note_json: String = row.get(0);
                    let created_at_ms: i64 = row.get(1);
                    last_created = Some(created_at_ms);
                    items.push((note_json, created_at_ms));
                }
                let next = if items.len() as i64 == limit {
                    last_created.map(|v| v.to_string())
                } else {
                    None
                };
                Ok(CollectionPage {
                    total: items.len() as u64,
                    items,
                    next,
                })
            }
        }
    }

    pub(super) fn search_relay_users(
        &self,
        q: &str,
        limit: u32,
        cursor: Option<i64>,
        base_template: &str,
        total_mode: SearchTotalMode,
    ) -> Result<CollectionPage<String>> {
        let limit = limit.clamp(1, 200) as i64;
        let q_norm = q.trim().to_lowercase();
        let q_like = if q_norm.is_empty() {
            "%".to_string()
        } else {
            format!("%{}%", escape_like(&q_norm))
        };
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let total_exact: u64 = if total_mode == SearchTotalMode::Exact {
                    let total_cache: u64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM user_cache WHERE lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_id) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\'",
                            params![q_like],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    let total_users: u64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM users WHERE disabled=0 AND lower(username) LIKE ?1 ESCAPE '\\'",
                            params![q_like],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    let total_peers: u64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM peer_directory WHERE lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\'",
                            params![q_like],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    let total_relay_users: u64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM relay_user_directory WHERE lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\'",
                            params![q_like],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    total_cache
                        .saturating_add(total_users)
                        .saturating_add(total_peers)
                        .saturating_add(total_relay_users)
                } else {
                    0
                };

                let mut stmt = if cursor.is_some() {
                    conn.prepare(
                        "SELECT actor_json, updated_at_ms FROM user_cache WHERE (lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_id) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\') AND updated_at_ms < ?2 ORDER BY updated_at_ms DESC LIMIT ?3",
                    )?
                } else {
                    conn.prepare(
                        "SELECT actor_json, updated_at_ms FROM user_cache WHERE lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_id) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT ?2",
                    )?
                };
                let mut rows = if let Some(cur) = cursor {
                    stmt.query(params![q_like, cur, limit])?
                } else {
                    stmt.query(params![q_like, limit])?
                };
                let mut items = Vec::<String>::new();
                let mut seen_ids = std::collections::HashSet::new();
                let mut last_updated = None;
                while let Some(row) = rows.next()? {
                    let actor_json: String = row.get(0)?;
                    let updated_at_ms: i64 = row.get(1)?;
                    last_updated = Some(updated_at_ms);
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                        if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                            if seen_ids.insert(id.to_string()) {
                                items.push(actor_json);
                            }
                        } else {
                            items.push(actor_json);
                        }
                    }
                }

                let mut stmt_users = conn.prepare(
                    "SELECT username, created_at_ms FROM users WHERE disabled=0 AND lower(username) LIKE ?1 ESCAPE '\\' ORDER BY created_at_ms DESC LIMIT ?2",
                )?;
                let mut user_rows = stmt_users.query(params![q_like, limit])?;
                while let Some(row) = user_rows.next()? {
                    let username: String = row.get(0)?;
                    if let Some(actor_json) = self.get_actor_cache(&username).unwrap_or(None) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                            if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                                if seen_ids.insert(id.to_string()) {
                                    items.push(actor_json);
                                }
                                continue;
                            }
                        }
                    }
                    let stub = actor_stub_json(&username, base_template);
                    let actor_json = serde_json::to_string(&stub).unwrap_or_default();
                    if let Some(id) = stub.get("id").and_then(|i| i.as_str()) {
                        if seen_ids.insert(id.to_string()) {
                            items.push(actor_json);
                        }
                    }
                }

                let mut stmt_peers = conn.prepare(
                    "SELECT username, actor_url FROM peer_directory WHERE lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT ?2",
                )?;
                let mut peer_rows = stmt_peers.query(params![q_like, limit])?;
                while let Some(row) = peer_rows.next()? {
                    let username: String = row.get(0)?;
                    let actor_url: String = row.get(1)?;
                    if let Some(actor_json) = self.get_actor_cache(&username).unwrap_or(None) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                            if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                                if seen_ids.insert(id.to_string()) {
                                    items.push(actor_json);
                                }
                                continue;
                            }
                        }
                    }
                    let stub = actor_stub_from_actor_url(&username, &actor_url, base_template);
                    let actor_json = serde_json::to_string(&stub).unwrap_or_default();
                    if let Some(id) = stub.get("id").and_then(|i| i.as_str()) {
                        if seen_ids.insert(id.to_string()) {
                            items.push(actor_json);
                        }
                    }
                }

                let mut stmt_remote = conn.prepare(
                    "SELECT username, actor_url FROM relay_user_directory WHERE lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT ?2",
                )?;
                let mut remote_rows = stmt_remote.query(params![q_like, limit])?;
                while let Some(row) = remote_rows.next()? {
                    let username: String = row.get(0)?;
                    let actor_url: String = row.get(1)?;
                    if let Some(actor_json) = self.get_actor_cache(&username).unwrap_or(None) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                            if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                                if seen_ids.insert(id.to_string()) {
                                    items.push(actor_json);
                                }
                                continue;
                            }
                        }
                    }
                    let stub = actor_stub_from_actor_url(&username, &actor_url, base_template);
                    let actor_json = serde_json::to_string(&stub).unwrap_or_default();
                    if let Some(id) = stub.get("id").and_then(|i| i.as_str()) {
                        if seen_ids.insert(id.to_string()) {
                            items.push(actor_json);
                        }
                    }
                }

                let next = if items.len() as i64 == limit {
                    last_updated.map(|v| v.to_string())
                } else {
                    None
                };
                let total = match total_mode {
                    SearchTotalMode::Exact => total_exact,
                    SearchTotalMode::Approx => items.len() as u64,
                    SearchTotalMode::None => 0,
                };
                Ok(CollectionPage { total, items, next })
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let total_exact: u64 = if total_mode == SearchTotalMode::Exact {
                    let total_cache: u64 = {
                        let row = conn.query_one(
                            "SELECT COUNT(*) FROM user_cache WHERE lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_id) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\'",
                            &[&q_like],
                        )?;
                        let n: i64 = row.get(0);
                        n.max(0) as u64
                    };
                    let total_users: u64 = {
                        let row = conn.query_one(
                            "SELECT COUNT(*) FROM users WHERE disabled=false AND lower(username) LIKE $1 ESCAPE '\\'",
                            &[&q_like],
                        )?;
                        let n: i64 = row.get(0);
                        n.max(0) as u64
                    };
                    let total_peers: u64 = {
                        let row = conn.query_one(
                            "SELECT COUNT(*) FROM peer_directory WHERE lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\'",
                            &[&q_like],
                        )?;
                        let n: i64 = row.get(0);
                        n.max(0) as u64
                    };
                    let total_relay_users: u64 = {
                        let row = conn.query_one(
                            "SELECT COUNT(*) FROM relay_user_directory WHERE lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\'",
                            &[&q_like],
                        )?;
                        let n: i64 = row.get(0);
                        n.max(0) as u64
                    };
                    total_cache
                        .saturating_add(total_users)
                        .saturating_add(total_peers)
                        .saturating_add(total_relay_users)
                } else {
                    0
                };

                let rows = if let Some(cur) = cursor {
                    conn.query(
                        "SELECT actor_json, updated_at_ms FROM user_cache WHERE (lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_id) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\') AND updated_at_ms < $2 ORDER BY updated_at_ms DESC LIMIT $3",
                        &[&q_like, &cur, &limit],
                    )?
                } else {
                    conn.query(
                        "SELECT actor_json, updated_at_ms FROM user_cache WHERE lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_id) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT $2",
                        &[&q_like, &limit],
                    )?
                };
                let mut items = Vec::<String>::new();
                let mut seen_ids = std::collections::HashSet::new();
                let mut last_updated = None;
                for row in rows {
                    let actor_json: String = row.get(0);
                    let updated_at_ms: i64 = row.get(1);
                    last_updated = Some(updated_at_ms);
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                        if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                            if seen_ids.insert(id.to_string()) {
                                items.push(actor_json);
                            }
                        } else {
                            items.push(actor_json);
                        }
                    }
                }

                let user_rows = conn.query(
                    "SELECT username, created_at_ms FROM users WHERE disabled=false AND lower(username) LIKE $1 ESCAPE '\\' ORDER BY created_at_ms DESC LIMIT $2",
                    &[&q_like, &limit],
                )?;
                for row in user_rows {
                    let username: String = row.get(0);
                    if let Some(actor_json) = self.get_actor_cache(&username).unwrap_or(None) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                            if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                                if seen_ids.insert(id.to_string()) {
                                    items.push(actor_json);
                                }
                                continue;
                            }
                        }
                    }
                    let stub = actor_stub_json(&username, base_template);
                    let actor_json = serde_json::to_string(&stub).unwrap_or_default();
                    if let Some(id) = stub.get("id").and_then(|i| i.as_str()) {
                        if seen_ids.insert(id.to_string()) {
                            items.push(actor_json);
                        }
                    }
                }

                let peer_rows = conn.query(
                    "SELECT username, actor_url FROM peer_directory WHERE lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT $2",
                    &[&q_like, &limit],
                )?;
                for row in peer_rows {
                    let username: String = row.get(0);
                    let actor_url: String = row.get(1);
                    if let Some(actor_json) = self.get_actor_cache(&username).unwrap_or(None) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                            if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                                if seen_ids.insert(id.to_string()) {
                                    items.push(actor_json);
                                }
                                continue;
                            }
                        }
                    }
                    let stub = actor_stub_from_actor_url(&username, &actor_url, base_template);
                    let actor_json = serde_json::to_string(&stub).unwrap_or_default();
                    if let Some(id) = stub.get("id").and_then(|i| i.as_str()) {
                        if seen_ids.insert(id.to_string()) {
                            items.push(actor_json);
                        }
                    }
                }

                let remote_rows = conn.query(
                    "SELECT username, actor_url FROM relay_user_directory WHERE lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT $2",
                    &[&q_like, &limit],
                )?;
                for row in remote_rows {
                    let username: String = row.get(0);
                    let actor_url: String = row.get(1);
                    if let Some(actor_json) = self.get_actor_cache(&username).unwrap_or(None) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&actor_json) {
                            if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                                if seen_ids.insert(id.to_string()) {
                                    items.push(actor_json);
                                }
                                continue;
                            }
                        }
                    }
                    let stub = actor_stub_from_actor_url(&username, &actor_url, base_template);
                    let actor_json = serde_json::to_string(&stub).unwrap_or_default();
                    if let Some(id) = stub.get("id").and_then(|i| i.as_str()) {
                        if seen_ids.insert(id.to_string()) {
                            items.push(actor_json);
                        }
                    }
                }

                let next = if items.len() as i64 == limit {
                    last_updated.map(|v| v.to_string())
                } else {
                    None
                };
                let total = match total_mode {
                    SearchTotalMode::Exact => total_exact,
                    SearchTotalMode::Approx => items.len() as u64,
                    SearchTotalMode::None => 0,
                };
                Ok(CollectionPage { total, items, next })
            }
        }
    }

    pub(super) fn search_relay_tags(&self, q: &str, limit: u32) -> Result<Vec<(String, u64)>> {
        let limit = limit.clamp(1, 200) as i64;
        let q_norm = q.trim().trim_start_matches('#').to_lowercase();
        let q_like = if q_norm.is_empty() {
            "%".to_string()
        } else {
            format!("%{}%", escape_like(&q_norm))
        };
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    r#"
            SELECT tag, count
            FROM relay_tag_counts
            WHERE lower(tag) LIKE ?1 ESCAPE '\\'
            ORDER BY count DESC
            LIMIT ?2
            "#,
                )?;
                let mut rows = stmt.query(params![q_like, limit])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    let tag: String = row.get(0)?;
                    let count: u64 = row.get(1)?;
                    out.push((tag, count));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    r#"
            SELECT tag, count
            FROM relay_tag_counts
            WHERE lower(tag) LIKE $1 ESCAPE '\\'
            ORDER BY count DESC
            LIMIT $2
            "#,
                    &[&q_like, &limit],
                )?;
                let mut out = Vec::new();
                for row in rows {
                    let tag: String = row.get(0);
                    let count: i64 = row.get(1);
                    out.push((tag, count.max(0) as u64));
                }
                Ok(out)
            }
        }
    }

    pub(super) fn upsert_peer_directory(
        &self,
        peer_id: &str,
        username: &str,
        actor_url: &str,
    ) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO peer_directory(peer_id, username, actor_url, updated_at_ms) VALUES (?1, ?2, ?3, ?4)\n             ON CONFLICT(peer_id) DO UPDATE SET username=excluded.username, actor_url=excluded.actor_url, updated_at_ms=excluded.updated_at_ms",
                    params![peer_id, username, actor_url, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO peer_directory(peer_id, username, actor_url, updated_at_ms) VALUES ($1, $2, $3, $4)\n             ON CONFLICT(peer_id) DO UPDATE SET username=EXCLUDED.username, actor_url=EXCLUDED.actor_url, updated_at_ms=EXCLUDED.updated_at_ms",
                    &[&peer_id, &username, &actor_url, &now],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn list_peer_directory(
        &self,
        q: &str,
        limit: u32,
    ) -> Result<Vec<(String, String, String)>> {
        let limit = limit.clamp(1, 200) as i64;
        let q_norm = q.trim().to_lowercase();
        let q_like = if q_norm.is_empty() {
            "%".to_string()
        } else {
            format!("%{}%", escape_like(&q_norm))
        };
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT peer_id, username, actor_url FROM peer_directory WHERE lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT ?2",
                )?;
                let mut rows = stmt.query(params![q_like, limit])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    out.push((row.get(0)?, row.get(1)?, row.get(2)?));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT peer_id, username, actor_url FROM peer_directory WHERE lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT $2",
                    &[&q_like, &limit],
                )?;
                let mut out = Vec::new();
                for row in rows {
                    out.push((row.get(0), row.get(1), row.get(2)));
                }
                Ok(out)
            }
        }
    }

    pub(super) fn upsert_relay_user_directory(
        &self,
        username: &str,
        actor_url: &str,
        relay_url: &str,
    ) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO relay_user_directory(actor_url, username, relay_url, updated_at_ms) VALUES (?1, ?2, ?3, ?4)\n             ON CONFLICT(actor_url) DO UPDATE SET username=excluded.username, relay_url=excluded.relay_url, updated_at_ms=excluded.updated_at_ms",
                    params![actor_url, username, relay_url, now],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO relay_user_directory(actor_url, username, relay_url, updated_at_ms) VALUES ($1, $2, $3, $4)\n             ON CONFLICT(actor_url) DO UPDATE SET username=EXCLUDED.username, relay_url=EXCLUDED.relay_url, updated_at_ms=EXCLUDED.updated_at_ms",
                    &[&actor_url, &username, &relay_url, &now],
                )?;
                Ok(())
            }
        }
    }

    #[allow(dead_code)]
    pub(super) fn list_relay_user_directory(
        &self,
        q: &str,
        limit: u32,
    ) -> Result<Vec<(String, String, String)>> {
        let limit = limit.clamp(1, 200) as i64;
        let q_norm = q.trim().to_lowercase();
        let q_like = if q_norm.is_empty() {
            "%".to_string()
        } else {
            format!("%{}%", escape_like(&q_norm))
        };
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    "SELECT username, actor_url, relay_url FROM relay_user_directory WHERE lower(username) LIKE ?1 ESCAPE '\\' OR lower(actor_url) LIKE ?1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT ?2",
                )?;
                let mut rows = stmt.query(params![q_like, limit])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    out.push((row.get(0)?, row.get(1)?, row.get(2)?));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    "SELECT username, actor_url, relay_url FROM relay_user_directory WHERE lower(username) LIKE $1 ESCAPE '\\' OR lower(actor_url) LIKE $1 ESCAPE '\\' ORDER BY updated_at_ms DESC LIMIT $2",
                    &[&q_like, &limit],
                )?;
                let mut out = Vec::new();
                for row in rows {
                    out.push((row.get(0), row.get(1), row.get(2)));
                }
                Ok(out)
            }
        }
    }

    pub(super) fn upsert_outbox_index_state(&self, username: &str, ok: bool) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    "INSERT INTO relay_outbox_index(username, last_index_ms, last_ok) VALUES (?1, ?2, ?3)\n             ON CONFLICT(username) DO UPDATE SET last_index_ms=excluded.last_index_ms, last_ok=excluded.last_ok",
                    params![username, now, if ok { 1 } else { 0 }],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    "INSERT INTO relay_outbox_index(username, last_index_ms, last_ok) VALUES ($1, $2, $3)\n             ON CONFLICT(username) DO UPDATE SET last_index_ms=EXCLUDED.last_index_ms, last_ok=EXCLUDED.last_ok",
                    &[&username, &now, &ok],
                )?;
                Ok(())
            }
        }
    }

    pub(super) fn count_outbox_indexed_since(&self, cutoff_ms: i64) -> Result<u64> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT COUNT(*) FROM relay_outbox_index WHERE last_index_ms >= ?1 AND last_ok=1",
                    params![cutoff_ms],
                    |r| r.get(0),
                )
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_one(
                    "SELECT COUNT(*) FROM relay_outbox_index WHERE last_index_ms >= $1 AND last_ok=true",
                    &[&cutoff_ms],
                )?;
                let n: i64 = row.get(0);
                Ok(n.max(0) as u64)
            }
        }
    }

    pub(super) fn get_outbox_index_state(&self, username: &str) -> Result<Option<(i64, bool)>> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.query_row(
                    "SELECT last_index_ms, last_ok FROM relay_outbox_index WHERE username=?1",
                    params![username],
                    |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0)),
                )
                .optional()
                .map_err(Into::into)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_opt(
                    "SELECT last_index_ms, last_ok FROM relay_outbox_index WHERE username=$1",
                    &[&username],
                )?;
                Ok(row.map(|r| (r.get(0), r.get(1))))
            }
        }
    }

    pub(super) fn count_webrtc_signals_for_peer(&self, to_peer_id: &str) -> Result<u64> {
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let n: u64 = conn.query_row(
                    "SELECT COUNT(*) FROM webrtc_signals WHERE to_peer_id=?1",
                    params![to_peer_id],
                    |r| r.get(0),
                )?;
                Ok(n)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let row = conn.query_one(
                    "SELECT COUNT(*) FROM webrtc_signals WHERE to_peer_id=$1",
                    &[&to_peer_id],
                )?;
                let n: i64 = row.get(0);
                Ok(n.max(0) as u64)
            }
        }
    }

    pub(super) fn insert_webrtc_signal(
        &self,
        signal_id: &str,
        to_peer_id: &str,
        from_actor: &str,
        session_id: &str,
        kind: &str,
        payload_json: &str,
    ) -> Result<()> {
        let now = now_ms();
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                conn.execute(
                    r#"
            INSERT OR REPLACE INTO webrtc_signals(
              signal_id, to_peer_id, from_actor, session_id, kind, payload_json, created_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
                    params![
                        signal_id,
                        to_peer_id,
                        from_actor,
                        session_id,
                        kind,
                        payload_json,
                        now
                    ],
                )?;
                Ok(())
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                conn.execute(
                    r#"
            INSERT INTO webrtc_signals(
              signal_id, to_peer_id, from_actor, session_id, kind, payload_json, created_at_ms
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT(signal_id) DO UPDATE SET
              to_peer_id=EXCLUDED.to_peer_id,
              from_actor=EXCLUDED.from_actor,
              session_id=EXCLUDED.session_id,
              kind=EXCLUDED.kind,
              payload_json=EXCLUDED.payload_json,
              created_at_ms=EXCLUDED.created_at_ms
            "#,
                    &[
                        &signal_id,
                        &to_peer_id,
                        &from_actor,
                        &session_id,
                        &kind,
                        &payload_json,
                        &now,
                    ],
                )?;
                Ok(())
            }
        }
    }

    #[allow(clippy::type_complexity)]
    pub(super) fn list_webrtc_signals(
        &self,
        to_peer_id: &str,
        limit: u32,
    ) -> Result<Vec<(String, String, String, String, String)>> {
        let limit = limit.clamp(1, 200) as i64;
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(
                    r#"
            SELECT signal_id, from_actor, session_id, kind, payload_json
            FROM webrtc_signals
            WHERE to_peer_id=?1
            ORDER BY created_at_ms ASC
            LIMIT ?2
            "#,
                )?;
                let mut rows = stmt.query(params![to_peer_id, limit])?;
                let mut out = Vec::new();
                while let Some(r) = rows.next()? {
                    out.push((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?));
                }
                Ok(out)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let rows = conn.query(
                    r#"
            SELECT signal_id, from_actor, session_id, kind, payload_json
            FROM webrtc_signals
            WHERE to_peer_id=$1
            ORDER BY created_at_ms ASC
            LIMIT $2
            "#,
                    &[&to_peer_id, &limit],
                )?;
                let mut out = Vec::new();
                for r in rows {
                    out.push((r.get(0), r.get(1), r.get(2), r.get(3), r.get(4)));
                }
                Ok(out)
            }
        }
    }

    pub(super) fn delete_webrtc_signals(&self, to_peer_id: &str, ids: &[String]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        match self.driver {
            DbDriver::Sqlite => {
                let mut conn = self.open_sqlite_conn()?;
                let tx = conn.transaction()?;
                let mut deleted: u64 = 0;
                for chunk in ids.chunks(DB_BATCH_DELETE_MAX) {
                    let placeholders = std::iter::repeat_n("?", chunk.len())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let sql = format!(
                        "DELETE FROM webrtc_signals WHERE to_peer_id=?1 AND signal_id IN ({placeholders})"
                    );
                    let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(chunk.len() + 1);
                    params.push(&to_peer_id);
                    params.extend(chunk.iter().map(|id| id as &dyn rusqlite::ToSql));
                    let n = tx.execute(&sql, rusqlite::params_from_iter(params))?;
                    deleted = deleted.saturating_add(n as u64);
                }
                tx.commit()?;
                Ok(deleted)
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let mut deleted: u64 = 0;
                for chunk in ids.chunks(DB_BATCH_DELETE_MAX) {
                    let params: &[&(dyn ToSql + Sync)] = &[&to_peer_id, &chunk];
                    let n = conn.execute(
                        "DELETE FROM webrtc_signals WHERE to_peer_id=$1 AND signal_id = ANY($2)",
                        params,
                    )?;
                    deleted = deleted.saturating_add(n);
                }
                Ok(deleted)
            }
        }
    }
}
