//! Shared integration harness for the relay: spawns the real binary with an
//! ephemeral SQLite database and port, no Redis and no Meilisearch.

#![allow(dead_code)]

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::Value;

static DIR_ID: AtomicU64 = AtomicU64::new(1);

pub const ADMIN_TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef";

pub fn temp_dir(tag: &str) -> PathBuf {
    let id = DIR_ID.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("fedi3-relay-it-{tag}-{}-{id}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

pub struct TestRelay {
    child: Child,
    pub port: u16,
    pub dir: PathBuf,
    pub db_path: PathBuf,
    pub admin_token: String,
    pub client: reqwest::Client,
}

impl TestRelay {
    pub async fn start() -> Self {
        Self::start_with_env(&[]).await
    }

    pub async fn start_with_env(extra: &[(&str, &str)]) -> Self {
        let port = free_port();
        let dir = temp_dir("relay");
        let db_path = dir.join("relay.db");
        let media_dir = dir.join("media");
        let stderr_path = dir.join("stderr.log");
        let base_url = format!("http://127.0.0.1:{port}");

        let stderr = std::fs::File::create(&stderr_path).unwrap();
        let stdout_path = dir.join("stdout.log");
        let stdout = std::fs::File::create(&stdout_path).unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_fedi3_relay"));
        cmd.env_clear()
            .env("FEDI3_RELAY_BIND", format!("127.0.0.1:{port}"))
            .env("FEDI3_RELAY_ADMIN_TOKEN", ADMIN_TOKEN)
            .env("FEDI3_RELAY_DB_DRIVER", "sqlite")
            .env("FEDI3_RELAY_DB", &db_path)
            .env("FEDI3_RELAY_SEARCH_BACKEND", "db")
            .env("FEDI3_RELAY_ALLOW_SELF_REGISTER", "true")
            .env("FEDI3_RELAY_PUBLIC_URL", &base_url)
            .env("FEDI3_RELAY_ALLOW_PRIVATE_ACTOR_FETCH", "true")
            // The harness servers live on loopback, so the SSRF guard needs the
            // escape hatch unless a test explicitly overrides it to "false".
            .env("FEDI3_ALLOW_PRIVATE_REMOTE_FETCH", "true")
            .env("FEDI3_RELAY_MEDIA_DIR", &media_dir)
            .env("FEDI3_RELAY_RL_REGISTER_PER_MIN", "10000")
            .env("FEDI3_RELAY_RL_TUNNEL_PER_MIN", "10000")
            .env("FEDI3_RELAY_RL_INBOX_PER_MIN", "10000")
            .env("FEDI3_RELAY_RL_FORWARD_PER_MIN", "10000")
            .env("FEDI3_RELAY_RL_ADMIN_PER_MIN", "10000")
            .env("RUST_LOG", "warn")
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        for (k, v) in extra {
            cmd.env(k, v);
        }
        let child = cmd.spawn().expect("spawn fedi3_relay");

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap();

        let relay = Self {
            child,
            port,
            dir,
            db_path,
            admin_token: ADMIN_TOKEN.to_string(),
            client,
        };
        relay.wait_ready(&stderr_path).await;
        relay
    }

    async fn wait_ready(&self, stderr_path: &Path) {
        let url = format!("http://127.0.0.1:{}/healthz", self.port);
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline {
            if let Ok(resp) = self.client.get(&url).send().await {
                if resp.status().is_success() {
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let log = std::fs::read_to_string(stderr_path).unwrap_or_default();
        panic!(
            "relay did not become ready on port {}; stderr:\n{log}",
            self.port
        );
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn ws_url(&self, path: &str) -> String {
        format!("ws://127.0.0.1:{}{path}", self.port)
    }

    pub fn admin_bearer(&self) -> String {
        format!("Bearer {}", self.admin_token)
    }

    pub async fn get(&self, path: &str) -> reqwest::Response {
        self.client
            .get(format!("{}{path}", self.base_url()))
            .send()
            .await
            .unwrap()
    }

    pub async fn get_with(&self, path: &str, headers: &[(&str, &str)]) -> reqwest::Response {
        let mut req = self.client.get(format!("{}{path}", self.base_url()));
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        req.send().await.unwrap()
    }

    pub async fn post_json(&self, path: &str, body: &Value) -> reqwest::Response {
        self.client
            .post(format!("{}{path}", self.base_url()))
            .json(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn post_json_with(
        &self,
        path: &str,
        body: &Value,
        headers: &[(&str, &str)],
    ) -> reqwest::Response {
        let mut req = self
            .client
            .post(format!("{}{path}", self.base_url()))
            .json(body);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        req.send().await.unwrap()
    }

    pub async fn post_bytes_with(
        &self,
        path: &str,
        body: &[u8],
        headers: &[(&str, &str)],
    ) -> reqwest::Response {
        let mut req = self
            .client
            .post(format!("{}{path}", self.base_url()))
            .body(body.to_vec());
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        req.send().await.unwrap()
    }

    pub async fn delete_with(&self, path: &str, headers: &[(&str, &str)]) -> reqwest::Response {
        let mut req = self.client.delete(format!("{}{path}", self.base_url()));
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        req.send().await.unwrap()
    }

    pub fn db(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(&self.db_path).unwrap()
    }

    pub async fn register(&self, username: &str, token: &str) -> reqwest::Response {
        self.post_json(
            "/register",
            &serde_json::json!({ "username": username, "token": token }),
        )
        .await
    }

    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for TestRelay {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Run the relay binary with invalid configuration and return (exit code, stderr).
pub fn spawn_expect_failure(extra: &[(&str, &str)]) -> (Option<i32>, String) {
    let dir = temp_dir("cfg");
    let db_path = dir.join("relay.db");
    let port = free_port();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fedi3_relay"));
    cmd.env_clear()
        .env("FEDI3_RELAY_BIND", format!("127.0.0.1:{port}"))
        .env("FEDI3_RELAY_ADMIN_TOKEN", ADMIN_TOKEN)
        .env("FEDI3_RELAY_DB_DRIVER", "sqlite")
        .env("FEDI3_RELAY_DB", &db_path)
        .env("FEDI3_RELAY_SEARCH_BACKEND", "db")
        .env("FEDI3_RELAY_MEDIA_DIR", dir.join("media"))
        .env("RUST_LOG", "warn");
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let output = cmd.output().expect("run relay for config test");
    let _ = std::fs::remove_dir_all(&dir);
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

/// Minimal mock actor server: serves an actor document with an RSA public key.
pub struct MockActor {
    pub base_url: String,
    pub handle: tokio::task::JoinHandle<()>,
    body: std::sync::Arc<std::sync::RwLock<String>>,
}

impl MockActor {
    /// Replace the served actor document (needed because the actor URL, and
    /// therefore the keyId, is only known after the server has a port).
    pub fn set_actor_json(&self, actor_json: String) {
        *self.body.write().unwrap() = actor_json;
    }
}

pub async fn spawn_mock_actor(initial: String) -> MockActor {
    use axum::{routing::get, Router};

    let body = std::sync::Arc::new(std::sync::RwLock::new(initial));
    let served = body.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = Router::new().route(
        "/users/:user",
        get(move || {
            let served = served.clone();
            async move {
                let body = served.read().unwrap().clone();
                (
                    [(
                        axum::http::header::CONTENT_TYPE,
                        "application/activity+json",
                    )],
                    body,
                )
            }
        }),
    );
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    MockActor {
        base_url: format!("http://127.0.0.1:{port}"),
        handle,
        body,
    }
}

/// RSA signer producing ActivityPub-compatible `Signature`/`Digest` headers.
pub struct Signer {
    signing: rsa::pkcs1v15::SigningKey<sha2::Sha256>,
    pub actor_url: String,
    pub key_id: String,
    pub public_pem: String,
}

impl Signer {
    pub fn new(actor_base: &str, user: &str) -> Self {
        use rand::rngs::OsRng;
        use rsa::pkcs8::{EncodePublicKey, LineEnding};

        let private = rsa::RsaPrivateKey::new(&mut OsRng, 2048).unwrap();
        let public = private.to_public_key();
        let public_pem = public.to_public_key_pem(LineEnding::LF).unwrap();
        let actor_url = format!("{actor_base}/users/{user}");
        let key_id = format!("{actor_url}#main-key");
        let signing = rsa::pkcs1v15::SigningKey::<sha2::Sha256>::new(private);
        Self {
            signing,
            actor_url,
            key_id,
            public_pem,
        }
    }

    pub fn actor_json(&self, user: &str) -> String {
        serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": self.actor_url,
            "type": "Person",
            "preferredUsername": user,
            "inbox": format!("{}/inbox", self.actor_url),
            "outbox": format!("{}/outbox", self.actor_url),
            "publicKey": {
                "id": self.key_id,
                "owner": self.actor_url,
                "publicKeyPem": self.public_pem
            }
        })
        .to_string()
    }

    /// Build the headers for a signed request. `include_digest=false` lets tests
    /// exercise the mandatory-Digest rejection path.
    pub fn headers(
        &self,
        method: &str,
        path_and_query: &str,
        host: &str,
        body: &[u8],
        include_digest: bool,
    ) -> Vec<(String, String)> {
        use base64::engine::general_purpose::STANDARD as B64;
        use base64::Engine as _;
        use rsa::signature::{SignatureEncoding, Signer as _};
        use sha2::{Digest, Sha256};

        let date = httpdate::fmt_http_date(std::time::SystemTime::now());
        let digest = format!("SHA-256={}", B64.encode(Sha256::digest(body)));
        let signed_headers = if include_digest {
            "(request-target) host date digest"
        } else {
            "(request-target) host date"
        };
        let mut signing = format!(
            "(request-target): {} {}\nhost: {}\ndate: {}",
            method.to_ascii_lowercase(),
            path_and_query,
            host,
            date
        );
        if include_digest {
            signing.push_str(&format!("\ndigest: {digest}"));
        }
        let sig = self.signing.sign(signing.as_bytes());
        let signature_header = format!(
            "keyId=\"{}\",algorithm=\"rsa-sha256\",headers=\"{}\",signature=\"{}\"",
            self.key_id,
            signed_headers,
            B64.encode(sig.to_bytes())
        );
        let mut headers = vec![
            ("Signature".to_string(), signature_header),
            ("Date".to_string(), date),
            ("Host".to_string(), host.to_string()),
        ];
        if include_digest {
            headers.push(("Digest".to_string(), digest));
        }
        headers
    }
}

/// Minimal HTTP server that counts every request it receives. Used to prove
/// that relay-initiated fetches are skipped by the SSRF policy.
pub struct CountingServer {
    pub base_url: String,
    hits: std::sync::Arc<AtomicUsize>,
    handle: tokio::task::JoinHandle<()>,
}

impl CountingServer {
    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }
}

impl Drop for CountingServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub async fn spawn_counting_server() -> CountingServer {
    use axum::Router;

    let hits = std::sync::Arc::new(AtomicUsize::new(0));
    let served = hits.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    // A bare handler in `fallback` matches every method; the relay sync worker
    // issues GETs and the telemetry worker issues POSTs against the same URL.
    let app = Router::new().fallback(move || {
        let served = served.clone();
        async move {
            served.fetch_add(1, Ordering::Relaxed);
            (
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                r#"{"items":[],"next":null}"#,
            )
        }
    });
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    CountingServer {
        base_url: format!("http://127.0.0.1:{port}"),
        hits,
        handle,
    }
}
