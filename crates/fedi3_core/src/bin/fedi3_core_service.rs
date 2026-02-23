/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use anyhow::{Context, Result};
use fedi3_core::runtime::{self, CoreStartConfig};
use serde_json::{json, Map, Value};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};

fn default_config_path() -> Result<PathBuf> {
    if cfg!(target_os = "windows") {
        let base = std::env::var("APPDATA")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        return Ok(PathBuf::from(base).join("Fedi3").join("config.json"));
    }
    if cfg!(target_os = "macos") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        return Ok(PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Fedi3")
            .join("config.json"));
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    Ok(PathBuf::from(home).join(".config").join("fedi3").join("config.json"))
}

fn hash_text(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn text_to_html(input: &str) -> String {
    let mut s = input.trim().to_string();
    if s.is_empty() {
        return String::new();
    }
    s = s.replace('&', "&amp;");
    s = s.replace('<', "&lt;");
    s = s.replace('>', "&gt;");
    s = s.replace('"', "&quot;");
    s = s.replace('\'', "&#39;");
    s = s.replace("\r\n", "\n").replace('\r', "\n");
    s = s.replace('\n', "<br>");
    format!("<p>{s}</p>")
}

fn get_str(map: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(v) = map.get(*k) {
            if let Some(s) = v.as_str() {
                let t = s.trim().to_string();
                if !t.is_empty() {
                    return Some(t);
                }
            }
        }
    }
    None
}

fn get_bool(map: &Map<String, Value>, keys: &[&str]) -> Option<bool> {
    for k in keys {
        if let Some(v) = map.get(*k) {
            if let Some(b) = v.as_bool() {
                return Some(b);
            }
        }
    }
    None
}

fn get_u64(map: &Map<String, Value>, keys: &[&str]) -> Option<u64> {
    for k in keys {
        if let Some(v) = map.get(*k) {
            if let Some(n) = v.as_u64() {
                return Some(n);
            }
        }
    }
    None
}

fn get_i64(map: &Map<String, Value>, keys: &[&str]) -> Option<i64> {
    for k in keys {
        if let Some(v) = map.get(*k) {
            if let Some(n) = v.as_i64() {
                return Some(n);
            }
        }
    }
    None
}

fn get_list(map: &Map<String, Value>, keys: &[&str]) -> Option<Vec<String>> {
    for k in keys {
        if let Some(v) = map.get(*k) {
            if let Some(list) = v.as_array() {
                let items = list
                    .iter()
                    .filter_map(|i| i.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>();
                if !items.is_empty() {
                    return Some(items);
                }
            }
        }
    }
    None
}

fn normalize_app_config(raw: Value) -> Result<Value> {
    let obj = raw
        .as_object()
        .context("config must be a JSON object")?;
    let is_core_style = obj.contains_key("relay_ws")
        || obj.contains_key("public_base_url")
        || obj.contains_key("relay_token");
    if is_core_style {
        return Ok(raw);
    }

    let username = get_str(obj, &["username"]).context("missing username")?;
    let domain = get_str(obj, &["domain"]).context("missing domain")?;
    let relay_ws = get_str(obj, &["relayWs"]).context("missing relayWs")?;
    let bind = get_str(obj, &["bind"]).context("missing bind")?;

    let mut out = Map::new();
    out.insert("username".to_string(), json!(username));
    out.insert("domain".to_string(), json!(domain));
    out.insert(
        "public_base_url".to_string(),
        json!(get_str(obj, &["publicBaseUrl"]).unwrap_or_default()),
    );
    out.insert("relay_ws".to_string(), json!(relay_ws));
    out.insert("bind".to_string(), json!(bind));

    if let Some(v) = get_str(obj, &["relayToken"]) {
        out.insert("relay_token".to_string(), json!(v));
    }
    if let Some(v) = get_str(obj, &["internalToken"]) {
        out.insert("internal_token".to_string(), json!(v));
    }
    if let Some(v) = get_str(obj, &["displayName"]) {
        out.insert("display_name".to_string(), json!(v));
    }
    if let Some(v) = get_str(obj, &["summary"]) {
        let html = text_to_html(&v);
        if !html.is_empty() {
            out.insert("summary".to_string(), json!(html));
        }
    }
    if let Some(v) = get_str(obj, &["iconUrl"]) {
        out.insert("icon_url".to_string(), json!(v));
    }
    if let Some(v) = get_str(obj, &["iconMediaType"]) {
        out.insert("icon_media_type".to_string(), json!(v));
    }
    if let Some(v) = get_str(obj, &["imageUrl"]) {
        out.insert("image_url".to_string(), json!(v));
    }
    if let Some(v) = get_str(obj, &["imageMediaType"]) {
        out.insert("image_media_type".to_string(), json!(v));
    }

    if let Some(fields) = obj.get("profileFields").and_then(|v| v.as_array()) {
        let mut out_fields = Vec::new();
        for f in fields {
            let fobj = match f.as_object() {
                Some(v) => v,
                None => continue,
            };
            let name = fobj
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let val = fobj
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                continue;
            }
            let html = text_to_html(&val);
            if html.is_empty() {
                continue;
            }
            out_fields.push(json!({"name": name, "value": html}));
        }
        if !out_fields.is_empty() {
            out.insert("profile_fields".to_string(), Value::Array(out_fields));
        }
    }

    if get_bool(obj, &["manuallyApprovesFollowers"]) == Some(true) {
        out.insert("manually_approves_followers".to_string(), json!(true));
    }
    if let Some(list) = get_list(obj, &["blockedDomains"]) {
        out.insert("blocked_domains".to_string(), json!(list));
    }
    if let Some(list) = get_list(obj, &["blockedActors"]) {
        out.insert("blocked_actors".to_string(), json!(list));
    }
    if let Some(list) = get_list(obj, &["apRelays"]) {
        out.insert("ap_relays".to_string(), json!(list));
    }
    if let Some(list) = get_list(obj, &["bootstrapFollowActors"]) {
        out.insert("bootstrap_follow_actors".to_string(), json!(list));
    }
    if let Some(v) = get_str(obj, &["previousPublicBaseUrl"]) {
        out.insert("previous_public_base_url".to_string(), json!(v));
    }
    if let Some(v) = get_str(obj, &["previousRelayToken"]) {
        out.insert("previous_relay_token".to_string(), json!(v));
    }
    if let Some(v) = get_i64(obj, &["upnpPortRangeStart"]) {
        out.insert("upnp_port_start".to_string(), json!(v));
    }
    if let Some(v) = get_i64(obj, &["upnpPortRangeEnd"]) {
        out.insert("upnp_port_end".to_string(), json!(v));
    }
    if let Some(v) = get_i64(obj, &["upnpLeaseSecs"]) {
        out.insert("upnp_lease_secs".to_string(), json!(v));
    }
    if let Some(v) = get_i64(obj, &["upnpTimeoutSecs"]) {
        out.insert("upnp_timeout_secs".to_string(), json!(v));
    }
    if let Some(v) = get_str(obj, &["postDeliveryMode"]) {
        out.insert("post_delivery_mode".to_string(), json!(v));
    }
    if let Some(v) = get_u64(obj, &["p2pRelayFallbackSecs"]) {
        out.insert("p2p_relay_fallback_secs".to_string(), json!(v));
    }
    if let Some(v) = get_u64(obj, &["p2pCacheTtlSecs"]) {
        out.insert("p2p_cache_ttl_secs".to_string(), json!(v));
    }

    Ok(Value::Object(out))
}

