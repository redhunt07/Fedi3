//! Authenticated opaque state store. No account private keys or plaintext social graph.
use super::*;
use fedi3_protocol::account_sync::{AccountEnvelope, AccountPage};
#[derive(Deserialize)]
pub(super) struct Upload {
    pub public_key_pem: String,
    pub records: Vec<AccountEnvelope>,
}
impl Db {
    pub(super) fn init_account_state(&self) -> Result<()> {
        let sql="CREATE TABLE IF NOT EXISTS account_keys(username TEXT PRIMARY KEY,actor TEXT NOT NULL,pem TEXT NOT NULL); CREATE TABLE IF NOT EXISTS account_state(seq INTEGER PRIMARY KEY AUTOINCREMENT,username TEXT NOT NULL,kind TEXT NOT NULL,row_key TEXT NOT NULL,clock BIGINT NOT NULL,device TEXT NOT NULL,terminal INTEGER NOT NULL DEFAULT 0,envelope TEXT NOT NULL,UNIQUE(username,kind,row_key)); CREATE INDEX IF NOT EXISTS account_state_cursor ON account_state(username,seq); CREATE TABLE IF NOT EXISTS account_tunnel_nonces(username TEXT NOT NULL,nonce TEXT NOT NULL,created_at_ms BIGINT NOT NULL,PRIMARY KEY(username,nonce));";
        match self.driver {
            DbDriver::Sqlite => {
                let c = self.open_sqlite_conn()?;
                c.execute_batch(sql)?;
                c.execute("INSERT OR IGNORE INTO account_keys SELECT c.username,json_extract(c.actor_json,'$.id'),json_extract(c.actor_json,'$.publicKey.publicKeyPem') FROM user_cache c JOIN users u ON u.username=c.username WHERE json_valid(c.actor_json) AND json_extract(c.actor_json,'$.publicKey.publicKeyPem') IS NOT NULL",[])?;
            }
            DbDriver::Postgres => {
                let mut c = self.open_pg_conn()?;
                c.batch_execute(&sql.replace(
                    "seq INTEGER PRIMARY KEY AUTOINCREMENT",
                    "seq BIGSERIAL PRIMARY KEY",
                ))?;
                c.execute("INSERT INTO account_keys SELECT c.username,c.actor_json::json->>'id',c.actor_json::json->'publicKey'->>'publicKeyPem' FROM user_cache c JOIN users u ON u.username=c.username WHERE c.actor_json::json->'publicKey'->>'publicKeyPem' IS NOT NULL ON CONFLICT(username) DO NOTHING",&[])?;
            }
        }
        Ok(())
    }
    pub(super) fn claim_account_nonce(&self, user: &str, nonce: &str) -> Result<()> {
        match self.driver {
            DbDriver::Sqlite => {
                let mut c = self.open_sqlite_conn()?;
                let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                tx.execute(
                    "DELETE FROM account_tunnel_nonces WHERE created_at_ms<?1",
                    [now_ms() - 300_000],
                )?;
                tx.execute(
                    "INSERT INTO account_tunnel_nonces VALUES(?1,?2,?3)",
                    params![user, nonce, now_ms()],
                )?;
                tx.commit()?;
            }
            DbDriver::Postgres => {
                let mut c = self.open_pg_conn()?;
                let mut tx = c.transaction()?;
                tx.execute(
                    "DELETE FROM account_tunnel_nonces WHERE created_at_ms<$1",
                    &[&(now_ms() - 300_000)],
                )?;
                tx.execute(
                    "INSERT INTO account_tunnel_nonces VALUES($1,$2,$3)",
                    &[&user, &nonce, &now_ms()],
                )?;
                tx.commit()?;
            }
        }
        Ok(())
    }
    pub(super) fn account_key(&self, user: &str) -> Result<Option<(String, String)>> {
        match self.driver {
            DbDriver::Sqlite => Ok(self
                .open_sqlite_conn()?
                .query_row(
                    "SELECT actor,pem FROM account_keys WHERE username=?1",
                    [user],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?),
            DbDriver::Postgres => Ok(self
                .open_pg_conn()?
                .query_opt(
                    "SELECT actor,pem FROM account_keys WHERE username=$1",
                    &[&user],
                )?
                .map(|r| (r.get(0), r.get(1)))),
        }
    }
    pub(super) fn bind_account_key(&self, user: &str, actor: &str, pem: &str) -> Result<()> {
        let same = |a: String, p: String| -> Result<()> {
            anyhow::ensure!(
                a == actor
                    && p.split_whitespace().collect::<String>()
                        == pem.split_whitespace().collect::<String>(),
                "account identity mismatch"
            );
            Ok(())
        };
        match self.driver {
            DbDriver::Sqlite => {
                let mut c = self.open_sqlite_conn()?;
                let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let enabled: Option<i64> = tx
                    .query_row(
                        "SELECT disabled FROM users WHERE username=?1",
                        [user],
                        |r| r.get(0),
                    )
                    .optional()?;
                anyhow::ensure!(enabled == Some(0), "account unavailable");
                if let Some((a, p)) = tx
                    .query_row(
                        "SELECT actor,pem FROM account_keys WHERE username=?1",
                        [user],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?
                {
                    same(a, p)?;
                } else {
                    tx.execute(
                        "INSERT INTO account_keys VALUES(?1,?2,?3)",
                        params![user, actor, pem],
                    )?;
                }
                tx.commit()?;
            }
            DbDriver::Postgres => {
                let mut c = self.open_pg_conn()?;
                let mut tx = c.transaction()?;
                let enabled = tx.query_opt(
                    "SELECT disabled FROM users WHERE username=$1 FOR UPDATE",
                    &[&user],
                )?;
                anyhow::ensure!(
                    enabled.is_some_and(|r| !r.get::<_, bool>(0)),
                    "account unavailable"
                );
                if let Some(r) = tx.query_opt(
                    "SELECT actor,pem FROM account_keys WHERE username=$1",
                    &[&user],
                )? {
                    same(r.get(0), r.get(1))?;
                } else {
                    tx.execute(
                        "INSERT INTO account_keys VALUES($1,$2,$3)",
                        &[&user, &actor, &pem],
                    )?;
                }
                tx.commit()?;
            }
        }
        Ok(())
    }
    pub(super) fn store_account_state(
        &self,
        user: &str,
        records: &[AccountEnvelope],
    ) -> Result<()> {
        let max_rows = std::env::var("FEDI3_RELAY_ACCOUNT_MAX_ROWS")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(1_000_000)
            .clamp(1000, 2_000_000);
        let max_bytes = std::env::var("FEDI3_RELAY_ACCOUNT_MAX_BYTES")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(1024 * 1024 * 1024)
            .clamp(1024 * 1024, 16 * 1024 * 1024 * 1024);
        match self.driver {
            DbDriver::Sqlite => {
                let mut c = self.open_sqlite_conn()?;
                let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let enabled: Option<i64> = tx
                    .query_row(
                        "SELECT disabled FROM users WHERE username=?1",
                        [user],
                        |r| r.get(0),
                    )
                    .optional()?;
                anyhow::ensure!(enabled == Some(0), "account unavailable");
                for e in records {
                    tx.execute("INSERT INTO account_state(username,kind,row_key,clock,device,terminal,envelope) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(username,kind,row_key) DO UPDATE SET seq=excluded.seq,clock=excluded.clock,device=excluded.device,terminal=excluded.terminal,envelope=excluded.envelope WHERE (account_state.terminal,account_state.clock,account_state.device)<(excluded.terminal,excluded.clock,excluded.device)",params![user,e.kind,e.key,e.clock,e.device,i64::from(e.terminal),serde_json::to_string(e)?])?;
                }
                let (rows,bytes):(i64,i64)=tx.query_row("SELECT COUNT(*),COALESCE(SUM(LENGTH(envelope)),0) FROM account_state WHERE username=?1",[user],|r|Ok((r.get(0)?,r.get(1)?)))?;
                anyhow::ensure!(
                    rows <= max_rows && bytes <= max_bytes,
                    "account state quota exceeded"
                );
                tx.commit()?;
            }
            DbDriver::Postgres => {
                let mut c = self.open_pg_conn()?;
                let mut tx = c.transaction()?;
                let r = tx.query_opt(
                    "SELECT disabled FROM users WHERE username=$1 FOR UPDATE",
                    &[&user],
                )?;
                anyhow::ensure!(
                    r.is_some_and(|r| !r.get::<_, bool>(0)),
                    "account unavailable"
                );
                for e in records {
                    tx.execute("INSERT INTO account_state(username,kind,row_key,clock,device,terminal,envelope) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(username,kind,row_key) DO UPDATE SET seq=excluded.seq,clock=excluded.clock,device=excluded.device,terminal=excluded.terminal,envelope=excluded.envelope WHERE (account_state.terminal,account_state.clock,account_state.device)<(excluded.terminal,excluded.clock,excluded.device)",&[&user,&e.kind,&e.key,&e.clock,&e.device,&i32::from(e.terminal),&serde_json::to_string(e)?])?;
                }
                let r=tx.query_one("SELECT COUNT(*),COALESCE(SUM(LENGTH(envelope)),0)::BIGINT FROM account_state WHERE username=$1",&[&user])?;
                anyhow::ensure!(
                    r.get::<_, i64>(0) <= max_rows && r.get::<_, i64>(1) <= max_bytes,
                    "account state quota exceeded"
                );
                tx.commit()?;
            }
        }
        Ok(())
    }
    pub(super) fn account_state_page(&self, user: &str, after: i64) -> Result<AccountPage> {
        let rows:Vec<(i64,String)>=match self.driver {
   DbDriver::Sqlite=>{let c=self.open_sqlite_conn()?;let mut q=c.prepare("SELECT seq,envelope FROM account_state WHERE username=?1 AND seq>?2 ORDER BY seq LIMIT 64")?;let rows=q.query_map(params![user,after],|r|Ok((r.get(0)?,r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;rows}
   DbDriver::Postgres=>self.open_pg_conn()?.query("SELECT seq,envelope FROM account_state WHERE username=$1 AND seq>$2 ORDER BY seq LIMIT 64",&[&user,&after])?.into_iter().map(|r|(r.get(0),r.get(1))).collect(),
  };
        let mut page = AccountPage {
            items: vec![],
            cursor: after,
        };
        let mut size = 0;
        for (seq, raw) in rows {
            if size > 0 && size + raw.len() > 8 * 1024 * 1024 {
                break;
            }
            size += raw.len();
            page.items.push((seq, serde_json::from_str(&raw)?));
            page.cursor = seq;
        }
        Ok(page)
    }
}
pub(super) async fn get_state(
    State(state): State<AppState>,
    Path(user): Path<String>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Response {
    if !operations::authorized(&state, &user, &headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let after = query_value(query.as_deref(), "after")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    match state
        .db
        .run(move |db| db.account_state_page(&user, after))
        .await
    {
        Ok(v) => ([(header::CACHE_CONTROL, "no-store")], axum::Json(v)).into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}
pub(super) async fn put_state(
    State(state): State<AppState>,
    Path(user): Path<String>,
    headers: HeaderMap,
    axum::Json(upload): axum::Json<Upload>,
) -> Response {
    if !operations::authorized(&state, &user, &headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if upload.records.is_empty() || upload.records.len() > 64 || upload.public_key_pem.len() > 16384
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let actor = upload.records[0].actor.clone();
    for e in &upload.records {
        if e.terminal && e.kind != "objects"
            || !e.valid_kind()
            || e.actor != actor
            || e.key.len() != 64
            || !e.key.bytes().all(|b| b.is_ascii_hexdigit())
            || e.device.is_empty()
            || e.device.len() > 128
            || e.clock < 0
            || e.clock > now_ms() + 86_400_000
            || e.ciphertext_b64.len() > 48 * 1024 * 1024
            || B64.decode(&e.nonce_b64).map_or(true, |n| n.len() != 12)
            || e.signature_b64
                .as_ref()
                .and_then(|s| B64.decode(s).ok())
                .is_none_or(|sig| {
                    fedi3_protocol::crypto::rsa_verify_sha256(
                        &upload.public_key_pem,
                        &e.signing_bytes().unwrap_or_default(),
                        &sig,
                    )
                    .is_err()
                })
        {
            return StatusCode::BAD_REQUEST.into_response();
        }
    }
    match state
        .db
        .run(move |db| {
            db.bind_account_key(&user, &actor, &upload.public_key_pem)?;
            db.store_account_state(&user, &upload.records)
        })
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            warn!(error=%e,"account replication rejected");
            (
                StatusCode::CONFLICT,
                "account identity mismatch or capacity reached; local data retained",
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(clock: i64, terminal: bool) -> AccountEnvelope {
        AccountEnvelope {
            actor: "https://self.example/users/alice".into(),
            kind: "objects".into(),
            key: "a".repeat(64),
            clock,
            device: "device-1".into(),
            terminal,
            nonce_b64: B64.encode([0u8; 12]),
            ciphertext_b64: B64.encode(b"opaque encrypted state"),
            signature_b64: None,
        }
    }
    #[test]
    fn state_store_retains_tombstones_and_account_keys_cannot_be_substituted() {
        let (db, _dir) = super::super::tests::test_db();
        db.create_user("alice", "fixture-token-0123456789").unwrap();
        let pem = include_str!("../../fedi3_protocol/tests/fixtures/legacy_rsa/public.pem");
        db.bind_account_key("alice", "https://self.example/users/alice", pem)
            .unwrap();
        assert!(db
            .bind_account_key("alice", "https://other.example/users/alice", pem)
            .is_err());
        let other = fedi3_protocol::crypto::rsa_generate_private_pem().unwrap();
        assert!(db
            .bind_account_key(
                "alice",
                "https://self.example/users/alice",
                &fedi3_protocol::crypto::rsa_public_pem(&other).unwrap()
            )
            .is_err());
        db.store_account_state("alice", &[row(10, false)]).unwrap();
        let first = db.account_state_page("alice", 0).unwrap();
        assert_eq!(first.items.len(), 1);
        db.store_account_state("alice", &[row(20, true)]).unwrap();
        db.store_account_state("alice", &[row(100, false)]).unwrap();
        let latest = db.account_state_page("alice", first.cursor).unwrap();
        assert_eq!(latest.items.len(), 1);
        assert!(latest.items[0].1.terminal);
        assert_eq!(latest.items[0].1.clock, 20);
        db.claim_account_nonce("alice", "nonce-1").unwrap();
        assert!(db.claim_account_nonce("alice", "nonce-1").is_err());
        db.delete_user("alice").unwrap();
        assert!(db.account_state_page("alice", 0).unwrap().items.is_empty());
        assert!(db.account_key("alice").unwrap().is_none());
        assert!(db.store_account_state("alice", &[row(200, false)]).is_err());
    }
}
