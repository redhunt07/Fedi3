//! A local client can recover media uploaded by another device of the same account.
mod common;
use common::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[tokio::test]
async fn missing_media_metadata_restores_bytes_and_survives_relay_outage() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let requests = hits.clone();
    let app = axum::Router::new().route(
        "/users/alice/media/voice.m4a",
        axum::routing::get(move || {
            requests.fetch_add(1, Ordering::Relaxed);
            async {
                (
                    [("Content-Type", "audio/mp4")],
                    b"persisted voice fixture".as_slice(),
                )
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let core = TestCore::with_config(|cfg| {
        cfg.public_base_url = base.clone();
        cfg.p2p_relay_fallback_secs = Some(0);
    })
    .unwrap();
    let db = fedi3_core::social_db::SocialDb::open(core.db_path()).unwrap();
    assert!(db.get_media("voice.m4a").unwrap().is_none());
    let first = core.get("/users/alice/media/voice.m4a").await;
    assert_eq!(first.status(), 200);
    assert_eq!(first.headers()["content-type"], "audio/mp4");
    assert_eq!(
        first.bytes().await.unwrap().as_ref(),
        b"persisted voice fixture"
    );
    let row = db.get_media("voice.m4a").unwrap().unwrap();
    assert_eq!(row.media_type, "audio/mp4");
    assert_eq!(row.size, 23);
    assert_eq!(row.local_name.as_deref(), Some("voice.m4a"));
    server.abort();
    let reopened = fedi3_core::social_db::SocialDb::open(core.db_path()).unwrap();
    assert!(reopened.get_media("voice.m4a").unwrap().is_some());
    let cached = core.get("/users/alice/media/voice.m4a").await;
    assert_eq!(cached.status(), 200);
    assert_eq!(
        cached.bytes().await.unwrap().as_ref(),
        b"persisted voice fixture"
    );
    assert_eq!(hits.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn missing_media_on_own_local_base_returns_404_without_recursing() {
    let core = TestCore::new().unwrap();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        core.get("/users/alice/media/missing.m4a"),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn p2p_only_does_not_restore_missing_media_from_relay() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let core = TestCore::with_config(|cfg| {
        cfg.public_base_url = base.clone();
        cfg.post_delivery_mode = Some("p2p_only".into());
        cfg.p2p_relay_fallback_secs = Some(0);
    })
    .unwrap();
    assert_eq!(
        core.get("/users/alice/media/missing.m4a").await.status(),
        404
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}
