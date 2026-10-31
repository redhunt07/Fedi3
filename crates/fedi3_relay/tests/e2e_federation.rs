/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

//! End-to-end federation test: two real core runtimes connected to a real relay binary.
//!
//! The scenario exercises the whole production path, not mocks:
//!
//! 1. the relay binary starts with SQLite, self-registration enabled;
//! 2. two cores start in-process, each with its own identity, database and relay tunnel;
//! 3. bob follows alice through the relay (signed Follow, verified, Accept delivered back);
//! 4. alice publishes a public note through her outbox;
//! 5. the relay forwards the signed Create to bob's inbox and bob's home timeline contains it.
//!
//! Everything runs on loopback, so the SSRF escape hatch is enabled for this process.

mod common;

use common::TestRelay;
use fedi3_core::runtime::{self, CoreStartConfig};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const INTERNAL: &str = "e2e-internal-token-0123456789abcdef";
const ALICE_TOKEN: &str = "alice-relay-token-0123456789";

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local addr").port()
}
const BOB_TOKEN: &str = "bob-relay-token-0123456789ab";

struct CoreHandle {
    handle: u64,
    base_url: String,
    actor_url: String,
    data_dir: fedi3_core::test_support::TempDir,
    client: reqwest::Client,
}

impl CoreHandle {
    fn start(username: &str, relay_ws: &str, token: &str) -> anyhow::Result<Self> {
        let dir = fedi3_core::test_support::TempDir::new(&format!("e2e-{username}"));
        let port = free_port();
        let cfg = CoreStartConfig {
            username: username.to_string(),
            domain: "127.0.0.1".to_string(),
            // Empty: inferred from the relay websocket URL, exactly like production.
            public_base_url: String::new(),
            relay_ws: relay_ws.to_string(),
            relay_token: Some(token.to_string()),
            bind: format!("127.0.0.1:{port}"),
            data_dir: Some(dir.path().to_string_lossy().to_string()),
            internal_token: Some(INTERNAL.to_string()),
            max_date_skew_secs: Some(300),
            p2p: None,
            ..Default::default()
        };
        let handle = runtime::start(cfg)?;
        let base_url = format!("http://127.0.0.1:{port}");
        let actor_url = format!("{}/users/{}", relay_base_from_ws(relay_ws), username);
        Ok(Self {
            handle,
            base_url,
            actor_url,
            data_dir: dir,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .build()?,
        })
    }

    fn internal_get(&self, path: &str) -> reqwest::RequestBuilder {
        self.client
            .get(format!("{}{}", self.base_url, path))
            .header("X-Fedi3-Internal", INTERNAL)
    }

    fn internal_post(&self, path: &str) -> reqwest::RequestBuilder {
        self.client
            .post(format!("{}{}", self.base_url, path))
            .header("X-Fedi3-Internal", INTERNAL)
    }

    fn stop(&self) {
        let _ = runtime::stop(self.handle);
    }
}

fn delivery_jobs(db_path: &std::path::Path) -> String {
    let Ok(conn) = rusqlite::Connection::open(db_path) else {
        return "<no db>".to_string();
    };
    let Ok(mut stmt) = conn.prepare(
        "SELECT target, status, attempt, COALESCE(last_error, '') FROM delivery_jobs ORDER BY id",
    ) else {
        return "<no delivery_jobs table>".to_string();
    };
    let rows: Vec<String> = stmt
        .query_map([], |r| {
            Ok(format!(
                "{} status={} attempt={} err={}",
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?
            ))
        })
        .map(|it| it.flatten().collect())
        .unwrap_or_default();
    rows.join(" | ")
}

fn audit_events(db_path: &std::path::Path) -> String {
    let Ok(conn) = rusqlite::Connection::open(db_path) else {
        return "<no db>".to_string();
    };
    let Ok(mut stmt) = conn.prepare(
        "SELECT kind, COALESCE(status, ''), COALESCE(detail, ''), COALESCE(actor_id, '') FROM audit_events ORDER BY id DESC LIMIT 20",
    ) else {
        return "<no audit_events table>".to_string();
    };
    let rows: Vec<String> = stmt
        .query_map([], |r| {
            Ok(format!(
                "{} status={} detail={} actor={}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?
            ))
        })
        .map(|it| it.flatten().collect())
        .unwrap_or_default();
    rows.join("\n")
}

