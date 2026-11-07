//! A core signature proves its HTTP handler returned a particular response.
use crate::ap::ApState;
use axum::body::Body;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use fedi3_protocol::receipts::*;
use http::{Request, Response};

pub async fn handle(state: &ApState, request: Request<Body>) -> Response<Body> {
    let nonce = request
        .headers()
        .get(FORWARD_NONCE_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    if matches!(
        *request.method(),
        http::Method::GET | http::Method::HEAD | http::Method::OPTIONS
    ) || nonce.len() != 64
        || !nonce.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return crate::ap::handle_request(state, request).await;
    }
    let (parts, body) = request.into_parts();
    let bytes = match axum::body::to_bytes(body, crate::relay_bridge::MAX_RELAY_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => return Response::builder().status(413).body(Body::empty()).unwrap(),
    };
    let digest = request_digest(
        parts.method.as_str(),
        parts.uri.path(),
        parts.uri.query().unwrap_or(""),
        &nonce,
        &bytes,
    );
    let response =
        crate::ap::handle_request(state, Request::from_parts(parts, Body::from(bytes))).await;
    let (mut parts, body) = response.into_parts();
    let bytes = match axum::body::to_bytes(body, crate::relay_bridge::MAX_RELAY_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => return Response::builder().status(502).body(Body::empty()).unwrap(),
    };
    let actor = crate::ap::canonical_actor_url(&state.cfg);
    let acknowledgment = CoreAcknowledgment {
        schema: "fedi3-core-forward-ack/v1".into(),
        actor: actor.clone(),
        request_digest: digest,
        response_digest: sha256(&bytes),
        status: parts.status.as_u16(),
        timestamp_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64,
    };
    if let Ok(payload) = serde_json::to_vec(&acknowledgment) {
        if let Ok(signature) =
            crate::http_sig::sign_bytes_rsa_sha256(&state.private_key_pem, &payload)
        {
            let envelope = SignedEnvelope {
                algorithm: "rsa-sha256".into(),
                payload_b64: B64.encode(payload),
                signature_b64: B64.encode(signature),
                key_id: format!("{actor}#main-key"),
            };
            if let Ok(value) =
                http::HeaderValue::from_str(&B64.encode(serde_json::to_vec(&envelope).unwrap()))
            {
                parts.headers.insert(CORE_ACK_HEADER, value);
            }
        }
    }
    Response::from_parts(parts, Body::from(bytes))
}
