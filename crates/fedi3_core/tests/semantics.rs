/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

//! Regression tests for the H-17 / H-39 / H-44 / H-05 / H-19 semantics fixes.

#![allow(clippy::field_reassign_with_default, clippy::let_unit_value)]

mod common;

use base64::Engine as _;
use common::*;
use fedi3_core::chat::{ChatEnvelope, ChatPayload};
use fedi3_core::social_db::SocialDb;
use reqwest::StatusCode;
use serde_json::{json, Value};

fn test_db(name: &str) -> (fedi3_core::test_support::TempDir, SocialDb) {
    let dir = fedi3_core::test_support::TempDir::new(name);
    let db = SocialDb::open(dir.path().join("fedi3.db")).expect("open test db");
    (dir, db)
}

fn envelope(thread_id: &str, sender: &str, message_id: &str) -> ChatEnvelope {
    ChatEnvelope {
        v: 1,
        thread_id: thread_id.to_string(),
        message_id: message_id.to_string(),
        sender_actor: sender.to_string(),
        sender_device: "dev-test".to_string(),
        sender_peer_id: String::new(),
        created_at_ms: 1_700_000_000_000,
        kem_alg: "kyber768".to_string(),
        kem_ciphertext_b64: String::new(),
        kem_key_id: "identity".to_string(),
        nonce_b64: String::new(),
        ciphertext_b64: String::new(),
        signature_b64: None,
    }
}

fn message_payload(text: &str) -> ChatPayload {
    ChatPayload {
        op: "message".to_string(),
        text: Some(text.to_string()),
        reply_to: None,
        message_id: None,
        status: None,
        thread_id: None,
        attachments: None,
        action: None,
        targets: None,
        members: None,
        title: None,
        reaction: None,
    }
}

// ---------------------------------------------------------------------------
// H-17: chat membership on inbound paths.
// ---------------------------------------------------------------------------

#[test]
fn h17_message_from_member_is_stored() {
    let (_dir, db) = test_db("h17-member");
    let local = "http://local.test/users/alice";
    let sender = "http://remote.test/users/bob";
    let thread = "thread-member";
    db.create_chat_thread(thread, "group", None).unwrap();
    db.upsert_chat_member(thread, local, "owner").unwrap();
    db.upsert_chat_member(thread, sender, "member").unwrap();

    let env = envelope(thread, sender, "msg-1");
    fedi3_core::chat::store_incoming_payload_db(&db, local, &env, &message_payload("hello"))
        .unwrap();

    let page = db.list_chat_messages(thread, 10, None).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].sender_actor, sender);
}

#[test]
fn h17_message_from_non_member_into_existing_thread_is_dropped() {
    let (_dir, db) = test_db("h17-nonmember");
    let local = "http://local.test/users/alice";
    let sender = "http://remote.test/users/bob";
    let thread = "thread-existing";
    db.create_chat_thread(thread, "group", None).unwrap();
    db.upsert_chat_member(thread, local, "owner").unwrap();

    let env = envelope(thread, sender, "msg-2");
    fedi3_core::chat::store_incoming_payload_db(&db, local, &env, &message_payload("intruder"))
        .unwrap();

    let page = db.list_chat_messages(thread, 10, None).unwrap();
    assert!(page.items.is_empty(), "non-member message must be dropped");
    assert!(db.chat_thread_exists(thread).unwrap());
}

#[test]
fn h17_unknown_thread_from_stranger_does_not_create_thread() {
    let (_dir, db) = test_db("h17-ghost");
    let local = "http://local.test/users/alice";
    let sender = "http://remote.test/users/eve";
    // A valid-looking DM id that is NOT the hash of (sender, local).
    let thread = "urn:fedi3:chat:dm:deadbeef";

    let env = envelope(thread, sender, "msg-3");
    fedi3_core::chat::store_incoming_payload_db(&db, local, &env, &message_payload("ghost"))
        .unwrap();

    assert!(
        !db.chat_thread_exists(thread).unwrap(),
        "unknown thread must not be created for a stranger"
    );
    assert!(db
        .list_chat_messages(thread, 10, None)
        .unwrap()
        .items
        .is_empty());
}

