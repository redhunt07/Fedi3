/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

//! Shared test infrastructure: a real core on an ephemeral port, a mock remote actor
//! server, and an HTTP signer. No external services are required.
#![allow(dead_code, clippy::field_reassign_with_default, clippy::let_unit_value)]

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use http::{HeaderMap, Method, Uri};
use rsa::{
    pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding},
    RsaPrivateKey, RsaPublicKey,
};
use sha2::Digest as _;
use std::sync::OnceLock;

pub const INTERNAL_TOKEN: &str = "test-internal-token-0123456789abcdef";

pub fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let port = l.local_addr().expect("local addr").port();
    drop(l);
    port
}

/// Federation code rejects private addresses (SSRF guard) by default; the test harness runs
/// everything on loopback, so the escape hatch is enabled once per test binary.
pub fn ensure_test_env() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        std::env::set_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH", "true");
    });
}

/// A real core runtime on a temporary data dir and an ephemeral local port.
pub struct TestCore {
    pub port: u16,
    pub handle: u64,
    pub data_dir: fedi3_core::test_support::TempDir,
    pub username: String,
    pub base_url: String,
}

impl TestCore {
    pub fn new() -> anyhow::Result<Self> {
        Self::with_config(|_| {})
    }

    pub fn with_config(
        mut f: impl FnMut(&mut fedi3_core::runtime::CoreStartConfig),
    ) -> anyhow::Result<Self> {
        ensure_test_env();
        let dir = fedi3_core::test_support::TempDir::new("core");
        let port = free_port();
        let username = "alice".to_string();
        let base_url = format!("http://127.0.0.1:{port}");
        let mut cfg = fedi3_core::runtime::CoreStartConfig::default();
        cfg.username = username.clone();
        cfg.domain = "example.test".to_string();
        cfg.public_base_url = base_url.clone();
        cfg.relay_ws = String::new();
        cfg.relay_token = None;
        cfg.bind = format!("127.0.0.1:{port}");
        cfg.data_dir = Some(dir.path().to_string_lossy().to_string());
        cfg.internal_token = Some(INTERNAL_TOKEN.to_string());
        cfg.max_date_skew_secs = Some(300);
        cfg.max_body_bytes = Some(1024 * 1024);
        cfg.p2p = None;
        cfg.media = None;
        cfg.storage = None;
        f(&mut cfg);
        let handle = fedi3_core::runtime::start(cfg)?;
        Ok(Self {
            port,
            handle,
            data_dir: dir,
            username,
            base_url,
        })
    }

    pub fn actor_url(&self) -> String {
        format!("{}/users/{}", self.base_url, self.username)
    }

    pub fn db_path(&self) -> std::path::PathBuf {
        self.data_dir.path().join("fedi3.db")
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    pub fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            // Tests may reuse a connection after a long-running request; do not reuse pooled
            // connections so a server-side keep-alive close cannot reset a POST.
            .pool_max_idle_per_host(0)
            .build()
            .expect("client")
    }

    pub async fn get(&self, path: &str) -> reqwest::Response {
        Self::client()
            .get(self.url(path))
            .header("X-Fedi3-Internal", INTERNAL_TOKEN)
            .send()
            .await
            .expect("GET request")
    }

    pub async fn post_internal(&self, path: &str, body: &[u8]) -> reqwest::Response {
        Self::client()
            .post(self.url(path))
            .header("X-Fedi3-Internal", INTERNAL_TOKEN)
            .header("Content-Type", "application/activity+json")
            .body(body.to_vec())
            .send()
            .await
            .expect("POST request")
    }

    /// POST a signed activity to the local inbox.
    pub async fn post_inbox_signed(&self, signer: &Signer, body: &[u8]) -> reqwest::Response {
        signer
            .signed_post(&self.url("/inbox"), body, SignOptions::default())
            .send()
            .await
            .expect("inbox request")
    }
}

impl Drop for TestCore {
    fn drop(&mut self) {
        let _ = fedi3_core::runtime::stop(self.handle);
    }
}

/// Shared RSA keypair (generation is expensive; tests reuse it).
fn shared_keypair() -> &'static (String, String) {
    static KP: OnceLock<(String, String)> = OnceLock::new();
    KP.get_or_init(|| {
        let mut rng = rand::rngs::OsRng;
        let private = RsaPrivateKey::new(&mut rng, 2048).expect("rsa keygen");
        let private_pem = private
            .to_pkcs8_pem(LineEnding::LF)
            .expect("private pem")
            .to_string();
        let public_pem = RsaPublicKey::from(&private)
            .to_public_key_pem(LineEnding::LF)
            .expect("public pem")
            .to_string();
        (private_pem, public_pem)
    })
}

