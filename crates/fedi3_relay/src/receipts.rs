//! Signed relay statements and independently verifiable core acknowledgments.
use super::*;
use fedi3_protocol::receipts::{
    request_digest, sha256, CoreAcknowledgment, SignedEnvelope, CORE_ACK_HEADER,
    FORWARD_NONCE_HEADER,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct LogHead {
    pub sequence: u64,
    pub hash: String,
    pub anchor_sequence: u64,
    pub anchor_hash: String,
}
impl Default for LogHead {
    fn default() -> Self {
        Self {
            sequence: 0,
            hash: "0".repeat(64),
            anchor_sequence: 0,
            anchor_hash: "0".repeat(64),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct ReceiptEvent {
    pub id: String,
    pub timestamp_ms: i64,
    pub outcome: String,
    pub request_digest: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub core_ack: Option<SignedEnvelope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub core_public_key_pem: Option<String>,
}

pub(super) fn random_nonce() -> String {
    use rand::RngCore;
    let mut value = [0; 32];
    rand::rngs::OsRng.fill_bytes(&mut value);
    value.iter().map(|b| format!("{b:02x}")).collect()
}

pub(super) fn ttl() -> u64 {
    std::env::var("FEDI3_RELAY_RECEIPT_TTL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30 * 86400)
        .clamp(60, 365 * 86400)
}
fn max_rows() -> u64 {
    std::env::var("FEDI3_RELAY_RECEIPT_MAX_ROWS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000)
        .clamp(1000, 1_000_000)
}

pub(super) async fn head(state: &AppState) -> Result<LogHead> {
    state
        .db
        .relay_meta_get("forward_log_head")
        .await?
        .map(|s| serde_json::from_str(&s))
        .transpose()
        .map(|v| v.unwrap_or_default())
        .map_err(Into::into)
}

pub(super) async fn record(state: &AppState, user: &str, event: ReceiptEvent) -> Result<String> {
    let (public, secret) = state.db.load_or_create_signing_keypair_b64().await?;
    let secret: [u8; 32] = B64
        .decode(secret)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("bad signing key"))?;
    state
        .db
        .append_forward_receipt(
            user.to_string(),
            state
                .cfg
                .public_url
                .clone()
                .unwrap_or_else(|| format!("urn:fedi3:relay:{}", sha256(public.as_bytes()))),
            event,
            public,
            secret,
            ttl(),
            max_rows(),
        )
        .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn spooled(
    state: &AppState,
    user: &str,
    method: &Method,
    path: &str,
    query: &str,
    body: &[u8],
    headers: &HeaderMap,
) {
    let nonce = headers
        .get(FORWARD_NONCE_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_owned)
        .unwrap_or_else(random_nonce);
    let event = ReceiptEvent {
        id: nonce.clone(),
        timestamp_ms: now_ms(),
        outcome: "spooled".into(),
        request_digest: request_digest(method.as_str(), path, query, &nonce, body),
        status: 202,
        core_ack: None,
        core_public_key_pem: None,
    };
    if record(state, user, event).await.is_err() {
        error!("spool receipt could not be persisted");
    }
}

pub(super) fn checked_core_ack(
    encoded: &str,
    pem: &str,
    actor: &str,
    digest: &str,
    status: u16,
    response: &[u8],
) -> Result<SignedEnvelope> {
    anyhow::ensure!(encoded.len() <= 8192, "ack too large");
    let envelope: SignedEnvelope = serde_json::from_slice(&B64.decode(encoded)?)?;
    anyhow::ensure!(
        envelope.algorithm == "rsa-sha256" && envelope.key_id == format!("{actor}#main-key"),
        "unexpected core key"
    );
    let bytes = B64.decode(&envelope.payload_b64)?;
    let ack: CoreAcknowledgment = serde_json::from_slice(&bytes)?;
    anyhow::ensure!(
        ack.schema == "fedi3-core-forward-ack/v1"
            && ack.actor == actor
            && ack.request_digest == digest
            && ack.status == status
            && ack.response_digest == sha256(response)
            && ack.timestamp_ms.abs_diff(now_ms()) <= 300_000,
        "core binding mismatch"
    );
    anyhow::ensure!(
        verify_signature_rsa_sha256(
            pem,
            &String::from_utf8(bytes)?,
            &B64.decode(&envelope.signature_b64)?
        ),
        "invalid core signature"
    );
    Ok(envelope)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn forward(
    state: AppState,
    user: String,
    method: Method,
    path: &str,
    query: String,
    mut headers: HeaderMap,
    body: Bytes,
) -> Response {
    if matches!(method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return super::forward_to_user_inner(state, user, method, path, query, headers, body).await;
    }
    if !is_valid_username(&user) || !state.db.user_exists(&user).await.unwrap_or(false) {
        return super::forward_to_user_inner(state, user, method, path, query, headers, body).await;
    }
    if !state.db.is_user_enabled(&user).await.unwrap_or(false) {
        return (StatusCode::FORBIDDEN, "account unavailable").into_response();
    }
    let nonce = random_nonce();
    headers.insert(FORWARD_NONCE_HEADER, HeaderValue::from_str(&nonce).unwrap());
    let digest = request_digest(method.as_str(), path, &query, &nonce, &body);
    let response = super::forward_to_user_inner(
        state.clone(),
        user.clone(),
        method,
        path,
        query,
        headers,
        body,
    )
    .await;
    let mut status = response.status().as_u16();
    let core_encoded = response
        .headers()
        .get(CORE_ACK_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    // The transport response is already bounded; do not touch streaming GETs.
    let (mut parts, body) = response.into_parts();
    let bytes = match axum::body::to_bytes(body, state.cfg.max_body_bytes).await {
        Ok(bytes) => bytes,
        Err(_) => {
            status = 502;
            parts.status = StatusCode::BAD_GATEWAY;
            parts.headers.remove(header::CONTENT_LENGTH);
            Bytes::new()
        }
    };
    let mut ack = None;
    let mut key = None;
    if let Some(encoded) = core_encoded {
        if let Ok(Some(actor_json)) = state.db.get_actor_cache(&user).await {
            if let (Some(pem), Some(actor)) = (
                user_public_key_pem(&actor_json),
                serde_json::from_str::<serde_json::Value>(&actor_json)
                    .ok()
                    .and_then(|v| v.get("id")?.as_str().map(str::to_owned)),
            ) {
                if let Ok(valid) = checked_core_ack(&encoded, &pem, &actor, &digest, status, &bytes)
                {
                    ack = Some(valid);
                    key = Some(pem);
                }
            }
        }
    }
    let outcome = if ack.is_some() {
        if (200..300).contains(&status) {
            "core_acknowledged"
        } else {
            "core_rejected"
        }
    } else {
        "unverified_forward_response"
    };
    let event = ReceiptEvent {
        id: nonce,
        timestamp_ms: now_ms(),
        outcome: outcome.into(),
        request_digest: digest,
        status,
        core_ack: ack,
        core_public_key_pem: key,
    };
    match record(&state, &user, event).await {
        Ok(receipt) => {
            // The caller can retain the receipt even without an account on this relay.
            if let Ok(value) = HeaderValue::from_str(&B64.encode(receipt)) {
                parts.headers.insert("x-fedi3-forward-receipt", value);
            }
        }
        Err(_) => {
            error!("forward receipt could not be persisted");
            parts.headers.insert(
                "x-fedi3-receipt-status",
                HeaderValue::from_static("unavailable"),
            );
        }
    }
    Response::from_parts(parts, axum::body::Body::from(bytes))
}

#[derive(Deserialize)]
pub(super) struct ReceiptQuery {
    #[serde(default)]
    after: i64,
    limit: Option<i64>,
}
pub(super) async fn public_log(
    State(state): State<AppState>,
    Query(query): Query<ReceiptQuery>,
) -> Response {
    // Read the head after the page; trim any rows that expired during the read.
    let rows = state
        .db
        .forward_receipt_rows(
            None,
            query.after.max(0),
            query.limit.unwrap_or(100).clamp(1, 200),
        )
        .await;
    match (rows, head(&state).await) {
        (Ok(mut items), Ok(head)) => {
            items.retain(|r| {
                r["sequence"]
                    .as_u64()
                    .is_some_and(|seq| seq > head.anchor_sequence && seq <= head.sequence)
            });
            let checkpoint=super::operations::sign_checkpoint(&state,serde_json::json!({"schema":"fedi3-forward-log-head/v1","head":head,"generated_at_ms":now_ms()})).await;
            match checkpoint {
                Ok(checkpoint)=>([(header::CACHE_CONTROL,"no-store")],axum::Json(serde_json::json!({"schema":"fedi3-forward-log/v1","head":head,"checkpoint":checkpoint,"items":items}))).into_response(),
                Err(_)=>StatusCode::SERVICE_UNAVAILABLE.into_response(),
            }
        }
        _ => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}
pub(super) async fn user_receipts(
    State(state): State<AppState>,
    Path(user): Path<String>,
    headers: HeaderMap,
    Query(query): Query<ReceiptQuery>,
) -> Response {
    if !super::operations::authorized(&state, &user, &headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match state
        .db
        .forward_receipt_rows(
            Some(user),
            query.after.max(0),
            query.limit.unwrap_or(10).clamp(1, 10),
        )
        .await
    {
        Ok(items) => (
            [(header::CACHE_CONTROL, "no-store")],
            axum::Json(serde_json::json!({"items":items})),
        )
            .into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{test_cfg, test_state};

    fn event(outcome: &str) -> ReceiptEvent {
        ReceiptEvent {
            id: random_nonce(),
            timestamp_ms: now_ms(),
            outcome: outcome.into(),
            request_digest: "d".repeat(64),
            status: 202,
            core_ack: None,
            core_public_key_pem: None,
        }
    }
    #[tokio::test]
    async fn concurrent_receipts_form_a_single_verifiable_chain() {
        let (state, _dir) = test_state(test_cfg()).await;
        state
            .db
            .create_user("alice", "receipt-test-token")
            .await
            .unwrap();
        let jobs = (0..16).map(|_| {
            let state = state.clone();
            async move { record(&state, "alice", event("spooled")).await.unwrap() }
        });
        futures_util::future::join_all(jobs).await;
        let public = state.db.forward_receipt_rows(None, 0, 200).await.unwrap();
        let private = state
            .db
            .forward_receipt_rows(Some("alice".into()), 0, 200)
            .await
            .unwrap();
        let mut previous = "0".repeat(64);
        let (public_key, _) = state.db.load_or_create_signing_keypair_b64().await.unwrap();
        let key: [u8; 32] = B64.decode(public_key).unwrap().try_into().unwrap();
        let verifier = ed25519_dalek::VerifyingKey::from_bytes(&key).unwrap();
        for (n, (entry, receipt)) in public.iter().zip(private.iter()).enumerate() {
            assert_eq!(entry["sequence"], (n + 1) as u64);
            assert_eq!(entry["previous_hash"], previous);
            let payload = B64
                .decode(receipt["payload_b64"].as_str().unwrap())
                .unwrap();
            let signature = B64
                .decode(receipt["signature_b64"].as_str().unwrap())
                .unwrap();
            verifier
                .verify_strict(
                    &payload,
                    &ed25519_dalek::Signature::from_slice(&signature).unwrap(),
                )
                .unwrap();
            assert_eq!(entry["commitment"], sha256(&payload));
            assert_eq!(
                entry["hash"],
                fedi3_protocol::receipts::chain_hash((n + 1) as u64, &previous, &sha256(&payload))
            );
            let decoded: serde_json::Value = serde_json::from_slice(&payload).unwrap();
            assert_eq!(decoded["previous_hash"], previous);
            previous = entry["hash"].as_str().unwrap().to_string();
            assert!(entry.get("event").is_none());
            assert!(entry.get("username").is_none());
        }
        assert_eq!(public.len(), 16);
        assert_eq!(head(&state).await.unwrap().hash, previous);
        assert!(state
            .db
            .forward_receipt_rows(Some("bob".into()), 0, 200)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn idle_retention_preserves_anchor_and_next_append_extends_it() {
        let (state, dir) = test_state(test_cfg()).await;
        record(&state, "alice", event("spooled")).await.unwrap();
        let before = head(&state).await.unwrap();
        Connection::open(dir.join("state.db"))
            .unwrap()
            .execute("UPDATE forward_receipts SET created_at_ms=0", [])
            .unwrap();
        state.db.prune_forward_receipts(60).await.unwrap();
        let anchor = head(&state).await.unwrap();
        assert_eq!(anchor.anchor_hash, before.hash);
        assert_eq!(anchor.anchor_sequence, 1);
        assert!(state
            .db
            .forward_receipt_rows(None, 0, 100)
            .await
            .unwrap()
            .is_empty());
        record(&state, "alice", event("spooled")).await.unwrap();
        let items = state.db.forward_receipt_rows(None, 0, 100).await.unwrap();
        assert_eq!(items[0]["previous_hash"], anchor.anchor_hash);
        assert_eq!(items[0]["sequence"], 2);
    }

    #[tokio::test]
    async fn late_receipt_after_account_deletion_keeps_no_private_metadata() {
        let (state, _dir) = test_state(test_cfg()).await;
        state
            .db
            .create_user("alice", "receipt-test-token")
            .await
            .unwrap();
        record(&state, "alice", event("spooled")).await.unwrap();
        state.db.set_disabled("alice", true).await.unwrap();
        assert!(state.db.delete_user("alice").await.unwrap());
        record(&state, "alice", event("spooled")).await.unwrap();
        assert!(state
            .db
            .forward_receipt_rows(Some("alice".into()), 0, 100)
            .await
            .unwrap()
            .is_empty());
        let items = state.db.forward_receipt_rows(None, 0, 100).await.unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[1]["previous_hash"], items[0]["hash"]);
    }

    #[test]
    fn core_ack_rejects_forgery_body_changes_and_nonce_replay() {
        let secret = fedi3_protocol::crypto::rsa_generate_private_pem().unwrap();
        let pem = fedi3_protocol::crypto::rsa_public_pem(&secret).unwrap();
        let actor = "https://actor.example/users/alice";
        let digest = request_digest("POST", "/inbox", "", "nonce", b"body");
        let ack = CoreAcknowledgment {
            schema: "fedi3-core-forward-ack/v1".into(),
            actor: actor.into(),
            request_digest: digest.clone(),
            response_digest: sha256(b"ok"),
            status: 202,
            timestamp_ms: now_ms(),
        };
        let payload = serde_json::to_vec(&ack).unwrap();
        let sig = fedi3_core::http_sig::sign_bytes_rsa_sha256(&secret, &payload).unwrap();
        let envelope = SignedEnvelope {
            algorithm: "rsa-sha256".into(),
            payload_b64: B64.encode(payload),
            signature_b64: B64.encode(sig),
            key_id: format!("{actor}#main-key"),
        };
        let encoded = B64.encode(serde_json::to_vec(&envelope).unwrap());
        assert!(checked_core_ack(&encoded, &pem, actor, &digest, 202, b"ok").is_ok());
        assert!(checked_core_ack(&encoded, &pem, actor, &digest, 202, b"changed").is_err());
        assert!(checked_core_ack(
            &encoded,
            &pem,
            actor,
            &request_digest("POST", "/inbox", "", "other-nonce", b"body"),
            202,
            b"ok"
        )
        .is_err());
        assert!(checked_core_ack(
            &encoded,
            &pem,
            "https://evil.example/users/a",
            &digest,
            202,
            b"ok"
        )
        .is_err());
    }
}
