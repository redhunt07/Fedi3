//! Discover includes cached public federation posts as well as P2P gossip.
mod common;
use common::*;
use serde_json::{json, Value};

#[tokio::test]
async fn discovery_merges_public_sources_without_private_history_or_pagination_gaps() {
    let core = TestCore::new().unwrap();
    let db = fedi3_core::social_db::SocialDb::open(core.db_path()).unwrap();
    for (id, p2p, federated) in [("a", true, false), ("b", true, true), ("c", false, true)] {
        let activity = serde_json::to_vec(&json!({
            "id": format!("https://remote.test/activities/{id}"), "type":"Create",
            "actor":"https://remote.test/users/author",
            "to":["https://www.w3.org/ns/activitystreams#Public"],
            "object":{"id":format!("https://remote.test/notes/{id}"),"type":"Note","content":id}
        }))
        .unwrap();
        let id = format!("https://remote.test/activities/{id}");
        if p2p {
            db.insert_global_feed_item(&id, None, activity.clone())
                .unwrap();
        }
        if federated {
            db.insert_federated_feed_item(&id, None, activity).unwrap();
        }
    }
    let conn = rusqlite::Connection::open(core.db_path()).unwrap();
    conn.execute("UPDATE global_feed SET created_at_ms=42", [])
        .unwrap();
    conn.execute("UPDATE federated_feed SET created_at_ms=42", [])
        .unwrap();
    let private = serde_json::to_vec(&json!({"id":"private","type":"Create","object":{"id":"private-note","type":"Note","content":"private"}})).unwrap();
    conn.execute("INSERT INTO inbox_items(activity_id,created_at_ms,actor_id,type,activity_json) VALUES ('private-inbox',99,'private-actor','Create',?1)", [&private]).unwrap();
    conn.execute("INSERT INTO outbox_items(id,created_at_ms,activity_json,visibility) VALUES ('private-outbox',99,?1,'direct')", [&private]).unwrap();
    let first: Value = core
        .get("/_fedi3/timeline/discover?limit=2")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(first["total"], 3);
    assert_eq!(first["items"].as_array().unwrap().len(), 2);
    assert_eq!(first["items"][0]["object"]["content"], "c");
    assert_eq!(first["items"][1]["object"]["content"], "b");
    let cursor = urlencoding::encode(first["next"].as_str().unwrap());
    let second: Value = core
        .get(&format!(
            "/_fedi3/timeline/discover?limit=2&cursor={cursor}"
        ))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(second["total"], 3);
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert_eq!(second["items"][0]["object"]["content"], "a");
    assert!(second["next"].is_null());
    assert!(!serde_json::to_string(&first).unwrap().contains("private"));
    assert!(!serde_json::to_string(&second).unwrap().contains("private"));
    assert_eq!(
        core.get("/_fedi3/timeline/discover?cursor=invalid")
            .await
            .status(),
        400
    );
    assert_eq!(
        TestCore::client()
            .get(core.url("/_fedi3/timeline/discover"))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
}
