use fedi3_core::{
    social_db::{ChatMessage, SocialDb},
    test_support::TempDir,
};

#[test]
fn accepted_chat_is_atomic_and_retry_is_idempotent() {
    let dir = TempDir::new("chat-acceptance");
    let db = SocialDb::open(dir.path().join("test.db")).unwrap();
    db.create_chat_thread("thread", "group", None).unwrap();
    let msg = ChatMessage {
        message_id: "request-1".into(),
        thread_id: "thread".into(),
        sender_actor: "https://me.example/users/me".into(),
        sender_device: "device".into(),
        created_at_ms: 1,
        edited_at_ms: None,
        deleted: false,
        body_json: "{\"text\":\"hello\"}".into(),
    };
    let recipients = vec!["https://other.example/users/friend".to_string()];
    assert!(db.accept_outgoing_chat(&msg, &recipients).unwrap());
    assert!(!db.accept_outgoing_chat(&msg, &recipients).unwrap());
    assert_eq!(
        db.list_chat_messages("thread", 10, None)
            .unwrap()
            .items
            .len(),
        1
    );
    assert_eq!(db.due_chat_deliveries(100, 10).unwrap().len(), 1);
    let mut changed = msg.clone();
    changed.body_json = "different payload".into();
    assert!(db.accept_outgoing_chat(&changed, &recipients).is_err());
    let conn = rusqlite::Connection::open(dir.path().join("test.db")).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_delivery BEFORE INSERT ON chat_pending_deliveries BEGIN SELECT RAISE(ABORT,'queue unavailable'); END;").unwrap();
    let mut rollback = msg.clone();
    rollback.message_id = "request-rollback".into();
    assert!(db.accept_outgoing_chat(&rollback, &recipients).is_err());
    assert!(db
        .get_chat_message_body("request-rollback")
        .unwrap()
        .is_none());
}
