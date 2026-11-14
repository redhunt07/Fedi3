//! Account state replication: LWW records, durable tombstones and independent encryption.
use crate::{
    ap::{ApConfig, ApState},
    social_db::SocialDb,
};
use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use fedi3_protocol::{
    account_sync::{AccountEnvelope, AccountPage},
    crypto,
    receipts::sha256,
};
use rusqlite::{
    params,
    types::{Value as SqlValue, ValueRef},
    OptionalExtension,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
const TABLES: &[&str] = &[
    "followers",
    "following",
    "inbox_follows",
    "follow_requests",
    "reactions",
    "note_replies",
    "objects",
    "outbox_items",
    "object_meta",
    "object_tags",
    "object_attachments",
];
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub kind: String,
    pub key: String,
    pub clock: i64,
    pub device: String,
    pub deleted: bool,
    pub data: BTreeMap<String, Value>,
}
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
fn schema(db: &SocialDb) -> Result<()> {
    db.connect()?.execute_batch("CREATE TABLE IF NOT EXISTS account_replica(kind TEXT NOT NULL,key TEXT NOT NULL,clock INTEGER NOT NULL,device TEXT NOT NULL,deleted INTEGER NOT NULL,data TEXT NOT NULL,dirty INTEGER NOT NULL,PRIMARY KEY(kind,key)); CREATE TABLE IF NOT EXISTS account_profile(id INTEGER PRIMARY KEY CHECK(id=1),data TEXT NOT NULL);")?;
    Ok(())
}
fn sql_json(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(n) => n.into(),
        ValueRef::Real(n) => serde_json::json!(n),
        ValueRef::Text(s) => String::from_utf8_lossy(s).to_string().into(),
        ValueRef::Blob(b) => serde_json::json!({"$blob":B64.encode(b)}),
    }
}
fn json_sql(v: &Value) -> Result<SqlValue> {
    Ok(match v {
        Value::Null => SqlValue::Null,
        Value::Number(n) => {
            if let Some(n) = n.as_i64() {
                SqlValue::Integer(n)
            } else {
                SqlValue::Real(n.as_f64().context("number")?)
            }
        }
        Value::String(s) => SqlValue::Text(s.clone()),
        Value::Object(o) if o.len() == 1 && o.contains_key("$blob") => {
            SqlValue::Blob(B64.decode(o["$blob"].as_str().context("blob")?)?)
        }
        _ => anyhow::bail!("invalid row value"),
    })
}
fn columns(c: &rusqlite::Connection, table: &str) -> Result<(Vec<String>, Vec<String>)> {
    let mut stmt = c.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(5)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok((
        rows.iter().map(|r| r.0.clone()).collect(),
        rows.into_iter().filter(|r| r.1 > 0).map(|r| r.0).collect(),
    ))
}
fn record_key(data: &BTreeMap<String, Value>, pk: &[String], salt: &str) -> Result<String> {
    Ok(sha256(
        &[
            salt.as_bytes(),
            serde_json::to_vec(
                &pk.iter()
                    .map(|p| data.get(p).context("missing row key"))
                    .collect::<Result<Vec<_>>>()?,
            )?
            .as_slice(),
        ]
        .concat(),
    ))
}
fn own_sql(alias: &str) -> String {
    format!("EXISTS(SELECT 1 FROM local_meta m WHERE m.key='owned_actor:' || COALESCE(NULLIF({alias}.actor_id,''),CASE WHEN json_valid(CAST({alias}.object_json AS TEXT)) THEN json_extract(CAST({alias}.object_json AS TEXT),'$.attributedTo') END))")
}
pub fn profile_value(cfg: &ApConfig) -> Value {
    serde_json::json!({"display_name":cfg.display_name,"summary":cfg.summary,"icon_url":cfg.icon_url,"icon_media_type":cfg.icon_media_type,"image_url":cfg.image_url,"image_media_type":cfg.image_media_type,"profile_fields":cfg.profile_fields,"manually_approves_followers":cfg.manually_approves_followers,"blocked_domains":cfg.blocked_domains,"blocked_actors":cfg.blocked_actors})
}
pub fn initialize(state: &ApState) -> Result<()> {
    schema(&state.social)?;
    state.social.set_local_meta(
        "account_replica_key_salt",
        &sha256(&encryption_key(&state.private_key_pem)),
    )?;
    update_local_profile(&state.social, profile_value(&state.cfg))?;
    if state
        .social
        .get_local_meta("account_replica_device")?
        .is_none()
    {
        state
            .social
            .set_local_meta("account_replica_device", &crate::chat::random_id())?;
    }
    Ok(())
}
fn update_local_profile(db: &SocialDb, candidate: Value) -> Result<()> {
    let encoded = serde_json::to_string(&candidate)?;
    let previous = db
        .get_local_meta("account_profile_config")?
        .and_then(|v| serde_json::from_str::<Value>(&v).ok());
    let c = db.connect()?;
    let existing: Option<String> = c
        .query_row("SELECT data FROM account_profile WHERE id=1", [], |r| {
            r.get(0)
        })
        .optional()?;
    let mut profile: Value = existing
        .as_ref()
        .map(|s| serde_json::from_str(s))
        .transpose()?
        .unwrap_or_else(|| candidate.clone());
    if let Some(previous) = previous {
        for (field, value) in candidate.as_object().context("profile object")? {
            if previous.get(field) != Some(value) {
                profile[field] = value.clone();
            }
        }
    }
    c.execute(
        "INSERT INTO account_profile VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
        [serde_json::to_string(&profile)?],
    )?;
    db.set_local_meta("account_profile_config", &encoded)?;
    Ok(())
}
pub fn current_profile(db: &SocialDb) -> Result<Option<Value>> {
    schema(db)?;
    let raw: Option<String> = db
        .connect()?
        .query_row("SELECT data FROM account_profile WHERE id=1", [], |r| {
            r.get(0)
        })
        .optional()?;
    raw.map(|s| serde_json::from_str(&s).map_err(Into::into))
        .transpose()
}
pub fn effective_config(state: &ApState) -> ApConfig {
    let mut cfg = state.cfg.clone();
    if let Ok(Some(v)) = current_profile(&state.social) {
        macro_rules! text{($($f:ident),*)=>{$(cfg.$f=v[stringify!($f)].as_str().map(str::to_owned);)*};}
        text!(
            display_name,
            summary,
            icon_url,
            icon_media_type,
            image_url,
            image_media_type
        );
        cfg.profile_fields =
            serde_json::from_value(v["profile_fields"].clone()).unwrap_or_default();
        cfg.manually_approves_followers =
            v["manually_approves_followers"].as_bool().unwrap_or(false);
        cfg.blocked_domains =
            serde_json::from_value(v["blocked_domains"].clone()).unwrap_or_default();
        cfg.blocked_actors =
            serde_json::from_value(v["blocked_actors"].clone()).unwrap_or_default();
    }
    cfg
}
pub fn capture(db: &SocialDb) -> Result<()> {
    schema(db)?;
    let device = db
        .get_local_meta("account_replica_device")?
        .unwrap_or_else(|| "initial".into());
    let mut c = db.connect()?;
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let seeded: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM local_meta WHERE key='account_replica_seeded')",
        [],
        |r| r.get(0),
    )?;
    let mut clock: i64 = tx.query_row(
        "SELECT COALESCE(MAX(clock),0) FROM account_replica",
        [],
        |r| r.get(0),
    )?;
    let salt = db
        .get_local_meta("account_replica_key_salt")?
        .unwrap_or_default();
    let mut found = HashSet::new();
    for table in TABLES.iter().copied().chain(std::iter::once("profile")) {
        let (cols, pk, query) = if table == "profile" {
            (
                vec!["data".into()],
                vec![],
                "SELECT data FROM account_profile WHERE id=1".to_string(),
            )
        } else {
            let (cols, pk) = columns(&tx, table)?;
            let filter = if table == "objects" {
                format!(" WHERE {}", own_sql("objects"))
            } else if table == "reactions" {
                format!(" WHERE EXISTS(SELECT 1 FROM objects o WHERE o.object_id=reactions.object_id AND {}) OR EXISTS(SELECT 1 FROM local_meta m WHERE m.key='owned_actor:' || reactions.actor_id)",own_sql("o"))
            } else if table == "note_replies" {
                format!(" WHERE EXISTS(SELECT 1 FROM objects o WHERE o.object_id=note_replies.note_id AND {})",own_sql("o"))
            } else if table.starts_with("object_") {
                format!(" WHERE EXISTS(SELECT 1 FROM objects o WHERE o.object_id={table}.object_id AND {})",own_sql("o"))
            } else {
                String::new()
            };
            (cols, pk, format!("SELECT * FROM {table}{filter}"))
        };
        let mut stmt = tx.prepare(&query)?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let data = if table == "profile" {
                serde_json::from_str::<BTreeMap<String, Value>>(&row.get::<_, String>(0)?)?
            } else {
                cols.iter()
                    .enumerate()
                    .map(|(i, k)| Ok((k.clone(), sql_json(row.get_ref(i)?))))
                    .collect::<Result<BTreeMap<_, _>>>()?
            };
            let rows = if table == "profile" {
                data.into_iter()
                    .map(|(k, v)| BTreeMap::from([(k, v)]))
                    .collect::<Vec<_>>()
            } else {
                vec![data]
            };
            for data in rows {
                let key = if table == "profile" {
                    sha256(format!("profile:{}", data.keys().next().unwrap()).as_bytes())
                } else {
                    record_key(&data, &pk, &salt)?
                };
                found.insert((table.to_string(), key.clone()));
                let raw = serde_json::to_string(&data)?;
                let old: Option<(String, bool)> = tx
                    .query_row(
                        "SELECT data,deleted FROM account_replica WHERE kind=?1 AND key=?2",
                        params![table, key],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                if old
                    .as_ref()
                    .is_some_and(|(s, deleted)| s == &raw && !*deleted)
                {
                    continue;
                }
                let initial_config = tx
                    .query_row(
                        "SELECT value FROM local_meta WHERE key='account_profile_config'",
                        [],
                        |r| r.get::<_, String>(0),
                    )
                    .optional()?
                    .and_then(|s| serde_json::from_str::<Value>(&s).ok());
                let seed_profile = table == "profile"
                    && old.is_none()
                    && data
                        .iter()
                        .all(|(k, v)| initial_config.as_ref().and_then(|c| c.get(k)) == Some(v));
                if seed_profile
                    && !tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM local_meta WHERE key='account_initial_pull')",
                        [],
                        |r| r.get::<_, bool>(0),
                    )?
                {
                    continue;
                }
                clock = if seed_profile {
                    0
                } else if seeded || table == "profile" {
                    clock.max(now()).saturating_add(1)
                } else {
                    data.get("updated_at_ms")
                        .or(data.get("created_at_ms"))
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                        .min(now())
                        .max(0)
                };
                let record_device = if seed_profile {
                    let value = data.values().next().unwrap();
                    let nonempty = match value {
                        Value::Null => false,
                        Value::String(s) => !s.is_empty(),
                        Value::Array(v) => !v.is_empty(),
                        Value::Bool(v) => *v,
                        _ => true,
                    };
                    format!(
                        "seed:{}:{:010}:{}",
                        u8::from(nonempty),
                        serde_json::to_vec(value)?.len(),
                        device
                    )
                } else {
                    device.clone()
                };
                tx.execute("INSERT INTO account_replica VALUES(?1,?2,?3,?4,0,?5,1) ON CONFLICT(kind,key) DO UPDATE SET clock=excluded.clock,device=excluded.device,deleted=0,data=excluded.data,dirty=1",params![table,key,clock,record_device,raw])?;
            }
        }
    }
    let missing = {
        let mut q = tx.prepare("SELECT kind,key FROM account_replica WHERE deleted=0")?;
        let rows = q
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for (kind, key) in missing {
        if kind == "profile" {
            continue;
        }
        if !found.contains(&(kind.clone(), key.clone())) {
            clock = clock.max(now()) + 1;
            tx.execute("UPDATE account_replica SET deleted=1,clock=?3,device=?4,dirty=1 WHERE kind=?1 AND key=?2",params![kind,key,clock,device])?;
        }
    }
    tx.execute(
        "INSERT OR REPLACE INTO local_meta VALUES('account_replica_seeded','1')",
        [],
    )?;
    tx.commit()?;
    Ok(())
}
pub fn records(db: &SocialDb, dirty: bool, after: i64) -> Result<Vec<Record>> {
    schema(db)?;
    let c = db.connect()?;
    let mut q=c.prepare("SELECT kind,key,clock,device,deleted,data FROM account_replica WHERE (?1=0 OR dirty=1) AND clock>=?2 ORDER BY clock,device,kind,key")?;
    let rows = q
        .query_map(params![dirty, after], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get::<_, String>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<(String, String, i64, String, bool, String)>>>()?;
    rows.into_iter()
        .map(|(kind, key, clock, device, deleted, data)| {
            Ok(Record {
                kind,
                key,
                clock,
                device,
                deleted,
                data: serde_json::from_str(&data)?,
            })
        })
        .collect()
}
fn terminal(record: &Record) -> bool {
    record.kind == "objects"
        && (record.deleted
            || record
                .data
                .get("deleted")
                .is_some_and(|v| v.as_i64() == Some(1) || v.as_bool() == Some(true)))
}
fn actual_row(
    c: &rusqlite::Connection,
    record: &Record,
) -> Result<Option<BTreeMap<String, Value>>> {
    if record.kind == "profile" {
        ensure!(record.data.len() == 1, "invalid profile row");
        let field = record.data.keys().next().context("profile field")?;
        let raw: Option<String> = c
            .query_row("SELECT data FROM account_profile WHERE id=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        return raw
            .map(|raw| -> Result<_> {
                let value: Value = serde_json::from_str(&raw)?;
                Ok(BTreeMap::from([(
                    field.clone(),
                    value.get(field).cloned().unwrap_or(Value::Null),
                )]))
            })
            .transpose();
    }
    ensure!(
        TABLES.contains(&record.kind.as_str()),
        "unknown account table"
    );
    let (cols, pk) = columns(c, &record.kind)?;
    let args = pk
        .iter()
        .map(|key| json_sql(record.data.get(key).context("missing key")?))
        .collect::<Result<Vec<_>>>()?;
    let query = format!(
        "SELECT * FROM {} WHERE {}",
        record.kind,
        pk.iter()
            .map(|k| format!("\"{k}\"=?"))
            .collect::<Vec<_>>()
            .join(" AND ")
    );
    let mut statement = c.prepare(&query)?;
    let mut rows = statement.query(rusqlite::params_from_iter(args))?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    Ok(Some(
        cols.into_iter()
            .enumerate()
            .map(|(i, k)| Ok((k, sql_json(row.get_ref(i)?))))
            .collect::<Result<BTreeMap<_, _>>>()?,
    ))
}
pub fn apply(db: &SocialDb, record: &Record) -> Result<bool> {
    schema(db)?;
    let mut c = db.connect()?;
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut old: Option<(i64, String, String, bool)> = tx
        .query_row(
            "SELECT clock,device,data,deleted FROM account_replica WHERE kind=?1 AND key=?2",
            params![record.kind, record.key],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    // Capture a local write racing this download in the same transaction.
    // Without this check a row committed after capture() could be overwritten
    // before its local revision was recorded.
    let actual = actual_row(&tx, record)?;
    let raw_actual = actual.as_ref().map(serde_json::to_string).transpose()?;
    let local_changed = match &old {
        Some((_, _, raw, deleted)) => {
            if *deleted {
                actual.is_some()
            } else {
                raw_actual.as_deref() != Some(raw.as_str())
            }
        }
        None => {
            actual.is_some()
                && matches!(
                    record.kind.as_str(),
                    "followers"
                        | "following"
                        | "inbox_follows"
                        | "follow_requests"
                        | "outbox_items"
                )
        }
    };
    if local_changed {
        let device = db
            .get_local_meta("account_replica_device")?
            .context("replica device")?;
        let clock = now()
            .max(record.clock)
            .max(old.as_ref().map(|r| r.0).unwrap_or(0))
            .saturating_add(1);
        let raw = raw_actual.unwrap_or_else(|| old.as_ref().unwrap().2.clone());
        let deleted = actual.is_none();
        tx.execute("INSERT INTO account_replica VALUES(?1,?2,?3,?4,?5,?6,1) ON CONFLICT(kind,key) DO UPDATE SET clock=excluded.clock,device=excluded.device,deleted=excluded.deleted,data=excluded.data,dirty=1",params![record.kind,record.key,clock,device,deleted,raw])?;
        old = Some((clock, device, raw, deleted));
    }
    if old.as_ref().is_some_and(|(c, d, raw, deleted)| {
        let old_terminal = record.kind == "objects"
            && (*deleted
                || serde_json::from_str::<BTreeMap<String, Value>>(raw)
                    .ok()
                    .is_some_and(|v| v.get("deleted").and_then(Value::as_i64) == Some(1)));
        (old_terminal, *c, d.as_str()) >= (terminal(record), record.clock, record.device.as_str())
    }) {
        tx.commit()?;
        return Ok(false);
    }
    if record.kind == "profile" {
        ensure!(!record.deleted && record.data.len() == 1, "invalid profile");
        let field = record.data.keys().next().unwrap();
        ensure!(
            [
                "display_name",
                "summary",
                "icon_url",
                "icon_media_type",
                "image_url",
                "image_media_type",
                "profile_fields",
                "manually_approves_followers",
                "blocked_domains",
                "blocked_actors"
            ]
            .contains(&field.as_str())
                && record.key == sha256(format!("profile:{field}").as_bytes()),
            "unknown profile field"
        );
        let raw: Option<String> = tx
            .query_row("SELECT data FROM account_profile WHERE id=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        let mut profile: BTreeMap<String, Value> = raw
            .map(|r| serde_json::from_str(&r))
            .transpose()?
            .unwrap_or_default();
        profile.extend(record.data.clone());
        tx.execute("INSERT INTO account_profile VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET data=excluded.data",[serde_json::to_string(&profile)?])?;
    } else {
        ensure!(
            TABLES.contains(&record.kind.as_str()),
            "unknown account table"
        );
        let (cols, pk) = columns(&tx, &record.kind)?;
        ensure!(
            record_key(
                &record.data,
                &pk,
                &db.get_local_meta("account_replica_key_salt")?
                    .unwrap_or_default()
            )? == record.key,
            "row key mismatch"
        );
        ensure!(
            record.data.keys().all(|k| cols.contains(k)),
            "unknown column"
        );
        if record.deleted {
            let args = pk
                .iter()
                .map(|k| json_sql(&record.data[k]))
                .collect::<Result<Vec<_>>>()?;
            tx.execute(
                &format!(
                    "DELETE FROM {} WHERE {}",
                    record.kind,
                    pk.iter()
                        .map(|k| format!("\"{k}\"=?"))
                        .collect::<Vec<_>>()
                        .join(" AND ")
                ),
                rusqlite::params_from_iter(args),
            )?;
        } else {
            let keys = record.data.keys().cloned().collect::<Vec<_>>();
            let args = keys
                .iter()
                .map(|k| json_sql(&record.data[k]))
                .collect::<Result<Vec<_>>>()?;
            let updates = keys
                .iter()
                .filter(|k| !pk.contains(k))
                .map(|k| format!("\"{k}\"=excluded.\"{k}\""))
                .collect::<Vec<_>>();
            tx.execute(
                &format!(
                    "INSERT INTO {}({}) VALUES({}) ON CONFLICT({}) DO {}",
                    record.kind,
                    keys.iter()
                        .map(|k| format!("\"{k}\""))
                        .collect::<Vec<_>>()
                        .join(","),
                    vec!["?"; keys.len()].join(","),
                    pk.join(","),
                    if updates.is_empty() {
                        "NOTHING".into()
                    } else {
                        format!("UPDATE SET {}", updates.join(","))
                    }
                ),
                rusqlite::params_from_iter(args),
            )?;
        }
    }
    tx.execute("INSERT INTO account_replica VALUES(?1,?2,?3,?4,?5,?6,0) ON CONFLICT(kind,key) DO UPDATE SET clock=excluded.clock,device=excluded.device,deleted=excluded.deleted,data=excluded.data,dirty=0",params![record.kind,record.key,record.clock,record.device,record.deleted,serde_json::to_string(&record.data)?])?;
    tx.commit()?;
    Ok(true)
}
fn encryption_key(pem: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(
        format!(
            "fedi3-account-state/v1:{}",
            pem.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
        )
        .as_bytes(),
    )
    .into()
}
pub fn seal(actor: &str, pem: &str, record: &Record) -> Result<AccountEnvelope> {
    use rand::RngCore;
    let mut nonce = [0; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let ciphertext = Aes256Gcm::new_from_slice(&encryption_key(pem))?
        .encrypt(
            Nonce::from_slice(&nonce),
            serde_json::to_vec(record)?.as_slice(),
        )
        .map_err(|_| anyhow::anyhow!("encrypt state"))?;
    let mut env = AccountEnvelope {
        actor: actor.into(),
        kind: record.kind.clone(),
        key: record.key.clone(),
        clock: record.clock,
        terminal: terminal(record),
        device: record.device.clone(),
        nonce_b64: B64.encode(nonce),
        ciphertext_b64: B64.encode(ciphertext),
        signature_b64: None,
    };
    env.signature_b64 = Some(B64.encode(crypto::rsa_sign_sha256(pem, &env.signing_bytes()?)?));
    Ok(env)
}
pub fn open(actor: &str, pem: &str, env: &AccountEnvelope) -> Result<Record> {
    ensure!(
        env.actor == actor && env.valid_kind(),
        "account binding mismatch"
    );
    crypto::rsa_verify_sha256(
        &crypto::rsa_public_pem(pem)?,
        &env.signing_bytes()?,
        &B64.decode(env.signature_b64.as_ref().context("signature")?)?,
    )?;
    let nonce = B64.decode(&env.nonce_b64)?;
    ensure!(nonce.len() == 12, "nonce");
    let bytes = Aes256Gcm::new_from_slice(&encryption_key(pem))?
        .decrypt(
            Nonce::from_slice(&nonce),
            B64.decode(&env.ciphertext_b64)?.as_slice(),
        )
        .map_err(|_| anyhow::anyhow!("account decrypt"))?;
    let record: Record = serde_json::from_slice(&bytes)?;
    ensure!(
        record.kind == env.kind
            && record.key == env.key
            && record.clock == env.clock
            && record.device == env.device
            && terminal(&record) == env.terminal,
        "record binding mismatch"
    );
    Ok(record)
}

pub async fn sync_once(state: &ApState, base: &str, token: &str) -> Result<()> {
    let social = state.social.clone();
    tokio::task::spawn_blocking(move || capture(&social)).await??;
    let actor = crate::ap::canonical_actor_url(&state.cfg);
    let endpoint = format!(
        "{}/_fedi3/account/{}/state",
        base.trim_end_matches('/'),
        urlencoding::encode(&state.cfg.username)
    );
    let cursor_key = format!(
        "account_relay_cursor:{}",
        sha256(base.trim_end_matches('/').as_bytes())
    );
    if state.social.get_local_meta(&cursor_key)?.is_none() {
        // A new relay has an independent sequence and needs the complete state,
        // including revisions already acknowledged by the previous relay.
        state
            .social
            .connect()?
            .execute("UPDATE account_replica SET dirty=1", [])?;
    }
    let mut after = state
        .social
        .get_local_meta(&cursor_key)?
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    loop {
        let response = state
            .http
            .get(format!("{endpoint}?after={after}"))
            .bearer_auth(token)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await?
            .error_for_status()?;
        let mut response = response;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                bytes.len() + chunk.len() <= 64 * 1024 * 1024,
                "account page too large"
            );
            bytes.extend_from_slice(&chunk);
        }
        let page: AccountPage = serde_json::from_slice(&bytes)?;
        if page.items.is_empty() {
            break;
        }
        ensure!(page.cursor > after, "account cursor did not advance");
        let mut page_changed = false;
        for (seq, env) in page.items {
            ensure!(
                seq > after && seq <= page.cursor,
                "invalid account page order"
            );
            let record = open(&actor, &state.private_key_pem, &env)?;
            let db = state.social.clone();
            let changed = tokio::task::spawn_blocking(move || apply(&db, &record)).await??;
            page_changed |= changed;
            after = seq;
        }
        if page_changed {
            for kind in ["account", "outbox"] {
                let _ = state
                    .ui_events
                    .send(crate::ui_events::UiEvent::new(kind, None, None));
            }
        }
        state
            .social
            .set_local_meta(&cursor_key, &after.to_string())?;
    }
    state
        .social
        .set_local_meta(&cursor_key, &after.to_string())?;
    state.social.set_local_meta("account_initial_pull", "1")?;
    let social = state.social.clone();
    tokio::task::spawn_blocking(move || capture(&social)).await??;
    let dirty = records(&state.social, true, 0)?;
    let mut batch = Vec::new();
    let mut bytes = 0;
    for record in dirty {
        let env = seal(&actor, &state.private_key_pem, &record)?;
        let size = serde_json::to_vec(&env)?.len();
        ensure!(
            size <= 48 * 1024 * 1024,
            "account record exceeds relay capacity"
        );
        if !batch.is_empty() && (bytes + size > 8 * 1024 * 1024 || batch.len() == 64) {
            upload_batch(state, &endpoint, token, &batch).await?;
            batch.clear();
            bytes = 0;
        }
        bytes += size;
        batch.push(env);
    }
    if !batch.is_empty() {
        upload_batch(state, &endpoint, token, &batch).await?;
    }
    Ok(())
}
async fn upload_batch(
    state: &ApState,
    url: &str,
    token: &str,
    batch: &[AccountEnvelope],
) -> Result<()> {
    state
        .http
        .put(url)
        .bearer_auth(token)
        .timeout(std::time::Duration::from_secs(30))
        .json(&serde_json::json!({"public_key_pem":state.cfg.public_key_pem,"records":batch}))
        .send()
        .await?
        .error_for_status()?;
    let mut conn = state.social.connect()?;
    let tx = conn.transaction()?;
    for env in batch {
        tx.execute("UPDATE account_replica SET dirty=0 WHERE kind=?1 AND key=?2 AND clock=?3 AND device=?4",params![env.kind,env.key,env.clock,env.device])?;
    }
    tx.commit()?;
    Ok(())
}
pub fn start(
    state: ApState,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<tokio::task::JoinHandle<()>> {
    initialize(&state)?;
    Ok(tokio::spawn(async move {
        loop {
            if let (Some(base), Some(token)) = (&state.cfg.relay_base_url, &state.cfg.relay_token) {
                if let Err(error) = sync_once(&state, base, token).await {
                    let _ = state
                        .social
                        .set_local_meta("account_sync_error", &error.to_string());
                    tracing::warn!(%error,"account sync deferred; local data retained");
                } else {
                    let _ = state.social.set_local_meta("account_sync_error", "");
                    let _ = state
                        .social
                        .set_local_meta("account_sync_last_ok", &now().to_string());
                }
            } else {
                let db = state.social.clone();
                let _ = tokio::task::spawn_blocking(move || capture(&db)).await;
            }
            tokio::select! {_=crate::power::sleep(5,60,300)=>{},_=shutdown.changed()=>{if *shutdown.borrow(){break;}}}
        }
    }))
}

pub fn peer_page(state: &ApState, offset: usize) -> Result<Vec<AccountEnvelope>> {
    let mut page = Vec::new();
    let mut size = 0;
    for record in records(&state.social, false, 0)?
        .into_iter()
        .skip(offset)
        .take(64)
    {
        let env = seal(
            &crate::ap::canonical_actor_url(&state.cfg),
            &state.private_key_pem,
            &record,
        )?;
        let bytes = serde_json::to_vec(&env)?.len();
        ensure!(
            bytes < crate::relay_bridge::MAX_RELAY_BODY_BYTES - 4096,
            "record exceeds peer transport capacity; local data retained"
        );
        if !page.is_empty() && size + bytes > 8 * 1024 * 1024 {
            break;
        }
        size += bytes;
        page.push(env);
    }
    Ok(page)
}
pub async fn pull_peer(state: &ApState, peer: &str, did: &str) -> Result<()> {
    use http::{HeaderMap, Method};
    let mut offset = 0;
    loop {
        let path = "/.fedi3/device/account";
        let query = format!("?offset={offset}");
        let uri: http::Uri = format!("http://localhost{path}{query}").parse()?;
        let mut headers = HeaderMap::new();
        headers.insert("Host", "localhost".parse()?);
        headers.insert("X-Fedi3-Did", did.parse()?);
        crate::http_sig::sign_request_rsa_sha256(
            &state.private_key_pem,
            &format!("{}#main-key", crate::ap::canonical_actor_url(&state.cfg)),
            &Method::GET,
            &uri,
            &mut headers,
            &[],
            &["(request-target)", "host", "date", "digest"],
        )?;
        let response = state
            .delivery
            .p2p_request(
                peer,
                fedi3_protocol::RelayHttpRequest {
                    id: format!("account-{}", crate::chat::random_id()),
                    method: "GET".into(),
                    path: path.into(),
                    query,
                    headers: headers
                        .iter()
                        .map(|(k, v)| {
                            (k.as_str().to_string(), v.to_str().unwrap_or("").to_string())
                        })
                        .collect(),
                    body_b64: String::new(),
                },
            )
            .await?;
        ensure!(response.status == 200, "peer account state unavailable");
        let items: Vec<AccountEnvelope> = serde_json::from_slice(&B64.decode(response.body_b64)?)?;
        if items.is_empty() {
            state.social.set_local_meta("account_initial_pull", "1")?;
            break;
        }
        for env in &items {
            let row = open(
                &crate::ap::canonical_actor_url(&state.cfg),
                &state.private_key_pem,
                env,
            )?;
            apply(&state.social, &row)?;
        }
        offset += items.len();
    }
    let db = state.social.clone();
    tokio::task::spawn_blocking(move || capture(&db)).await??;
    let _ = state
        .ui_events
        .send(crate::ui_events::UiEvent::new("account", None, None));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db(name: &str) -> (crate::test_support::TempDir, SocialDb) {
        let dir = crate::test_support::TempDir::new(name);
        let db = SocialDb::open(dir.path().join("fedi3.db")).unwrap();
        schema(&db).unwrap();
        db.set_local_meta("account_replica_device", name).unwrap();
        db.set_local_meta("account_initial_pull", "1").unwrap();
        (dir, db)
    }
    #[test]
    fn local_write_during_download_is_retained_and_queued() {
        let (_dir, db) = db("racing-local-write");
        db.connect()
            .unwrap()
            .execute(
                "INSERT INTO following VALUES('https://one.example/actor',1,1)",
                [],
            )
            .unwrap();
        capture(&db).unwrap();
        let mut remote = records(&db, false, 0).unwrap().remove(0);
        remote.clock += 100;
        remote.device = "other-device".into();
        remote.data.insert("status".into(), serde_json::json!(0));
        db.connect()
            .unwrap()
            .execute("UPDATE following SET status=2", [])
            .unwrap();
        assert!(!apply(&db, &remote).unwrap());
        let pending = records(&db, true, 0).unwrap().remove(0);
        assert_eq!(pending.data["status"], 2);
        assert!(pending.clock > remote.clock);
        db.connect()
            .unwrap()
            .execute("DELETE FROM following", [])
            .unwrap();
        remote.clock = pending.clock + 100;
        assert!(!apply(&db, &remote).unwrap());
        assert!(records(&db, true, 0).unwrap()[0].deleted);
    }
    #[test]
    fn disjoint_edits_deletions_and_replays_converge_without_echo() {
        let (_a, a) = db("replica-a");
        let (_b, b) = db("replica-b");
        a.connect()
            .unwrap()
            .execute(
                "INSERT INTO following VALUES('https://one.example/actor',1,1)",
                [],
            )
            .unwrap();
        capture(&a).unwrap();
        let initial = records(&a, false, 0).unwrap();
        for row in &initial {
            apply(&b, row).unwrap();
        }
        capture(&b).unwrap();
        assert!(records(&b, true, 0).unwrap().is_empty());
        a.connect()
            .unwrap()
            .execute("DELETE FROM following", [])
            .unwrap();
        capture(&a).unwrap();
        for row in records(&a, false, 0).unwrap() {
            apply(&b, &row).unwrap();
        }
        assert!(!apply(&b, &initial[0]).unwrap());
        assert_eq!(
            b.connect()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM following", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        a.connect()
            .unwrap()
            .execute(
                "INSERT INTO following VALUES('https://a.example/actor',1,1)",
                [],
            )
            .unwrap();
        b.connect()
            .unwrap()
            .execute(
                "INSERT INTO followers VALUES('https://b.example/actor',1)",
                [],
            )
            .unwrap();
        capture(&a).unwrap();
        capture(&b).unwrap();
        for r in records(&a, false, 0).unwrap() {
            apply(&b, &r).unwrap();
        }
        for r in records(&b, false, 0).unwrap() {
            apply(&a, &r).unwrap();
        }
        for table in ["following", "followers"] {
            let count = |db: &SocialDb| {
                db.connect()
                    .unwrap()
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
                        r.get::<_, i64>(0)
                    })
                    .unwrap()
            };
            assert_eq!(count(&a), 1);
            assert_eq!(count(&a), count(&b));
        }
    }
    #[test]
    fn post_deletion_wins_over_later_offline_edit_and_ciphertext_is_account_bound() {
        let (_a, a) = db("replica-delete");
        let (_b, b) = db("replica-import");
        let actor = "https://self.example/users/alice";
        a.set_local_meta(&format!("owned_actor:{actor}"), "1")
            .unwrap();
        b.set_local_meta(&format!("owned_actor:{actor}"), "1")
            .unwrap();
        a.upsert_object_with_actor(
            "https://self.example/post",
            Some(actor),
            serde_json::to_vec(
                &serde_json::json!({"type":"Note","attributedTo":actor,"content":"private post"}),
            )
            .unwrap(),
        )
        .unwrap();
        capture(&a).unwrap();
        let original = records(&a, false, 0)
            .unwrap()
            .into_iter()
            .find(|r| r.kind == "objects")
            .unwrap();
        apply(&b, &original).unwrap();
        a.connect()
            .unwrap()
            .execute("UPDATE objects SET deleted=1", [])
            .unwrap();
        capture(&a).unwrap();
        let deleted = records(&a, false, 0)
            .unwrap()
            .into_iter()
            .find(|r| r.kind == "objects")
            .unwrap();
        apply(&b, &deleted).unwrap();
        let mut edited = original.clone();
        edited.clock = deleted.clock + 1000;
        assert!(!apply(&b, &edited).unwrap());
        assert!(b.is_object_deleted("https://self.example/post").unwrap());
        let pem = include_str!("../../fedi3_protocol/tests/fixtures/legacy_rsa/private.pem");
        let envelope = seal(actor, pem, &deleted).unwrap();
        assert!(!serde_json::to_string(&envelope)
            .unwrap()
            .contains("private post"));
        assert!(open("https://other.example/users/alice", pem, &envelope).is_err());
        let mut modified = envelope.clone();
        modified.clock += 1;
        assert!(open(actor, pem, &modified).is_err());
        assert_eq!(open(actor, pem, &envelope).unwrap().key, deleted.key);
    }
    #[test]
    fn changing_one_local_config_field_preserves_newer_remote_fields_on_restart() {
        let (_dir, db) = db("profile-config-merge");
        update_local_profile(
            &db,
            serde_json::json!({"display_name":"Old name","summary":"Old biography"}),
        )
        .unwrap();
        db.connect()
            .unwrap()
            .execute(
                "UPDATE account_profile SET data=json_set(data,'$.summary','Remote new biography')",
                [],
            )
            .unwrap();
        update_local_profile(
            &db,
            serde_json::json!({"display_name":"New name","summary":"Old biography"}),
        )
        .unwrap();
        let profile = current_profile(&db).unwrap().unwrap();
        assert_eq!(profile["display_name"], "New name");
        assert_eq!(profile["summary"], "Remote new biography");
    }
    #[test]
    fn concurrent_profile_fields_merge_and_blank_bootstrap_cannot_erase_profile() {
        let (_a, a) = db("profile-a");
        let (_b, b) = db("profile-b");
        for (db, name) in [(&a, "Alice"), (&b, "")] {
            let data = serde_json::json!({"display_name":name,"summary":""}).to_string();
            db.connect()
                .unwrap()
                .execute("INSERT INTO account_profile VALUES(1,?1)", [&data])
                .unwrap();
            db.set_local_meta("account_profile_config", &data).unwrap();
            capture(db).unwrap();
        }
        for r in records(&a, false, 0).unwrap() {
            apply(&b, &r).unwrap();
        }
        for r in records(&b, false, 0).unwrap() {
            apply(&a, &r).unwrap();
        }
        assert_eq!(
            current_profile(&b).unwrap().unwrap()["display_name"],
            "Alice"
        );
        a.connect()
            .unwrap()
            .execute(
                "UPDATE account_profile SET data=json_set(data,'$.display_name','New name')",
                [],
            )
            .unwrap();
        b.connect()
            .unwrap()
            .execute(
                "UPDATE account_profile SET data=json_set(data,'$.summary','New biography')",
                [],
            )
            .unwrap();
        capture(&a).unwrap();
        capture(&b).unwrap();
        for r in records(&a, false, 0).unwrap() {
            apply(&b, &r).unwrap();
        }
        for r in records(&b, false, 0).unwrap() {
            apply(&a, &r).unwrap();
        }
        assert_eq!(current_profile(&a).unwrap(), current_profile(&b).unwrap());
        assert_eq!(
            current_profile(&b).unwrap().unwrap()["summary"],
            "New biography"
        );
    }
}
