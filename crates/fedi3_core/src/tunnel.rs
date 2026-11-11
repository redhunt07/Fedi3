/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use crate::net_metrics::NetMetrics;
use crate::relay_bridge::handle_relay_http_request;
use axum::body::Body;
use fedi3_protocol::{RelayHttpRequest, RelayHttpResponse};
use futures_util::{SinkExt, StreamExt};
use http::Request;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite;
use tracing::{error, info, warn};

/// Reconnection policy for the relay tunnel. Defaults are production-safe; tests use shorter
/// values via `TunnelOptions`.
#[derive(Clone, Copy, Debug)]
pub struct TunnelOptions {
    pub connect_timeout: std::time::Duration,
    pub pong_timeout: std::time::Duration,
    pub ping_interval: std::time::Duration,
    pub backoff_base: std::time::Duration,
    pub backoff_max: std::time::Duration,
    /// A connection that stayed up at least this long resets the backoff counter.
    pub stable_after: std::time::Duration,
}

impl Default for TunnelOptions {
    fn default() -> Self {
        Self {
            connect_timeout: std::time::Duration::from_secs(10),
            pong_timeout: std::time::Duration::from_secs(30),
            ping_interval: std::time::Duration::from_secs(5),
            backoff_base: std::time::Duration::from_secs(1),
            backoff_max: std::time::Duration::from_secs(300),
            stable_after: std::time::Duration::from_secs(60),
        }
    }
}

pub async fn run_tunnel<C>(
    user: &str,
    relay_ws_url: &str,
    relay_token: &str,
    handler: C,
) -> anyhow::Result<()>
where
    C: Clone
        + Send
        + 'static
        + tower::Service<
            Request<Body>,
            Response = http::Response<Body>,
            Error = std::convert::Infallible,
        >,
    C::Future: Send + 'static,
{
    let (_tx, rx) = watch::channel(false);
    run_tunnel_with_shutdown(
        user,
        relay_ws_url,
        relay_token,
        handler,
        rx,
        std::sync::Arc::new(NetMetrics::new()),
    )
    .await
}

pub async fn run_tunnel_with_shutdown<C>(
    user: &str,
    relay_ws_url: &str,
    relay_token: &str,
    handler: C,
    shutdown: watch::Receiver<bool>,
    metrics: std::sync::Arc<NetMetrics>,
) -> anyhow::Result<()>
where
    C: Clone
        + Send
        + 'static
        + tower::Service<
            Request<Body>,
            Response = http::Response<Body>,
            Error = std::convert::Infallible,
        >,
    C::Future: Send + 'static,
{
    run_tunnel_with_options(
        user,
        relay_ws_url,
        relay_token,
        handler,
        shutdown,
        metrics,
        TunnelOptions::default(),
    )
    .await
}

