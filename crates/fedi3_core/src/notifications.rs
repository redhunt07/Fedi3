//! Selective account replication and device-local, durable alert claims.
use crate::{ap::ApState, social_db::SocialDb};
use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde_json::Value;

pub(crate) fn schema(db: &SocialDb) -> Result<()> {
    let c = db.connect()?;
    c.execute_batch("CREATE TABLE IF NOT EXISTS personal_notifications(activity_id TEXT PRIMARY KEY,created_at_ms INTEGER NOT NULL,actor_id TEXT,type TEXT NOT NULL,activity_json BLOB NOT NULL); CREATE TABLE IF NOT EXISTS notification_alerts(activity_id TEXT PRIMARY KEY,claimed INTEGER NOT NULL DEFAULT 0);")?;
    c.execute(
        "INSERT OR IGNORE INTO local_meta(key,value) VALUES('notification_alert_epoch',?1)",
        [now().to_string()],
    )?;
    Ok(())
}

fn alert(c: &rusqlite::Connection, id: &str, ts: i64, eligible: bool) -> Result<()> {
    let epoch: String = c.query_row(
        "SELECT value FROM local_meta WHERE key='notification_alert_epoch'",
        [],
        |r| r.get(0),
    )?;
    let claimed = !eligible || ts < epoch.parse::<i64>().unwrap_or(i64::MAX);
    c.execute(
        "INSERT OR IGNORE INTO notification_alerts(activity_id,claimed) VALUES(?1,?2)",
        params![id, claimed],
    )?;
    Ok(())
}

pub(crate) fn record(state: &ApState, activity: &Value, historical: bool) -> Result<bool> {
    schema(&state.social)?;
    let id = crate::ap::activity_dedup_id(activity);
    let mut c = state.social.connect()?;
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let ts = tx
        .query_row(
            "SELECT created_at_ms FROM inbox_items WHERE activity_id=?1",
            [&id],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or_else(now);
    let actor = activity.get("actor").and_then(Value::as_str);
    let kind = activity.get("type").and_then(Value::as_str).unwrap_or("");
    let raw = serde_json::to_vec(activity)?;
    let added = tx.execute(
        "INSERT OR IGNORE INTO personal_notifications VALUES(?1,?2,?3,?4,?5)",
        params![id, ts, actor, kind, raw],
    )? > 0;
    tx.execute("INSERT OR IGNORE INTO inbox_items(activity_id,created_at_ms,actor_id,type,activity_json) VALUES(?1,?2,?3,?4,?5)",params![id,ts,actor,kind,raw])?;
    if added {
        alert(&tx, &id, ts, !historical)?;
    }
    tx.commit()?;
    if added {
        state.social.account_notify.notify_one();
    }
    Ok(added)
}

pub(crate) fn backfill(state: &ApState) -> Result<()> {
    schema(&state.social)?;
    if state
        .social
        .get_local_meta("notifications_backfill_v1")?
        .is_some()
    {
        return Ok(());
    }
    let c = state.social.connect()?;
    let mut q = c.prepare("SELECT activity_json FROM inbox_items WHERE type IN ('Follow','Accept','Reject','Announce','Like','EmojiReact','Create')")?;
    let rows = q
        .query_map([], |r| r.get::<_, Vec<u8>>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let me = crate::ap::canonical_actor_url(&state.cfg);
    let handle = format!("@{}@{}", state.cfg.username, state.cfg.domain);
    for raw in rows {
        if let Ok(v) = serde_json::from_slice::<Value>(&raw) {
            if crate::ap::is_notification(state, &v, &me, &handle) {
                record(state, &v, true)?;
            }
        }
    }
    state
        .social
        .set_local_meta("notifications_backfill_v1", "1")?;
    Ok(())
}

/// Mirror imported records without rerunning federation side effects (Accept, delivery, etc.).
pub(crate) fn mirror(c: &rusqlite::Connection, id: &str) -> Result<bool> {
    let row: (i64,Option<String>,String,Vec<u8>) = c.query_row("SELECT created_at_ms,actor_id,type,activity_json FROM personal_notifications WHERE activity_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
    let added = c.execute("INSERT OR IGNORE INTO inbox_items(activity_id,created_at_ms,actor_id,type,activity_json) VALUES(?1,?2,?3,?4,?5)",params![id,row.0,row.1,row.2,row.3])? > 0;
    alert(c, id, row.0, added)?;
    Ok(added)
}

pub(crate) fn claim(db: &SocialDb) -> Result<Vec<Value>> {
    schema(db)?;
    let mut c = db.connect()?;
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let rows = {
        let mut q = tx.prepare("SELECT n.activity_id,n.created_at_ms,n.type FROM notification_alerts a JOIN personal_notifications n USING(activity_id) WHERE a.claimed=0 ORDER BY n.created_at_ms,n.activity_id LIMIT 200")?;
        let rows=q.query_map([],|r|Ok(serde_json::json!({"activity_id":r.get::<_,String>(0)?,"ts":r.get::<_,i64>(1)?,"type":r.get::<_,String>(2)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for v in &rows {
        tx.execute(
            "UPDATE notification_alerts SET claimed=1 WHERE activity_id=?1",
            [v["activity_id"].as_str().unwrap()],
        )?;
    }
    tx.commit()?;
    Ok(rows)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
