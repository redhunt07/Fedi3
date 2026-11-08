/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

#![allow(clippy::field_reassign_with_default, clippy::let_unit_value)]

mod common;

use common::*;
use fedi3_core::delivery::Delivery;
use fedi3_core::delivery_queue::{DeliveryQueue, PostDeliveryMode, QueueSettings};
use fedi3_core::social_db::SocialDb;
use std::sync::Arc;
use std::time::Duration;

fn temp_db() -> (fedi3_core::test_support::TempDir, std::path::PathBuf) {
    let dir = fedi3_core::test_support::TempDir::new("db");
    let path = dir.path().join("fedi3.db");
    (dir, path)
}

async fn wait_until<F, Fut>(mut f: F, timeout: Duration) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if f().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    f().await
}

#[test]
fn d01_schema_creation_has_tables_and_version() {
    let (_dir, path) = temp_db();
    let db = SocialDb::open(&path).unwrap();
    let _ = db.health_check().unwrap();
    let _queue = DeliveryQueue::open(&path).unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    for table in [
        "inbox_seen",
        "inbox_items",
        "objects",
        "outbox_items",
        "followers",
        "following",
        "media_items",
        "delivery_jobs",
        "schema_version",
    ] {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                rusqlite::params![table],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "missing table {table}");
    }
    let version: i64 = conn
        .query_row("SELECT version FROM schema_version LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(version, fedi3_core::social_db::SCHEMA_VERSION as i64);
}

#[test]
fn d02_migration_from_legacy_schema_preserves_data() {
    let (_dir, path) = temp_db();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE objects (
              object_id TEXT PRIMARY KEY,
              created_at_ms INTEGER NOT NULL,
              updated_at_ms INTEGER NOT NULL,
              deleted INTEGER NOT NULL,
              object_json BLOB NOT NULL
            );
            "#,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO objects(object_id, created_at_ms, updated_at_ms, deleted, object_json) VALUES ('https://x.test/objects/1', 1, 1, 0, X'7B7D')",
            [],
        )
        .unwrap();
    }
    let db = SocialDb::open(&path).unwrap();
    let json = db.get_object_json("https://x.test/objects/1").unwrap();
    assert!(json.is_some());
    let conn = rusqlite::Connection::open(&path).unwrap();
    let cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(objects)").unwrap();
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        rows
    };
    assert!(cols.contains(&"actor_id".to_string()));
    assert!(cols.contains(&"size_bytes".to_string()));
}

#[test]
fn d03_newer_schema_version_is_rejected() {
    let (_dir, path) = temp_db();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version(version INTEGER NOT NULL); INSERT INTO schema_version(version) VALUES (999);",
        )
        .unwrap();
    }
    let err = match SocialDb::open(&path) {
        Ok(_) => panic!("newer schema version must be rejected"),
        Err(e) => e.to_string(),
    };
    assert!(err.contains("newer than this binary"), "{err}");
}

#[test]
fn d04_object_round_trip() {
    let (_dir, path) = temp_db();
    let db = SocialDb::open(&path).unwrap();
    let json = br#"{"id":"https://x.test/objects/2","type":"Note","content":"hi"}"#.to_vec();
    db.upsert_object_with_actor(
        "https://x.test/objects/2",
        Some("https://x.test/users/a"),
        json.clone(),
    )
    .unwrap();
    assert_eq!(
        db.get_object_json("https://x.test/objects/2")
            .unwrap()
            .unwrap(),
        json
    );
    assert_eq!(
        db.get_object_actor("https://x.test/objects/2")
            .unwrap()
            .as_deref(),
        Some("https://x.test/users/a")
    );
}

#[test]
fn d05_inbox_dedup_helpers() {
    let (_dir, path) = temp_db();
    let db = SocialDb::open(&path).unwrap();
    assert!(!db.has_inbox_seen("act-1").unwrap());
    assert!(db.mark_inbox_seen("act-1").unwrap());
    assert!(db.has_inbox_seen("act-1").unwrap());
    assert!(!db.mark_inbox_seen("act-1").unwrap());
    db.forget_inbox_seen("act-1").unwrap();
    assert!(!db.has_inbox_seen("act-1").unwrap());
}

