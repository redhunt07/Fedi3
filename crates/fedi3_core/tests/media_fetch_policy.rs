//! Public media requests must neither fetch nor redirect clients to a private URL.
mod common;
use common::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[tokio::test]
async fn media_get_does_not_contact_or_redirect_to_private_addresses() {
    let core = TestCore::with_config(|cfg| {
        cfg.p2p_relay_fallback_secs = Some(0);
    })
    .unwrap();
    // This test binary contains only this policy test. Unlike federation fixtures,
    // it must run with the production SSRF guard enabled.
    std::env::remove_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = format!("http://{}/private", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let requests = hits.clone();
    let app = axum::Router::new().route(
        "/private",
        axum::routing::get(move || {
            let requests = requests.clone();
            async move {
                requests.fetch_add(1, Ordering::Relaxed);
                "private contents"
            }
        }),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let db = fedi3_core::social_db::SocialDb::open(core.db_path()).unwrap();
    db.upsert_media(
        "test-media.png",
        &target,
        "image/png",
        16,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    let response = core.get("/users/alice/media/test-media.png").await;
    assert_eq!(response.status(), 502);
    assert!(!response.headers().contains_key("location"));
    assert_eq!(hits.load(Ordering::Relaxed), 0);
    server.abort();
}
