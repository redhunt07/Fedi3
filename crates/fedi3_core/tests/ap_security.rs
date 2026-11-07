/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

#![allow(clippy::field_reassign_with_default, clippy::let_unit_value)]

mod common;

use base64::Engine as _;
use common::*;
use reqwest::StatusCode;
use sha2::Digest as _;

async fn text(resp: reqwest::Response) -> String {
    resp.text().await.unwrap_or_default()
}

#[tokio::test]
async fn a01_actor_document_has_required_fields() {
    let core = TestCore::new().unwrap();
    let resp = core.get(&format!("/users/{}", core.username)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&text(resp).await).unwrap();
    assert!(v["@context"].is_array() || v["@context"].is_string());
    assert_eq!(v["type"], "Person");
    assert_eq!(v["preferredUsername"], core.username);
    assert!(v["inbox"].as_str().unwrap().ends_with("/inbox"));
    assert!(v["outbox"].as_str().unwrap().ends_with("/outbox"));
    assert!(v["followers"].as_str().unwrap().ends_with("/followers"));
    assert!(v["publicKey"]["publicKeyPem"]
        .as_str()
        .unwrap()
        .contains("BEGIN PUBLIC KEY"));
    assert_eq!(
        v["publicKey"]["id"].as_str().unwrap(),
        format!("{}/users/{}#main-key", core.base_url, core.username)
    );
}

#[tokio::test]
async fn a02_actor_document_is_activitypub_compatible() {
    let core = TestCore::new().unwrap();
    let resp = core.get(&format!("/users/{}", core.username)).await;
    let v: serde_json::Value = serde_json::from_str(&text(resp).await).unwrap();
    // Mastodon/Misskey require endpoints.sharedInbox, inbox, outbox and a public key owner.
    assert!(v["endpoints"]["sharedInbox"].is_string());
    assert!(v["publicKey"]["owner"].is_string());
    assert_eq!(v["publicKey"]["owner"], v["id"]);
}

#[tokio::test]
async fn a03_webfinger_returns_self_link() {
    let core = TestCore::new().unwrap();
    let url = format!(
        "{}/.well-known/webfinger?resource=acct:{}@example.test",
        core.base_url, core.username
    );
    let resp = TestCore::client().get(url).send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&text(resp).await).unwrap();
    assert_eq!(v["subject"], format!("acct:{}@example.test", core.username));
    let links = v["links"].as_array().unwrap();
    assert!(links.iter().any(|l| l["rel"] == "self"));
}

#[tokio::test]
async fn a04_webfinger_unknown_resource_is_404() {
    let core = TestCore::new().unwrap();
    let url = format!(
        "{}/.well-known/webfinger?resource=acct:bob@example.test",
        core.base_url
    );
    let resp = TestCore::client().get(url).send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a05_nodeinfo_schema() {
    let core = TestCore::new().unwrap();
    let resp = core.get("/nodeinfo/2.0").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&text(resp).await).unwrap();
    assert_eq!(v["version"], "2.0");
    assert!(v["software"]["name"].is_string());
    assert!(v["protocols"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p == "activitypub"));
}

#[tokio::test]
async fn a06_followers_collection() {
    let core = TestCore::new().unwrap();
    let resp = core
        .get(&format!("/users/{}/followers", core.username))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&text(resp).await).unwrap();
    assert_eq!(v["type"], "OrderedCollection");
    assert!(v["totalItems"].is_number());
}

