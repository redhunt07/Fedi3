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
use tracing::{error, info};

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
) -> anyhow::Result<()> {
    let token = urlencoding::encode(relay_token);
    let url = format!("{relay_ws_url}/tunnel/{user}?token={token}");
    info!(%url, "connecting tunnel");

    let (ws, _) = match tokio_tungstenite::connect_async(url).await {
        Ok(v) => v,
        Err(e) => {
            metrics.set_relay_error(e.to_string());
            return Err(e.into());
        }
    };
    let (mut ws_tx, mut ws_rx) = ws.split();
    metrics.set_relay_connected(true);

    let mut ping = tokio::time::interval(std::time::Duration::from_secs(5));
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            _ = ping.tick() => {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let payload = now_ms.to_be_bytes().to_vec();
                if let Err(e) = ws_tx.send(tungstenite::Message::Ping(payload)).await {
                    metrics.set_relay_error(e.to_string());
                    break;
                }
            }
            msg = ws_rx.next() => {
                let Some(msg) = msg else { break; };
                let msg = msg?;
                let text = match msg {
                    tungstenite::Message::Text(t) => t,
                    tungstenite::Message::Pong(p) => {
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
                        // Reply to keep the tunnel healthy across NATs.
                        let _ = ws_tx.send(tungstenite::Message::Pong(p)).await;
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

                let response = handle_relay_http_request(&mut handler, req.clone()).await;
                let json = serde_json::to_string(&response)?;
                metrics.relay_tx_add(json.as_bytes().len() as u64);
                ws_tx.send(tungstenite::Message::Text(json)).await?;
            }
        }
    }

    metrics.set_relay_connected(false);
    Ok(())
}

// moved to relay_bridge.rs