#[test]
fn d06_pagination_with_equal_timestamps_loses_nothing() {
    let (_dir, path) = temp_db();
    let db = SocialDb::open(&path).unwrap();
    for i in 0..5 {
        let body = serde_json::to_vec(&serde_json::json!({"id": format!("act-{i}")})).unwrap();
        db.store_outbox_at(&format!("act-{i}"), 1000, body).unwrap();
    }
    let mut seen = std::collections::HashSet::new();
    let mut cursor: Option<String> = None;
    loop {
        let page = db.list_outbox(2, cursor.clone()).unwrap();
        for raw in page.items {
            let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
            seen.insert(v["id"].as_str().unwrap().to_string());
        }
        match page.next {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    assert_eq!(
        seen.len(),
        5,
        "pagination must not skip same-ms rows: {seen:?}"
    );
}

#[test]
fn d06b_followers_pagination_with_equal_timestamps() {
    let (_dir, path) = temp_db();
    let db = SocialDb::open(&path).unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    for i in 0..5 {
        conn.execute(
            "INSERT INTO followers(actor_id, created_at_ms) VALUES (?1, 2000)",
            rusqlite::params![format!("https://x.test/users/{i}")],
        )
        .unwrap();
    }
    drop(conn);
    let mut seen = std::collections::HashSet::new();
    let mut cursor: Option<String> = None;
    loop {
        let page = db.list_followers(2, cursor.clone()).unwrap();
        for a in page.items {
            seen.insert(a);
        }
        match page.next {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    assert_eq!(seen.len(), 5);
}

#[test]
fn d07_pruning_respects_counts() {
    let (_dir, path) = temp_db();
    let db = SocialDb::open(&path).unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    for i in 0..10 {
        conn.execute(
            "INSERT INTO inbox_items(activity_id, created_at_ms, activity_json) VALUES (?1, ?2, X'7B7D')",
            rusqlite::params![format!("a-{i}"), i as i64],
        )
        .unwrap();
    }
    drop(conn);
    let deleted = db.prune_inbox_items(3).unwrap();
    assert_eq!(deleted, 7);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let left: i64 = conn
        .query_row("SELECT COUNT(*) FROM inbox_items", [], |r| r.get(0))
        .unwrap();
    assert_eq!(left, 3);
}

#[test]
fn d10_search_escapes_like_wildcards() {
    let (_dir, path) = temp_db();
    let db = SocialDb::open(&path).unwrap();
    db.upsert_object(
        "https://x.test/objects/a",
        br#"{"content":"100% done"}"#.to_vec(),
    )
    .unwrap();
    db.upsert_object(
        "https://x.test/objects/b",
        br#"{"content":"1000 done"}"#.to_vec(),
    )
    .unwrap();
    db.upsert_object(
        "https://x.test/objects/c",
        br#"{"content":"foo_bar"}"#.to_vec(),
    )
    .unwrap();
    db.upsert_object(
        "https://x.test/objects/d",
        br#"{"content":"fooXbar"}"#.to_vec(),
    )
    .unwrap();

    let page = db.search_notes_by_text("100%", 10, None).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].object_id, "https://x.test/objects/a");
    let page = db.search_notes_by_text("foo_bar", 10, None).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].object_id, "https://x.test/objects/c");
}