pub fn shared_private_pem() -> String {
    shared_keypair().0.clone()
}

pub fn shared_public_pem() -> String {
    shared_keypair().1.clone()
}

#[derive(Clone, Debug)]
pub struct SignOptions {
    pub signed_headers: Vec<String>,
    pub date_offset_secs: i64,
    pub tamper_signature: bool,
    pub extra_headers: Vec<(String, String)>,
}

impl Default for SignOptions {
    fn default() -> Self {
        Self {
            signed_headers: vec![
                "(request-target)".to_string(),
                "host".to_string(),
                "date".to_string(),
                "digest".to_string(),
            ],
            date_offset_secs: 0,
            tamper_signature: false,
            extra_headers: Vec::new(),
        }
    }
}

pub struct Signer {
    pub key_id: String,
    pub private_pem: String,
    pub public_pem: String,
}

impl Signer {
    pub fn new(key_id: impl Into<String>) -> Self {
        let (private_pem, public_pem) = shared_keypair().clone();
        Self {
            key_id: key_id.into(),
            private_pem,
            public_pem,
        }
    }

    pub fn signed_post(
        &self,
        url: &str,
        body: &[u8],
        opts: SignOptions,
    ) -> reqwest::RequestBuilder {
        let uri: Uri = url.parse().expect("valid url");
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            "application/activity+json".parse().expect("content-type"),
        );
        if let Some(auth) = uri.authority() {
            headers.insert("host", auth.as_str().parse().expect("host"));
        }
        let now = std::time::SystemTime::now();
        let delta = std::time::Duration::from_secs(opts.date_offset_secs.unsigned_abs());
        let date = if opts.date_offset_secs >= 0 {
            now + delta
        } else {
            now - delta
        };
        headers.insert("date", httpdate::fmt_http_date(date).parse().expect("date"));
        let digest = sha2::Sha256::digest(body);
        headers.insert(
            "digest",
            format!("SHA-256={}", B64.encode(digest))
                .parse()
                .expect("digest"),
        );
        for (k, v) in &opts.extra_headers {
            headers.insert(
                http::header::HeaderName::from_bytes(k.as_bytes()).expect("header name"),
                v.parse().expect("header value"),
            );
        }

        let signed_refs: Vec<&str> = opts.signed_headers.iter().map(|s| s.as_str()).collect();
        fedi3_core::http_sig::sign_request_rsa_sha256(
            &self.private_pem,
            &self.key_id,
            &Method::POST,
            &uri,
            &mut headers,
            body,
            &signed_refs,
        )
        .expect("sign");

        if opts.tamper_signature {
            let sig = headers
                .get("Signature")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            // Flip one base64 character in the signature value.
            let mut tampered = sig.clone();
            if let Some(pos) = tampered.find("signature=\"") {
                let idx = pos + "signature=\"".len();
                let bytes = unsafe { tampered.as_bytes_mut() };
                if idx < bytes.len() {
                    bytes[idx] = if bytes[idx] == b'A' { b'B' } else { b'A' };
                }
            }
            headers.insert("Signature", tampered.parse().expect("signature"));
        }

        let mut req = TestCore::client().post(url).body(body.to_vec());
        for (k, v) in headers.iter() {
            req = req.header(k, v);
        }
        req
    }

    /// GET with an HTTP Signature covering `(request-target)`, `host` and `date`.
    pub fn signed_get(&self, url: &str) -> reqwest::RequestBuilder {
        let uri: Uri = url.parse().expect("valid url");
        let mut headers = HeaderMap::new();
        if let Some(auth) = uri.authority() {
            headers.insert("host", auth.as_str().parse().expect("host"));
        }
        headers.insert(
            "date",
            httpdate::fmt_http_date(std::time::SystemTime::now())
                .parse()
                .expect("date"),
        );
        fedi3_core::http_sig::sign_request_rsa_sha256(
            &self.private_pem,
            &self.key_id,
            &Method::GET,
            &uri,
            &mut headers,
            b"",
            &["(request-target)", "host", "date"],
        )
        .expect("sign");
        let mut req = TestCore::client().get(url);
        for (k, v) in headers.iter() {
            req = req.header(k, v);
        }
        req
    }
}

