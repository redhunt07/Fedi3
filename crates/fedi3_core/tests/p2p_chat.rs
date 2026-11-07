//! Real local cores: encrypted chat, receipts, presence and call signaling over P2P.
mod common;

use common::*;
use fedi3_core::p2p::P2pConfig;
use serde_json::{json, Value};
use std::time::Duration;

fn core() -> TestCore {
    let p2p_port = free_port();
    TestCore::with_config(|cfg| {
        cfg.p2p = Some(P2pConfig {
            enable: true,
            public_discovery_enable: Some(false),
            mdns_enable: Some(false),
            listen: Some(format!("/ip4/127.0.0.1/tcp/{p2p_port}")),
            announce: Some(vec![format!("/ip4/127.0.0.1/tcp/{p2p_port}")]),
            discovery_enable: Some(false),
            gossip_enable: Some(false),
            sync_enable: Some(false),
            device_sync_enable: Some(false),
            ..Default::default()
        });
    })
    .unwrap()
}

async fn post(core: &TestCore, path: &str, body: Value) -> Value {
    let response = tokio::time::timeout(
        Duration::from_secs(8),
        core.post_internal(path, &serde_json::to_vec(&body).unwrap()),
    )
    .await
    .expect("local API stalled");
    let status = response.status();
    let text = response.text().await.unwrap();
    assert!(status.is_success(), "{path}: {status}: {text}");
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
async fn known_chat_peer_remains_reachable_when_public_http_profile_is_offline() {
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, OnceLock,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let public_base = format!("http://{}", listener.local_addr().unwrap());
    let online = Arc::new(AtomicBool::new(true));
    let requests = Arc::new(AtomicUsize::new(0));
    let target = Arc::new(OnceLock::<String>::new());
    let proxy = {
        let online = online.clone();
        let target = target.clone();
        let requests = requests.clone();
        axum::Router::new().fallback(move |uri: axum::http::Uri| {
            let online = online.clone();
            let target = target.clone();
            let requests = requests.clone();
            async move {
                requests.fetch_add(1, Ordering::Relaxed);
                if !online.load(Ordering::Relaxed) {
                    return (axum::http::StatusCode::SERVICE_UNAVAILABLE, Vec::new());
                }
                let response = reqwest::get(format!("{}{}", target.get().unwrap(), uri.path()))
                    .await
                    .unwrap();
                (response.status(), response.bytes().await.unwrap().to_vec())
            }
        })
    };
    let server = tokio::spawn(async move {
        axum::serve(listener, proxy).await.unwrap();
    });
    let p2p_port = free_port();
    let b = TestCore::with_config(|cfg| {
        cfg.public_base_url = public_base.clone();
        cfg.p2p = Some(P2pConfig {
            enable: true,
            public_discovery_enable: Some(false),
            mdns_enable: Some(false),
            listen: Some(format!("/ip4/127.0.0.1/tcp/{p2p_port}")),
            announce: Some(vec![format!("/ip4/127.0.0.1/tcp/{p2p_port}")]),
            discovery_enable: Some(false),
            gossip_enable: Some(false),
            sync_enable: Some(false),
            ..Default::default()
        });
    })
    .unwrap();
    target.set(b.base_url.clone()).unwrap();
    let actor = format!("{public_base}/users/alice");
    let a = core();
    let first = post(
        &a,
        "/_fedi3/chat/send",
        json!({"recipients": [actor], "text": "First contact"}),
    )
    .await;
    assert_eq!(first["sent"], 1);
    online.store(false, Ordering::Relaxed);
    let before = requests.load(Ordering::Relaxed);
    let second = post(
        &a,
        "/_fedi3/chat/send",
        json!({"recipients": [actor], "text": "HTTP relay offline"}),
    )
    .await;
    assert_eq!(second["sent"], 1);
    assert_eq!(second["queued"], 0);
    assert_eq!(
        requests.load(Ordering::Relaxed),
        before,
        "fresh binding must avoid the HTTP relay"
    );
    // Expire the fresh window, retaining a trusted binding inside its seven-day
    // lifetime. A 503 from the HTTP origin must still allow direct delivery.
    let conn = rusqlite::Connection::open(a.db_path()).unwrap();
    conn.execute(
        "UPDATE chat_actor_cache SET fetched_at_ms=fetched_at_ms-7200000",
        [],
    )
    .unwrap();
    let third = post(
        &a,
        "/_fedi3/chat/send",
        json!({"recipients": [actor], "text": "Cached binding fallback"}),
    )
    .await;
    assert_eq!(third["sent"], 1);
    assert!(requests.load(Ordering::Relaxed) > before);
    let query = format!(
        "/_fedi3/chat/threads/{}",
        urlencoding::encode(third["thread_id"].as_str().unwrap())
    );
    let received: Value = b.get(&query).await.json().await.unwrap();
    assert!(received["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["message_id"] == third["message_id"]));
    server.abort();
}

#[tokio::test]
async fn encrypted_message_exact_receipt_presence_and_call_reach_other_core() {
    let a = core();
    let b = core();
    let sent = post(
        &a,
        "/_fedi3/chat/send",
        json!({
            "recipients": [b.actor_url()], "text": "P2P regression test",
        }),
    )
    .await;
    assert_eq!(
        sent["sent"], 1,
        "message must reach recipient, not just a mailbox: {sent}"
    );
    assert_eq!(sent["queued"], 0);
    let thread = sent["thread_id"].as_str().unwrap();
    let message = sent["message_id"].as_str().unwrap();
    let query = format!("/_fedi3/chat/threads/{}", urlencoding::encode(thread));
    let received: Value = b.get(&query).await.json().await.unwrap();
    assert!(received["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["message_id"] == message));
    let db = fedi3_core::social_db::SocialDb::open(a.db_path()).unwrap();
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let statuses = db
                .list_chat_message_statuses(&[message.to_string()])
                .unwrap();
            if statuses
                .iter()
                .any(|s| s.1 == b.actor_url() && s.2 == "delivered")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("exact message delivery receipt missing");

    post(&a, "/_fedi3/presence", json!({"status": "busy"})).await;
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let presence: Value = b.get("/_fedi3/presence").await.json().await.unwrap();
            if presence["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["actor_id"] == a.actor_url() && p["status"] == "busy")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("P2P presence missing");

    let call = post(
        &a,
        "/_fedi3/chat/call/signal",
        json!({
            "thread_id": thread, "call_id": "regression-call", "action": "offer",
            "to": b.actor_url(), "data": {"media": "audio", "sdp": "test-signaling-only"},
        }),
    )
    .await;
    assert_eq!(call["sent"], 1);
    let signals: Value = b
        .get(&format!(
            "/_fedi3/chat/call/signals?thread_id={}&after_ms=0",
            urlencoding::encode(thread)
        ))
        .await
        .json()
        .await
        .unwrap();
    assert!(signals["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["call_id"] == "regression-call"
            && s["from_actor"] == a.actor_url()
            && s["data"]["sdp"] == "test-signaling-only"
            && s["data"]["media"] == "audio"));
}
