//! Public policy/checkpoints and authenticated, opt-in mobile/ICE provisioning.
use super::*;
use axum::Json;

pub(super) async fn relay_directory() -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "public, max-age=300"),
        ],
        include_str!("../../../relays/relays.json"),
    )
        .into_response()
}

pub(super) async fn authorized(state: &AppState, user: &str, headers: &HeaderMap) -> bool {
    if !is_valid_username(user) {
        return false;
    }
    let Some(token) = bearer_token(headers) else {
        return false;
    };
    state
        .db
        .verify_user_token(user, &token)
        .await
        .unwrap_or(false)
        && state.db.is_user_enabled(user).await.unwrap_or(false)
}

fn policy(state: &AppState) -> serde_json::Value {
    serde_json::json!({
        "ttl_secs": state.cfg.spool_ttl_secs,
        "max_rows_per_user": state.cfg.spool_max_rows_per_user,
        "max_body_bytes_per_user": state.cfg.spool_max_bytes_per_user,
        "overflow": "evict_oldest",
        "expiry": "not_delivered_after_ttl",
        "cleanup_interval_secs": state.cfg.webrtc_signal_cleanup_interval_secs.max(10),
        "acknowledgment": "core_http_success; not_a_read_receipt",
        "encryption": "RSA-OAEP/AES-256-GCM_when_actor_key_cached; otherwise_plaintext"
    })
}

pub(super) async fn transparency_json(State(state): State<AppState>) -> Response {
    let payload = serde_json::json!({
        "schema": "fedi3-relay-transparency/v1",
        "relay": state.cfg.public_url,
        "generated_at_ms": now_ms(),
        "online_tunnels": state.tunnels.read().await.len(),
        "spool": policy(&state),
        "push": {"configured": gateway_config().is_some(), "payload": "opaque_wakeup_only"},
        "receipt_log": match super::receipts::head(&state).await { Ok(head)=>head, Err(_)=>return StatusCode::SERVICE_UNAVAILABLE.into_response() },
        "receipt_retention_secs": super::receipts::ttl(),
        "receipt_scope": "relay_statement; core_acknowledged_requires_recipient_signature; not_a_read_receipt"
    });
    match sign_checkpoint(&state, payload).await {
        Ok(value) => ([(header::CACHE_CONTROL, "no-store")], Json(value)).into_response(),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "checkpoint unavailable").into_response(),
    }
}

pub(super) async fn sign_checkpoint(
    state: &AppState,
    payload: serde_json::Value,
) -> Result<serde_json::Value> {
    // Return exact bytes being signed; consumers need no JSON canonicalization.
    let bytes = serde_json::to_vec(&payload)?;
    let (public, secret) = state.db.load_or_create_signing_keypair_b64().await?;
    let secret: [u8; 32] = B64
        .decode(secret)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("bad key"))?;
    let signature = ed25519_dalek::SigningKey::from_bytes(&secret).sign(&bytes);
    Ok(
        serde_json::json!({"payload_b64": B64.encode(bytes), "public_key_b64": public,
        "algorithm": "ed25519", "signature_b64": B64.encode(signature.to_bytes())}),
    )
}