/// A mock remote actor server: serves the actor document + public key so the core can
/// resolve signatures, and records requests received on its inbox.
pub struct MockActorServer {
    pub inbox_status: std::sync::Arc<std::sync::atomic::AtomicU16>,
    pub base_url: String,
    pub actor_url: String,
    pub key_id: String,
    pub username: String,
    shutdown: tokio::sync::watch::Sender<bool>,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl MockActorServer {
    pub async fn new(username: &str) -> anyhow::Result<Self> {
        ensure_test_env();
        let port = free_port();
        let base_url = format!("http://127.0.0.1:{port}");
        let actor_url = format!("{base_url}/users/{username}");
        let key_id = format!("{actor_url}#main-key");
        let (_, public_pem) = shared_keypair().clone();
        let actor = serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": actor_url,
            "type": "Person",
            "preferredUsername": username,
            "inbox": format!("{actor_url}/inbox"),
            "outbox": format!("{actor_url}/outbox"),
            "followers": format!("{actor_url}/followers"),
            "following": format!("{actor_url}/following"),
            "publicKey": {
                "id": key_id,
                "owner": actor_url,
                "publicKeyPem": public_pem,
            }
        });
        let actor_bytes = serde_json::to_vec(&actor)?;
        let inbox_status = std::sync::Arc::new(std::sync::atomic::AtomicU16::new(202));
        let inbox_status_for_router = inbox_status.clone();
        let (shutdown, mut rx) = tokio::sync::watch::channel(false);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
        let join = tokio::spawn(async move {
            let router = axum::Router::new().fallback(axum::routing::any(
                move |req: axum::http::Request<axum::body::Body>| {
                    let actor_bytes = actor_bytes.clone();
                    async move {
                        let path = req.uri().path();
                        let is_inbox_post =
                            req.method() == axum::http::Method::POST && path.ends_with("/inbox");
                        if is_inbox_post {
                            let status =
                                inbox_status_for_router.load(std::sync::atomic::Ordering::Relaxed);
                            axum::response::Response::builder()
                                .status(status)
                                .body(axum::body::Body::empty())
                                .unwrap()
                        } else if path.starts_with("/users/") {
                            axum::response::Response::builder()
                                .status(200)
                                .header("content-type", "application/activity+json")
                                .body(axum::body::Body::from(actor_bytes))
                                .unwrap()
                        } else if req.method() == axum::http::Method::POST {
                            axum::response::Response::builder()
                                .status(202)
                                .body(axum::body::Body::empty())
                                .unwrap()
                        } else {
                            axum::response::Response::builder()
                                .status(404)
                                .body(axum::body::Body::empty())
                                .unwrap()
                        }
                    }
                },
            ));
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = rx.changed().await;
                })
                .await
                .ok();
        });
        Ok(Self {
            inbox_status,
            base_url,
            actor_url,
            key_id,
            username: username.to_string(),
            shutdown,
            join: Some(join),
        })
    }
}

impl Drop for MockActorServer {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
        if let Some(j) = self.join.take() {
            j.abort();
        }
    }
}

/// Build a canonical Mastodon-style Create(Note) activity.
pub fn create_note(actor: &str, note_id: &str, content: &str) -> serde_json::Value {
    serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{actor}/statuses/{}", note_id.rsplit('/').next().unwrap_or("1")),
        "type": "Create",
        "actor": actor,
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
        "cc": [format!("{actor}/followers")],
        "object": {
            "id": note_id,
            "type": "Note",
            "attributedTo": actor,
            "content": content,
            "published": "2026-01-01T00:00:00Z",
            "to": ["https://www.w3.org/ns/activitystreams#Public"],
            "cc": [format!("{actor}/followers")]
        }
    })
}

/// Query a SQLite table directly (test assertions on persisted state).
pub fn sqlite_count(db_path: &std::path::Path, sql: &str, params: &[&dyn rusqlite::ToSql]) -> i64 {
    let conn = rusqlite::Connection::open(db_path).expect("open db");
    conn.query_row(sql, params, |r| r.get(0)).unwrap_or(0)
}

pub fn sqlite_exists(
    db_path: &std::path::Path,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> bool {
    sqlite_count(db_path, sql, params) > 0
}

pub fn json_body(v: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(v).expect("json")
}

pub fn decode_body(resp: &fedi3_protocol::RelayHttpResponse) -> Vec<u8> {
    B64.decode(resp.body_b64.as_bytes()).unwrap_or_default()
}

pub fn context(msg: &str) -> anyhow::Error {
    anyhow::anyhow!("{}", msg).context("test failure")
}
