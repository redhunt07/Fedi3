/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use axum::body::Body;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use bytes::Bytes;
use fedi3_protocol::{RelayHttpRequest, RelayHttpResponse};
use http::{HeaderMap, HeaderName, HeaderValue, Method, Request, Uri};
use tower::util::ServiceExt;

/// Hard cap on relay request bodies (defence in depth; the relay caps uploads at 64 MiB).
pub const MAX_RELAY_BODY_BYTES: usize = 16 * 1024 * 1024;

fn error_response(id: String, status: u16, message: &str) -> RelayHttpResponse {
    RelayHttpResponse {
        id,
        status,
        headers: vec![(
            "content-type".to_string(),
            "text/plain; charset=utf-8".to_string(),
        )],
        body_b64: B64.encode(message.as_bytes()),
    }
}

fn is_hop_by_hop(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

/// Convert a relay request into a local HTTP request without panicking on remote input.
/// Malformed paths/queries produce a 400 response instead of unwinding the tunnel/P2P task.
pub async fn handle_relay_http_request<S>(
    handler: &mut S,
    req: RelayHttpRequest,
) -> RelayHttpResponse
where
    S: tower::Service<
            Request<Body>,
            Response = http::Response<Body>,
            Error = std::convert::Infallible,
        > + Clone,
{
    let id = req.id.clone();

    let method = match req.method.trim().parse::<Method>() {
        Ok(v) => v,
        Err(_) => return error_response(id, 400, "invalid method"),
    };

    let raw_path = req.path.trim();
    if raw_path.is_empty() {
        return error_response(id, 400, "empty path");
    }
    let path = if raw_path.starts_with('/') {
        raw_path.to_string()
    } else {
        format!("/{raw_path}")
    };
    let query = req.query.trim().trim_start_matches('?');
    let path_and_query = if query.is_empty() {
        path
    } else {
        format!("{path}?{query}")
    };
    let uri = match Uri::builder()
        .scheme("http")
        .authority("localhost")
        .path_and_query(path_and_query.as_str())
        .build()
    {
        Ok(v) => v,
        Err(e) => return error_response(id, 400, &format!("invalid path: {e}")),
    };

    let body_bytes = match B64.decode(req.body_b64.as_bytes()) {
        Ok(v) => v,
        Err(_) => return error_response(id, 400, "invalid base64 body"),
    };
    if body_bytes.len() > MAX_RELAY_BODY_BYTES {
        return error_response(id, 413, "body too large");
    }

    let mut request = Request::builder().method(method).uri(uri);
    for (k, v) in &req.headers {
        if is_hop_by_hop(k) {
            continue;
        }
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            request = request.header(name, value);
        }
    }

    let request = match request.body(Body::from(body_bytes)) {
        Ok(v) => v,
        Err(e) => return error_response(id, 400, &format!("invalid request: {e}")),
    };

    let resp = match handler.clone().oneshot(request).await {
        Ok(v) => v,
        Err(_) => return error_response(id, 400, "handler error"),
    };
    let status = resp.status();
    let headers_vec = headers_to_vec(resp.headers());

    let body = axum::body::to_bytes(resp.into_body(), MAX_RELAY_BODY_BYTES)
        .await
        .unwrap_or_else(|_| Bytes::new());

    RelayHttpResponse {
        id: req.id,
        status: status.as_u16(),
        headers: headers_vec,
        body_b64: B64.encode(body),
    }
}

fn headers_to_vec(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter(|(k, _)| !is_hop_by_hop(k.as_str()))
        .filter_map(|(k, v)| v.to_str().ok().map(|vs| (k.to_string(), vs.to_string())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::service_fn;

    fn req(path: &str, query: &str) -> RelayHttpRequest {
        RelayHttpRequest {
            id: "r1".to_string(),
            method: "GET".to_string(),
            path: path.to_string(),
            query: query.to_string(),
            headers: Vec::new(),
            body_b64: String::new(),
        }
    }

    fn ok_handler() -> impl tower::Service<
        Request<Body>,
        Response = http::Response<Body>,
        Error = std::convert::Infallible,
    > + Clone {
        service_fn(|r: Request<Body>| async move {
            let pq = r
                .uri()
                .path_and_query()
                .map(|v| v.as_str().to_string())
                .unwrap_or_default();
            Ok::<_, std::convert::Infallible>(
                http::Response::builder()
                    .status(200)
                    .body(Body::from(pq))
                    .unwrap(),
            )
        })
    }

    #[tokio::test]
    async fn path_without_leading_slash_is_normalized() {
        let mut h = ok_handler();
        let resp = handle_relay_http_request(&mut h, req("users/a", "")).await;
        assert_eq!(resp.status, 200);
        let body = B64.decode(resp.body_b64).unwrap();
        assert_eq!(body, b"/users/a");
    }

    #[tokio::test]
    async fn query_without_question_mark_is_normalized() {
        let mut h = ok_handler();
        let resp = handle_relay_http_request(&mut h, req("/users/a", "x=1&y=2")).await;
        assert_eq!(resp.status, 200);
        let body = B64.decode(resp.body_b64).unwrap();
        assert_eq!(body, b"/users/a?x=1&y=2");
    }

    #[tokio::test]
    async fn invalid_path_returns_400_without_panic() {
        let mut h = ok_handler();
        let resp = handle_relay_http_request(&mut h, req("/users/a b", "")).await;
        assert_eq!(resp.status, 400);
    }

    #[tokio::test]
    async fn hop_by_hop_headers_are_filtered() {
        let mut h = ok_handler();
        let mut r = req("/x", "");
        r.headers = vec![
            ("Connection".to_string(), "keep-alive".to_string()),
            ("Transfer-Encoding".to_string(), "chunked".to_string()),
            ("X-Test".to_string(), "1".to_string()),
        ];
        let resp = handle_relay_http_request(&mut h, r).await;
        assert_eq!(resp.status, 200);
        assert!(!resp
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("connection")));
        assert!(!resp
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("transfer-encoding")));
    }

    #[tokio::test]
    async fn oversized_body_is_rejected() {
        let mut h = ok_handler();
        let mut r = req("/x", "");
        r.method = "POST".to_string();
        r.body_b64 = B64.encode(vec![0u8; MAX_RELAY_BODY_BYTES + 1]);
        let resp = handle_relay_http_request(&mut h, r).await;
        assert_eq!(resp.status, 413);
    }
}