pub async fn run_tunnel_with_options<C>(
    user: &str,
    relay_ws_url: &str,
    relay_token: &str,
    handler: C,
    mut shutdown: watch::Receiver<bool>,
    metrics: std::sync::Arc<NetMetrics>,
    options: TunnelOptions,
) -> anyhow::Result<()>
where
    C: Clone
        + Send
        + 'static
        + tower::Service<
            Request<Body>,
            Response = http::Response<Body>,
            Error = std::convert::Infallible,
        >,
    C::Future: Send + 'static,
{
    crate::init_tls_provider();
    use tungstenite::client::IntoClientRequest;
    let url = format!("{relay_ws_url}/tunnel/{user}");

    let mut attempt: u32 = 0;
    loop {
        if *shutdown.borrow() {
            break;
        }

        info!(%url, attempt, "connecting tunnel");
        let mut request = url.as_str().into_client_request()?;
        request
            .headers_mut()
            .insert("Authorization", format!("Bearer {relay_token}").parse()?);
        let connect = tokio::time::timeout(
            options.connect_timeout,
            tokio_tungstenite::connect_async(request),
        )
        .await;
        let connected_at = std::time::Instant::now();
        match connect {
            Err(_) => {
                metrics.set_relay_error("tunnel connect timeout".to_string());
                warn!(
                    "tunnel connect timed out after {:?}",
                    options.connect_timeout
                );
            }
            Ok(Err(e)) => {
                metrics.set_relay_error(e.to_string());
                warn!("tunnel connect failed: {e}");
            }
            Ok(Ok((ws, _))) => {
                metrics.set_relay_connected(true);
                info!("tunnel connected");
                let reason = run_connection(
                    ws,
                    &mut shutdown,
                    handler.clone(),
                    metrics.clone(),
                    &options,
                )
                .await;
                metrics.set_relay_connected(false);

                if *shutdown.borrow() {
                    break;
                }
                match reason {
                    Ok(()) => {
                        // Connection closed cleanly by the relay: reconnect after backoff.
                        metrics.set_relay_error("relay closed the tunnel".to_string());
                        warn!("relay closed the tunnel; reconnecting");
                    }
                    Err(e) => {
                        metrics.set_relay_error(e.to_string());
                        warn!("tunnel connection lost: {e}");
                    }
                }

                // Reset the backoff only after a connection that stayed up long enough.
                if connected_at.elapsed() >= options.stable_after {
                    attempt = 0;
                }
            }
        }

        attempt = attempt.saturating_add(1);
        let delay = backoff_delay(attempt, &options);
        info!("tunnel reconnect in {:?}", delay);
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = shutdown.changed() => {
                if *shutdown.borrow() { break; }
            }
        }
    }

    metrics.set_relay_connected(false);
    Ok(())
}

/// Outbound websocket frames produced by the tunnel.
enum TunnelOut {
    Text(String),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close,
}

