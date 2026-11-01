/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

//! End-to-end test for the two decentralization guarantees added in this iteration:
//!
//! 1. **identity anchor**: the actor `id` lives on a static anchor domain while inbox/outbox
//!    stay on the relay. Remote servers that fetch the relay URL are redirected to the anchor,
//!    WebFinger resolves to the anchor actor, and delivery still reaches the core.
//! 2. **blind spool**: while the core is offline the relay stores the body encrypted with the
//!    user's public key; the plaintext never appears in the relay database, and the item is
//!    flushed to the core (which alone holds the private key) once the tunnel reconnects.

mod common;

use axum::{routing::get, Router};
use base64::Engine as _;
use common::TestRelay;
use fedi3_core::runtime::{self, CoreStartConfig};
use serde_json::Value;
use std::path::Path;
use std::time::{Duration, Instant};

const INTERNAL: &str = "anchor-internal-token-0123456789abcdef";
const ALICE_TOKEN: &str = "alice-anchor-token-0123456789";
const BOB_TOKEN: &str = "bob-anchor-token-0123456789ab";
const NOTE_MARKER: &str = "anchor-federation-marker";
const SPOOL_MARKER: &str = "anchor-spool-marker";

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local addr").port()
}

fn relay_base_from_ws(ws: &str) -> String {
    ws.replacen("ws://", "http://", 1)
        .replacen("wss://", "https://", 1)
        .trim_end_matches('/')
        .to_string()
}

fn core_cfg(
    username: &str,
    relay_ws: &str,
    token: &str,
    dir: &Path,
    port: u16,
    anchor: Option<&str>,
) -> CoreStartConfig {
    CoreStartConfig {
        username: username.to_string(),
        domain: "127.0.0.1".to_string(),
        public_base_url: String::new(),
        anchor_actor_url: anchor.map(|s| s.to_string()),
        relay_ws: relay_ws.to_string(),
        relay_token: Some(token.to_string()),
        bind: format!("127.0.0.1:{port}"),
        data_dir: Some(dir.to_string_lossy().to_string()),
        internal_token: Some(INTERNAL.to_string()),
        max_date_skew_secs: Some(300),
        p2p: None,
        ..Default::default()
    }
}

async fn wait_until<F, Fut>(what: &str, timeout: Duration, mut check: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if check().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    eprintln!("timeout waiting for {what}");
    false
}

