//! Independent additive journal namespace: existing relay schemas remain readable.
use super::*;
use crate::receipts::{LogHead, ReceiptEvent};
use fedi3_protocol::receipts::{chain_hash, sha256};

impl Db {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn append_forward_receipt(
        &self,
        user: &str,
        relay: &str,
        event: ReceiptEvent,
        public: &str,
        secret: &[u8; 32],
        ttl_secs: u64,
        max_rows: u64,
    ) -> Result<String> {
        fn build(
            mut head: LogHead,
            relay: &str,
            event: ReceiptEvent,
            public: &str,
            secret: &[u8; 32],
        ) -> Result<(LogHead, String, String, String)> {
            let payload = serde_json::to_vec(&serde_json::json!({
                "schema":"fedi3-forward-receipt/v1", "sequence":head.sequence+1,
                "previous_hash":head.hash, "relay":relay, "event":event
            }))?;
            let commitment = sha256(&payload);
            let hash = chain_hash(head.sequence + 1, &head.hash, &commitment);
            let signature = ed25519_dalek::SigningKey::from_bytes(secret).sign(&payload);
            let envelope = serde_json::json!({"algorithm":"ed25519","payload_b64":B64.encode(payload),
                "signature_b64":B64.encode(signature.to_bytes()),"public_key_b64":public,"commitment":commitment});
            head.sequence += 1;
            head.hash = hash.clone();
            Ok((head, envelope.to_string(), hash, commitment))
        }
        let timestamp = event.timestamp_ms;
        let previous;
        let cutoff = now_ms().saturating_sub(ttl_secs.min(365 * 86400) as i64 * 1000);
        let envelope = match self.driver {
            DbDriver::Sqlite => {
                let mut connection = self.open_sqlite_conn()?;
                let tx = connection
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM users WHERE username=?1)",
                    [user],
                    |r| r.get(0),
                )?;
                let stored_user = if exists { user } else { "" };
                let raw: Option<String> = tx
                    .query_row(
                        "SELECT value FROM relay_meta WHERE key='forward_log_head'",
                        [],
                        |r| r.get(0),
                    )
                    .optional()?;
                let old: LogHead = raw
                    .map(|s| serde_json::from_str(&s))
                    .transpose()?
                    .unwrap_or_default();
                previous = old.hash.clone();
                let (mut next, signed, hash, commitment) =
                    build(old, relay, event, public, secret)?;
                tx.execute("INSERT INTO forward_receipts(seq,username,created_at_ms,previous_hash,entry_hash,payload_hash,envelope_json) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![next.sequence as i64,stored_user,timestamp,previous,hash,commitment,if exists {signed.as_str()} else {""}])?;
                let boundary: i64 = tx.query_row(
                    "SELECT COALESCE(MIN(seq),?1) FROM forward_receipts WHERE created_at_ms>=?2",
                    params![next.sequence as i64 + 1, cutoff],
                    |r| r.get(0),
                )?;
                let trim =
                    (next.sequence.saturating_sub(max_rows)).max(boundary.saturating_sub(1) as u64);
                if trim > next.anchor_sequence {
                    if let Some(hash) = tx
                        .query_row(
                            "SELECT entry_hash FROM forward_receipts WHERE seq=?1",
                            [trim as i64],
                            |r| r.get::<_, String>(0),
                        )
                        .optional()?
                    {
                        next.anchor_sequence = trim;
                        next.anchor_hash = hash;
                        tx.execute("DELETE FROM forward_receipts WHERE seq<=?1", [trim as i64])?;
                    }
                }
                tx.execute("INSERT INTO relay_meta(key,value) VALUES ('forward_log_head',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[serde_json::to_string(&next)?])?;
                tx.commit()?;
                signed
            }
            DbDriver::Postgres => {
                let mut connection = self.open_pg_conn()?;
                let mut tx = connection.transaction()?;
                // Match account deletion's user-first lock order. Late events retain
                // only their public commitment once the account has been deleted.
                let exists = !tx
                    .query(
                        "SELECT username FROM users WHERE username=$1 FOR KEY SHARE",
                        &[&user],
                    )?
                    .is_empty();
                let stored_user = if exists { user } else { "" };
                tx.execute("INSERT INTO relay_meta(key,value) VALUES ('forward_log_head',$1) ON CONFLICT(key) DO NOTHING", &[&serde_json::to_string(&LogHead::default())?])?;
                let raw: String = tx
                    .query_one(
                        "SELECT value FROM relay_meta WHERE key='forward_log_head' FOR UPDATE",
                        &[],
                    )?
                    .get(0);
                let old: LogHead = serde_json::from_str(&raw)?;
                previous = old.hash.clone();
                let (mut next, signed, hash, commitment) =
                    build(old, relay, event, public, secret)?;
                tx.execute("INSERT INTO forward_receipts(seq,username,created_at_ms,previous_hash,entry_hash,payload_hash,envelope_json) VALUES ($1,$2,$3,$4,$5,$6,$7)",&[&(next.sequence as i64),&stored_user,&timestamp,&previous,&hash,&commitment,&if exists {signed.as_str()} else {""}])?;
                let boundary:i64=tx.query_one("SELECT COALESCE(MIN(seq),$1) FROM forward_receipts WHERE created_at_ms>=$2",&[&(next.sequence as i64+1),&cutoff])?.get(0);
                let trim =
                    (next.sequence.saturating_sub(max_rows)).max(boundary.saturating_sub(1) as u64);
                if trim > next.anchor_sequence {
                    let row = tx.query(
                        "SELECT entry_hash FROM forward_receipts WHERE seq=$1",
                        &[&(trim as i64)],
                    )?;
                    if let Some(row) = row.first() {
                        next.anchor_sequence = trim;
                        next.anchor_hash = row.get(0);
                        tx.execute(
                            "DELETE FROM forward_receipts WHERE seq<=$1",
                            &[&(trim as i64)],
                        )?;
                    }
                }
                tx.execute(
                    "UPDATE relay_meta SET value=$1 WHERE key='forward_log_head'",
                    &[&serde_json::to_string(&next)?],
                )?;
                tx.commit()?;
                signed
            }
        };
        Ok(envelope)
    }