pub(super) async fn transparency_page(State(state): State<AppState>) -> Response {
    // All interpolated fields are integers. Never render operator or user HTML.
    axum::response::Html(format!(r#"<!doctype html><html lang="en"><meta charset="utf-8">
<title>Fedi3 relay transparency</title><h1>Relay transparency</h1>
<p>Offline queue retention: {} seconds. Expired items are never replayed.</p>
<p>Queue limits per account: {} items / {} body bytes. Overflow evicts oldest items.</p>
<p>Spool encryption requires a cached actor public key; otherwise items are stored in plaintext.</p>
<p>A successful core HTTP response acknowledges processing, not reading by a person.</p>
<p><a href="/_fedi3/relay/transparency">Download signed policy snapshot</a> (Ed25519).
Pin the relay key independently. This snapshot anchors the current forward log. A core acknowledgment proves an HTTP response, not reading by a person.</p>
<p><a href="/_fedi3/relay/forward-log">Public forward commitments</a>. Detailed receipts require account authentication. Retained log prefixes may expire; compare saved signed checkpoints to detect inconsistent history.</p>
<p>Mobile wakeups require an explicitly configured push gateway; OS background execution is best effort.</p></html>"#,
        state.cfg.spool_ttl_secs, state.cfg.spool_max_rows_per_user, state.cfg.spool_max_bytes_per_user)).into_response()
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WebPushKeys {
    pub_key: String,
    auth: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PushRegistration {
    provider: String,
    token: String,
    #[serde(default)]
    expires_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    webpush_keys: Option<WebPushKeys>,
}

fn gateway_config() -> Option<(String, String)> {
    let url = std::env::var("FEDI3_RELAY_PUSH_GATEWAY_URL").ok()?;
    let url = reqwest::Url::parse(&url).ok()?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let token = std::env::var("FEDI3_RELAY_PUSH_GATEWAY_TOKEN").ok()?;
    if token.len() < 32 {
        return None;
    }
    Some((url.to_string(), token))
}

fn providers() -> Vec<String> {
    std::env::var("FEDI3_RELAY_PUSH_PROVIDERS")
        .unwrap_or_else(|_| "unifiedpush,apns".into())
        .split(',')
        .map(str::trim)
        .filter(|s| matches!(*s, "unifiedpush" | "apns"))
        .map(str::to_owned)
        .collect()
}
fn vapid() -> Option<String> {
    std::env::var("FEDI3_RELAY_PUSH_VAPID_PUBLIC_KEY")
        .ok()
        .filter(|key| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(key)
                .is_ok_and(|bytes| bytes.len() == 65 && bytes[0] == 4)
        })
}
pub(super) async fn push_config() -> Response {
    ([(header::CACHE_CONTROL,"no-store")],Json(serde_json::json!({"enabled":gateway_config().is_some(),"providers":providers(),"vapid_public_key":vapid()}))).into_response()
}
pub(super) async fn push_status(
    State(state): State<AppState>,
    Path(user): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !authorized(&state, &user, &headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match state.db.spool_stats(&user).await {
        Ok((items, _)) => (
            [(header::CACHE_CONTROL, "no-store")],
            Json(serde_json::json!({"pending_items":items})),
        )
            .into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub(super) async fn push_register(
    State(state): State<AppState>,
    Path(user): Path<String>,
    headers: HeaderMap,
    Json(mut registration): Json<PushRegistration>,
) -> Response {
    if !authorized(&state, &user, &headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if gateway_config().is_none() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "push gateway not configured",
        )
            .into_response();
    }
    if !matches!(registration.provider.as_str(), "unifiedpush" | "apns")
        || registration.token.is_empty()
        || registration.token.len() > 2048
        || registration.token.chars().any(char::is_control)
    {
        return (StatusCode::BAD_REQUEST, "invalid registration").into_response();
    }
    if !providers().contains(&registration.provider) {
        return (StatusCode::SERVICE_UNAVAILABLE, "provider disabled").into_response();
    }
    if registration.provider == "unifiedpush" {
        if ensure_fetch_target_allowed(&registration.token, false)
            .await
            .is_err()
            || !registration.token.starts_with("https://")
        {
            return (StatusCode::BAD_REQUEST, "invalid push endpoint").into_response();
        }
        // Web Push distributors return subscription keys; topic-style
        // distributors (e.g. ntfy) accept the plain JSON wakeup and omit them.
        if let Some(keys) = registration.webpush_keys.as_ref() {
            let decode = |s: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s);
            let valid = decode(&keys.pub_key).is_ok_and(|b| b.len() == 65 && b[0] == 4)
                && decode(&keys.auth).is_ok_and(|b| b.len() == 16);
            if !valid {
                return (StatusCode::BAD_REQUEST, "invalid webpush encryption keys").into_response();
            }
        }
    } else if registration.token.len() < 32
        || registration.token.len() > 512
        || registration.token.len() % 2 != 0
        || !registration.token.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return (StatusCode::BAD_REQUEST, "invalid APNs token").into_response();
    }
    // One current installation per account; replacement is atomic in relay_meta.
    registration.expires_at_ms = now_ms().saturating_add(30 * 24 * 3600 * 1000);
    let raw = serde_json::to_string(&registration).expect("registration JSON");
    if state
        .db
        .relay_meta_set(&format!("push:{user}"), &raw)
        .await
        .is_err()
    {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({"expires_at_ms": registration.expires_at_ms})),
    )
        .into_response()
}