#[tokio::test(flavor = "multi_thread")]
async fn anchor_identity_and_blind_spool() {
    std::env::set_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH", "true");

    let relay = TestRelay::start_with_env(&[("RUST_LOG", "info")]).await;
    let relay_ws = relay.ws_url("");
    let relay_base = relay_base_from_ws(&relay_ws);

    let anchor_port = free_port();
    let anchor_base = format!("http://127.0.0.1:{anchor_port}");
    let actor_url = format!("{anchor_base}/users/alice");

    let alice_dir = fedi3_core::test_support::TempDir::new("anchor-alice");
    let alice_port = free_port();
    let alice_cfg = core_cfg(
        "alice",
        &relay_ws,
        ALICE_TOKEN,
        alice_dir.path(),
        alice_port,
        Some(&anchor_base),
    );

    let mut alice = runtime::start(alice_cfg.clone()).expect("start alice core");

    // The core publishes the anchor documents at startup; wait for them before serving.
    let anchor_ready = wait_until("anchor documents", Duration::from_secs(20), || async {
        alice_dir.path().join("anchor/actor.json").exists()
            && alice_dir.path().join("anchor/webfinger").exists()
    })
    .await;
    assert!(anchor_ready, "core did not publish anchor documents");
    let actor_json = std::fs::read_to_string(alice_dir.path().join("anchor/actor.json"))
        .expect("anchor actor document");
    let wf_json = std::fs::read_to_string(alice_dir.path().join("anchor/webfinger"))
        .expect("anchor webfinger document");

    // Serve the static anchor documents exactly like a cheap static host would.
    let app = Router::new()
        .route(
            "/users/alice",
            get(move || {
                let body = actor_json.clone();
                async move { ([("Content-Type", "application/activity+json")], body) }
            }),
        )
        .route(
            "/.well-known/webfinger",
            get(move || {
                let body = wf_json.clone();
                async move { ([("Content-Type", "application/jrd+json")], body) }
            }),
        );
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", anchor_port))
        .await
        .expect("bind anchor");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    // 1. The relay must redirect remote servers to the canonical anchor actor.
    let no_redirect = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");
    let redirected = wait_until("relay redirect to anchor", Duration::from_secs(60), || {
        let client = no_redirect.clone();
        let url = format!("{relay_base}/users/alice");
        let expected = actor_url.clone();
        async move {
            match client.get(&url).send().await {
                Ok(resp) => {
                    resp.status().is_redirection()
                        && resp
                            .headers()
                            .get("location")
                            .and_then(|v| v.to_str().ok())
                            .map(|v| v == expected)
                            .unwrap_or(false)
                }
                Err(_) => false,
            }
        }
    })
    .await;
    assert!(
        redirected,
        "relay did not redirect /users/alice to the anchor"
    );

    // 2. The anchor document keeps the identity on the anchor and the transport on the relay.
    let actor: Value = no_redirect
        .get(&actor_url)
        .send()
        .await
        .expect("anchor fetch")
        .json()
        .await
        .expect("anchor json");
    assert_eq!(actor["id"], actor_url);
    assert_eq!(
        actor["inbox"],
        format!("{relay_base}/users/alice/inbox"),
        "inbox must stay on the relay transport"
    );
    assert_eq!(actor["publicKey"]["id"], format!("{actor_url}#main-key"));
    assert!(
        actor["alsoKnownAs"]
            .as_array()
            .map(|a| a
                .iter()
                .any(|v| v == &Value::String(format!("{relay_base}/users/alice"))))
            .unwrap_or(false),
        "relay alias should be advertised in alsoKnownAs"
    );

    // 3. WebFinger on the relay resolves to the anchor actor.
    let wf: Value = no_redirect
        .get(format!(
            "{relay_base}/.well-known/webfinger?resource=acct:alice@127.0.0.1"
        ))
        .send()
        .await
        .expect("webfinger")
        .json()
        .await
        .expect("webfinger json");
    assert_eq!(wf["links"][0]["href"], actor_url);

    // 4. Federation still works in anchor mode.
    let bob_dir = fedi3_core::test_support::TempDir::new("anchor-bob");
    let bob_port = free_port();
    let bob = runtime::start(core_cfg(
        "bob",
        &relay_ws,
        BOB_TOKEN,
        bob_dir.path(),
        bob_port,
        None,
    ))
    .expect("start bob core");
    let bob_base = format!("http://127.0.0.1:{bob_port}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .expect("client");

    let follow = client
        .post(format!("{bob_base}/_fedi3/social/follow"))
        .header("X-Fedi3-Internal", INTERNAL)
        .json(&serde_json::json!({"actor": actor_url}))
        .send()
        .await
        .expect("follow request");
    assert!(
        follow.status().is_success(),
        "follow failed: {}",
        follow.status()
    );
    let accepted = wait_until("bob follows alice", Duration::from_secs(60), || {
        let client = client.clone();
        let url = format!("{bob_base}/_fedi3/social/status?actor={actor_url}");
        async move {
            match client
                .get(&url)
                .header("X-Fedi3-Internal", INTERNAL)
                .send()
                .await
            {
                Ok(resp) => resp
                    .json::<Value>()
                    .await
                    .map(|v| v["status"] == "accepted")
                    .unwrap_or(false),
                Err(_) => false,
            }
        }
    })
    .await;

    // Diagnostics for the anchor-mode follow path.
    let bob_health = match client
        .get(format!("{bob_base}/_fedi3/health"))
        .header("X-Fedi3-Internal", INTERNAL)
        .send()
        .await
    {
        Ok(r) => r.text().await.unwrap_or_default(),
        Err(_) => String::new(),
    };
    eprintln!("bob health: {bob_health}");
    let bob_status = match client
        .get(format!("{bob_base}/_fedi3/social/status?actor={actor_url}"))
        .header("X-Fedi3-Internal", INTERNAL)
        .send()
        .await
    {
        Ok(r) => r.text().await.unwrap_or_default(),
        Err(_) => String::new(),
    };
    eprintln!("bob status: {bob_status}");
    let alice_health = match client
        .get(format!("http://127.0.0.1:{alice_port}/_fedi3/health"))
        .header("X-Fedi3-Internal", INTERNAL)
        .send()
        .await
    {
        Ok(r) => r.text().await.unwrap_or_default(),
        Err(_) => String::new(),
    };
    eprintln!("alice health: {alice_health}");
    let relay_log = std::fs::read_to_string(relay.dir.join("stdout.log")).unwrap_or_default();
    eprintln!(
        "relay log tail:\n{}",
        relay_log
            .lines()
            .rev()
            .take(25)
            .collect::<Vec<_>>()
            .join("\n")
    );
    {
        if let Ok(conn) = rusqlite::Connection::open(alice_dir.path().join("fedi3.db")) {
            let mut stmt = conn
                .prepare("SELECT status, attempt, target, last_error FROM delivery_jobs")
                .ok();
            if let Some(stmt) = stmt.as_mut() {
                let rows = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, Option<String>>(3)?,
                        ))
                    })
                    .map(|it| it.flatten().collect::<Vec<_>>())
                    .unwrap_or_default();
                eprintln!("alice delivery jobs: {rows:?}");
            }
        }
    }
    assert!(accepted, "bob did not get accepted");

    let now = "2026-01-01T00:00:00Z";
    let note = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Create",
        "actor": format!("{relay_base}/users/alice"),
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
        "cc": [format!("{relay_base}/users/alice/followers")],
        "object": {
            "type": "Note",
            "content": format!("<p>{NOTE_MARKER}</p>"),
            "published": now,
            "to": ["https://www.w3.org/ns/activitystreams#Public"],
            "cc": [format!("{relay_base}/users/alice/followers")]
        }
    });
    let posted = client
        .post(format!("{relay_base}/users/alice/outbox"))
        .header("X-Fedi3-Internal", INTERNAL)
        .json(&note)
        .send()
        .await
        .expect("post note");
    assert!(
        posted.status().is_success() || posted.status().as_u16() == 202,
        "post failed: {}",
        posted.status()
    );
    let received = wait_until("bob receives alice's note", Duration::from_secs(60), || {
        let client = client.clone();
        let url = format!("{bob_base}/_fedi3/timeline/home?limit=10");
        async move {
            match client
                .get(&url)
                .header("X-Fedi3-Internal", INTERNAL)
                .send()
                .await
            {
                Ok(resp) => match resp.text().await {
                    Ok(text) => text.contains(NOTE_MARKER),
                    Err(_) => false,
                },
                Err(_) => false,
            }
        }
    })
    .await;
    if !received {
        let bob_tl = match client
            .get(format!("{bob_base}/_fedi3/timeline/home?limit=10"))
            .header("X-Fedi3-Internal", INTERNAL)
            .send()
            .await
        {
            Ok(r) => r.text().await.unwrap_or_default(),
            Err(e) => format!("err {e}"),
        };
        eprintln!("bob timeline: {bob_tl}");
        if let Ok(conn) = rusqlite::Connection::open(alice_dir.path().join("fedi3.db")) {
            if let Ok(mut stmt) =
                conn.prepare("SELECT status, attempt, target, last_error FROM delivery_jobs")
            {
                let rows = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, Option<String>>(3)?,
                        ))
                    })
                    .map(|it| it.flatten().collect::<Vec<_>>())
                    .unwrap_or_default();
                eprintln!("alice delivery jobs: {rows:?}");
            }
        }
        let relay_log = std::fs::read_to_string(relay.dir.join("stdout.log")).unwrap_or_default();
        eprintln!(
            "relay log tail:\n{}",
            relay_log
                .lines()
                .rev()
                .take(20)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    assert!(received, "bob never received the note in anchor mode");

    // 5. Blind spool: stop alice, let bob send a direct note, inspect the relay database.
    runtime::stop(alice).expect("stop alice core");
    tokio::time::sleep(Duration::from_secs(2)).await;

    let spool_note = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Create",
        "actor": format!("{relay_base}/users/bob"),
        "to": [actor_url],
        "object": {
            "type": "Note",
            "content": format!("<p>{SPOOL_MARKER}</p>"),
            "published": now,
            "to": [actor_url],
            "tag": [{"type": "Mention", "href": actor_url, "name": "@alice@127.0.0.1"}]
        }
    });
    let spool_post = client
        .post(format!("{relay_base}/users/bob/outbox"))
        .header("X-Fedi3-Internal", INTERNAL)
        .json(&spool_note)
        .send()
        .await
        .expect("post spool note");
    assert!(
        spool_post.status().is_success() || spool_post.status().as_u16() == 202,
        "spool post failed: {}",
        spool_post.status()
    );

    assert!(
        wait_until("relay spool row", Duration::from_secs(30), || async {
            let conn = relay.db();
            conn.query_row(
                "SELECT COUNT(*) FROM inbox_spool WHERE username='alice'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false)
        })
        .await,
        "no spool row for alice"
    );

    {
        let conn = relay.db();
        let (body_b64, headers_json): (String, String) = conn
            .query_row(
                "SELECT body_b64, headers_json FROM inbox_spool WHERE username='alice' ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("spool row");
        let headers: Vec<(String, String)> =
            serde_json::from_str(&headers_json).expect("headers json");
        assert!(
            headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("x-fedi3-spool-enc")
                    && v == "aes256gcm+rsa-oaep-sha256"),
            "spool row is not marked encrypted: {headers:?}"
        );
        let body = base64::engine::general_purpose::STANDARD
            .decode(body_b64.as_bytes())
            .expect("decode body");
        let body_text = String::from_utf8_lossy(&body);
        assert!(
            !body_text.contains(SPOOL_MARKER),
            "plaintext leaked into the relay spool: {body_text}"
        );
    }

    // 6. Reconnect alice: the relay flushes the encrypted envelope and the core decrypts it.
    alice = runtime::start(alice_cfg.clone()).expect("restart alice core");
    assert!(
        wait_until(
            "alice receives spooled note",
            Duration::from_secs(90),
            || {
                let client = client.clone();
                let url = format!("http://127.0.0.1:{alice_port}/_fedi3/notifications?limit=10");
                async move {
                    match client
                        .get(&url)
                        .header("X-Fedi3-Internal", INTERNAL)
                        .send()
                        .await
                    {
                        Ok(resp) => match resp.text().await {
                            Ok(text) => text.contains(SPOOL_MARKER),
                            Err(_) => false,
                        },
                        Err(_) => false,
                    }
                }
            }
        )
        .await,
        "alice never received the spooled note"
    );

    let _ = runtime::stop(bob);
    let _ = runtime::stop(alice);
}
