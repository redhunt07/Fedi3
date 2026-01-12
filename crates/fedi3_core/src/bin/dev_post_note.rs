/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use anyhow::{Context, Result};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    let user = std::env::var("FEDI3_USER").unwrap_or_else(|_| "alice".to_string());
    let base_url = std::env::var("FEDI3_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8787".to_string());
    let target = std::env::var("FEDI3_TARGET").context("missing FEDI3_TARGET (actor url or inbox url)")?;
    let content = std::env::var("FEDI3_CONTENT").unwrap_or_else(|_| "Hello from Fedi3".to_string());

    let base = base_url.trim_end_matches('/');
    let actor = format!("{base}/users/{user}");
    let outbox = format!("{actor}/outbox");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let id = format!("{actor}/objects/{ts}");

    let activity = json!({
      "@context": "https://www.w3.org/ns/activitystreams",
      "id": id,
      "type": "Create",
      "actor": actor,
      "to": [target],
      "object": {
        "type": "Note",
        "content": content
      }
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(outbox)
        .header(ACCEPT, "application/activity+json")
        .header(CONTENT_TYPE, "application/activity+json")
        .json(&activity)
        .send()
        .await?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() && status.as_u16() != 202 {
        anyhow::bail!("outbox rejected: {} {}", status, text);
    }
    println!("ok: {} {}", status, text);
    Ok(())
}
