//! Two independent cores with a real relay: complete bootstrap, offline changes and tombstones.
mod common;
use common::TestRelay;
use fedi3_core::runtime::{self, CoreStartConfig};
use serde_json::json;
use std::time::{Duration, Instant};
fn debug_panics() {
    std::panic::set_hook(Box::new(|info| eprintln!("{info}")));
}
struct Core(u64);
impl Drop for Core {
    fn drop(&mut self) {
        let _ = runtime::stop(self.0);
    }
}
fn port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
fn cfg(relay: &TestRelay, dir: &std::path::Path, pem: &str, bind: u16) -> CoreStartConfig {
    CoreStartConfig {
        username: "alice".into(),
        domain: "127.0.0.1".into(),
        public_base_url: relay.base_url(),
        relay_ws: relay.ws_url(""),
        relay_token: Some("account-test-token-0123456789".into()),
        bind: format!("127.0.0.1:{bind}"),
        data_dir: Some(dir.display().to_string()),
        identity_private_key_pem: Some(pem.into()),
        internal_token: Some("test-internal-token".into()),
        display_name: Some("Alice original".into()),
        ..Default::default()
    }
}
async fn until(mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(45);
    while !check() {
        assert!(Instant::now() < deadline, "account replication deadline");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
fn count(path: &std::path::Path, table: &str) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn complete_account_converges_across_two_cores_and_offline_restart() {
    let relay = TestRelay::start_with_env(&[("FEDI3_RELAY_DIRECTORY_URLS", "")]).await;
    let a = fedi3_core::test_support::TempDir::new("account-a");
    let b = fedi3_core::test_support::TempDir::new("account-b");
    let pem = fedi3_protocol::crypto::rsa_generate_private_pem().unwrap();
    let ca = cfg(&relay, a.path(), &pem, port());
    let cb = cfg(&relay, b.path(), &pem, port());
    let actor = format!("{}/users/alice", relay.base_url());
    let a_core = Core(runtime::start(ca.clone()).unwrap());
    debug_panics();
    let adb = a.path().join("fedi3.db");
    let bdb = b.path().join("fedi3.db");
    eprintln!("waiting for core A initialization");
    until(|| {
        rusqlite::Connection::open(&adb).ok().is_some_and(|c| {
            c.query_row("SELECT EXISTS(SELECT 1 FROM account_profile)", [], |r| {
                r.get::<_, bool>(0)
            })
            .unwrap_or(false)
        })
    })
    .await;
    {
        let mut c = rusqlite::Connection::open(&adb).unwrap();
        let tx = c.transaction().unwrap();
        for n in 0..140 {
            tx.execute(
                "INSERT INTO followers VALUES(?1,1)",
                [format!("https://follower.example/{n}")],
            )
            .unwrap();
        }
        tx.execute(
            "INSERT INTO following VALUES('https://followed.example/user',1,1)",
            [],
        )
        .unwrap();
        for n in 0..90 {
            let id = format!("{actor}/objects/{n}");
            let note=serde_json::to_vec(&json!({"id":id,"type":"Note","attributedTo":actor,"content":format!("durable post {n}")})).unwrap();
            tx.execute("INSERT INTO objects(object_id,created_at_ms,updated_at_ms,deleted,object_json,pinned,actor_id,size_bytes,last_access_ms) VALUES(?1,1,1,0,?2,0,?3,1,1)",rusqlite::params![id,note,actor]).unwrap();
            tx.execute("INSERT INTO outbox_items VALUES(?1,1,?2,'public')",rusqlite::params![format!("activity-{n}"),serde_json::to_vec(&json!({"id":format!("activity-{n}"),"type":"Create","actor":actor,"object":json!({"id":id,"type":"Note","attributedTo":actor,"content":"post"})})).unwrap()]).unwrap();
        }
        tx.execute("UPDATE account_profile SET data=?1 WHERE id=1",[json!({"display_name":"Replicated Alice","summary":"Biography survives","icon_url":"https://media.example/avatar.png","profile_fields":[],"blocked_domains":[],"blocked_actors":[],"manually_approves_followers":false}).to_string()]).unwrap();
        tx.commit().unwrap();
    }
    let b_core = Core(runtime::start(cb.clone()).unwrap());
    debug_panics();
    eprintln!("waiting for graph bootstrap");
    until(|| {
        count(&bdb, "followers") == 140
            && count(&bdb, "following") == 1
            && count(&bdb, "outbox_items") == 90
            && count(&bdb, "objects") == 90
    })
    .await;
    until(|| {
        rusqlite::Connection::open(&bdb)
            .unwrap()
            .query_row("SELECT data FROM account_profile", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap()
            .contains("Replicated Alice")
    })
    .await;
    let c = rusqlite::Connection::open(&bdb).unwrap();
    let raw: String = c
        .query_row("SELECT data FROM account_profile", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&raw).unwrap()["display_name"],
        "Replicated Alice"
    );
    assert_eq!(count(&bdb, "objects"), 90);
    drop(b_core);
    let c = rusqlite::Connection::open(&adb).unwrap();
    c.execute("DELETE FROM following", []).unwrap();
    c.execute(
        "DELETE FROM followers WHERE actor_id='https://follower.example/0'",
        [],
    )
    .unwrap();
    c.execute(
        "UPDATE objects SET deleted=1 WHERE object_id=?1",
        [format!("{actor}/objects/0")],
    )
    .unwrap();
    tokio::time::sleep(Duration::from_secs(6)).await;
    let _b_core = Core(runtime::start(cb.clone()).unwrap());
    debug_panics();
    eprintln!("waiting for offline deletions");
    until(|| count(&bdb, "following") == 0 && count(&bdb, "followers") == 139).await;
    until(|| {
        rusqlite::Connection::open(&bdb)
            .unwrap()
            .query_row(
                "SELECT deleted FROM objects WHERE object_id=?1",
                [format!("{actor}/objects/0")],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
            == 1
    })
    .await;
    assert_eq!(
        fedi3_protocol::crypto::rsa_public_pem(
            &std::fs::read_to_string(b.path().join("identity_private_key.pem")).unwrap()
        )
        .unwrap(),
        fedi3_protocol::crypto::rsa_public_pem(&pem).unwrap()
    );
    // Possession of a relay token without the pinned private key cannot replace the account.
    let bad = fedi3_protocol::crypto::rsa_generate_private_pem().unwrap();
    let outsider = fedi3_core::test_support::TempDir::new("account-wrong-key");
    let _wrong = Core(runtime::start(cfg(&relay, outsider.path(), &bad, port())).unwrap());
    debug_panics();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let response = relay
        .client
        .get(format!("{}/users/alice", relay.base_url()))
        .header("Accept", "application/activity+json")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let profile: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        profile["publicKey"]["publicKeyPem"],
        fedi3_protocol::crypto::rsa_public_pem(&pem).unwrap()
    );
    drop(a_core);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reset_device_keeps_old_contact_offline_without_disconnects_between_paired_devices() {
    let relay = TestRelay::start_with_env(&[("FEDI3_RELAY_DIRECTORY_URLS", "")]).await;
    let a = fedi3_core::test_support::TempDir::new("session-desktop");
    let b = fedi3_core::test_support::TempDir::new("session-old-phone");
    let c = fedi3_core::test_support::TempDir::new("session-reset-phone");
    let pem = fedi3_protocol::crypto::rsa_generate_private_pem().unwrap();
    let start = |path: &std::path::Path| {
        let mut config = cfg(&relay, path, &pem, port());
        config.p2p = Some(fedi3_core::p2p::P2pConfig {
            enable: true,
            mdns_enable: Some(false),
            public_discovery_enable: Some(false),
            discovery_enable: Some(false),
            gossip_enable: Some(false),
            sync_enable: Some(false),
            listen: Some("/ip4/127.0.0.1/tcp/0".into()),
            ..Default::default()
        });
        Core(runtime::start(config).unwrap())
    };
    let desktop = start(a.path());
    let phone = start(b.path());
    let snapshot = || async {
        let data: serde_json::Value = relay
            .client
            .get(format!("{}/_fedi3/relay/peers", relay.base_url()))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let items = data["items"].as_array().unwrap();
        (
            items.len(),
            items.iter().filter(|p| p["online"] == true).count(),
        )
    };
    let deadline = Instant::now() + Duration::from_secs(45);
    while snapshot().await != (2, 2) {
        assert!(
            Instant::now() < deadline,
            "paired devices did not both stay online"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    for _ in 0..4 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(snapshot().await, (2, 2));
    }
    drop(phone);
    let reset_phone = start(c.path());
    let deadline = Instant::now() + Duration::from_secs(45);
    while snapshot().await != (3, 2) {
        assert!(
            Instant::now() < deadline,
            "reset phone did not replace its old online state"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    drop(reset_phone);
    let deadline = Instant::now() + Duration::from_secs(15);
    while snapshot().await != (3, 1) {
        assert!(Instant::now() < deadline, "disconnected phone still online");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    drop(desktop);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn personal_follow_notification_reaches_both_devices_and_alert_survives_restart_once() {
    std::env::set_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH", "true");
    let relay = TestRelay::start_with_env(&[("FEDI3_RELAY_DIRECTORY_URLS", "")]).await;
    let a = fedi3_core::test_support::TempDir::new("notifications-a");
    let b = fedi3_core::test_support::TempDir::new("notifications-b");
    let sender = fedi3_core::test_support::TempDir::new("notifications-sender");
    let pem = fedi3_protocol::crypto::rsa_generate_private_pem().unwrap();
    let mut ca = cfg(&relay, a.path(), &pem, port());
    let mut cb = cfg(&relay, b.path(), &pem, port());
    ca.p2p = Some(fedi3_core::p2p::P2pConfig {
        enable: false,
        device_sync_enable: Some(true),
        ..Default::default()
    });
    cb.p2p = ca.p2p.clone();
    let _a_core = Core(runtime::start(ca.clone()).unwrap());
    let b_core = Core(runtime::start(cb.clone()).unwrap());
    let mut cs = cfg(
        &relay,
        sender.path(),
        &fedi3_protocol::crypto::rsa_generate_private_pem().unwrap(),
        port(),
    );
    cs.username = "bob".into();
    cs.p2p = ca.p2p.clone();
    let _sender = Core(runtime::start(cs.clone()).unwrap());
    let client = reqwest::Client::new();
    let actor = format!("{}/users/alice", relay.base_url());
    tokio::time::sleep(Duration::from_millis(400)).await;
    let response = client
        .post(format!("http://{}/_fedi3/social/follow", cs.bind))
        .header("X-Fedi3-Internal", "test-internal-token")
        .json(&json!({"actor":actor}))
        .send()
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "{}",
        response.text().await.unwrap()
    );
    let adb = a.path().join("fedi3.db");
    let bdb = b.path().join("fedi3.db");
    until(|| {
        count(&adb, "personal_notifications") == 1 && count(&bdb, "personal_notifications") == 1
    })
    .await;
    for config in [&ca, &cb] {
        let body: serde_json::Value = client
            .get(format!("http://{}/_fedi3/notifications", config.bind))
            .header("X-Fedi3-Internal", "test-internal-token")
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["items"][0]["activity"]["type"], "Follow");
    }
    until(|| count(&sender.path().join("fedi3.db"), "personal_notifications") == 1).await;
    let accepted: serde_json::Value = client
        .get(format!("http://{}/_fedi3/notifications", cs.bind))
        .header("X-Fedi3-Internal", "test-internal-token")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(accepted["items"][0]["activity"]["type"], "Accept");
    assert_eq!(
        client
            .post(format!("http://{}/_fedi3/notifications/claim", ca.bind))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );
    // Do not open SSE or claim on B before restarting: startup must retain the alert.
    let claim = |bind: String| {
        let client = client.clone();
        async move {
            client
                .post(format!("http://{bind}/_fedi3/notifications/claim"))
                .header("X-Fedi3-Internal", "test-internal-token")
                .send()
                .await
                .unwrap()
                .json::<serde_json::Value>()
                .await
                .unwrap()
        }
    };
    assert_eq!(
        claim(ca.bind.clone()).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        claim(ca.bind.clone()).await["items"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    drop(b_core);
    let _b_core = Core(runtime::start(cb.clone()).unwrap());
    assert_eq!(
        claim(cb.bind.clone()).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        claim(cb.bind.clone()).await["items"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let spool: serde_json::Value = client
        .get(format!("{}/_fedi3/chat/spool?user=alice", relay.base_url()))
        .bearer_auth("account-test-token-0123456789")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(spool["notifications_cursor"].as_i64().unwrap() > 0);
    // An older client must never receive an unsupported notification record.
    let old: serde_json::Value = client
        .get(format!("{}/_fedi3/account/alice/state", relay.base_url()))
        .bearer_auth("account-test-token-0123456789")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(old["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row[1]["kind"] != "personal_notifications"));
}
