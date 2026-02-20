/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use axum::body::Body;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use bytes::Bytes;
use fedi3_protocol::{RelayHttpRequest, RelayHttpResponse};
use http::{HeaderMap, HeaderName, HeaderValue, Method, Request};
use tower::util::ServiceExt;

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
    let method = req.method.parse::<Method>().unwrap_or(Method::GET);
    let uri = format!("http://localhost{}{}", req.path, req.query);

    let mut request = Request::builder().method(method).uri(uri);
    for (k, v) in &req.headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            request = request.header(name, value);
        }
    }

    let body_bytes = B64.decode(req.body_b64.as_bytes()).unwrap_or_default();
    let request = request.body(Body::from(body_bytes)).unwrap();

    let resp = handler.clone().oneshot(request).await.unwrap();
    let status = resp.status();
    let headers_vec = headers_to_vec(resp.headers());

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap_or(Bytes::new());

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
        .filter_map(|(k, v)| v.to_str().ok().map(|vs| (k.to_string(), vs.to_string())))
        .collect()
}
