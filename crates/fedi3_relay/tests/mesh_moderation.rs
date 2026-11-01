/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

//! Relay mesh and moderation:
//!
//! 1. two relays exchange their user directories (peers) and route a request for a user
//!    hosted on the other relay to its home relay;
//! 2. relay-wide and per-user blocklists are enforced on the inbox path, so blocked senders
//!    are acknowledged without being forwarded or spooled.

mod common;

use common::TestRelay;
use serde_json::json;
use std::time::{Duration, Instant};

const ALICE_TOKEN: &str = "alice-mesh-token-0123456789";
const SPAM_ACTOR: &str = "https://spam.example/users/evil";
const OTHER_ACTOR: &str = "https://other.example/users/friend";

fn note_from(actor: &str) -> serde_json::Value {
    json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Create",
        "actor": actor,
        "to": ["http://127.0.0.1/users/alice"],
        "object": {
            "type": "Note",
            "content": "<p>mesh test</p>",
            "to": ["http://127.0.0.1/users/alice"]
        }
    })
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
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    eprintln!("timeout waiting for {what}");
    false
}

#[tokio::test(flavor = "multi_thread")]
async fn relay_mesh_routes_and_moderation_blocks() {
    let env = &[("RUST_LOG", "info"), ("FEDI3_RELAY_PEER_SYNC_SECS", "5")];
    let relay_a = TestRelay::start_with_env(env).await;
    let relay_b = TestRelay::start_with_env(env).await;

    // alice is hosted on relay A only.
    let reg = relay_a.register("alice", ALICE_TOKEN).await;
    assert!(
        reg.status().is_success(),
        "register alice: {}",
        reg.status()
    );

    // Seed A's actor cache as if the core had connected once (the tunnel is offline here).
    let actor_id = format!("{}/users/alice", relay_a.base_url());
    let actor_json = json!({
        "id": actor_id,
        "type": "Person",
        "preferredUsername": "alice",
        "inbox": format!("{}/users/alice/inbox", relay_a.base_url()),
        "outbox": format!("{}/users/alice/outbox", relay_a.base_url()),
        "publicKey": {
            "id": format!("{actor_id}#main-key"),
            "owner": actor_id,
            "publicKeyPem": "-----BEGIN PUBLIC KEY-----\nMIIB\n-----END PUBLIC KEY-----"
        }
    });
    relay_a
        .db()
        .execute(
            "INSERT OR REPLACE INTO user_cache(username, actor_json, updated_at_ms, actor_id, actor_url)
             VALUES ('alice', ?1, 0, ?2, ?2)",
            rusqlite::params![actor_json.to_string(), actor_id],
        )
        .expect("seed actor cache");

    // Peer both relays.
    for (relay, peer) in [
        (&relay_a, relay_b.base_url()),
        (&relay_b, relay_a.base_url()),
    ] {
        let resp = relay
            .post_json_with(
                "/admin/peers",
                &json!({"url": peer}),
                &[("Authorization", &relay.admin_bearer())],
            )
            .await;
        assert!(
            resp.status().is_success(),
            "add peer failed: {}",
            resp.status()
        );
    }

    // Wait for the directory sync: B must learn that alice lives on A.
    assert!(
        wait_until("peer directory sync", Duration::from_secs(30), || async {
            relay_b
                .db()
                .query_row(
                    "SELECT relay_url FROM remote_users WHERE username='alice'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .map(|url| url == relay_a.base_url())
                .unwrap_or(false)
        })
        .await,
        "relay B never learned about alice"
    );

    // 1. A request for alice addressed to B is forwarded to her home relay.
    let resp = relay_b.get("/users/alice").await;
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "mesh forward failed: {}",
        resp.status()
    );
    let body: serde_json::Value = resp.json().await.expect("actor json");
    assert_eq!(body["id"], actor_id);

    // 2. An inbox POST to B for alice reaches A, which spools it (tunnel offline).
    let post = relay_b
        .post_bytes_with(
            "/users/alice/inbox",
            serde_json::to_vec(&note_from(OTHER_ACTOR))
                .unwrap()
                .as_slice(),
            &[("Content-Type", "application/activity+json")],
        )
        .await;
    assert_eq!(
        post.status(),
        reqwest::StatusCode::SERVICE_UNAVAILABLE,
        "expected spool 503 from the home relay, got {}",
        post.status()
    );
    let spooled = relay_a
        .db()
        .query_row(
            "SELECT COUNT(*) FROM inbox_spool WHERE username='alice'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);
    assert_eq!(spooled, 1, "the home relay should have spooled the request");

    // 3. Relay-wide block: a blocked sender is acknowledged, not forwarded, not spooled.
    let blocked = relay_a
        .post_json_with(
            "/admin/blocks",
            &json!({"kind": "actor", "value": SPAM_ACTOR, "reason": "spam"}),
            &[("Authorization", &relay_a.admin_bearer())],
        )
        .await;
    assert!(
        blocked.status().is_success(),
        "block failed: {}",
        blocked.status()
    );

    let post = relay_b
        .post_bytes_with(
            "/users/alice/inbox",
            serde_json::to_vec(&note_from(SPAM_ACTOR))
                .unwrap()
                .as_slice(),
            &[("Content-Type", "application/activity+json")],
        )
        .await;
    assert_eq!(
        post.status(),
        reqwest::StatusCode::ACCEPTED,
        "blocked sender should be acknowledged, got {}",
        post.status()
    );
    let spooled = relay_a
        .db()
        .query_row(
            "SELECT COUNT(*) FROM inbox_spool WHERE username='alice'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);
    assert_eq!(spooled, 1, "blocked sender must not be spooled");

    // 4. Per-user blocklist pushed by the core with the relay token.
    let put = relay_a
        .client
        .put(format!(
            "{}/_fedi3/relay/user_blocks/alice",
            relay_a.base_url()
        ))
        .header("X-Fedi3-Relay-Token", ALICE_TOKEN)
        .json(&json!({"blocked_actors": [OTHER_ACTOR], "blocked_domains": []}))
        .send()
        .await
        .expect("put user blocks");
    assert!(
        put.status().is_success(),
        "user blocks failed: {}",
        put.status()
    );

    let post = relay_b
        .post_bytes_with(
            "/users/alice/inbox",
            serde_json::to_vec(&note_from(OTHER_ACTOR))
                .unwrap()
                .as_slice(),
            &[("Content-Type", "application/activity+json")],
        )
        .await;
    assert_eq!(
        post.status(),
        reqwest::StatusCode::ACCEPTED,
        "per-user blocked sender should be acknowledged, got {}",
        post.status()
    );

    // A wrong token is rejected.
    let bad = relay_a
        .client
        .put(format!(
            "{}/_fedi3/relay/user_blocks/alice",
            relay_a.base_url()
        ))
        .header("X-Fedi3-Relay-Token", "wrong-token-0123456789")
        .json(&json!({"blocked_actors": [], "blocked_domains": []}))
        .send()
        .await
        .expect("bad token request");
    assert_eq!(bad.status(), reqwest::StatusCode::UNAUTHORIZED);

    // 5. Admin listing shows the relay block.
    let list = relay_a
        .get_with(
            "/admin/blocks",
            &[("Authorization", &relay_a.admin_bearer())],
        )
        .await;
    assert!(list.status().is_success());
    let body: serde_json::Value = list.json().await.expect("blocks json");
    assert!(
        body["blocks"]
            .as_array()
            .map(|a| a.iter().any(|b| b["value"] == SPAM_ACTOR))
            .unwrap_or(false),
        "relay block not listed: {body}"
    );

    // 6. Unknown users still 404 (no infinite mesh hop).
    let ghost = relay_b.get("/users/ghost").await;
    assert_eq!(ghost.status(), reqwest::StatusCode::NOT_FOUND);
}
