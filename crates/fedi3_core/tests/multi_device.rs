//! Multi-device chat sync: shared account identity import/export, identity-key-only
//! envelopes, self-sync copies of locally originated payloads and the relay chat spool
//! poller. The relay is a small in-process mock that implements exactly the endpoints
//! described by docs/multi_device_sync.md.
#![allow(clippy::field_reassign_with_default)]

mod common;

use common::*;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

const RELAY_TOKEN: &str = "test-relay-token-0123456789abcdef";

struct MockRelay {
    base_url: String,
    ws_url: String,
    inbox: Arc<tokio::sync::Mutex<Vec<Value>>>,
    shutdown: tokio::sync::watch::Sender<bool>,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl MockRelay {
    async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{addr}");
        let ws_url = format!("ws://{addr}");
        let inbox = Arc::new(tokio::sync::Mutex::new(Vec::<Value>::new()));
        let inbox_for_post = inbox.clone();
        let inbox_for_spool = inbox.clone();
        let router = axum::Router::new()
            .route(
                "/users/:user/_fedi3/chat/inbox",
                axum::routing::post(move |body: axum::body::Bytes| {
                    let inbox = inbox_for_post.clone();
                    async move {
                        let envelope: Value =
                            serde_json::from_slice(&body).expect("chat envelope JSON");
                        inbox.lock().await.push(envelope);
                        axum::http::StatusCode::ACCEPTED
                    }
                }),
            )
            .route(
                "/_fedi3/chat/spool",
                axum::routing::get(
                    move |headers: axum::http::HeaderMap, uri: axum::http::Uri| {
                        let inbox = inbox_for_spool.clone();
                        async move {
                            use axum::response::IntoResponse as _;
                            let authorized = headers
                                .get(axum::http::header::AUTHORIZATION)
                                .and_then(|value| value.to_str().ok())
                                .map(|value| value == format!("Bearer {RELAY_TOKEN}"))
                                .unwrap_or(false);
                            if !authorized {
                                return axum::response::Response::builder()
                                    .status(axum::http::StatusCode::UNAUTHORIZED)
                                    .body(axum::body::Body::empty())
                                    .unwrap();
                            }
                            let query = uri.query().unwrap_or("");
                            let after = query
                                .split('&')
                                .find_map(|pair| pair.strip_prefix("after="))
                                .and_then(|value| value.parse::<i64>().ok())
                                .unwrap_or(0);
                            let envelopes = inbox.lock().await;
                            let mut items = Vec::new();
                            for (idx, envelope) in envelopes.iter().enumerate() {
                                let id = (idx + 1) as i64;
                                if id > after {
                                    items.push(json!({"id": id, "envelope": envelope}));
                                }
                            }
                            let latest_id = envelopes.len() as i64;
                            axum::Json(json!({"items": items, "latest_id": latest_id}))
                                .into_response()
                        }
                    },
                ),
            );
        let (shutdown, mut rx) = tokio::sync::watch::channel(false);
        let join = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = rx.changed().await;
                })
                .await
                .ok();
        });
        Self {
            base_url,
            ws_url,
            inbox,
            shutdown,
            join: Some(join),
        }
    }

    async fn envelopes(&self) -> Vec<Value> {
        self.inbox.lock().await.clone()
    }
}

impl Drop for MockRelay {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
        if let Some(join) = self.join.take() {
            join.abort();
        }
    }
}

fn account_actor(relay: &MockRelay) -> String {
    format!("{}/users/alice", relay.base_url)
}

async fn export_identity(core: &TestCore) -> Value {
    let response = core.get("/_fedi3/identity/export").await;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    assert!(
        status.is_success(),
        "identity export failed: {status}: {text}"
    );
    serde_json::from_str(&text).expect("identity export JSON")
}

/// A core with a fresh local profile that only talks to the relay transport.
fn fresh_account(relay: &MockRelay) -> TestCore {
    TestCore::with_config(|cfg| {
        cfg.public_base_url = relay.base_url.clone();
        cfg.relay_ws = relay.ws_url.clone();
        cfg.relay_token = None;
    })
    .unwrap()
}