#[tokio::test]
async fn a07_health_endpoints() {
    let core = TestCore::new().unwrap();
    let resp = TestCore::client()
        .get(core.url("/healthz"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = TestCore::client()
        .get(core.url("/readyz"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = core.get("/_fedi3/health").await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn a08_internal_endpoint_without_token_is_rejected() {
    let core = TestCore::new().unwrap();
    let resp = TestCore::client()
        .get(core.url("/_fedi3/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a09_internal_endpoint_with_wrong_token_is_rejected() {
    let core = TestCore::new().unwrap();
    let resp = TestCore::client()
        .get(core.url("/_fedi3/health"))
        .header("X-Fedi3-Internal", "wrong-token")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a10_internal_endpoint_with_token_is_allowed() {
    let core = TestCore::new().unwrap();
    let resp = core.get("/_fedi3/health").await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn i01_inbox_without_signature_is_401() {
    let core = TestCore::new().unwrap();
    let body = json_body(&create_note(
        "https://remote.test/users/bob",
        "https://remote.test/objects/1",
        "hi",
    ));
    let resp = TestCore::client()
        .post(core.url("/inbox"))
        .header("Content-Type", "application/activity+json")
        .header(
            "Date",
            httpdate::fmt_http_date(std::time::SystemTime::now()),
        )
        .header(
            "Digest",
            format!(
                "SHA-256={}",
                base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(&body))
            ),
        )
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn i02_inbox_with_invalid_signature_is_401() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let body = json_body(&create_note(
        &mock.actor_url,
        &format!("{}/objects/1", mock.actor_url),
        "hi",
    ));
    let resp = signer
        .signed_post(
            &core.url("/inbox"),
            &body,
            SignOptions {
                tamper_signature: true,
                ..Default::default()
            },
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn i03_valid_signed_activity_is_accepted_and_stored() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let note_id = format!("{}/objects/1", mock.actor_url);
    let activity = create_note(&mock.actor_url, &note_id, "hello federation");
    let body = json_body(&activity);
    let resp = core.post_inbox_signed(&signer, &body).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    assert!(sqlite_exists(
        &core.db_path(),
        "SELECT COUNT(*) FROM objects WHERE object_id=?1 AND deleted=0",
        &[&note_id]
    ));
}

#[tokio::test]
async fn i04_signed_activity_with_foreign_actor_is_401() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    // Signed by bob's key but claims to be eve.
    let eve = "https://evil.test/users/eve";
    let body = json_body(&create_note(
        eve,
        "https://evil.test/objects/9",
        "impersonation",
    ));
    let resp = core.post_inbox_signed(&signer, &body).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert!(!sqlite_exists(
        &core.db_path(),
        "SELECT COUNT(*) FROM objects WHERE object_id=?1",
        &[&"https://evil.test/objects/9"]
    ));
}

#[tokio::test]
async fn i05_post_without_digest_in_signed_headers_is_401() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let body = json_body(&create_note(
        &mock.actor_url,
        &format!("{}/objects/2", mock.actor_url),
        "x",
    ));
    let opts = SignOptions {
        signed_headers: vec![
            "(request-target)".to_string(),
            "host".to_string(),
            "date".to_string(),
        ],
        ..Default::default()
    };
    let resp = signer
        .signed_post(&core.url("/inbox"), &body, opts)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn i06_post_with_wrong_digest_is_401() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let body = json_body(&create_note(
        &mock.actor_url,
        &format!("{}/objects/3", mock.actor_url),
        "x",
    ));
    let wrong = "SHA-256=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    let opts = SignOptions {
        extra_headers: vec![("digest".to_string(), wrong.to_string())],
        ..Default::default()
    };
    let resp = signer
        .signed_post(&core.url("/inbox"), &body, opts)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn i07_date_skew_beyond_300s_is_401() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let body = json_body(&create_note(
        &mock.actor_url,
        &format!("{}/objects/4", mock.actor_url),
        "x",
    ));
    let opts = SignOptions {
        date_offset_secs: 3600,
        ..Default::default()
    };
    let resp = signer
        .signed_post(&core.url("/inbox"), &body, opts)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn i08_duplicate_activity_is_not_stored_twice() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let activity = create_note(
        &mock.actor_url,
        &format!("{}/objects/5", mock.actor_url),
        "dup",
    );
    let body = json_body(&activity);
    let first = core.post_inbox_signed(&signer, &body).await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    let second = core.post_inbox_signed(&signer, &body).await;
    assert_eq!(second.status(), StatusCode::ACCEPTED);
    assert_eq!(text(second).await, "duplicate");
    assert_eq!(
        sqlite_count(
            &core.db_path(),
            "SELECT COUNT(*) FROM inbox_items WHERE activity_id=?1",
            &[&activity["id"].as_str().unwrap()]
        ),
        1
    );
}

#[tokio::test]
async fn i09_failed_processing_is_retried_and_dedup_happens_after_success() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let note_id = format!("{}/objects/6", mock.actor_url);
    let activity = create_note(&mock.actor_url, &note_id, "retry me");
    let body = json_body(&activity);

    // Deterministic, fast processing failure: a trigger aborts the inbox_items insert.
    {
        let conn = rusqlite::Connection::open(core.db_path()).unwrap();
        conn.execute_batch(
            r#"
            CREATE TRIGGER fail_inbox_insert
            BEFORE INSERT ON inbox_items
            WHEN NEW.activity_id LIKE '%boom%'
            BEGIN
              SELECT RAISE(ABORT, 'simulated failure');
            END;
            "#,
        )
        .unwrap();
    }

    let failing = create_note(&mock.actor_url, &note_id, "retry me");
    let mut failing = failing;
    failing["id"] = serde_json::json!(format!("{}/activities/boom-1", mock.actor_url));
    let first = core.post_inbox_signed(&signer, &json_body(&failing)).await;
    assert_eq!(
        first.status(),
        StatusCode::BAD_GATEWAY,
        "processing failure must return 5xx so the sender retries"
    );

    // Remove the fault and retry: the activity must be reprocessed, not reported as duplicate.
    {
        let conn = rusqlite::Connection::open(core.db_path()).unwrap();
        conn.execute_batch("DROP TRIGGER fail_inbox_insert;")
            .unwrap();
    }
    let retry = core.post_inbox_signed(&signer, &json_body(&failing)).await;
    assert_eq!(retry.status(), StatusCode::ACCEPTED);
    assert_ne!(text(retry).await, "duplicate");
    assert!(sqlite_exists(
        &core.db_path(),
        "SELECT COUNT(*) FROM objects WHERE object_id=?1",
        &[&note_id]
    ));
    // The original body (with the same note) can still be processed as a distinct activity.
    let other = core.post_inbox_signed(&signer, &body).await;
    assert_eq!(other.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn i13_accept_from_wrong_actor_does_not_mark_accepted() {
    let core = TestCore::new().unwrap();
    let mock_a = MockActorServer::new("alice").await.unwrap();
    let mock_b = MockActorServer::new("bob").await.unwrap();
    let signer_a = Signer::new(mock_a.key_id.clone());

    // Seed a pending follow towards bob.
    {
        let conn = rusqlite::Connection::open(core.db_path()).unwrap();
        conn.execute(
            "INSERT INTO following(actor_id,status,created_at_ms) VALUES (?1,0,?2)",
            rusqlite::params![mock_b.actor_url, 1i64],
        )
        .unwrap();
    }

    let me = core.actor_url();
    let follow = serde_json::json!({
        "id": format!("{}/activities/follow-1", me),
        "type": "Follow",
        "actor": me,
        "object": mock_b.actor_url,
    });
    let accept = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{}/activities/accept-1", mock_a.actor_url),
        "type": "Accept",
        "actor": mock_a.actor_url,
        "object": follow,
    });
    let resp = core.post_inbox_signed(&signer_a, &json_body(&accept)).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let status: i64 = sqlite_count(
        &core.db_path(),
        "SELECT status FROM following WHERE actor_id=?1",
        &[&mock_b.actor_url],
    );
    assert_eq!(status, 0, "wrong accept actor must not mark accepted");
}

#[tokio::test]
async fn i13b_correct_accept_marks_follow_accepted() {
    let core = TestCore::new().unwrap();
    let mock_b = MockActorServer::new("bob").await.unwrap();
    let signer_b = Signer::new(mock_b.key_id.clone());
    {
        let conn = rusqlite::Connection::open(core.db_path()).unwrap();
        conn.execute(
            "INSERT INTO following(actor_id,status,created_at_ms) VALUES (?1,0,?2)",
            rusqlite::params![mock_b.actor_url, 1i64],
        )
        .unwrap();
    }
    let me = core.actor_url();
    let follow = serde_json::json!({
        "id": format!("{}/activities/follow-2", me),
        "type": "Follow",
        "actor": me,
        "object": mock_b.actor_url,
    });
    let accept = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{}/activities/accept-2", mock_b.actor_url),
        "type": "Accept",
        "actor": mock_b.actor_url,
        "object": follow,
    });
    let resp = core.post_inbox_signed(&signer_b, &json_body(&accept)).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let status: i64 = sqlite_count(
        &core.db_path(),
        "SELECT status FROM following WHERE actor_id=?1",
        &[&mock_b.actor_url],
    );
    assert_eq!(status, 1);
}

#[tokio::test]
async fn i15_public_note_appears_in_federated_feed() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let note_id = format!("{}/objects/7", mock.actor_url);
    let activity = create_note(&mock.actor_url, &note_id, "public note");
    let resp = core.post_inbox_signed(&signer, &json_body(&activity)).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    assert!(sqlite_exists(
        &core.db_path(),
        "SELECT COUNT(*) FROM federated_feed WHERE activity_id=?1",
        &[&activity["id"].as_str().unwrap()]
    ));
}

#[tokio::test]
async fn i17_update_of_someone_elses_note_is_ignored() {
    let core = TestCore::new().unwrap();
    let mock_a = MockActorServer::new("alice").await.unwrap();
    let mock_b = MockActorServer::new("bob").await.unwrap();
    let signer_a = Signer::new(mock_a.key_id.clone());
    let signer_b = Signer::new(mock_b.key_id.clone());
    let note_id = format!("{}/objects/8", mock_a.actor_url);

    let create = create_note(&mock_a.actor_url, &note_id, "original");
    assert_eq!(
        core.post_inbox_signed(&signer_a, &json_body(&create))
            .await
            .status(),
        StatusCode::ACCEPTED
    );

    // Bob tries to overwrite alice's note (attributedTo stays alice).
    let update = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{}/activities/update-1", mock_b.actor_url),
        "type": "Update",
        "actor": mock_b.actor_url,
        "object": {
            "id": note_id,
            "type": "Note",
            "attributedTo": mock_a.actor_url,
            "content": "hijacked"
        }
    });
    let resp = core.post_inbox_signed(&signer_b, &json_body(&update)).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let json: String = {
        let conn = rusqlite::Connection::open(core.db_path()).unwrap();
        conn.query_row(
            "SELECT CAST(object_json AS TEXT) FROM objects WHERE object_id=?1",
            rusqlite::params![note_id],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert!(
        json.contains("original"),
        "object must not be overwritten: {json}"
    );
    assert!(!json.contains("hijacked"));
}

#[tokio::test]
async fn i18_delete_of_someone_elses_note_is_ignored() {
    let core = TestCore::new().unwrap();
    let mock_a = MockActorServer::new("alice").await.unwrap();
    let mock_b = MockActorServer::new("bob").await.unwrap();
    let signer_a = Signer::new(mock_a.key_id.clone());
    let signer_b = Signer::new(mock_b.key_id.clone());
    let note_id = format!("{}/objects/9", mock_a.actor_url);
    let create = create_note(&mock_a.actor_url, &note_id, "keep me");
    assert_eq!(
        core.post_inbox_signed(&signer_a, &json_body(&create))
            .await
            .status(),
        StatusCode::ACCEPTED
    );

    let delete = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{}/activities/delete-1", mock_b.actor_url),
        "type": "Delete",
        "actor": mock_b.actor_url,
        "object": note_id,
    });
    let resp = core.post_inbox_signed(&signer_b, &json_body(&delete)).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let deleted: i64 = sqlite_count(
        &core.db_path(),
        "SELECT deleted FROM objects WHERE object_id=?1",
        &[&note_id],
    );
    assert_eq!(deleted, 0);
}

