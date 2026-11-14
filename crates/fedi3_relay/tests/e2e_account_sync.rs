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