#[test]
fn d11_invalid_cursor_returns_first_page() {
    let (_dir, path) = temp_db();
    let db = SocialDb::open(&path).unwrap();
    db.store_outbox("act-1", br#"{"id":"act-1"}"#.to_vec())
        .unwrap();
    let page = db
        .list_outbox(10, Some("not-a-cursor".to_string()))
        .unwrap();
    assert_eq!(page.items.len(), 1);
}

#[test]
fn d09_eight_parallel_writers_do_not_hit_sqlite_busy() {
    let (_dir, path) = temp_db();
    let db = Arc::new(SocialDb::open(&path).unwrap());
    let mut handles = Vec::new();
    for t in 0..8 {
        let db = db.clone();
        handles.push(std::thread::spawn(move || {
            for i in 0..40 {
                db.insert_audit_event(
                    "test",
                    Some("https://x.test/users/a"),
                    None,
                    Some(&format!("act-{t}-{i}")),
                    true,
                    Some("202"),
                    None,
                )
                .expect("concurrent insert must not fail with SQLITE_BUSY");
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let conn = rusqlite::Connection::open(&path).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM audit_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 320);
}

fn queue_settings() -> QueueSettings {
    QueueSettings {
        max_attempts: 1,
        base_backoff_secs: 1,
        max_backoff_secs: 2,
        post_delivery_mode: PostDeliveryMode::P2pRelay,
        p2p_relay_fallback_secs: 0,
        p2p_cache_ttl_secs: 60,
    }
}

#[tokio::test]
async fn q01_enqueue_persists_jobs() {
    let (_dir, path) = temp_db();
    let q = DeliveryQueue::open(&path).unwrap();
    let pending = q
        .enqueue_activity(
            br#"{"id":"https://x.test/activities/1","type":"Create"}"#.to_vec(),
            vec![
                "https://x.test/users/a".to_string(),
                "https://x.test/users/b".to_string(),
            ],
        )
        .await
        .unwrap();
    assert_eq!(pending, 2);
    let stats = q.stats().await.unwrap();
    assert_eq!(stats.pending, 2);
}

#[test]
fn q02_backoff_is_monotonic_and_capped() {
    let d1 = fedi3_core::delivery_queue::next_backoff(1, 5, 3600);
    let d3 = fedi3_core::delivery_queue::next_backoff(3, 5, 3600);
    let d20 = fedi3_core::delivery_queue::next_backoff(20, 5, 3600);
    assert!(d1 < d3);
    assert!(d20 <= Duration::from_secs(3600) + Duration::from_millis(1000));
}

#[tokio::test]
async fn q03_max_attempts_dead_letters_job() {
    let (_dir, path) = temp_db();
    let mock = MockActorServer::new("bob").await.unwrap();
    mock.inbox_status
        .store(500, std::sync::atomic::Ordering::Relaxed);
    let q = Arc::new(DeliveryQueue::open(&path).unwrap());
    let (tx, rx) = tokio::sync::watch::channel(false);
    q.start_worker(
        rx,
        Arc::new(Delivery::new()),
        shared_private_pem(),
        "https://x.test/users/alice#main-key".to_string(),
        queue_settings(),
    );
    q.enqueue_activity(
        br#"{"id":"https://x.test/activities/2","type":"Create"}"#.to_vec(),
        vec![mock.actor_url.clone()],
    )
    .await
    .unwrap();
    let dead = wait_until(
        || async { q.stats().await.map(|s| s.dead == 1).unwrap_or(false) },
        Duration::from_secs(20),
    )
    .await;
    let _ = tx.send(true);
    assert!(dead, "job should be dead-lettered after max_attempts");
}

#[tokio::test]
async fn q04_receipt_marks_delivered() {
    let (_dir, path) = temp_db();
    let q = DeliveryQueue::open(&path).unwrap();
    q.enqueue_activity(
        br#"{"id":"https://x.test/activities/3","type":"Create"}"#.to_vec(),
        vec!["https://x.test/users/a".to_string()],
    )
    .await
    .unwrap();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute("UPDATE delivery_jobs SET status=3, attempt=1", [])
            .unwrap();
    }
    let updated = q
        .mark_delivered_by_receipt("https://x.test/activities/3", "https://x.test/users/a")
        .await
        .unwrap();
    assert_eq!(updated, 1);
    let stats = q.stats().await.unwrap();
    assert_eq!(stats.delivered, 1);
}

#[tokio::test]
async fn q05_jobs_awaiting_receipt_reach_max_attempts() {
    let (_dir, path) = temp_db();
    let q = Arc::new(DeliveryQueue::open(&path).unwrap());
    q.enqueue_activity(
        br#"{"id":"https://x.test/activities/4","type":"Create"}"#.to_vec(),
        vec!["https://x.test/users/a".to_string()],
    )
    .await
    .unwrap();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "UPDATE delivery_jobs SET status=3, attempt=10, next_attempt_at_ms=0",
            [],
        )
        .unwrap();
    }
    let (tx, rx) = tokio::sync::watch::channel(false);
    q.start_worker(
        rx,
        Arc::new(Delivery::new()),
        shared_private_pem(),
        "https://x.test/users/alice#main-key".to_string(),
        QueueSettings {
            max_attempts: 10,
            ..queue_settings()
        },
    );
    let dead = wait_until(
        || async { q.stats().await.map(|s| s.dead == 1).unwrap_or(false) },
        Duration::from_secs(10),
    )
    .await;
    let _ = tx.send(true);
    assert!(dead, "status=3 job must be dead-lettered at max_attempts");
}

#[tokio::test]
async fn q07_prune_removes_old_jobs() {
    let (_dir, path) = temp_db();
    let q = DeliveryQueue::open(&path).unwrap();
    for i in 0..3 {
        q.enqueue_activity(
            serde_json::to_vec(&serde_json::json!({"id": format!("act-{i}")})).unwrap(),
            vec![format!("https://x.test/users/{i}")],
        )
        .await
        .unwrap();
    }
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute("UPDATE delivery_jobs SET status=1, created_at_ms=1", [])
            .unwrap();
    }
    let deleted = q.prune_delivery_jobs(0, 0).await.unwrap();
    assert_eq!(deleted, 3);
}

#[tokio::test]
async fn q11_pending_jobs_resume_after_reopen() {
    let (_dir, path) = temp_db();
    {
        let q = DeliveryQueue::open(&path).unwrap();
        q.enqueue_activity(
            br#"{"id":"https://x.test/activities/5","type":"Create"}"#.to_vec(),
            vec!["https://x.test/users/a".to_string()],
        )
        .await
        .unwrap();
    }
    let q = DeliveryQueue::open(&path).unwrap();
    let stats = q.stats().await.unwrap();
    assert_eq!(stats.pending, 1);
}

#[tokio::test]
async fn q06_worker_survives_transient_db_error() {
    let (_dir, path) = temp_db();
    let mock = MockActorServer::new("bob").await.unwrap();
    let q = Arc::new(DeliveryQueue::open(&path).unwrap());
    let (tx, rx) = tokio::sync::watch::channel(false);
    q.start_worker(
        rx,
        Arc::new(Delivery::new()),
        shared_private_pem(),
        "https://x.test/users/alice#main-key".to_string(),
        QueueSettings {
            max_attempts: 3,
            ..queue_settings()
        },
    );
    q.enqueue_activity(
        br#"{"id":"https://x.test/activities/6","type":"Create"}"#.to_vec(),
        vec![mock.actor_url.clone()],
    )
    .await
    .unwrap();

    // Simulate a transient DB error while the worker is running.
    let lock_conn = rusqlite::Connection::open(&path).unwrap();
    lock_conn.execute_batch("BEGIN IMMEDIATE;").unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    lock_conn.execute_batch("ROLLBACK;").unwrap();
    drop(lock_conn);

    let delivered = wait_until(
        || async { q.stats().await.map(|s| s.delivered == 1).unwrap_or(false) },
        Duration::from_secs(20),
    )
    .await;
    let _ = tx.send(true);
    assert!(
        delivered,
        "worker must recover and deliver after a transient DB error"
    );
}

#[test]
fn d90_chat_pending_delivery_queue_round_trip() {
    let (_dir, path) = temp_db();
    let db = SocialDb::open(&path).unwrap();
    let payload = br#"{"op":"message","text":"ciao","thread_id":"urn:fedi3:chat:dm:x"}"#;
    db.enqueue_chat_delivery("urn:fedi3:chat:dm:x", "https://relay/users/alice", "urn:fedi3:chat:msg:1", payload, 1_000)
        .unwrap();
    // Duplicate enqueue must not create a second row.
    db.enqueue_chat_delivery("urn:fedi3:chat:dm:x", "https://relay/users/alice", "urn:fedi3:chat:msg:1", payload, 1_000)
        .unwrap();
    assert_eq!(db.count_chat_deliveries().unwrap(), 1);

    let due = db.due_chat_deliveries(1_000, 10).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].actor_id, "https://relay/users/alice");
    assert_eq!(due[0].message_id, "urn:fedi3:chat:msg:1");
    assert_eq!(due[0].attempts, 0);

    // Not due before its retry time.
    assert!(db.due_chat_deliveries(999, 10).unwrap().is_empty());

    db.reschedule_chat_delivery(due[0].id, 1, 5_000).unwrap();
    let due = db.due_chat_deliveries(5_000, 10).unwrap();
    assert_eq!(due[0].attempts, 1);

    db.finish_chat_delivery(due[0].id).unwrap();
    assert_eq!(db.count_chat_deliveries().unwrap(), 0);
}
