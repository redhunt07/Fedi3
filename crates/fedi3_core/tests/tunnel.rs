/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

#![allow(clippy::field_reassign_with_default, clippy::let_unit_value)]

mod common;

use axum::body::Body;
use fedi3_core::net_metrics::NetMetrics;
use fedi3_core::tunnel::{run_tunnel_with_options, TunnelOptions};
use futures_util::{SinkExt, StreamExt as _};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tower::service_fn;

type Handler = tower::util::ServiceFn<
    fn(
        axum::http::Request<Body>,
    ) -> std::future::Ready<Result<axum::http::Response<Body>, std::convert::Infallible>>,
>;

fn handler() -> Handler {
    fn respond(
        _req: axum::http::Request<Body>,
    ) -> std::future::Ready<Result<axum::http::Response<Body>, std::convert::Infallible>> {
        std::future::ready(Ok(axum::http::Response::builder()
            .status(200)
            .body(Body::from("ok"))
            .unwrap()))
    }
    service_fn(respond)
}

fn test_options() -> TunnelOptions {
    TunnelOptions {
        connect_timeout: Duration::from_secs(2),
        pong_timeout: Duration::from_millis(800),
        ping_interval: Duration::from_millis(200),
        backoff_base: Duration::from_millis(100),
        backoff_max: Duration::from_millis(500),
        stable_after: Duration::from_secs(60),
    }
}

fn spawn_tunnel(
    url: String,
    shutdown_rx: watch::Receiver<bool>,
    metrics: Arc<NetMetrics>,
) -> tokio::task::JoinHandle<anyhow::Result<()>> {
    tokio::spawn(async move {
        run_tunnel_with_options(
            "alice",
            &url,
            "0123456789abcdef",
            handler(),
            shutdown_rx,
            metrics,
            test_options(),
        )
        .await
    })
}

/// Relay that accepts and immediately closes each connection.
async fn closing_relay() -> (
    String,
    Arc<AtomicUsize>,
    watch::Sender<bool>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let count = Arc::new(AtomicUsize::new(0));
    let count2 = count.clone();
    let (tx, mut rx) = watch::channel(false);
    let join = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = rx.changed() => { if *rx.borrow() { break; } }
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { break };
                    count2.fetch_add(1, Ordering::Relaxed);
                    tokio::spawn(async move {
                        if let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await {
                            let _ = ws.send(tokio_tungstenite::tungstenite::Message::Close(None)).await;
                        }
                    });
                }
            }
        }
    });
    (format!("ws://127.0.0.1:{port}"), count, tx, join)
}

/// Relay that accepts and never reads/writes, so the client never receives a pong.
async fn silent_relay() -> (
    String,
    Arc<AtomicUsize>,
    watch::Sender<bool>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let count = Arc::new(AtomicUsize::new(0));
    let count2 = count.clone();
    let (tx, mut rx) = watch::channel(false);
    let join = tokio::spawn(async move {
        let mut held = Vec::new();
        loop {
            tokio::select! {
                _ = rx.changed() => { if *rx.borrow() { break; } }
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { break };
                    count2.fetch_add(1, Ordering::Relaxed);
                    held.push(tokio::spawn(async move {
                        if let Ok(ws) = tokio_tungstenite::accept_async(stream).await {
                            // Never poll: no automatic pong is sent.
                            std::future::pending::<()>().await;
                            drop(ws);
                        }
                    }));
                }
            }
        }
        for h in held {
            h.abort();
        }
    });
    (format!("ws://127.0.0.1:{port}"), count, tx, join)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t02_tunnel_reconnects_after_relay_close() {
    let (url, count, stop_relay, relay_join) = closing_relay().await;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let metrics = Arc::new(NetMetrics::new());
    let tunnel = spawn_tunnel(url, shutdown_rx, metrics);

    let start = std::time::Instant::now();
    while count.load(Ordering::Relaxed) < 2 && start.elapsed() < Duration::from_secs(8) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        count.load(Ordering::Relaxed) >= 2,
        "tunnel must reconnect after a close"
    );
    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(5), tunnel)
        .await
        .expect("clean shutdown");
    let _ = stop_relay.send(true);
    let _ = relay_join.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t03_tunnel_reconnects_when_no_pong_arrives() {
    let (url, count, stop_relay, relay_join) = silent_relay().await;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let metrics = Arc::new(NetMetrics::new());
    let tunnel = spawn_tunnel(url, shutdown_rx, metrics);
    let start = std::time::Instant::now();
    while count.load(Ordering::Relaxed) < 2 && start.elapsed() < Duration::from_secs(8) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        count.load(Ordering::Relaxed) >= 2,
        "watchdog must reconnect when no pong arrives"
    );
    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(5), tunnel)
        .await
        .expect("clean shutdown");
    let _ = stop_relay.send(true);
    let _ = relay_join.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t04_shutdown_stops_the_tunnel() {
    let (url, _count, stop_relay, relay_join) = silent_relay().await;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let metrics = Arc::new(NetMetrics::new());
    let tunnel = spawn_tunnel(url, shutdown_rx, metrics.clone());
    tokio::time::sleep(Duration::from_millis(300)).await;
    let _ = shutdown_tx.send(true);
    let res = tokio::time::timeout(Duration::from_secs(5), tunnel).await;
    assert!(res.is_ok(), "tunnel must stop promptly on shutdown");
    assert!(!metrics.relay_connected.load(Ordering::Relaxed));
    let _ = stop_relay.send(true);
    let _ = relay_join.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t01_ping_pong_updates_rtt() {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, mut rx) = watch::channel(false);
    let join = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = rx.changed() => { if *rx.borrow() { break; } }
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { break };
                    tokio::spawn(async move {
                        if let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await {
                            // Polling makes tungstenite auto-reply to Ping frames.
                            while ws.next().await.is_some() {}
                        }
                    });
                }
            }
        }
    });
    let url = format!("ws://127.0.0.1:{port}");
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let metrics = Arc::new(NetMetrics::new());
    let tunnel = spawn_tunnel(url, shutdown_rx, metrics.clone());
    let start = std::time::Instant::now();
    while metrics.relay_rtt_ema_ms.load(Ordering::Relaxed) == 0
        && start.elapsed() < Duration::from_secs(5)
    {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        metrics.relay_rtt_ema_ms.load(Ordering::Relaxed) > 0,
        "ping/pong RTT must be measured"
    );
    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(5), tunnel).await;
    let _ = tx.send(true);
    let _ = join.await;
}