#[test]
fn h17_legitimate_first_dm_creates_thread_with_both_members() {
    let (_dir, db) = test_db("h17-first-dm");
    let local = "http://local.test/users/alice";
    let sender = "http://remote.test/users/bob";
    let mut members = vec![sender.to_string(), local.to_string()];
    members.sort();
    let thread = fedi3_core::ap::chat_dm_thread_id(&members);

    let env = envelope(&thread, sender, "msg-4");
    fedi3_core::chat::store_incoming_payload_db(
        &db,
        local,
        &env,
        &message_payload("first message"),
    )
    .unwrap();

    assert!(db.chat_thread_exists(&thread).unwrap());
    let mut stored: Vec<String> = db
        .list_chat_members(&thread)
        .unwrap()
        .into_iter()
        .map(|(actor, _role)| actor)
        .collect();
    stored.sort();
    assert_eq!(stored, members);
    assert_eq!(
        db.list_chat_messages(&thread, 10, None)
            .unwrap()
            .items
            .len(),
        1
    );
}

// ---------------------------------------------------------------------------
// H-39: outbox visibility.
// ---------------------------------------------------------------------------

fn create_note_activity(actor: &str, id: &str, to: Value, cc: Value) -> Value {
    json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": id,
        "type": "Create",
        "actor": actor,
        "to": to,
        "cc": cc,
        "object": {
            "id": format!("{id}/object"),
            "type": "Note",
            "attributedTo": actor,
            "content": "test",
            "to": to,
            "cc": cc,
        }
    })
}

#[test]
fn h39_direct_message_is_never_in_outbox() {
    let (_dir, db) = test_db("h39-dm");
    let dm = create_note_activity(
        "http://local.test/users/alice",
        "http://local.test/activities/dm",
        json!(["http://remote.test/users/bob"]),
        json!([]),
    );
    db.store_outbox("dm", serde_json::to_vec(&dm).unwrap())
        .unwrap();

    let anonymous = db.list_outbox_visible(10, None, None, false).unwrap();
    assert_eq!(anonymous.total, 0);
    assert!(anonymous.items.is_empty());

    let follower = db
        .list_outbox_visible(10, None, Some("http://remote.test/users/bob"), true)
        .unwrap();
    assert!(follower.items.is_empty(), "direct is never in the outbox");
}

#[test]
fn h39_followers_only_visibility_requires_a_follower() {
    let (_dir, db) = test_db("h39-followers");
    let followers_url = "http://local.test/users/alice/followers";
    let followers = create_note_activity(
        "http://local.test/users/alice",
        "http://local.test/activities/followers",
        json!([followers_url]),
        json!([]),
    );
    db.store_outbox("followers", serde_json::to_vec(&followers).unwrap())
        .unwrap();

    let anonymous = db.list_outbox_visible(10, None, None, false).unwrap();
    assert_eq!(anonymous.total, 0, "anonymous must not see followers-only");

    let non_follower = db
        .list_outbox_visible(10, None, Some("http://remote.test/users/bob"), false)
        .unwrap();
    assert_eq!(non_follower.total, 0, "non-follower must not see it");

    let follower = db
        .list_outbox_visible(10, None, Some("http://remote.test/users/bob"), true)
        .unwrap();
    assert_eq!(follower.total, 1, "follower must see followers-only");
    assert_eq!(follower.items.len(), 1);
}