pub(super) async fn push_unregister(
    State(state): State<AppState>,
    Path(user): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !authorized(&state, &user, &headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match state.db.relay_meta_set(&format!("push:{user}"), "").await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub(super) async fn retry_push_worker(state: AppState) {
    let mut tick = tokio::time::interval(Duration::from_secs(60));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut cursor = String::new();
    loop {
        tokio::select! { _=state.shutdown.cancelled()=>break, _=tick.tick()=>{} }
        if gateway_config().is_none() {
            continue;
        }
        let cutoff =
            now_ms().saturating_sub(state.cfg.spool_ttl_secs.min(30 * 86400) as i64 * 1000);
        let users = match state.db.pending_push_users(cursor.clone(), cutoff).await {
            Ok(users) => users,
            Err(_) => continue,
        };
        if users.is_empty() {
            cursor.clear();
            continue;
        }
        cursor = users.last().cloned().unwrap_or_default();
        let jobs = futures_util::stream::iter(users)
            .map(|user| {
                let state = state.clone();
                async move {
                    if !state.tunnels.read().await.contains_key(&user) {
                        wake_user(&state, &user).await;
                    }
                }
            })
            .buffer_unordered(8)
            .collect::<Vec<_>>();
        tokio::select! {_=state.shutdown.cancelled()=>break,_=jobs=>{}}
    }
}

pub(super) fn notify_user(state: &AppState, user: &str) {
    if gateway_config().is_none() {
        return;
    }
    let Ok(permit) = state.push_tasks.clone().try_acquire_owned() else {
        return;
    };
    let state = state.clone();
    let user = user.to_string();
    tokio::spawn(async move {
        let _permit = permit;
        tokio::select! {_=state.shutdown.cancelled()=>{},_=tokio::time::timeout(Duration::from_secs(4),wake_user_inner(&state,&user))=>{}}
    });
}
pub(super) async fn wake_user(state: &AppState, user: &str) {
    let Ok(_permit) = state.push_tasks.clone().try_acquire_owned() else {
        return;
    };
    let _ = tokio::time::timeout(Duration::from_secs(4), wake_user_inner(state, user)).await;
}
async fn wake_user_inner(state: &AppState, user: &str) {
    let Some((url, token)) = gateway_config() else {
        return;
    };
    if !state
        .limiter
        .check(user.to_string(), "push_wakeup", 1)
        .await
    {
        return;
    }
    // Queue contents, actor identities and relay credentials never enter the wakeup.
    let raw = state
        .db
        .relay_meta_get(&format!("push:{user}"))
        .await
        .ok()
        .flatten();
    let Some(registration) = raw.and_then(|s| serde_json::from_str::<PushRegistration>(&s).ok())
    else {
        return;
    };
    if registration.expires_at_ms <= now_ms() {
        return;
    }
    let client = fedi3_protocol::net::guard_client(reqwest::Client::builder(), false)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(4))
        .build();
    let Ok(client) = client else {
        return;
    };
    if ensure_fetch_target_allowed(&url, false).await.is_err() {
        return;
    }
    // The gateway owns provider credentials, endpoint allowlisting and coalescing.
    let result = client
        .post(url)
        .bearer_auth(token)
        .json(&serde_json::json!({
            "provider": registration.provider, "token": registration.token,
            "event": "sync", "ttl_secs": state.cfg.spool_ttl_secs.min(86400),
            "webpush_keys": registration.webpush_keys
        }))
        .send()
        .await;
    if !matches!(result, Ok(ref r) if r.status().is_success()) {
        warn!("mobile wakeup gateway unavailable; spool retained");
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IceServer {
    urls: Vec<String>,
    username: String,
    credential: String,
}

fn validate_pool(pool: &[IceServer]) -> bool {
    !pool.is_empty()
        && pool.len() <= 8
        && pool.iter().all(|s| {
            !s.urls.is_empty()
                && s.urls.len() <= 4
                && !s.username.is_empty()
                && !s.credential.is_empty()
                && s.username.len() <= 256
                && s.credential.len() <= 1024
                && s.urls.iter().all(|u| {
                    u.len() <= 512
                        && !u.chars().any(char::is_whitespace)
                        && (u.starts_with("turn:") || u.starts_with("turns:"))
                        && !u.contains('@')
                })
        })
}

/// Builds a single-operator ICE pool from a relay host and credentials.
fn build_turn_pool(
    host: &str,
    port: u16,
    username: String,
    credential: String,
    explicit: Vec<String>,
) -> Option<Vec<IceServer>> {
    if host.is_empty() || username.is_empty() || credential.is_empty() {
        return None;
    }
    let urls = if explicit.is_empty() {
        vec![
            format!("turn:{host}:{port}?transport=udp"),
            format!("turn:{host}:{port}?transport=tcp"),
        ]
    } else {
        explicit
    };
    let pool = vec![IceServer {
        urls,
        username,
        credential,
    }];
    validate_pool(&pool).then_some(pool)
}

/// Builds the ICE pool from relay environment variables when no pool file is
/// mounted. Operators already configure coturn credentials for the `turn`
/// compose profile, so publishing them here lets clients provision TURN without
/// hand-typed URLs — which is what makes calls work across NAT/VPN.
fn turn_pool_from_env(cfg: &RelayConfig) -> Option<Vec<IceServer>> {
    let username = std::env::var("FEDI3_RELAY_TURN_USERNAME")
        .ok()?
        .trim()
        .to_string();
    let credential = std::env::var("FEDI3_RELAY_TURN_CREDENTIAL")
        .ok()?
        .trim()
        .to_string();
    if username.is_empty() || credential.is_empty() {
        return None;
    }
    let explicit = std::env::var("FEDI3_RELAY_TURN_URLS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let public_url = cfg.public_url.as_deref()?;
    let uri = http::Uri::try_from(public_url).ok()?;
    let host = uri.host()?.to_string();
    let port = std::env::var("FEDI3_RELAY_TURN_PORT")
        .ok()
        .and_then(|value| value.trim().parse::<u16>().ok())
        .unwrap_or(3478);
    build_turn_pool(&host, port, username, credential, explicit)
}

pub(super) async fn turn_pool(
    State(state): State<AppState>,
    Path(user): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !authorized(&state, &user, &headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(path) = std::env::var_os("FEDI3_RELAY_TURN_POOL_FILE") else {
        return match turn_pool_from_env(&state.cfg) {
            Some(pool) => ([(header::CACHE_CONTROL, "no-store")], Json(pool)).into_response(),
            None => StatusCode::NO_CONTENT.into_response(),
        };
    };
    let pool = async {
        // Bounded even for a misconfigured file (do not read it all first).
        use tokio::io::AsyncReadExt;
        let file = tokio::fs::File::open(path).await?;
        let mut bytes = Vec::new();
        file.take(65537).read_to_end(&mut bytes).await?;
        anyhow::ensure!(bytes.len() <= 65536, "pool file too large");
        let pool: Vec<IceServer> = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(validate_pool(&pool), "invalid pool");
        Ok::<_, anyhow::Error>(pool)
    }
    .await;
    match pool {
        Ok(pool) => ([(header::CACHE_CONTROL, "no-store")], Json(pool)).into_response(),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "TURN pool unavailable").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{test_cfg, test_state};

    #[tokio::test]
    async fn checkpoint_is_verifiable_and_excludes_private_metadata() {
        let (state, _dir) = test_state(test_cfg()).await;
        let payload = serde_json::json!({"schema":"test", "spool":policy(&state)});
        let checkpoint = sign_checkpoint(&state, payload).await.unwrap();
        let public: [u8; 32] = B64
            .decode(checkpoint["public_key_b64"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        let signature = B64
            .decode(checkpoint["signature_b64"].as_str().unwrap())
            .unwrap();
        let bytes = B64
            .decode(checkpoint["payload_b64"].as_str().unwrap())
            .unwrap();
        let key = ed25519_dalek::VerifyingKey::from_bytes(&public).unwrap();
        key.verify_strict(
            &bytes,
            &ed25519_dalek::Signature::from_slice(&signature).unwrap(),
        )
        .unwrap();
        let mut altered = bytes.clone();
        altered[0] ^= 1;
        assert!(key
            .verify_strict(
                &altered,
                &ed25519_dalek::Signature::from_slice(&signature).unwrap()
            )
            .is_err());
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(value.get("users").is_none());
        assert!(value.get("tokens").is_none());
    }

    #[tokio::test]
    async fn provisioning_requires_enabled_account_token() {
        let (state, _dir) = test_state(test_cfg()).await;
        assert!(!authorized(&state, "alice", &HeaderMap::new()).await);
        state
            .db
            .create_user("alice", "test-token-with-at-least-16-chars")
            .await
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer test-token-with-at-least-16-chars"),
        );
        assert!(authorized(&state, "alice", &headers).await);
        state.db.set_disabled("alice", true).await.unwrap();
        assert!(!authorized(&state, "alice", &headers).await);
    }

    #[test]
    fn automatic_pool_derives_udp_and_tcp_turn_for_the_relay_host() {
        let pool = build_turn_pool(
            "relay.example",
            3478,
            "relay".into(),
            "secret".into(),
            Vec::new(),
        )
        .expect("derived pool");
        assert_eq!(pool.len(), 1);
        assert_eq!(
            pool[0].urls,
            vec![
                "turn:relay.example:3478?transport=udp".to_string(),
                "turn:relay.example:3478?transport=tcp".to_string()
            ]
        );
        assert_eq!(pool[0].username, "relay");
        assert_eq!(pool[0].credential, "secret");
    }

    #[test]
    fn automatic_pool_needs_host_and_credentials() {
        assert!(build_turn_pool("", 3478, "relay".into(), "secret".into(), Vec::new()).is_none());
        assert!(build_turn_pool(
            "relay.example",
            3478,
            "relay".into(),
            String::new(),
            Vec::new()
        )
        .is_none());
        let explicit = build_turn_pool(
            "relay.example",
            3478,
            "relay".into(),
            "secret".into(),
            vec!["turns:turn.example:5349".into()],
        )
        .expect("explicit pool");
        assert_eq!(explicit[0].urls, vec!["turns:turn.example:5349"]);
    }

    #[test]
    fn pool_keeps_operator_credentials_separate_and_rejects_bad_input() {
        let pool: Vec<IceServer> = serde_json::from_value(serde_json::json!([
            {"urls":["turn:one.example:3478?transport=udp"],"username":"one","credential":"secret-one"},
            {"urls":["turns:two.example:5349"],"username":"two","credential":"secret-two"}
        ])).unwrap();
        assert!(validate_pool(&pool));
        let mut bad = pool;
        bad[1].urls = vec!["https://private.example".into()];
        assert!(!validate_pool(&bad));
    }
}