/// A core that imports the account identity package into its own data directory.
fn start_linked_core(relay: &MockRelay, export: &Value, relay_token: Option<&str>) -> TestCore {
    let private_pem = export["private_key_pem"].as_str().unwrap().to_string();
    let chat_public = export["chat_identity_public_b64"]
        .as_str()
        .unwrap()
        .to_string();
    let chat_secret = export["chat_identity_secret_b64"]
        .as_str()
        .unwrap()
        .to_string();
    let relay_token = relay_token.map(str::to_string);
    TestCore::with_config(|cfg| {
        cfg.public_base_url = relay.base_url.clone();
        cfg.relay_ws = relay.ws_url.clone();
        cfg.relay_token = relay_token.clone();
        cfg.identity_private_key_pem = Some(private_pem.clone());
        cfg.chat_identity_public_b64 = Some(chat_public.clone());
        cfg.chat_identity_secret_b64 = Some(chat_secret.clone());
    })
    .unwrap()
}

async fn post(core: &TestCore, path: &str, body: Value) -> (reqwest::StatusCode, Value) {
    let response = tokio::time::timeout(
        Duration::from_secs(30),
        core.post_internal(path, &serde_json::to_vec(&body).unwrap()),
    )
    .await
    .expect("local API stalled");
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let value = serde_json::from_str(&text).unwrap_or(Value::Null);
    (status, value)
}