#[test]
fn h39_public_and_unlisted_are_always_visible() {
    let (_dir, db) = test_db("h39-public");
    let public = create_note_activity(
        "http://local.test/users/alice",
        "http://local.test/activities/public",
        json!(["https://www.w3.org/ns/activitystreams#Public"]),
        json!(["http://local.test/users/alice/followers"]),
    );
    let unlisted = create_note_activity(
        "http://local.test/users/alice",
        "http://local.test/activities/unlisted",
        json!(["http://local.test/users/alice/followers"]),
        json!(["https://www.w3.org/ns/activitystreams#Public"]),
    );
    db.store_outbox("public", serde_json::to_vec(&public).unwrap())
        .unwrap();
    db.store_outbox("unlisted", serde_json::to_vec(&unlisted).unwrap())
        .unwrap();

    let anonymous = db.list_outbox_visible(10, None, None, false).unwrap();
    assert_eq!(anonymous.total, 2);
    assert_eq!(anonymous.items.len(), 2);
}

#[test]
fn h39_legacy_null_visibility_rows_are_treated_as_public() {
    let (dir, _db) = test_db("h39-legacy");
    {
        let conn = rusqlite::Connection::open(dir.path().join("fedi3.db")).unwrap();
        conn.execute(
            "INSERT INTO outbox_items(id, created_at_ms, activity_json) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "legacy",
                1i64,
                br#"{"id":"legacy","type":"Announce","actor":"http://local.test/users/alice"}"#
                    .to_vec()
            ],
        )
        .unwrap();
    }
    let db = SocialDb::open(dir.path().join("fedi3.db")).unwrap();
    let anonymous = db.list_outbox_visible(10, None, None, false).unwrap();
    assert_eq!(anonymous.total, 1);
}