fn table_counts(db_path: &std::path::Path) -> String {
    let Ok(conn) = rusqlite::Connection::open(db_path) else {
        return "<no db>".to_string();
    };
    let mut out = Vec::new();
    for table in [
        "inbox_items",
        "inbox_seen",
        "objects",
        "followers",
        "following",
        "outbox_items",
        "federated_feed",
    ] {
        let n: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap_or(-1);
        out.push(format!("{table}={n}"));
    }
    out.join(" ")
}

fn relay_base_from_ws(relay_ws: &str) -> String {
    relay_ws
        .replacen("ws://", "http://", 1)
        .replacen("wss://", "https://", 1)
        .trim_end_matches('/')
        .to_string()
}

/// Poll `f` until it returns true or the deadline expires.
async fn wait_until<F, Fut>(what: &str, timeout: Duration, mut f: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if f().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    eprintln!("timeout waiting for {what}");
    false
}

#[tokio::test(flavor = "multi_thread")]
async fn two_cores_federate_through_the_relay() {
    // Loopback-only test: allow federation fetches to private addresses in this process.
    std::env::set_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH", "true");

    let relay = TestRelay::start_with_env(&[("RUST_LOG", "info")]).await;
    let relay_ws = relay.ws_url("");
    let alice = CoreHandle::start("alice", &relay_ws, ALICE_TOKEN).expect("start alice core");
    let bob = CoreHandle::start("bob", &relay_ws, BOB_TOKEN).expect("start bob core");

    // 1. Both tunnels must be up: the relay proxies the actor document to the core.
    let ap_accept = "application/activity+json";
    assert!(
        wait_until("alice tunnel", Duration::from_secs(60), || {
            let client = relay.client.clone();
            let url = format!("{}/users/alice", relay.base_url());
            async move {
                client
                    .get(url)
                    .header("Accept", ap_accept)
                    .send()
                    .await
                    .map(|r| r.status().is_success())
                    .unwrap_or(false)
            }
        })
        .await,
        "alice's actor document was never served through the relay"
    );
    assert!(
        wait_until("bob tunnel", Duration::from_secs(60), || {
            let client = relay.client.clone();
            let url = format!("{}/users/bob", relay.base_url());
            async move {
                client
                    .get(url)
                    .header("Accept", ap_accept)
                    .send()
                    .await
                    .map(|r| r.status().is_success())
                    .unwrap_or(false)
            }
        })
        .await,
        "bob's actor document was never served through the relay"
    );

    // 2. bob follows alice: the Follow goes through the relay, alice accepts, bob records it.
    let follow = json!({ "actor": alice.actor_url });
    let resp = bob
        .internal_post("/_fedi3/social/follow")
        .json(&follow)
        .send()
        .await
        .expect("send follow");
    assert!(
        resp.status().is_success(),
        "follow request failed: {}",
        resp.status()
    );

    let status_url = format!("/_fedi3/social/status?actor={}", alice.actor_url);
    assert!(
        wait_until("follow acceptance", Duration::from_secs(90), || {
            let req = bob.internal_get(&status_url);
            async move {
                let Ok(resp) = req.send().await else {
                    return false;
                };
                let Ok(body) = resp.json::<Value>().await else {
                    return false;
                };
                body.get("status").and_then(|v| v.as_str()) == Some("accepted")
            }
        })
        .await,
        "bob never saw his Follow accepted by alice"
    );

    // Diagnostic: bob must be listed in alice's followers collection through the relay.
    let followers = relay
        .client
        .get(format!(
            "{}/users/alice/followers?page=true",
            relay.base_url()
        ))
        .header("Accept", ap_accept)
        .send()
        .await
        .expect("fetch followers")
        .text()
        .await
        .unwrap_or_default();
    assert!(
        followers.contains(&bob.actor_url),
        "bob is not in alice's followers collection: {followers}"
    );

    // 3. alice publishes a public note addressed to her followers.
    let marker = "e2e-federation-note-marker";
    let create = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Create",
        "actor": alice.actor_url,
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
        "cc": [format!("{}/followers", alice.actor_url)],
        "object": {
            "type": "Note",
            "content": format!("<p>{marker}</p>"),
            "to": ["https://www.w3.org/ns/activitystreams#Public"],
            "cc": [format!("{}/followers", alice.actor_url)],
        }
    });
    let resp = alice
        .client
        .post(format!("{}/users/alice/outbox", alice.base_url))
        .header("X-Fedi3-Internal", INTERNAL)
        .json(&create)
        .send()
        .await
        .expect("post note");
    assert!(
        resp.status().is_success(),
        "outbox POST failed: {}",
        resp.status()
    );

    // Diagnostic: the note is immediately readable through the relay outbox proxy.
    let outbox_now = relay
        .client
        .get(format!("{}/users/alice/outbox?page=true", relay.base_url()))
        .header("Accept", ap_accept)
        .send()
        .await
        .expect("fetch outbox")
        .text()
        .await
        .unwrap_or_default();
    assert!(
        outbox_now.contains(marker),
        "the note was not stored in alice's outbox: {outbox_now}"
    );

    // 4. The signed Create must reach bob's inbox and appear in his home timeline.
    let delivered = wait_until("note delivery", Duration::from_secs(40), || {
        let req = bob.internal_get("/_fedi3/timeline/home?limit=50");
        async move {
            let Ok(resp) = req.send().await else {
                return false;
            };
            let Ok(body) = resp.json::<Value>().await else {
                return false;
            };
            body.to_string().contains(marker)
        }
    })
    .await;
    if !delivered {
        let timeline = match bob
            .internal_get("/_fedi3/timeline/home?limit=50")
            .send()
            .await
        {
            Ok(resp) => resp.text().await.unwrap_or_default(),
            Err(e) => format!("<request failed: {e}>"),
        };
        let jobs = delivery_jobs(&alice.data_dir.path().join("fedi3.db"));
        let bob_counts = table_counts(&bob.data_dir.path().join("fedi3.db"));
        let bob_audit = audit_events(&bob.data_dir.path().join("fedi3.db"));
        let federated = match bob
            .internal_get("/_fedi3/timeline/federated?limit=50")
            .send()
            .await
        {
            Ok(resp) => resp.text().await.unwrap_or_default(),
            Err(e) => format!("<request failed: {e}>"),
        };
        let relay_log = format!(
            "stdout:\n{}\nstderr:\n{}",
            std::fs::read_to_string(relay.dir.join("stdout.log")).unwrap_or_default(),
            std::fs::read_to_string(relay.dir.join("stderr.log")).unwrap_or_default()
        );
        panic!(
            "bob never received alice's note through the relay\n\
             alice delivery jobs: {jobs}\n\
             bob tables: {bob_counts}\n\
             bob audit: {bob_audit}\n\
             bob federated: {federated}\n\
             bob timeline: {timeline}\n\
             relay stderr: {relay_log}"
        );
    }

    // 5. The note is also publicly readable through the relay (outbox proxy).
    let resp = relay
        .client
        .get(format!("{}/users/alice/outbox?page=true", relay.base_url()))
        .header("Accept", ap_accept)
        .send()
        .await
        .expect("fetch outbox through relay");
    assert!(
        resp.status().is_success(),
        "outbox through relay failed: {}",
        resp.status()
    );
    let outbox = resp.text().await.unwrap_or_default();
    assert!(
        outbox.contains(marker),
        "the public outbox does not contain the note: {outbox}"
    );

    alice.stop();
    bob.stop();
    // Keep the temp dirs alive until the cores are stopped, then drop them.
    drop(alice.data_dir);
    drop(bob.data_dir);
    drop(relay);
}