async fn run_connection<S>(
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    shutdown: &mut watch::Receiver<bool>,
    handler: S,
    metrics: std::sync::Arc<NetMetrics>,
    options: &TunnelOptions,
) -> anyhow::Result<()>
where
    S: tower::Service<
            Request<Body>,
            Response = http::Response<Body>,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    let (mut ws_tx, mut ws_rx) = ws.split();
    // All writes go through a single writer task: requests are handled concurrently (a nested
    // federation fetch must never block the tunnel read loop, otherwise two peers fetching each
    // other's keys deadlock until the relay timeout), while the websocket sink stays single-owner.
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<TunnelOut>(128);
    let writer = tokio::spawn(async move {
        while let Some(out) = out_rx.recv().await {
            let msg = match out {
                TunnelOut::Text(t) => tungstenite::Message::Text(t),
                TunnelOut::Ping(p) => tungstenite::Message::Ping(p),
                TunnelOut::Pong(p) => tungstenite::Message::Pong(p),
                TunnelOut::Close => tungstenite::Message::Close(None),
            };
            if let Err(e) = ws_tx.send(msg).await {
                error!("tunnel write failed: {e}");
                break;
            }
        }
    });

    // The tunnel ping is the main radio wakeup while the app is backgrounded:
    // a 5 s heartbeat keeps the phone's modem busy all day. Stretch it when
    // the UI is away; a pong timeout above the interval keeps the watchdog
    // meaningful without dropping a healthy tunnel.
    fn scaled(
        base: std::time::Duration,
        background_secs: u64,
        doze_secs: u64,
    ) -> std::time::Duration {
        let secs = match crate::power::mode() {
            crate::power::FOREGROUND => return base,
            crate::power::BACKGROUND => background_secs,
            _ => doze_secs,
        };
        std::time::Duration::from_secs(secs).max(base)
    }
    let mut next_ping = tokio::time::Instant::now() + scaled(options.ping_interval, 30, 60);
    let mut last_pong = std::time::Instant::now();

    let mut result: anyhow::Result<()> = Ok(());
    let capacity = std::sync::Arc::new(tokio::sync::Semaphore::new(64));
    let mut handlers = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = handlers.join_next(), if !handlers.is_empty() => {},
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    let _ = out_tx.send(TunnelOut::Close).await;
                    break;
                }
            }
            _ = tokio::time::sleep_until(next_ping) => {
                next_ping = tokio::time::Instant::now() + scaled(options.ping_interval, 30, 60);
                let pong_timeout = scaled(options.pong_timeout, 60, 120);
                // Watchdog: a half-open tunnel that never answers pings must be replaced.
                if last_pong.elapsed() > pong_timeout {
                    result = Err(anyhow::anyhow!(
                        "no pong for {:?} (watchdog)",
                        last_pong.elapsed()
                    ));
                    break;
                }
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let payload = now_ms.to_be_bytes().to_vec();
                if out_tx.send(TunnelOut::Ping(payload)).await.is_err() {
                    result = Err(anyhow::anyhow!("tunnel writer stopped"));
                    break;
                }
            }
            msg = ws_rx.next() => {
                let Some(msg) = msg else {
                    break;
                };
                let msg = match msg {
                    Ok(m) => m,
                    Err(e) => {
                        result = Err(anyhow::anyhow!("tunnel read failed: {e}"));
                        break;
                    }
                };
                let text = match msg {
                    tungstenite::Message::Text(t) => t,
                    tungstenite::Message::Pong(p) => {
                        last_pong = std::time::Instant::now();
                        if p.len() == 8 {
                            let mut a = [0u8; 8];
                            a.copy_from_slice(&p);
                            let sent_ms = u64::from_be_bytes(a);
                            let now_ms = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as u64;
                            metrics.relay_rtt_update(now_ms.saturating_sub(sent_ms));
                        }
                        continue;
                    }
                    tungstenite::Message::Ping(p) => {
                        let _ = out_tx.send(TunnelOut::Pong(p)).await;
                        continue;
                    }
                    tungstenite::Message::Close(_) => break,
                    _ => continue,
                };
                metrics.relay_rx_add(text.as_bytes().len() as u64);
                let req: RelayHttpRequest = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(e) => {
                        error!("bad request json: {e}");
                        continue;
                    }
                };

                // Dispatch concurrently: a request handler may itself perform a federation
                // fetch that comes back through this tunnel.
                let permit = match capacity.clone().try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => {
                        let response = RelayHttpResponse { id: req.id, status: 503, headers: Vec::new(), body_b64: String::new() };
                        if let Ok(json) = serde_json::to_string(&response) { let _ = out_tx.send(TunnelOut::Text(json)).await; }
                        continue;
                    }
                };
                let mut h = handler.clone();
                let out = out_tx.clone();
                let metrics_task = metrics.clone();
                handlers.spawn(async move {
                    let _permit = permit;
                    let response = handle_relay_http_request(&mut h, req).await;
                    match serde_json::to_string(&response) {
                        Ok(json) => {
                            metrics_task.relay_tx_add(json.as_bytes().len() as u64);
                            let _ = out.send(TunnelOut::Text(json)).await;
                        }
                        Err(e) => error!("encode tunnel response: {e}"),
                    }
                });
            }
        }
    }
    handlers.abort_all();
    while handlers.join_next().await.is_some() {}
    drop(out_tx);
    let _ = writer.await;
    result
}

fn backoff_delay(attempt: u32, options: &TunnelOptions) -> std::time::Duration {
    let pow = attempt.saturating_sub(1).min(20);
    let mut millis = (options.backoff_base.as_millis() as u64)
        .max(1)
        .saturating_mul(1u64 << pow);
    let max = (options.backoff_max.as_millis() as u64).max(1);
    if millis > max {
        millis = max;
    }
    // Jitter 0..100ms to avoid a reconnect thundering herd.
    use rand::Rng as _;
    let jitter_ms = rand::thread_rng().gen_range(0u64..100);
    std::time::Duration::from_millis(millis.saturating_add(jitter_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_is_capped() {
        let opts = TunnelOptions {
            backoff_base: std::time::Duration::from_millis(100),
            backoff_max: std::time::Duration::from_secs(2),
            ..Default::default()
        };
        let d1 = backoff_delay(1, &opts);
        let d3 = backoff_delay(3, &opts);
        let d20 = backoff_delay(20, &opts);
        assert!(d1 >= std::time::Duration::from_millis(100));
        assert!(d3 >= d1);
        assert!(d20 <= std::time::Duration::from_secs(2) + std::time::Duration::from_millis(500));
    }
}
