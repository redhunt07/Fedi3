/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use fedi3_core::delivery::Delivery;
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    let actor_url = env::args().nth(1).unwrap_or_default();
    if actor_url.trim().is_empty() {
        anyhow::bail!("usage: dev_resolve_actor <actor_url>");
    }

    let d = Delivery::new();
    let info = d.resolve_actor_info(actor_url.trim()).await?;
    println!("inbox={}", info.inbox);
    if let Some(pk) = info.public_key_pem.as_deref() {
        println!("public_key_pem_len={}", pk.len());
    }
    if let Some(pid) = info.p2p_peer_id.as_deref() {
        println!("p2p_peer_id={pid}");
    }
    Ok(())
}