#[tokio::test]
async fn h39_outbox_endpoint_hides_dm_from_anonymous_view() {
    let core = TestCore::new().unwrap();
    let dm = create_note_activity(
        &core.actor_url(),
        "http://local.test/activities/http-dm",
        json!(["http://remote.test/users/bob"]),
        json!([]),
    );
    let public = create_note_activity(
        &core.actor_url(),
        "http://local.test/activities/http-public",
        json!(["https://www.w3.org/ns/activitystreams#Public"]),
        json!([]),
    );
    assert_eq!(
        core.post_internal(&format!("/users/{}/outbox", core.username), &json_body(&dm))
            .await
            .status(),
        StatusCode::ACCEPTED
    );
    assert_eq!(
        core.post_internal(
            &format!("/users/{}/outbox", core.username),
            &json_body(&public)
        )
        .await
        .status(),
        StatusCode::ACCEPTED
    );

    let resp = TestCore::client()
        .get(core.url(&format!("/users/{}/outbox?page=true", core.username)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    let ids: Vec<&str> = v["orderedItems"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect();
    assert!(ids.contains(&"http://local.test/activities/http-public"));
    assert!(!ids.contains(&"http://local.test/activities/http-dm"));
}

#[tokio::test]
async fn h39_signed_follower_sees_followers_only_in_outbox() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    {
        let conn = rusqlite::Connection::open(core.db_path()).unwrap();
        conn.execute(
            "INSERT INTO followers(actor_id, created_at_ms) VALUES (?1, ?2)",
            rusqlite::params![mock.actor_url, 1i64],
        )
        .unwrap();
    }
    let followers_url = format!("{}/users/{}/followers", core.base_url, core.username);
    let followers_only = create_note_activity(
        &core.actor_url(),
        "http://local.test/activities/http-followers",
        json!([followers_url]),
        json!([]),
    );
    assert_eq!(
        core.post_internal(
            &format!("/users/{}/outbox", core.username),
            &json_body(&followers_only)
        )
        .await
        .status(),
        StatusCode::ACCEPTED
    );

    let outbox = core.url(&format!("/users/{}/outbox?page=true", core.username));
    let anonymous: Value = serde_json::from_str(
        &TestCore::client()
            .get(&outbox)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(anonymous["orderedItems"].as_array().unwrap().is_empty());

    let signed: Value = serde_json::from_str(
        &signer
            .signed_get(&outbox)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(signed["orderedItems"].as_array().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// H-44: locked accounts (manuallyApprovesFollowers).
// ---------------------------------------------------------------------------

fn follow_activity(mock: &MockActorServer, core: &TestCore, suffix: &str) -> Value {
    json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{}/activities/{suffix}", mock.actor_url),
        "type": "Follow",
        "actor": mock.actor_url,
        "object": core.actor_url(),
    })
}

#[tokio::test]
async fn h44_locked_account_follow_is_pending_without_accept() {
    let core = TestCore::with_config(|cfg| cfg.manually_approves_followers = Some(true)).unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let follow = follow_activity(&mock, &core, "follow-locked");
    let resp = core.post_inbox_signed(&signer, &json_body(&follow)).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);

    assert_eq!(
        sqlite_count(
            &core.db_path(),
            "SELECT COUNT(*) FROM follow_requests WHERE actor_id=?1 AND state='pending'",
            &[&mock.actor_url],
        ),
        1
    );
    assert_eq!(
        sqlite_count(
            &core.db_path(),
            "SELECT COUNT(*) FROM followers WHERE actor_id=?1",
            &[&mock.actor_url],
        ),
        0,
        "locked account must not add the follower before approval"
    );
    assert_eq!(
        sqlite_count(
            &core.db_path(),
            "SELECT COUNT(*) FROM outbox_items WHERE CAST(activity_json AS TEXT) LIKE '%\"type\":\"Accept\"%'",
            &[],
        ),
        0,
        "locked account must not send Accept before approval"
    );

    let list: Value = serde_json::from_str(
        &core
            .get("/_fedi3/follow_requests")
            .await
            .text()
            .await
            .unwrap(),
    )
    .unwrap();
    let items = list.as_array().expect("JSON array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["actor"], mock.actor_url);
    assert_eq!(items[0]["state"], "pending");
}

#[tokio::test]
async fn h44_approve_adds_follower_and_sends_accept() {
    let core = TestCore::with_config(|cfg| cfg.manually_approves_followers = Some(true)).unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let follow = follow_activity(&mock, &core, "follow-approve");
    assert_eq!(
        core.post_inbox_signed(&signer, &json_body(&follow))
            .await
            .status(),
        StatusCode::ACCEPTED
    );

    let path = format!(
        "/_fedi3/follow_requests/{}/approve",
        urlencoding::encode(&mock.actor_url)
    );
    let resp = core.post_internal(&path, b"").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    assert_eq!(body["ok"], true);
    assert_eq!(body["accept"]["type"], "Accept");
    assert_eq!(body["accept"]["object"]["type"], "Follow");
    assert_eq!(body["accept"]["object"]["actor"], mock.actor_url);

    assert_eq!(
        sqlite_count(
            &core.db_path(),
            "SELECT COUNT(*) FROM followers WHERE actor_id=?1",
            &[&mock.actor_url],
        ),
        1
    );
    assert_eq!(
        sqlite_count(
            &core.db_path(),
            "SELECT COUNT(*) FROM follow_requests WHERE actor_id=?1",
            &[&mock.actor_url],
        ),
        0
    );
    assert_eq!(
        sqlite_count(
            &core.db_path(),
            "SELECT COUNT(*) FROM outbox_items WHERE CAST(activity_json AS TEXT) LIKE '%\"type\":\"Accept\"%'",
            &[],
        ),
        1
    );
}

#[tokio::test]
async fn h44_reject_sends_reject_and_keeps_actor_out() {
    let core = TestCore::with_config(|cfg| cfg.manually_approves_followers = Some(true)).unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let follow = follow_activity(&mock, &core, "follow-reject");
    assert_eq!(
        core.post_inbox_signed(&signer, &json_body(&follow))
            .await
            .status(),
        StatusCode::ACCEPTED
    );

    let path = format!(
        "/_fedi3/follow_requests/{}/reject",
        urlencoding::encode(&mock.actor_url)
    );
    let resp = core.post_internal(&path, b"").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    assert_eq!(body["ok"], true);
    assert_eq!(body["reject"]["type"], "Reject");
    assert_eq!(body["reject"]["object"]["type"], "Follow");

    assert_eq!(
        sqlite_count(
            &core.db_path(),
            "SELECT COUNT(*) FROM followers WHERE actor_id=?1",
            &[&mock.actor_url],
        ),
        0
    );
    assert_eq!(
        sqlite_count(
            &core.db_path(),
            "SELECT COUNT(*) FROM follow_requests WHERE actor_id=?1",
            &[&mock.actor_url],
        ),
        0
    );
    assert_eq!(
        sqlite_count(
            &core.db_path(),
            "SELECT COUNT(*) FROM outbox_items WHERE CAST(activity_json AS TEXT) LIKE '%\"type\":\"Reject\"%'",
            &[],
        ),
        1
    );
}

#[tokio::test]
async fn h44_unlocked_account_still_accepts_immediately() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let follow = follow_activity(&mock, &core, "follow-unlocked");
    assert_eq!(
        core.post_inbox_signed(&signer, &json_body(&follow))
            .await
            .status(),
        StatusCode::ACCEPTED
    );

    assert_eq!(
        sqlite_count(
            &core.db_path(),
            "SELECT COUNT(*) FROM followers WHERE actor_id=?1",
            &[&mock.actor_url],
        ),
        1
    );
    assert_eq!(
        sqlite_count(
            &core.db_path(),
            "SELECT COUNT(*) FROM follow_requests WHERE actor_id=?1",
            &[&mock.actor_url],
        ),
        0
    );
    assert_eq!(
        sqlite_count(
            &core.db_path(),
            "SELECT COUNT(*) FROM outbox_items WHERE CAST(activity_json AS TEXT) LIKE '%\"type\":\"Accept\"%'",
            &[],
        ),
        1
    );
}

// ---------------------------------------------------------------------------
// H-05: media decode limits.
// ---------------------------------------------------------------------------

fn pixel_png() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==")
        .unwrap()
}

async fn upload_media(
    core: &TestCore,
    filename: &str,
    content_type: &str,
    bytes: Vec<u8>,
) -> reqwest::Response {
    TestCore::client()
        .post(core.url(&format!("/users/{}/media", core.username)))
        .header("X-Fedi3-Internal", INTERNAL_TOKEN)
        .header("X-Filename", filename)
        .header("Content-Type", content_type)
        .body(bytes)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn h05_oversize_body_is_rejected_before_buffering() {
    let core = TestCore::new().unwrap();
    let big = vec![0u8; 2 * 1024 * 1024];
    let resp = upload_media(&core, "big.png", "image/png", big).await;
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn h05_non_image_bytes_are_rejected() {
    let core = TestCore::new().unwrap();
    let resp = upload_media(&core, "fake.png", "image/png", b"not an image".to_vec()).await;
    assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn h05_oversized_dimensions_are_rejected() {
    let core = TestCore::new().unwrap();
    let img = image::RgbImage::from_pixel(9000, 1, image::Rgb([255u8, 0, 0]));
    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    assert!(png.len() < 1024 * 1024, "test image must fit the body cap");
    let resp = upload_media(&core, "wide.png", "image/png", png).await;
    assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn h05_valid_small_image_is_accepted() {
    let core = TestCore::new().unwrap();
    let resp = upload_media(&core, "pixel.png", "image/png", pixel_png()).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let v: Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    assert_eq!(v["width"], 1);
    assert_eq!(v["height"], 1);
}

// ---------------------------------------------------------------------------
// H-19: SQLite file permissions.
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[tokio::test]
async fn h19_sqlite_db_file_is_0600() {
    use std::os::unix::fs::PermissionsExt as _;

    let core = TestCore::new().unwrap();
    let mode = std::fs::metadata(core.db_path())
        .expect("db metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "fedi3.db must be owner-only");

    for suffix in ["-wal", "-shm"] {
        let path = std::path::PathBuf::from(format!("{}{suffix}", core.db_path().display()));
        if path.exists() {
            let mode = std::fs::metadata(&path)
                .expect("journal metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "{suffix} must be owner-only");
        }
    }
}