#[tokio::test]
async fn i19_owner_can_delete_own_note() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let note_id = format!("{}/objects/10", mock.actor_url);
    let create = create_note(&mock.actor_url, &note_id, "delete me");
    assert_eq!(
        core.post_inbox_signed(&signer, &json_body(&create))
            .await
            .status(),
        StatusCode::ACCEPTED
    );
    let delete = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{}/activities/delete-2", mock.actor_url),
        "type": "Delete",
        "actor": mock.actor_url,
        "object": note_id,
    });
    assert_eq!(
        core.post_inbox_signed(&signer, &json_body(&delete))
            .await
            .status(),
        StatusCode::ACCEPTED
    );
    let deleted: i64 = sqlite_count(
        &core.db_path(),
        "SELECT deleted FROM objects WHERE object_id=?1",
        &[&note_id],
    );
    assert_eq!(deleted, 1);
}

#[tokio::test]
async fn i23_blocked_actor_is_forbidden() {
    let mock = MockActorServer::new("bob").await.unwrap();
    let blocked = mock.actor_url.clone();
    let core = TestCore::with_config(|c| c.blocked_actors = Some(vec![blocked.clone()])).unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let body = json_body(&create_note(
        &mock.actor_url,
        &format!("{}/objects/11", mock.actor_url),
        "blocked",
    ));
    let resp = core.post_inbox_signed(&signer, &body).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn i27_invalid_json_is_400() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let resp = core.post_inbox_signed(&signer, b"not json").await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn o01_outbox_post_without_token_or_signature_is_401() {
    let core = TestCore::new().unwrap();
    let body = json_body(&serde_json::json!({
        "type": "Create",
        "actor": core.actor_url(),
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
        "object": {"type": "Note", "content": "hi"}
    }));
    let resp = TestCore::client()
        .post(core.url(&format!("/users/{}/outbox", core.username)))
        .header("Content-Type", "application/activity+json")
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn o02_outbox_post_with_internal_token_is_accepted() {
    let core = TestCore::new().unwrap();
    let body = json_body(&serde_json::json!({
        "type": "Create",
        "actor": core.actor_url(),
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
        "cc": [format!("{}/followers", core.actor_url())],
        "object": {"type": "Note", "content": "hello"}
    }));
    let resp = core
        .post_internal(&format!("/users/{}/outbox", core.username), &body)
        .await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn o03_outbox_post_with_foreign_actor_is_forbidden() {
    let core = TestCore::new().unwrap();
    let body = json_body(&serde_json::json!({
        "type": "Create",
        "actor": "https://evil.test/users/eve",
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
        "object": {"type": "Note", "content": "hi"}
    }));
    let resp = core
        .post_internal(&format!("/users/{}/outbox", core.username), &body)
        .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn o04_outbox_post_invalid_json_is_400() {
    let core = TestCore::new().unwrap();
    let resp = core
        .post_internal(&format!("/users/{}/outbox", core.username), b"{")
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn o05_outbox_get_returns_collection() {
    let core = TestCore::new().unwrap();
    let body = json_body(&serde_json::json!({
        "type": "Create",
        "actor": core.actor_url(),
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
        "cc": [format!("{}/followers", core.actor_url())],
        "object": {"type": "Note", "content": "paged"}
    }));
    assert_eq!(
        core.post_internal(&format!("/users/{}/outbox", core.username), &body)
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
    let v: serde_json::Value = serde_json::from_str(&text(resp).await).unwrap();
    assert_eq!(v["type"], "OrderedCollectionPage");
    assert!(!v["orderedItems"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn m01_media_upload_with_token_creates_file() {
    let core = TestCore::new().unwrap();
    let png = base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==")
        .unwrap();
    let resp = TestCore::client()
        .post(core.url(&format!("/users/{}/media", core.username)))
        .header("X-Fedi3-Internal", INTERNAL_TOKEN)
        .header("X-Filename", "pixel.png")
        .header("Content-Type", "image/png")
        .body(png)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let v: serde_json::Value = serde_json::from_str(&text(resp).await).unwrap();
    let id = v["id"].as_str().unwrap();
    assert!(id.ends_with(".png"));
    assert!(core.data_dir.path().join("media").join(id).exists());
}

#[tokio::test]
async fn m02_media_upload_without_token_is_rejected() {
    let core = TestCore::new().unwrap();
    let resp = TestCore::client()
        .post(core.url(&format!("/users/{}/media", core.username)))
        .header("X-Filename", "pixel.png")
        .body(vec![1u8, 2, 3])
        .send()
        .await
        .unwrap();
    assert!(matches!(
        resp.status(),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ));
}

#[tokio::test]
async fn m03_media_upload_over_limit_is_413() {
    let core = TestCore::new().unwrap();
    let big = vec![0u8; 2 * 1024 * 1024];
    let resp = TestCore::client()
        .post(core.url(&format!("/users/{}/media", core.username)))
        .header("X-Fedi3-Internal", INTERNAL_TOKEN)
        .header("X-Filename", "big.png")
        .header("Content-Type", "image/png")
        .body(big)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn m07_media_get_with_invalid_id_is_400() {
    let core = TestCore::new().unwrap();
    let resp = TestCore::client()
        .get(core.url(&format!("/users/{}/media/a..b", core.username)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn receipt_with_foreign_actor_is_401() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let body = json_body(&serde_json::json!({
        "activity_id": "https://example.test/activities/1",
        "actor": "https://evil.test/users/eve",
    }));
    let resp = signer
        .signed_post(&core.url("/.fedi3/receipt"), &body, SignOptions::default())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn receipt_with_valid_actor_is_ok() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let body = json_body(&serde_json::json!({
        "activity_id": "https://example.test/activities/2",
        "actor": mock.actor_url,
    }));
    let resp = signer
        .signed_post(&core.url("/.fedi3/receipt"), &body, SignOptions::default())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn receipt_without_digest_is_401() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let signer = Signer::new(mock.key_id.clone());
    let body = json_body(&serde_json::json!({"activity_id": "x"}));
    let opts = SignOptions {
        signed_headers: vec!["(request-target)".into(), "host".into(), "date".into()],
        ..Default::default()
    };
    let resp = signer
        .signed_post(&core.url("/.fedi3/receipt"), &body, opts)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn global_ingest_rejects_activity_from_foreign_origin() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let note_id = format!("{}/objects/12", mock.actor_url);
    let activity = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": "https://evil.test/activities/1",
        "type": "Create",
        "actor": mock.actor_url,
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
        "object": {
            "id": note_id,
            "type": "Note",
            "attributedTo": mock.actor_url,
            "content": "foreign id"
        }
    });
    let resp = core
        .post_internal("/_fedi3/global/ingest", &json_body(&activity))
        .await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    assert_eq!(text(resp).await, "ignored (activity id origin)");
}

#[tokio::test]
async fn global_ingest_accepts_activity_with_matching_origin() {
    let core = TestCore::new().unwrap();
    let mock = MockActorServer::new("bob").await.unwrap();
    let note_id = format!("{}/objects/13", mock.actor_url);
    let activity = create_note(&mock.actor_url, &note_id, "gossip");
    let resp = core
        .post_internal("/_fedi3/global/ingest", &json_body(&activity))
        .await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    assert_eq!(text(resp).await, "ok");
    assert!(sqlite_exists(
        &core.db_path(),
        "SELECT COUNT(*) FROM global_feed WHERE activity_id=?1",
        &[&activity["id"].as_str().unwrap()]
    ));
}

#[tokio::test]
async fn empty_internal_token_is_generated_and_does_not_allow_anonymous_access() {
    let core = TestCore::with_config(|cfg| {
        cfg.internal_token = Some("   ".to_string());
    })
    .unwrap();
    let response = TestCore::client()
        .get(core.url("/_fedi3/chat/threads"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn device_sync_requires_digest_and_all_signed_request_headers() {
    let core = TestCore::new().unwrap();
    let private =
        std::fs::read_to_string(core.data_dir.path().join("identity_private_key.pem")).unwrap();
    let uri: http::Uri = core.url("/.fedi3/device/outbox").parse().unwrap();
    for signed in [
        vec!["date"],
        vec!["(request-target)", "host", "date"],
        vec!["(request-target)", "host", "date", "digest"],
    ] {
        let mut headers = http::HeaderMap::new();
        headers.insert("Host", format!("127.0.0.1:{}", core.port).parse().unwrap());
        fedi3_core::http_sig::sign_request_rsa_sha256(
            &private,
            &format!("{}#main-key", core.actor_url()),
            &http::Method::GET,
            &uri,
            &mut headers,
            &[],
            &signed,
        )
        .unwrap();
        let response = TestCore::client()
            .get(uri.to_string())
            .headers(headers)
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if signed.len() == 4 {
                StatusCode::OK
            } else {
                StatusCode::UNAUTHORIZED
            }
        );
    }
}
