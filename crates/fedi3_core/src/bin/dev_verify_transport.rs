//! Read-only comparison of a live core's objects/media over P2P and public HTTP.
use anyhow::{ensure, Context, Result};
use axum::Router;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use fedi3_core::{
    net_metrics::NetMetrics,
    p2p::{self, P2pConfig},
    social_db::SocialDb,
};
use fedi3_protocol::RelayHttpRequest;
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    ensure!(
        args.len() == 6,
        "Usage: dev_verify_transport PEER_ID MULTIADDR OBJECT_URL MEDIA_ID MEDIA_URL"
    );
    // A separate, disposable identity never follows, posts or changes the live account.
    let dir = std::env::temp_dir().join(format!("fedi3-transport-verify-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let social = Arc::new(SocialDb::open(dir.join("verify.db"))?);
    let (stop, shutdown) = watch::channel(false);
    let handle = p2p::start_p2p(
        P2pConfig {
            enable: true,
            listen: Some("/ip4/127.0.0.1/tcp/0".into()),
            bootstrap: Some(Vec::new()),
            public_discovery_enable: Some(false),
            peer_exchange_enable: Some(false),
            mdns_enable: Some(false),
            gossip_enable: Some(false),
            discovery_enable: Some(false),
            auto_force_relay_only: Some(false),
            ..Default::default()
        },
        libp2p::identity::Keypair::generate_ed25519(),
        "did:fedi3:verification".into(),
        "https://verification.invalid/users/reader".into(),
        "verification-local-only".into(),
        String::new(),
        social,
        Arc::new(NetMetrics::new()),
        Router::new(),
        shutdown,
    )?
    .context("P2P disabled")?;
    handle
        .add_peer_addrs(&args[1], vec![args[2].clone()])
        .await?;
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()?;
    for (kind, path, query, url) in [
        (
            "object",
            "/_fedi3/object",
            format!("?url={}", urlencoding::encode(&args[3])),
            args[3].as_str(),
        ),
        (
            "media",
            "/.fedi3/media",
            format!("?id={}", urlencoding::encode(&args[4])),
            args[5].as_str(),
        ),
    ] {
        let response = tokio::time::timeout(
            Duration::from_secs(45),
            handle.request(
                &args[1],
                RelayHttpRequest {
                    id: format!("verify-{kind}-{}", std::process::id()),
                    method: "GET".into(),
                    path: path.into(),
                    query,
                    headers: vec![("accept".into(), "application/activity+json, */*".into())],
                    body_b64: String::new(),
                },
            ),
        )
        .await??;
        ensure!(
            response.status == 200,
            "{kind}: P2P status {}",
            response.status
        );
        let peer_bytes = B64.decode(&response.body_b64)?;
        let remote = http
            .get(url)
            .header("Accept", "application/activity+json, */*")
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        if kind == "object" {
            let peer: serde_json::Value = serde_json::from_slice(&peer_bytes)?;
            let relay: serde_json::Value = serde_json::from_slice(&remote)?;
            ensure!(
                peer["id"] == args[3] && peer == relay,
                "P2P and relay objects differ"
            );
        } else {
            ensure!(
                peer_bytes.as_slice() == remote.as_ref(),
                "P2P and relay media differ"
            );
        }
        println!(
            "{kind}: P2P=200 relay=200 equal=true bytes={} sha256={:x}",
            peer_bytes.len(),
            Sha256::digest(&peer_bytes)
        );
    }
    stop.send(true)?;
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}
