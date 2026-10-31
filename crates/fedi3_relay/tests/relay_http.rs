mod common;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use common::{spawn_expect_failure, spawn_mock_actor, temp_dir, Signer, TestRelay};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

fn user_token() -> String {
    "user-token-0123456789abcdef".to_string()
}

fn relay_host(relay: &TestRelay) -> String {
    format!("127.0.0.1:{}", relay.port)
}

fn local_actor_url(relay: &TestRelay, user: &str) -> String {
    format!("http://{}/users/{user}", relay_host(relay))
}

fn activity_to(_relay: &TestRelay, id: &str, actor: &str, recipients: &[String]) -> Value {
    json!({
        "id": id,
        "type": "Create",
        "actor": actor,
        "to": recipients,
        "object": { "id": format!("{id}/note"), "type": "Note", "content": "hello" }
    })
}

async fn signed_post(
    relay: &TestRelay,
    signer: &Signer,
    path: &str,
    body: &Value,
    include_digest: bool,
) -> reqwest::Response {
    let bytes = serde_json::to_vec(body).unwrap();
    let headers = signer.headers("POST", path, &relay_host(relay), &bytes, include_digest);
    let mut req = relay
        .client
        .post(format!("{}{path}", relay.base_url()))
        .header("Content-Type", "application/activity+json")
        .body(bytes);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    req.send().await.unwrap()
}

fn corrupt_signature(headers: &mut [(String, String)]) {
    for (k, v) in headers.iter_mut() {
        if k.eq_ignore_ascii_case("Signature") {
            if let Some(idx) = v.find("signature=\"") {
                let pos = idx + "signature=\"".len();
                let replacement = if v.as_bytes()[pos] == b'A' { "B" } else { "A" };
                v.replace_range(pos..pos + 1, replacement);
            }
        }
    }
}

fn audit_count(relay: &TestRelay) -> i64 {
    relay
        .db()
        .query_row("SELECT COUNT(*) FROM admin_audit", [], |r| r.get(0))
        .unwrap()
}

fn spool_count(relay: &TestRelay, user: &str) -> i64 {
    relay
        .db()
        .query_row(
            "SELECT COUNT(*) FROM inbox_spool WHERE username=?1",
            [user],
            |r| r.get(0),
        )
        .unwrap()
}

fn insert_users(relay: &TestRelay, users: &[String]) {
    let conn = relay.db();
    for user in users {
        conn.execute(
            "INSERT OR IGNORE INTO users(username, token_sha256, created_at_ms) VALUES (?1, ?2, ?3)",
            rusqlite::params![user, "x".repeat(64), 0i64],
        )
        .unwrap();
    }
}

#[tokio::test]
async fn relay_starts_without_redis_or_meili_and_health_has_no_audit() {
    let relay = TestRelay::start().await;
    let health = relay.get("/healthz").await;
    assert_eq!(health.status(), 200);
    let ready = relay.get("/readyz").await;
    assert_eq!(ready.status(), 200);
    assert_eq!(audit_count(&relay), 0);
}

#[tokio::test]
async fn readyz_reports_media_failure() {
    let dir = temp_dir("media-file");
    let file = dir.join("media-is-a-file");
    std::fs::write(&file, b"x").unwrap();
    let relay =
        TestRelay::start_with_env(&[("FEDI3_RELAY_MEDIA_DIR", file.to_str().unwrap())]).await;
    let ready = relay.get("/readyz").await;
    assert_eq!(ready.status(), 503);
    assert_eq!(audit_count(&relay), 0);
}

#[tokio::test]
async fn admin_token_authentication() {
    let relay = TestRelay::start().await;
    let ok = relay
        .get_with("/admin/users", &[("Authorization", &relay.admin_bearer())])
        .await;
    assert_eq!(ok.status(), 200);
    let wrong = relay
        .get_with("/admin/users", &[("Authorization", "Bearer wrong-token")])
        .await;
    assert_eq!(wrong.status(), 401);
    let token = relay.admin_token.clone();
    let almost = format!("{}x", &token[..token.len() - 1]);
    let almost = format!("Bearer {almost}");
    let resp = relay
        .get_with("/admin/users", &[("Authorization", &almost)])
        .await;
    assert_eq!(resp.status(), 401);
    let missing = relay.get("/admin/users").await;
    assert_eq!(missing.status(), 401);
}

