use fedi3_core::{
    runtime::{self, CoreStartConfig},
    social_db::SocialDb,
};
fn port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
fn config(base: &std::path::Path, domain: &str) -> CoreStartConfig {
    CoreStartConfig {
        username: "alice".into(),
        domain: domain.into(),
        public_base_url: format!("https://{domain}"),
        relay_ws: String::new(),
        relay_token: None,
        bind: format!("127.0.0.1:{}", port()),
        data_dir_base: Some(base.display().to_string()),
        ..Default::default()
    }
}
#[test]
fn equal_usernames_on_different_relays_do_not_share_storage_and_legacy_data_survives() {
    let root = fedi3_core::test_support::TempDir::new("account-scoped-storage");
    let legacy = root.path().join("alice");
    std::fs::create_dir_all(&legacy).unwrap();
    let db = SocialDb::open(legacy.join("fedi3.db")).unwrap();
    db.set_local_meta("public_base_url", "https://one.example")
        .unwrap();
    db.add_follower("https://friend.example/actor").unwrap();
    let original = fedi3_core::keys::load_or_generate_identity(&legacy).unwrap();
    let cfg = config(root.path(), "one.example");
    let handle = runtime::start(cfg.clone()).unwrap();
    runtime::stop(handle).unwrap();
    assert_eq!(
        std::fs::read_to_string(legacy.join("identity_private_key.pem")).unwrap(),
        original.private_key_pem
    );
    let other = config(root.path(), "two.example");
    let handle = runtime::start(other).unwrap();
    runtime::stop(handle).unwrap();
    assert_eq!(
        std::fs::read_to_string(legacy.join("identity_private_key.pem")).unwrap(),
        original.private_key_pem
    );
    assert_eq!(db.list_followers(100, None).unwrap().total, 1);
    let dirs = std::fs::read_dir(root.path().join("accounts"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(dirs.len(), 1);
    assert_ne!(
        std::fs::read_to_string(dirs[0].path().join("identity_private_key.pem")).unwrap(),
        original.private_key_pem
    );
    let mut wrong = cfg;
    wrong.bind = format!("127.0.0.1:{}", port());
    wrong.identity_private_key_pem =
        Some(fedi3_protocol::crypto::rsa_generate_private_pem().unwrap());
    assert!(runtime::start(wrong).is_err());
    assert_eq!(
        std::fs::read_to_string(legacy.join("identity_private_key.pem")).unwrap(),
        original.private_key_pem
    );
}