fn load_config(text: &str) -> Result<CoreStartConfig> {
    let raw: Value = serde_json::from_str(text).context("parse config json")?;
    let normalized = normalize_app_config(raw)?;
    let cfg: CoreStartConfig = serde_json::from_value(normalized).context("decode CoreStartConfig")?;
    Ok(cfg)
}

fn parse_config_path() -> Result<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--config" {
            if let Some(path) = args.next() {
                return Ok(PathBuf::from(path));
            }
            return Err(anyhow::anyhow!("--config requires a path"));
        }
    }
    if let Ok(path) = std::env::var("FEDI3_CONFIG") {
        if !path.trim().is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    default_config_path()
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg_path = parse_config_path()?;
    info!("fedi3 core service starting");
    info!("config: {}", cfg_path.display());

    let mut handle: Option<u64> = None;
    let mut last_hash: Option<u64> = None;
    let mut last_failed_hash: Option<u64> = None;
    let mut missing_logged = false;

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        tokio::select! {
            _ = &mut ctrl_c => {
                info!("shutdown requested");
                break;
            }
            _ = sleep(Duration::from_secs(2)) => {}
        }

        let text = match std::fs::read_to_string(&cfg_path) {
            Ok(t) => t,
            Err(e) => {
                if !missing_logged {
                    warn!("config missing: {} ({e})", cfg_path.display());
                    missing_logged = true;
                }
                continue;
            }
        };
        missing_logged = false;

        let hash = hash_text(&text);
        if last_hash == Some(hash) || last_failed_hash == Some(hash) {
            continue;
        }

        match load_config(&text) {
            Ok(cfg) => {
                if let Some(h) = handle.take() {
                    if let Err(e) = runtime::stop(h) {
                        warn!("failed to stop previous core: {e:#}");
                    }
                }
                match runtime::start(cfg) {
                    Ok(h) => {
                        handle = Some(h);
                        last_hash = Some(hash);
                        last_failed_hash = None;
                        info!("core started (handle={h})");
                    }
                    Err(e) => {
                        error!("failed to start core: {e:#}");
                        last_failed_hash = Some(hash);
                    }
                }
            }
            Err(e) => {
                warn!("invalid config: {e:#}");
                last_failed_hash = Some(hash);
            }
        }
    }

    if let Some(h) = handle.take() {
        if let Err(e) = runtime::stop(h) {
            warn!("failed to stop core: {e:#}");
        }
    }
    Ok(())
}