async fn wait_envelope_count(relay: &MockRelay, count: usize) -> Vec<Value> {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let envelopes = relay.envelopes().await;
            if envelopes.len() >= count {
                return envelopes;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("self-sync envelope missing on the relay")
}

async fn post_envelope_to_core(core: &TestCore, envelope: &Value) -> (reqwest::StatusCode, String) {
    let response = TestCore::client()
        .post(core.url("/_fedi3/chat/inbox"))
        .header("content-type", "application/json")
        .body(serde_json::to_vec(envelope).unwrap())
        .send()
        .await
        .expect("local chat inbox request");
    let status = response.status();
    (status, response.text().await.unwrap_or_default())
}

fn create_thread_for_device(core: &TestCore, thread_id: &str, kind: &str, members: &[&str]) {
    let db = fedi3_core::social_db::SocialDb::open(core.db_path()).unwrap();
    db.create_chat_thread(thread_id, kind, None).unwrap();
    for (idx, member) in members.iter().enumerate() {
        let role = if idx == 0 { "owner" } else { "member" };
        db.upsert_chat_member(thread_id, member, role).unwrap();
    }
}

#[tokio::test]
async fn identity_export_import_shares_actor_and_chat_identity() {
    let relay = MockRelay::new().await;
    // A fresh profile establishes the canonical identity package of the account.
    let source = fresh_account(&relay);
    let export = export_identity(&source).await;
    for field in [
        "public_key_pem",
        "private_key_pem",
        "chat_identity_public_b64",
        "chat_identity_secret_b64",
    ] {
        assert!(
            !export[field].as_str().unwrap_or_default().is_empty(),
            "identity export is missing {field}"
        );
    }
    let did = fedi3_core::keys::did_from_public_key_pem(export["public_key_pem"].as_str().unwrap());

    // Two clients import the same package into different data directories.
    let a = start_linked_core(&relay, &export, None);
    let b = start_linked_core(&relay, &export, None);
    let export_a = export_identity(&a).await;
    let export_b = export_identity(&b).await;
    assert_eq!(export_a["public_key_pem"], export["public_key_pem"]);
    assert_eq!(export_b["public_key_pem"], export["public_key_pem"]);
    assert_eq!(export_a["private_key_pem"], export["private_key_pem"]);
    assert_eq!(
        export_b["chat_identity_public_b64"],
        export["chat_identity_public_b64"]
    );
    assert_eq!(
        export_a["chat_identity_secret_b64"],
        export["chat_identity_secret_b64"]
    );
    assert_eq!(
        fedi3_core::keys::did_from_public_key_pem(export_b["public_key_pem"].as_str().unwrap()),
        did,
        "imported identity must keep the same actor DID"
    );

    // A message originated on the first client is self-synced as an envelope encrypted
    // for the account chat identity key.
    let (status, sent) = post(
        &a,
        "/_fedi3/chat/send",
        json!({
            "recipients": [account_actor(&relay)],
            "text": "linked device history",
        }),
    )
    .await;
    assert!(status.is_success(), "chat send failed: {status}: {sent}");
    let thread_id = sent["thread_id"].as_str().unwrap().to_string();
    let message_id = sent["message_id"].as_str().unwrap().to_string();

    let envelopes = wait_envelope_count(&relay, 1).await;
    let envelope = envelopes[0].clone();
    assert_eq!(envelope["thread_id"], thread_id.as_str());
    assert_eq!(envelope["sender_actor"], account_actor(&relay).as_str());

    // The second client knows the thread (the pairing snapshot carries recent history)
    // and decrypts the shared envelope with the imported chat identity key.
    create_thread_for_device(&b, &thread_id, "group", &[&account_actor(&relay)]);
    let (status, text) = post_envelope_to_core(&b, &envelope).await;
    assert!(
        status.is_success(),
        "second client rejected the shared envelope: {status}: {text}"
    );

    let query = format!("/_fedi3/chat/threads/{}", urlencoding::encode(&thread_id));
    let messages: Value = b.get(&query).await.json().await.unwrap();
    assert!(
        messages["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["message_id"] == message_id.as_str()),
        "shared envelope was not stored on the second client: {messages}"
    );
}

#[tokio::test]
async fn chat_bundle_advertises_no_prekeys() {
    let core = TestCore::new().unwrap();
    let bundle: Value = core.get("/_fedi3/chat/bundle").await.json().await.unwrap();
    assert_eq!(
        bundle["prekeys"].as_array().unwrap().len(),
        0,
        "linked clients share the identity key, so no per-device prekey may be advertised"
    );
    assert!(
        !bundle["kem_public_b64"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "the bundle must still advertise the chat identity key"
    );
}

#[tokio::test]
async fn self_sync_message_is_applied_by_linked_client_poller_and_emits_ui_event() {
    let relay = MockRelay::new().await;
    let source = fresh_account(&relay);
    let export = export_identity(&source).await;
    // The sender does not poll; the linked client does.
    let a = start_linked_core(&relay, &export, None);
    let b = start_linked_core(&relay, &export, Some(RELAY_TOKEN));
    let bob = MockActorServer::new("bob").await.unwrap();

    let mut members = vec![account_actor(&relay), bob.actor_url.clone()];
    members.sort();
    let thread_id = fedi3_core::ap::chat_dm_thread_id(&members);
    // The linked client has no local thread yet: the poller must bootstrap it from the
    // self-synced message, which is what a manual-login client relies on.

    // Subscribe to the linked client UI stream before the envelope can arrive.
    let base = b.base_url.clone();
    let sse_thread = thread_id.clone();
    let sse = tokio::spawn(async move {
        let mut response = TestCore::client()
            .get(format!("{base}/_fedi3/stream?kind=chat"))
            .header("X-Fedi3-Internal", INTERNAL_TOKEN)
            .send()
            .await
            .expect("chat stream request");
        let mut buffer = String::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    buffer.push_str(&String::from_utf8_lossy(&chunk));
                    if buffer.contains(&sse_thread)
                        && buffer.contains("\"activity_type\":\"message\"")
                    {
                        return buffer;
                    }
                }
                _ => return buffer,
            }
        }
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let (status, sent) = post(
        &a,
        "/_fedi3/chat/send",
        json!({
            "recipients": [bob.actor_url.clone()],
            "text": "device two should see this",
        }),
    )
    .await;
    assert!(status.is_success(), "chat send failed: {status}: {sent}");
    assert_eq!(sent["thread_id"], thread_id.as_str());

    // Exactly one self-sync copy is posted to the relay chat inbox.
    let envelopes = wait_envelope_count(&relay, 1).await;
    assert_eq!(envelopes[0]["thread_id"], thread_id.as_str());
    assert_eq!(envelopes[0]["sender_actor"], account_actor(&relay).as_str());

    let message_id = sent["message_id"].as_str().unwrap().to_string();
    let query = format!("/_fedi3/chat/threads/{}", urlencoding::encode(&thread_id));
    tokio::time::timeout(Duration::from_secs(25), async {
        loop {
            let received: Value = b.get(&query).await.json().await.unwrap();
            if received["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|message| message["message_id"] == message_id.as_str())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("linked client poller did not apply the spooled message");

    // The unknown thread was bootstrapped from the spooled message: the local actor is the
    // owner of the self-sync DM.
    let threads: Value = b
        .get("/_fedi3/chat/threads?limit=20")
        .await
        .json()
        .await
        .unwrap();
    assert!(
        threads["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|thread| thread["thread_id"] == thread_id.as_str() && thread["kind"] == "dm"),
        "linked client did not bootstrap the unknown thread: {threads}"
    );
    let members: Value = b
        .get(&format!(
            "/_fedi3/chat/thread/members?thread_id={}",
            urlencoding::encode(&thread_id)
        ))
        .await
        .json()
        .await
        .unwrap();
    assert!(
        members["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|member| member["actor_id"] == account_actor(&relay) && member["role"] == "owner"),
        "bootstrapped thread must keep the local actor as owner: {members}"
    );

    let cursor = fedi3_core::social_db::SocialDb::open(b.db_path())
        .unwrap()
        .get_local_meta("chat_spool_after")
        .unwrap();
    assert!(
        cursor
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0)
            >= 1,
        "chat spool cursor was not persisted"
    );

    let events = tokio::time::timeout(Duration::from_secs(10), sse)
        .await
        .expect("chat UI event was not emitted")
        .expect("chat stream task");
    assert!(
        events.contains("\"kind\":\"chat\""),
        "missing chat UI event: {events}"
    );
    assert!(
        events.contains(&thread_id),
        "chat UI event does not reference the thread: {events}"
    );
}

#[tokio::test]
async fn accept_call_signal_is_self_synced() {
    let relay = MockRelay::new().await;
    let source = fresh_account(&relay);
    let export = export_identity(&source).await;
    let a = start_linked_core(&relay, &export, None);
    let b = start_linked_core(&relay, &export, None);
    let bob = MockActorServer::new("bob").await.unwrap();

    let (status, created) = post(
        &a,
        "/_fedi3/chat/thread/create",
        json!({
            "recipients": [bob.actor_url.clone()],
            "kind": "group",
            "title": "Call sync",
        }),
    )
    .await;
    assert!(
        status.is_success(),
        "thread create failed: {status}: {created}"
    );
    let thread_id = created["thread_id"].as_str().unwrap().to_string();
    create_thread_for_device(
        &b,
        &thread_id,
        "group",
        &[&account_actor(&relay), &bob.actor_url],
    );
    // The group announcement is self-synced as well.
    wait_envelope_count(&relay, 1).await;

    // An accept signal is broadcast to the thread members (no single `to` target), so
    // every linked client of the answering account can see it.
    let (status, signal) = post(
        &a,
        "/_fedi3/chat/call/signal",
        json!({
            "thread_id": thread_id,
            "call_id": "call-accept-1",
            "action": "accept",
            "data": {"media": "audio"},
        }),
    )
    .await;
    // The recipient has no P2P peer in this test, so the direct delivery is queued or
    // reported as a soft failure; the self-sync copy must still be posted.
    assert!(
        status.is_success() || status == reqwest::StatusCode::BAD_GATEWAY,
        "unexpected call signal status: {status}: {signal}"
    );

    let envelopes = wait_envelope_count(&relay, 2).await;
    let accept = envelopes[1].clone();
    assert_eq!(accept["thread_id"], thread_id.as_str());
    assert_eq!(accept["sender_actor"], account_actor(&relay).as_str());

    let (status, text) = post_envelope_to_core(&b, &accept).await;
    assert!(
        status.is_success(),
        "linked client rejected the self-synced accept: {status}: {text}"
    );
    let signals: Value = b
        .get(&format!(
            "/_fedi3/chat/call/signals?thread_id={}&after_ms=0",
            urlencoding::encode(&thread_id)
        ))
        .await
        .json()
        .await
        .unwrap();
    assert!(
        signals["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["call_id"] == "call-accept-1"
                && item["action"] == "accept"
                && item["from_actor"] == account_actor(&relay)),
        "self-synced accept signal missing on the linked client: {signals}"
    );
}

#[tokio::test]
async fn invalid_provided_identity_pem_fails_start() {
    let dir = fedi3_core::test_support::TempDir::new("bad-identity");
    let port = free_port();
    let mut cfg = fedi3_core::runtime::CoreStartConfig::default();
    cfg.username = "alice".to_string();
    cfg.domain = "example.test".to_string();
    cfg.public_base_url = format!("http://127.0.0.1:{port}");
    cfg.relay_ws = String::new();
    cfg.relay_token = None;
    cfg.bind = format!("127.0.0.1:{port}");
    cfg.data_dir = Some(dir.path().to_string_lossy().to_string());
    cfg.identity_private_key_pem = Some("not a pem".to_string());
    assert!(
        fedi3_core::runtime::start(cfg).is_err(),
        "a corrupt provided identity PEM must fail loudly"
    );
}

#[tokio::test]
async fn chat_snapshot_roundtrip_restores_threads_messages_statuses_reactions_pins_and_signals() {
    let relay = MockRelay::new().await;
    let source = fresh_account(&relay);
    let export = export_identity(&source).await;
    // A polls its own spool so its self-synced status and call signal are stored locally
    // before the snapshot is exported.
    let a = start_linked_core(&relay, &export, Some(RELAY_TOKEN));
    // B is a fresh linked client: same identity package, empty chat database.
    let b = start_linked_core(&relay, &export, None);
    let bob = MockActorServer::new("bob").await.unwrap();

    let (status, created) = post(
        &a,
        "/_fedi3/chat/thread/create",
        json!({
            "recipients": [bob.actor_url.clone()],
            "kind": "group",
            "title": "Snapshot thread",
        }),
    )
    .await;
    assert!(
        status.is_success(),
        "thread create failed: {status}: {created}"
    );
    let thread_id = created["thread_id"].as_str().unwrap().to_string();

    let (status, sent) = post(
        &a,
        "/_fedi3/chat/send",
        json!({
            "thread_id": thread_id,
            "recipients": [bob.actor_url.clone()],
            "text": "snapshot message",
        }),
    )
    .await;
    assert!(status.is_success(), "chat send failed: {status}: {sent}");
    let message_id = sent["message_id"].as_str().unwrap().to_string();

    let (status, _) = post(
        &a,
        "/_fedi3/chat/react",
        json!({"message_id": message_id, "reaction": "👍"}),
    )
    .await;
    assert!(status.is_success(), "react failed: {status}");

    let (status, _) = post(
        &a,
        "/_fedi3/chat/thread/pin",
        json!({"thread_id": thread_id, "message_id": message_id, "pinned": true}),
    )
    .await;
    assert!(status.is_success(), "pin failed: {status}");

    let (status, signal) = post(
        &a,
        "/_fedi3/chat/call/signal",
        json!({
            "thread_id": thread_id,
            "call_id": "call-snapshot-1",
            "action": "invite",
            "data": {"media": "audio"},
        }),
    )
    .await;
    assert!(
        status.is_success() || status == reqwest::StatusCode::BAD_GATEWAY,
        "unexpected call signal status: {status}: {signal}"
    );

    let (status, _) = post(
        &a,
        "/_fedi3/chat/seen",
        json!({"thread_id": thread_id, "message_id": message_id}),
    )
    .await;
    assert!(status.is_success(), "seen failed: {status}");

    // Wait until the poller applied the self-synced seen receipt and call signal locally.
    let signals_url = format!(
        "/_fedi3/chat/call/signals?thread_id={}&after_ms=0",
        urlencoding::encode(&thread_id)
    );
    tokio::time::timeout(Duration::from_secs(25), async {
        loop {
            let statuses = post(
                &a,
                "/_fedi3/chat/status",
                json!({"message_ids": [message_id]}),
            )
            .await
            .1;
            let seen = statuses["items"]
                .as_array()
                .map(|items| {
                    items.iter().any(|item| {
                        item["message_id"] == message_id.as_str()
                            && item["actor_id"] == account_actor(&relay)
                            && item["status"] == "seen"
                    })
                })
                .unwrap_or(false);
            let signals: Value = a.get(&signals_url).await.json().await.unwrap();
            let signalled = signals["items"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .any(|item| item["call_id"] == "call-snapshot-1")
                })
                .unwrap_or(false);
            if seen && signalled {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("sender poller did not store the self-synced status/call signal");

    let snapshot: Value = a.get("/_fedi3/chat/snapshot").await.json().await.unwrap();
    assert_eq!(snapshot["v"], 1);
    assert_eq!(
        snapshot["truncated"], false,
        "a small snapshot must not be truncated"
    );
    assert!(
        snapshot["threads"]
            .as_array()
            .unwrap()
            .iter()
            .any(|thread| thread["thread_id"] == thread_id.as_str()
                && thread["title"] == "Snapshot thread"),
        "snapshot is missing the thread: {snapshot}"
    );
    assert!(
        snapshot["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["message_id"] == message_id.as_str()),
        "snapshot is missing the message: {snapshot}"
    );
    assert!(
        snapshot["statuses"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["message_id"] == message_id.as_str() && item["status"] == "seen"),
        "snapshot is missing the status: {snapshot}"
    );
    assert!(
        snapshot["reactions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["message_id"] == message_id.as_str() && item["reaction"] == "👍"),
        "snapshot is missing the reaction: {snapshot}"
    );
    assert!(
        snapshot["pins"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["message_id"] == message_id.as_str()),
        "snapshot is missing the pin: {snapshot}"
    );
    assert!(
        snapshot["call_signals"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["call_id"] == "call-snapshot-1"),
        "snapshot is missing the call signal: {snapshot}"
    );

    // Import the snapshot into the fresh linked client.
    let response = TestCore::client()
        .post(b.url("/_fedi3/chat/snapshot"))
        .header("X-Fedi3-Internal", INTERNAL_TOKEN)
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&snapshot).unwrap())
        .send()
        .await
        .expect("snapshot import request");
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    assert!(
        status.is_success(),
        "snapshot import failed: {status}: {text}"
    );

    // Idempotent re-import must not duplicate anything.
    let response = TestCore::client()
        .post(b.url("/_fedi3/chat/snapshot"))
        .header("X-Fedi3-Internal", INTERNAL_TOKEN)
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&snapshot).unwrap())
        .send()
        .await
        .expect("snapshot re-import request");
    assert!(response.status().is_success(), "snapshot re-import failed");

    // Malformed JSON is rejected.
    let response = TestCore::client()
        .post(b.url("/_fedi3/chat/snapshot"))
        .header("X-Fedi3-Internal", INTERNAL_TOKEN)
        .header("content-type", "application/json")
        .body(b"not a snapshot".to_vec())
        .send()
        .await
        .expect("malformed snapshot request");
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);

    let threads: Value = b
        .get("/_fedi3/chat/threads?limit=20")
        .await
        .json()
        .await
        .unwrap();
    assert!(
        threads["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|thread| thread["thread_id"] == thread_id.as_str()
                && thread["title"] == "Snapshot thread"),
        "imported thread missing on the linked client: {threads}"
    );

    let messages: Value = b
        .get(&format!(
            "/_fedi3/chat/threads/{}",
            urlencoding::encode(&thread_id)
        ))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        messages["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|message| message["message_id"] == message_id.as_str())
            .count(),
        1,
        "imported message must exist exactly once: {messages}"
    );

    let statuses = post(
        &b,
        "/_fedi3/chat/status",
        json!({"message_ids": [message_id]}),
    )
    .await
    .1;
    assert!(
        statuses["items"].as_array().unwrap().iter().any(|item| {
            item["message_id"] == message_id.as_str()
                && item["actor_id"] == account_actor(&relay)
                && item["status"] == "seen"
        }),
        "imported status missing: {statuses}"
    );

    let reactions = post(
        &b,
        "/_fedi3/chat/reactions",
        json!({"message_ids": [message_id]}),
    )
    .await
    .1;
    assert!(
        reactions["items"].as_array().unwrap().iter().any(|item| {
            item["message_id"] == message_id.as_str()
                && item["reactions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|reaction| {
                        reaction["reaction"] == "👍"
                            && reaction["count"] == 1
                            && reaction["me"] == true
                    })
        }),
        "imported reaction missing: {reactions}"
    );

    let pin: Value = b
        .get(&format!(
            "/_fedi3/chat/thread/pin?thread_id={}",
            urlencoding::encode(&thread_id)
        ))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        pin["pin"]["message_id"],
        message_id.as_str(),
        "imported pin missing: {pin}"
    );

    let signals: Value = b.get(&signals_url).await.json().await.unwrap();
    assert!(
        signals["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["call_id"] == "call-snapshot-1"
                && item["from_actor"] == account_actor(&relay)),
        "imported call signal missing: {signals}"
    );
}
