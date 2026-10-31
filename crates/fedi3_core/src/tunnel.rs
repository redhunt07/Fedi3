/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use crate::net_metrics::NetMetrics;
use crate::relay_bridge::handle_relay_http_request;
use axum::body::Body;
use fedi3_protocol::RelayHttpRequest;
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

pub async fn run_tunnel(
    user: &str,
    relay_ws_url: &str,
    relay_token: &str,
    handler: impl Clone
        + Send
        + 'static
        + tower::Service<
            Request<Body>,
            Response = http::Response<Body>,
            Error = std::convert::Infallible,
        >,
) -> anyhow::Result<()> {
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

pub async fn run_tunnel_with_shutdown(
    user: &str,
    relay_ws_url: &str,
    relay_token: &str,
    handler: impl Clone
        + Send
        + 'static
        + tower::Service<
            Request<Body>,
            Response = http::Response<Body>,
            Error = std::convert::Infallible,
        >,
    shutdown: watch::Receiver<bool>,
    metrics: std::sync::Arc<NetMetrics>,
) -> anyhow::Result<()> {
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

pub async fn run_tunnel_with_options(
    user: &str,
    relay_ws_url: &str,
    relay_token: &str,
    mut handler: impl Clone
        + Send
        + 'static
        + tower::Service<
            Request<Body>,
            Response = http::Response<Body>,
            Error = std::convert::Infallible,
        >,
    mut shutdown: watch::Receiver<bool>,
    metrics: std::sync::Arc<NetMetrics>,
    options: TunnelOptions,
) -> anyhow::Result<()> {
    let token = urlencoding::encode(relay_token);
    let url = format!("{relay_ws_url}/tunnel/{user}?token={token}");

    let mut attempt: u32 = 0;
    loop {
        if *shutdown.borrow() {
            break;
        }

        info!(%url, attempt, "connecting tunnel");
        let connect = tokio::time::timeout(
            options.connect_timeout,
            tokio_tungstenite::connect_async(&url),
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
                let reason =
                    run_connection(ws, &mut shutdown, &mut handler, &metrics, &options).await;
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

async fn run_connection<S>(
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    shutdown: &mut watch::Receiver<bool>,
    handler: &mut S,
    metrics: &NetMetrics,
    options: &TunnelOptions,
) -> anyhow::Result<()>
where
    S: tower::Service<
            Request<Body>,
            Response = http::Response<Body>,
            Error = std::convert::Infallible,
        > + Clone,
{
    let (mut ws_tx, mut ws_rx) = ws.split();
    let mut ping = tokio::time::interval(options.ping_interval);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_pong = std::time::Instant::now();

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    let _ = ws_tx.send(tungstenite::Message::Close(None)).await;
                    return Ok(());
                }
            }
            _ = ping.tick() => {
                // Watchdog: a half-open tunnel that never answers pings must be replaced.
                if last_pong.elapsed() > options.pong_timeout {
                    return Err(anyhow::anyhow!(
                        "no pong for {:?} (watchdog)",
                        last_pong.elapsed()
                    ));
                }
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let payload = now_ms.to_be_bytes().to_vec();
                if let Err(e) = ws_tx.send(tungstenite::Message::Ping(payload)).await {
                    return Err(anyhow::anyhow!("tunnel ping failed: {e}"));
                }
            }
            msg = ws_rx.next() => {
                let Some(msg) = msg else {
                    return Ok(());
                };
                let msg = msg.map_err(|e| anyhow::anyhow!("tunnel read failed: {e}"))?;
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
                        let _ = ws_tx.send(tungstenite::Message::Pong(p)).await;
                        continue;
                    }
                    tungstenite::Message::Close(_) => return Ok(()),
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

                let response = handle_relay_http_request(handler, req.clone()).await;
                let json = serde_json::to_string(&response)
                    .map_err(|e| anyhow::anyhow!("encode tunnel response: {e}"))?;
                metrics.relay_tx_add(json.as_bytes().len() as u64);
                ws_tx
                    .send(tungstenite::Message::Text(json))
                    .await
                    .map_err(|e| anyhow::anyhow!("tunnel write failed: {e}"))?;
            }
        }
    }
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
