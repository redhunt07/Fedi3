/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

//! H-03/H-04/H-46 hardening tests: bounded bodies on federation fetches and the SSRF guard.
//!
//! All tests in this binary serialize on `ENV_LOCK` because the SSRF escape hatch is a
//! process-wide environment variable.

mod common;

use fedi3_core::http_sig::KeyResolver;
use fedi3_core::net_policy::{MAX_ACTOR_DOC_BYTES, MAX_OBJECT_DOC_BYTES};
use serde_json::json;
use std::sync::Arc;

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn lock_env(allow_private: bool) -> tokio::sync::MutexGuard<'static, ()> {
    let guard = ENV_LOCK.lock().await;
    if allow_private {
        std::env::set_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH", "true");
    } else {
        std::env::remove_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH");
    }
    guard
}

/// A loopback HTTP server that always answers `/doc` with `body`.
struct BodyServer;

impl BodyServer {
    async fn spawn(body: String, port: u16) -> Self {
        let state = Arc::new(body);
        let app = axum::Router::new()
            .route(
                "/doc",
                axum::routing::get(
                    |axum::extract::State(b): axum::extract::State<Arc<String>>| async move {
                        (
                            [(
                                axum::http::header::CONTENT_TYPE,
                                "application/activity+json",
                            )],
                            (*b).clone(),
                        )
                    },
                ),
            )
            .with_state(state);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("bind test server");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Self
    }
}

fn actor_doc(actor_url: &str, pad: usize) -> String {
    json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": actor_url,
        "type": "Person",
        "preferredUsername": "bob",
        "publicKey": {
            "id": format!("{actor_url}#main-key"),
            "owner": actor_url,
            "publicKeyPem": common::shared_public_pem(),
        },
        "summary": "x".repeat(pad),
    })
    .to_string()
}

#[tokio::test]
async fn key_resolver_accepts_small_actor_document() {
    let _guard = lock_env(true).await;
    let port = common::free_port();
    let url = format!("http://127.0.0.1:{port}/doc");
    let _server = BodyServer::spawn(actor_doc(&url, 0), port).await;
    // The document id must match the fetched URL for the resolver to accept it.
    let resolver = KeyResolver::new();
    let summary = resolver
        .resolve_actor_summary_for_key_id(&format!("{url}#main-key"))
        .await
        .expect("small actor document must resolve");
    assert_eq!(summary.key_id, format!("{url}#main-key"));
    assert!(!summary.public_key_pem.is_empty());
}

#[tokio::test]
async fn key_resolver_rejects_oversized_actor_document() {
    let _guard = lock_env(true).await;
    let port = common::free_port();
    let url = format!("http://127.0.0.1:{port}/doc");
    let _server = BodyServer::spawn(actor_doc(&url, MAX_ACTOR_DOC_BYTES + 1024), port).await;
    let resolver = KeyResolver::new();
    let err = resolver
        .resolve_actor_summary_for_key_id(&format!("{url}#main-key"))
        .await
        .expect_err("oversized actor document must be rejected");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("too large") || msg.contains("exceeds"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn object_fetch_rejects_oversized_document() {
    let _guard = lock_env(true).await;
    let port = common::free_port();
    let url = format!("http://127.0.0.1:{port}/doc");
    let body = json!({
        "id": url,
        "type": "Note",
        "attributedTo": "http://127.0.0.1:1/users/bob",
        "content": "y".repeat(MAX_OBJECT_DOC_BYTES + 4096),
    })
    .to_string();
    let _server = BodyServer::spawn(body, port).await;
    let http = fedi3_core::net_policy::federation_client();
    let err = fedi3_core::object_fetch::fetch_one(&http, &None, &url)
        .await
        .expect_err("oversized object must not be buffered");
    let msg = format!("{err:#}");
    assert!(msg.contains("bytes"), "unexpected error: {msg}");
}

#[tokio::test]
async fn object_fetch_accepts_small_document() {
    let _guard = lock_env(true).await;
    let port = common::free_port();
    let url = format!("http://127.0.0.1:{port}/doc");
    let body = json!({
        "id": url,
        "type": "Note",
        "attributedTo": "http://127.0.0.1:1/users/bob",
        "content": "hello",
    })
    .to_string();
    let _server = BodyServer::spawn(body, port).await;
    let http = fedi3_core::net_policy::federation_client();
    let fetched = fedi3_core::object_fetch::fetch_one(&http, &None, &url)
        .await
        .expect("small object fetch");
    let (id, bytes, tombstone) = fetched.expect("object must be returned");
    assert_eq!(id, url);
    assert!(!bytes.is_empty());
    assert!(!tombstone);
}

#[tokio::test]
async fn object_fetch_missing_url_is_not_an_error() {
    let _guard = lock_env(true).await;
    let port = common::free_port();
    let _server = BodyServer::spawn(String::new(), port).await;
    let url = format!("http://127.0.0.1:{port}/missing");
    let http = fedi3_core::net_policy::federation_client();
    let fetched = fedi3_core::object_fetch::fetch_one(&http, &None, &url)
        .await
        .expect("404 must be treated as not-yet-fetchable");
    assert!(fetched.is_none());
}

#[tokio::test]
async fn key_resolver_rejects_loopback_when_guard_enabled() {
    let _guard = lock_env(false).await;
    let port = common::free_port();
    let url = format!("http://127.0.0.1:{port}/doc");
    let _server = BodyServer::spawn(actor_doc(&url, 0), port).await;
    let resolver = KeyResolver::new();
    let err = resolver
        .resolve_actor_summary_for_key_id(&format!("{url}#main-key"))
        .await
        .expect_err("loopback actor must be rejected when the guard is on");
    let msg = format!("{err:#}");
    assert!(msg.contains("non-public"), "unexpected error: {msg}");
}