#[tokio::test]
async fn invalid_numeric_config_fails_startup() {
    let (code, stderr) = spawn_expect_failure(&[("FEDI3_RELAY_MAX_BODY_BYTES", "64M")]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("MAX_BODY_BYTES"), "stderr: {stderr}");
}

#[tokio::test]
async fn invalid_db_driver_fails_startup() {
    let (code, stderr) = spawn_expect_failure(&[("FEDI3_RELAY_DB_DRIVER", "pg16")]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("DB_DRIVER"), "stderr: {stderr}");
}

#[tokio::test]
async fn invalid_search_backend_fails_startup() {
    let (code, stderr) = spawn_expect_failure(&[("FEDI3_RELAY_SEARCH_BACKEND", "meilisearch")]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("SEARCH_BACKEND"), "stderr: {stderr}");
}

#[tokio::test]
async fn short_admin_token_fails_startup() {
    let (code, stderr) = spawn_expect_failure(&[("FEDI3_RELAY_ADMIN_TOKEN", "short")]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("ADMIN_TOKEN"), "stderr: {stderr}");
}

#[tokio::test]
async fn inbox_rejects_missing_signature() {
    let relay = TestRelay::start().await;
    relay.register("alice", &user_token()).await;
    let body = activity_to(
        &relay,
        "https://remote.example/activities/1",
        "https://remote.example/users/bob",
        &[local_actor_url(&relay, "alice")],
    );
    let resp = relay.post_json("/inbox", &body).await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn inbox_rejects_corrupted_signature() {
    let relay = TestRelay::start().await;
    relay.register("alice", &user_token()).await;
    let mock = spawn_mock_actor(String::new()).await;
    let signer = Signer::new(&mock.base_url, "bob");
    mock.set_actor_json(signer.actor_json("bob"));
    let body = activity_to(
        &relay,
        "https://remote.example/activities/2",
        &signer.actor_url,
        &[local_actor_url(&relay, "alice")],
    );
    let bytes = serde_json::to_vec(&body).unwrap();
    let mut headers = signer.headers("POST", "/inbox", &relay_host(&relay), &bytes, true);
    corrupt_signature(&mut headers);
    let mut req = relay
        .client
        .post(format!("{}/inbox", relay.base_url()))
        .header("Content-Type", "application/activity+json")
        .body(bytes);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    assert_eq!(req.send().await.unwrap().status(), 401);
}

#[tokio::test]
async fn inbox_requires_digest_header() {
    let relay = TestRelay::start().await;
    relay.register("alice", &user_token()).await;
    let mock = spawn_mock_actor(String::new()).await;
    let signer = Signer::new(&mock.base_url, "bob");
    mock.set_actor_json(signer.actor_json("bob"));
    let body = activity_to(
        &relay,
        "https://remote.example/activities/3",
        &signer.actor_url,
        &[local_actor_url(&relay, "alice")],
    );
    let resp = signed_post(&relay, &signer, "/inbox", &body, false).await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn inbox_accepts_valid_signature_and_spools_offline_user() {
    let relay = TestRelay::start().await;
    relay.register("alice", &user_token()).await;
    let mock = spawn_mock_actor(String::new()).await;
    let signer = Signer::new(&mock.base_url, "bob");
    mock.set_actor_json(signer.actor_json("bob"));
    let body = activity_to(
        &relay,
        "https://remote.example/activities/10",
        &signer.actor_url,
        &[local_actor_url(&relay, "alice")],
    );
    let resp = signed_post(&relay, &signer, "/inbox", &body, true).await;
    let status = resp.status();
    let text = resp.text().await.unwrap();
    assert_eq!(status, 202, "relay said: {text}");
    assert_eq!(spool_count(&relay, "alice"), 1);

    // Replaying the same activity id must not enqueue it twice (RI-06).
    let resp = signed_post(&relay, &signer, "/inbox", &body, true).await;
    assert_eq!(resp.status(), 202);
    assert_eq!(spool_count(&relay, "alice"), 1);
}

#[tokio::test]
async fn inbox_rejects_non_local_recipients() {
    let relay = TestRelay::start().await;
    let mock = spawn_mock_actor(String::new()).await;
    let signer = Signer::new(&mock.base_url, "bob");
    mock.set_actor_json(signer.actor_json("bob"));
    let body = activity_to(
        &relay,
        "https://remote.example/activities/11",
        &signer.actor_url,
        &["https://remote.example/users/carol".to_string()],
    );
    let resp = signed_post(&relay, &signer, "/inbox", &body, true).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn inbox_rejects_fanout_over_limit() {
    let relay = TestRelay::start().await;
    let users: Vec<String> = (0..30).map(|i| format!("user{i}")).collect();
    insert_users(&relay, &users);
    let recipients: Vec<String> = users.iter().map(|u| local_actor_url(&relay, u)).collect();
    let mock = spawn_mock_actor(String::new()).await;
    let signer = Signer::new(&mock.base_url, "bob");
    mock.set_actor_json(signer.actor_json("bob"));
    let body = activity_to(
        &relay,
        "https://remote.example/activities/12",
        &signer.actor_url,
        &recipients,
    );
    let resp = signed_post(&relay, &signer, "/inbox", &body, true).await;
    assert_eq!(resp.status(), 413);
}

#[tokio::test]
async fn media_upload_requires_token_and_rejects_html() {
    let relay = TestRelay::start().await;
    relay.register("alice", &user_token()).await;
    let no_token = relay
        .post_bytes_with(
            "/users/alice/media",
            b"data",
            &[("Content-Type", "image/png")],
        )
        .await;
    assert_eq!(no_token.status(), 401);

    let bearer = format!("Bearer {}", user_token());
    let html = relay
        .post_bytes_with(
            "/users/alice/media",
            b"<p>hi</p>",
            &[
                ("Content-Type", "text/html"),
                ("X-Filename", "a.html"),
                ("Authorization", &bearer),
            ],
        )
        .await;
    assert_eq!(html.status(), 415);
}

#[tokio::test]
async fn media_upload_get_delete_roundtrip() {
    let relay = TestRelay::start().await;
    relay.register("alice", &user_token()).await;
    let bearer = format!("Bearer {}", user_token());
    let upload = relay
        .post_bytes_with(
            "/users/alice/media",
            b"\x89PNG\r\n\x1a\nfake",
            &[
                ("Content-Type", "image/png"),
                ("X-Filename", "pic.png"),
                ("Authorization", &bearer),
            ],
        )
        .await;
    assert_eq!(upload.status(), 201);
    let value: Value = upload.json().await.unwrap();
    let id = value["id"].as_str().unwrap().to_string();
    assert_eq!(
        relay
            .db()
            .query_row("SELECT COUNT(*) FROM media_items", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );

    let get = relay.get(&format!("/users/alice/media/{id}")).await;
    assert_eq!(get.status(), 200);
    assert_eq!(
        get.bytes().await.unwrap().as_ref(),
        b"\x89PNG\r\n\x1a\nfake"
    );

    let del = relay
        .delete_with(
            &format!("/users/alice/media/{id}"),
            &[("Authorization", &bearer)],
        )
        .await;
    assert_eq!(del.status(), 200);
    assert_eq!(
        relay
            .get(&format!("/users/alice/media/{id}"))
            .await
            .status(),
        404
    );
}

#[tokio::test]
async fn media_get_rejects_bad_id() {
    let relay = TestRelay::start().await;
    let resp = relay.get("/users/alice/media/..%2Fetc").await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn webfinger_returns_local_actor_and_validates_host() {
    let relay = TestRelay::start().await;
    relay.register("alice", &user_token()).await;
    let resp = relay
        .get("/.well-known/webfinger?resource=acct:alice@127.0.0.1")
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["subject"],
        format!("acct:alice@{}", relay_host(&relay))
    );
    assert!(body["links"][0]["href"]
        .as_str()
        .unwrap()
        .ends_with("/users/alice"));
}

#[tokio::test]
async fn webfinger_rejects_invalid_host_with_base_domain() {
    let relay = TestRelay::start_with_env(&[
        ("FEDI3_RELAY_BASE_DOMAIN", "relay.example"),
        ("FEDI3_RELAY_PUBLIC_URL", "http://relay.example"),
    ])
    .await;
    relay.register("alice", &user_token()).await;

    let bad = relay
        .get_with(
            "/.well-known/webfinger?resource=acct:alice@evil.example",
            &[("Host", "evil.example")],
        )
        .await;
    assert_eq!(bad.status(), 421);

    let ok = relay
        .get_with(
            "/.well-known/webfinger?resource=acct:alice@alice.relay.example",
            &[("Host", "alice.relay.example")],
        )
        .await;
    assert_eq!(ok.status(), 200);
    let body: Value = ok.json().await.unwrap();
    assert_eq!(body["subject"], "acct:alice@alice.relay.example");
    assert_eq!(
        body["links"][0]["href"],
        "http://alice.relay.example/users/alice"
    );
}

#[tokio::test]
async fn webfinger_redirects_to_canonical_origin_with_port() {
    let relay = TestRelay::start().await;
    let resp = relay
        .get_with(
            "/.well-known/webfinger?resource=acct:alice@127.0.0.1",
            &[("Host", "127.0.0.1")],
        )
        .await;
    assert_eq!(resp.status(), 308);
    let location = resp.headers().get("Location").unwrap().to_str().unwrap();
    assert_eq!(
        location,
        format!(
            "http://127.0.0.1:{}/.well-known/webfinger?resource=acct:alice@127.0.0.1",
            relay.port
        )
    );

    let same = relay
        .get_with(
            "/.well-known/webfinger?resource=acct:alice@127.0.0.1",
            &[("Host", &relay_host(&relay))],
        )
        .await;
    // alice exists? Not registered here: redirect is evaluated before lookup,
    // so with the canonical Host we get 404 instead of another redirect.
    assert_eq!(same.status(), 404);
}

#[tokio::test]
async fn tunnel_accepts_valid_token_and_bearer_header() {
    let relay = TestRelay::start().await;
    relay.register("alice", &user_token()).await;
    let url = relay.ws_url(&format!("/tunnel/alice?token={}", user_token()));
    let (ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    drop(ws);

    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut req = relay.ws_url("/tunnel/alice").into_client_request().unwrap();
    req.headers_mut().insert(
        "Authorization",
        tokio_tungstenite::tungstenite::http::HeaderValue::from_str(&format!(
            "Bearer {}",
            user_token()
        ))
        .unwrap(),
    );
    let (ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
    drop(ws);
}

#[tokio::test]
async fn tunnel_rejects_invalid_username() {
    let relay = TestRelay::start().await;
    relay.register("alice", &user_token()).await;
    let url = relay.ws_url(&format!("/tunnel/Alice?token={}", user_token()));
    if let Ok((mut ws, _)) = tokio_tungstenite::connect_async(url).await {
        // The server drops the socket without upgrading.
        let next = ws.next().await;
        assert!(next.is_none() || next.unwrap().is_err());
    }
    // The invalid username must not have been created.
    let exists: i64 = relay
        .db()
        .query_row(
            "SELECT COUNT(*) FROM users WHERE username='Alice'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(exists, 0);
}

#[tokio::test]
async fn forward_to_offline_user_spools_and_returns_503() {
    let relay = TestRelay::start().await;
    relay.register("alice", &user_token()).await;
    let resp = relay
        .post_json("/users/alice/outbox", &json!({ "type": "Create" }))
        .await;
    assert_eq!(resp.status(), 503);
    assert_eq!(spool_count(&relay, "alice"), 1);
}

#[tokio::test]
async fn slow_tunnel_times_out_and_spools() {
    let relay = TestRelay::start_with_env(&[("FEDI3_RELAY_TUNNEL_TIMEOUT_SECS", "1")]).await;
    relay.register("alice", &user_token()).await;
    let url = relay.ws_url(&format!("/tunnel/alice?token={}", user_token()));
    let (mut ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    // Keep the socket alive but never answer requests.
    let hold = tokio::spawn(async move { while let Some(Ok(_)) = ws.next().await {} });
    let resp = relay
        .post_json("/users/alice/outbox", &json!({ "type": "Create" }))
        .await;
    assert_eq!(resp.status(), 504);
    assert_eq!(spool_count(&relay, "alice"), 1);
    hold.abort();
}

#[tokio::test]
async fn reconnect_does_not_lose_new_tunnel() {
    let relay = TestRelay::start().await;
    relay.register("alice", &user_token()).await;
    let url = relay.ws_url(&format!("/tunnel/alice?token={}", user_token()));
    let (ws1, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let (mut ws2, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let responder = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws2.next().await {
            if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                let v: Value = serde_json::from_str(&text).unwrap();
                let id = v["id"].as_str().unwrap_or_default().to_string();
                let actor = json!({
                    "@context": "https://www.w3.org/ns/activitystreams",
                    "id": "http://127.0.0.1/users/alice",
                    "type": "Person",
                    "preferredUsername": "alice"
                });
                let resp = json!({
                    "id": id,
                    "status": 200,
                    "headers": [["content-type", "application/activity+json"]],
                    "body_b64": B64.encode(actor.to_string())
                });
                ws2.send(tokio_tungstenite::tungstenite::Message::Text(
                    resp.to_string(),
                ))
                .await
                .unwrap();
            }
        }
    });

    // Disconnect the *old* socket after the new one is registered: its cleanup
    // must not remove the new tunnel entry (H-30 / RT-04).
    drop(ws1);
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Wait until the new tunnel is visible to forwards.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut status = 0;
    while Instant::now() < deadline {
        let resp = relay
            .get_with("/users/alice", &[("Accept", "application/activity+json")])
            .await;
        status = resp.status().as_u16();
        if status == 200 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(status, 200, "new tunnel was removed by old disconnect");
    responder.abort();
}

#[tokio::test]
async fn search_notes_uses_db_backend_and_escapes_like() {
    let relay = TestRelay::start().await;
    relay.register("alice", &user_token()).await;
    {
        let conn = relay.db();
        conn.execute(
            "INSERT INTO relay_notes(note_id, content_text, content_html, note_json, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                "n1",
                "100% off",
                "<b>marker</b>",
                r#"{"id":"n1","type":"Note"}"#,
                chrono::Utc::now().timestamp_millis()
            ],
        )
        .unwrap();
    }
    let bearer = format!("Bearer {}", user_token());
    let resp = relay
        .get_with(
            "/_fedi3/relay/search/notes?username=alice&q=100%25",
            &[("Authorization", &bearer)],
        )
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["id"], "n1");

    let resp = relay
        .get_with(
            "/_fedi3/relay/search/notes?username=alice&q=marker",
            &[("Authorization", &bearer)],
        )
        .await;
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn webrtc_signal_requires_signature() {
    let relay = TestRelay::start().await;
    let body = json!({
        "to_peer_id": "peer-1",
        "session_id": "session-1",
        "kind": "offer",
        "payload": { "sdp": "x" }
    });
    let resp = relay.post_json("/_fedi3/webrtc/send", &body).await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn signed_move_notice_fanout_is_accepted_by_peer_relay() {
    let relay_a = TestRelay::start().await;
    let relay_b = TestRelay::start().await;
    let admin = relay_a.admin_bearer();

    // Exchange signed telemetry so each relay pins the other's Ed25519 key.
    let a_stats: Value = relay_a
        .get_with("/_fedi3/relay/stats", &[("Authorization", &admin)])
        .await
        .json()
        .await
        .unwrap();
    let admin_b = relay_b.admin_bearer();
    let b_stats: Value = relay_b
        .get_with("/_fedi3/relay/stats", &[("Authorization", &admin_b)])
        .await
        .json()
        .await
        .unwrap();
    let resp = relay_b.post_json("/_fedi3/relay/telemetry", &a_stats).await;
    assert_eq!(resp.status(), 200);
    let resp = relay_a.post_json("/_fedi3/relay/telemetry", &b_stats).await;
    assert_eq!(resp.status(), 200);

    let notice = json!({
        "username": "alice",
        "moved_to_actor": "https://new.example/users/alice",
        "ts_ms": chrono::Utc::now().timestamp_millis(),
        "nonce": "n-1"
    });
    let resp = relay_a
        .post_json_with(
            "/_fedi3/relay/move_notice",
            &notice,
            &[("Authorization", &relay_a.admin_bearer())],
        )
        .await;
    assert_eq!(resp.status(), 200);

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut moved = None;
    while Instant::now() < deadline {
        moved = relay_b
            .db()
            .query_row(
                "SELECT moved_to_actor FROM user_moves WHERE username='alice'",
                [],
                |r| r.get::<_, String>(0),
            )
            .ok();
        if moved.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(
        moved.as_deref(),
        Some("https://new.example/users/alice"),
        "signed fan-out was not accepted by the peer relay"
    );
}