    pub(super) fn forward_receipt_rows(
        &self,
        user: Option<&str>,
        after: i64,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>> {
        let limit = limit.clamp(1, 200);
        let sql="SELECT seq,previous_hash,entry_hash,payload_hash,envelope_json FROM forward_receipts WHERE seq>?1 AND (?2='' OR username=?2) ORDER BY seq LIMIT ?3";
        let username = user.unwrap_or("");
        let mut rows = Vec::new();
        let row_value = |sequence: i64,
                         previous: String,
                         hash: String,
                         commitment: String,
                         envelope: String|
         -> Result<serde_json::Value> {
            if user.is_some() {
                Ok(serde_json::from_str(&envelope)?)
            } else {
                Ok(
                    serde_json::json!({"sequence":sequence,"previous_hash":previous,"hash":hash,"commitment":commitment}),
                )
            }
        };
        match self.driver {
            DbDriver::Sqlite => {
                let conn = self.open_sqlite_conn()?;
                let mut stmt = conn.prepare(sql)?;
                let mut cursor = stmt.query(params![after, username, limit])?;
                while let Some(row) = cursor.next()? {
                    rows.push(row_value(
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    )?);
                }
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                for row in conn.query(
                    &sql.replace("?1", "$1")
                        .replace("?2", "$2")
                        .replace("?3", "$3"),
                    &[&after, &username, &limit],
                )? {
                    rows.push(row_value(
                        row.get(0),
                        row.get(1),
                        row.get(2),
                        row.get(3),
                        row.get(4),
                    )?);
                }
            }
        }
        Ok(rows)
    }
}

impl Db {
    pub(super) fn prune_forward_receipts(&self, ttl_secs: u64) -> Result<()> {
        let cutoff = now_ms().saturating_sub(ttl_secs.min(365 * 86400) as i64 * 1000);
        match self.driver {
            DbDriver::Sqlite => {
                let mut conn = self.open_sqlite_conn()?;
                let tx =
                    conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let raw: Option<String> = tx
                    .query_row(
                        "SELECT value FROM relay_meta WHERE key='forward_log_head'",
                        [],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(raw) = raw {
                    let mut head: LogHead = serde_json::from_str(&raw)?;
                    let first:i64=tx.query_row("SELECT COALESCE(MIN(seq),?1) FROM forward_receipts WHERE created_at_ms>=?2",params![head.sequence as i64+1,cutoff],|r|r.get(0))?;
                    let trim = first.saturating_sub(1) as u64;
                    if trim > head.anchor_sequence {
                        let hash: String = tx.query_row(
                            "SELECT entry_hash FROM forward_receipts WHERE seq=?1",
                            [trim as i64],
                            |r| r.get(0),
                        )?;
                        head.anchor_sequence = trim;
                        head.anchor_hash = hash;
                        tx.execute("DELETE FROM forward_receipts WHERE seq<=?1", [trim as i64])?;
                        tx.execute(
                            "UPDATE relay_meta SET value=?1 WHERE key='forward_log_head'",
                            [serde_json::to_string(&head)?],
                        )?;
                    }
                }
                tx.commit()?;
            }
            DbDriver::Postgres => {
                let mut conn = self.open_pg_conn()?;
                let mut tx = conn.transaction()?;
                let rows = tx.query(
                    "SELECT value FROM relay_meta WHERE key='forward_log_head' FOR UPDATE",
                    &[],
                )?;
                if let Some(row) = rows.first() {
                    let raw: String = row.get(0);
                    let mut head: LogHead = serde_json::from_str(&raw)?;
                    let first:i64=tx.query_one("SELECT COALESCE(MIN(seq),$1) FROM forward_receipts WHERE created_at_ms>=$2",&[&(head.sequence as i64+1),&cutoff])?.get(0);
                    let trim = first.saturating_sub(1) as u64;
                    if trim > head.anchor_sequence {
                        let hash: String = tx
                            .query_one(
                                "SELECT entry_hash FROM forward_receipts WHERE seq=$1",
                                &[&(trim as i64)],
                            )?
                            .get(0);
                        head.anchor_sequence = trim;
                        head.anchor_hash = hash;
                        tx.execute(
                            "DELETE FROM forward_receipts WHERE seq<=$1",
                            &[&(trim as i64)],
                        )?;
                        tx.execute(
                            "UPDATE relay_meta SET value=$1 WHERE key='forward_log_head'",
                            &[&serde_json::to_string(&head)?],
                        )?;
                    }
                }
                tx.commit()?;
            }
        }
        Ok(())
    }
}
