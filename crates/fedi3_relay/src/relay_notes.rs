/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use crate::now_ms;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RelaySyncNoteItem {
    pub note: serde_json::Value,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RelaySyncNotesResponse {
    pub items: Vec<RelaySyncNoteItem>,
    pub next: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RelaySyncMediaItem {
    pub url: String,
    pub media_type: Option<String>,
    pub name: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub blurhash: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RelaySyncActorItem {
    pub actor_url: String,
    pub username: Option<String>,
    pub actor_json: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RelaySyncBundle {
    pub relay_url: String,
    pub created_at_ms: i64,
    pub notes: Vec<RelaySyncNoteItem>,
    pub media: Vec<RelaySyncMediaItem>,
    pub actors: Vec<RelaySyncActorItem>,
    pub next: Option<String>,
    pub signature_b64: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RelayNoteIndex {
    pub note_id: String,
    pub actor_id: Option<String>,
    pub published_ms: Option<i64>,
    pub content_text: String,
    pub content_html: String,
    pub note_json: String,
    pub created_at_ms: i64,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RelayMediaIndex {
    pub url: String,
    pub media_type: Option<String>,
    pub name: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub blurhash: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct RelayActorIndex {
    pub actor_url: String,
    pub username: Option<String>,
    pub actor_json: String,
    pub updated_at_ms: i64,
}

pub fn extract_notes_from_value(value: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    match value {
        serde_json::Value::Object(map) => {
            let ty = map.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if ty == "Note" {
                out.push(value.clone());
                return out;
            }
            if ty == "Create" || ty == "Announce" {
                if let Some(obj) = map.get("object") {
                    if let serde_json::Value::Object(obj_map) = obj {
                        let inner = if obj_map.get("type").and_then(|t| t.as_str()) == Some("Note")
                        {
                            Some(obj)
                        } else {
                            obj_map.get("object")
                        };
                        if let Some(note) = inner {
                            if note.get("type").and_then(|t| t.as_str()) == Some("Note") {
                                out.push(note.clone());
                                return out;
                            }
                        }
                    }
                }
            }
            if ty == "OrderedCollection"
                || ty == "OrderedCollectionPage"
                || ty == "Collection"
                || ty == "CollectionPage"
            {
                if let Some(items) = map.get("orderedItems").or_else(|| map.get("items")) {
                    if let serde_json::Value::Array(arr) = items {
                        for item in arr {
                            out.extend(extract_notes_from_value(item));
                        }
                    }
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                out.extend(extract_notes_from_value(item));
            }
        }
        _ => {}
    }
    out
}

pub fn note_to_index(note: &serde_json::Value) -> Option<RelayNoteIndex> {
    let id = note
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if id.is_empty() {
        return None;
    }
    let actor_id = note
        .get("attributedTo")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string());
    let published_ms = note
        .get("published")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp_millis());
    let content_html = note
        .get("content")
        .or_else(|| note.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let content_text = strip_html(&content_html);
    let tags = extract_tags(note.get("tag"));
    let note_json = serde_json::to_string(note).unwrap_or_default();
    Some(RelayNoteIndex {
        note_id: id,
        actor_id,
        published_ms,
        content_text,
        content_html,
        note_json,
        created_at_ms: now_ms(),
        tags,
    })
}

pub fn extract_media_from_note(note: &serde_json::Value) -> Vec<RelayMediaIndex> {
    let mut out = Vec::new();
    let Some(att) = note.get("attachment") else {
        return out;
    };
    extract_media_from_value(att, &mut out);
    out
}

pub fn actor_to_index_from_note(note: &serde_json::Value) -> Option<RelayActorIndex> {
    let actor_url = note
        .get("attributedTo")
        .and_then(|v| v.as_str())
        .or_else(|| note.get("actor").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_string();
    if actor_url.is_empty() {
        return None;
    }
    let username = actor_username_from_url(&actor_url);
    let actor_json = actor_stub_json(&actor_url, username.as_deref());
    Some(RelayActorIndex {
        actor_url,
        username,
        actor_json,
        updated_at_ms: now_ms(),
    })
}

fn strip_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for c in input.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ => {
                if !in_tag {
                    out.push(c);
                }
            }
        }
    }
    out
}

fn extract_tags(tag_value: Option<&serde_json::Value>) -> Vec<String> {
    let Some(tag_value) = tag_value else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut push_tag = |name: &str| {
        let t = name.trim().trim_start_matches('#').to_string();
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    };
    match tag_value {
        serde_json::Value::Array(arr) => {
            for item in arr {
                if let Some(tag) = extract_tag_name(item) {
                    push_tag(&tag);
                }
            }
        }
        _ => {
            if let Some(tag) = extract_tag_name(tag_value) {
                push_tag(&tag);
            }
        }
    }
    out
}

fn extract_tag_name(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => {
            if s.trim().starts_with('#') {
                Some(s.trim().to_string())
            } else {
                None
            }
        }
        serde_json::Value::Object(map) => {
            let ty = map.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let name = map.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if ty == "Hashtag" || name.starts_with('#') {
                Some(name.to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn extract_media_from_value(value: &serde_json::Value, out: &mut Vec<RelayMediaIndex>) {
    match value {
        serde_json::Value::String(url) => {
            if let Some(idx) = media_index_from_fields(url, None, None, None, None, None) {
                out.push(idx);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                extract_media_from_value(item, out);
            }
        }
        serde_json::Value::Object(map) => {
            let media_type = map.get("mediaType").and_then(|v| v.as_str()).map(|s| s.to_string());
            let name = map.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
            let blurhash = map.get("blurhash").and_then(|v| v.as_str()).map(|s| s.to_string());
            let width = map.get("width").and_then(|v| v.as_i64());
            let height = map.get("height").and_then(|v| v.as_i64());
            if let Some(url_val) = map.get("url").or_else(|| map.get("href")) {
                extract_media_from_value_with_defaults(
                    url_val,
                    media_type.as_ref(),
                    name.as_ref(),
                    width,
                    height,
                    blurhash.as_ref(),
                    out,
                );
            }
        }
        _ => {}
    }
}

fn extract_media_from_value_with_defaults(
    value: &serde_json::Value,
    media_type: Option<&String>,
    name: Option<&String>,
    width: Option<i64>,
    height: Option<i64>,
    blurhash: Option<&String>,
    out: &mut Vec<RelayMediaIndex>,
) {
    match value {
        serde_json::Value::String(url) => {
            if let Some(idx) = media_index_from_fields(
                url,
                media_type.cloned(),
                name.cloned(),
                width,
                height,
                blurhash.cloned(),
            ) {
                out.push(idx);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                extract_media_from_value_with_defaults(
                    item,
                    media_type,
                    name,
                    width,
                    height,
                    blurhash,
                    out,
                );
            }
        }
        serde_json::Value::Object(map) => {
            let url = map
                .get("href")
                .and_then(|v| v.as_str())
                .or_else(|| map.get("url").and_then(|v| v.as_str()));
            let media_type = map
                .get("mediaType")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| media_type.cloned());
            let name = map
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| name.cloned());
            let blurhash = map
                .get("blurhash")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| blurhash.cloned());
            let width = map.get("width").and_then(|v| v.as_i64()).or(width);
            let height = map.get("height").and_then(|v| v.as_i64()).or(height);
            if let Some(url) = url {
                if let Some(idx) = media_index_from_fields(
                    url,
                    media_type,
                    name,
                    width,
                    height,
                    blurhash,
                ) {
                    out.push(idx);
                }
            }
        }
        _ => {}
    }
}

fn media_index_from_fields(
    url: &str,
    media_type: Option<String>,
    name: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    blurhash: Option<String>,
) -> Option<RelayMediaIndex> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    Some(RelayMediaIndex {
        url: url.to_string(),
        media_type,
        name,
        width,
        height,
        blurhash,
        created_at_ms: now_ms(),
    })
}

fn actor_username_from_url(url: &str) -> Option<String> {
    let url = url.trim();
    let without_scheme = url
        .split("://")
        .nth(1)
        .unwrap_or(url)
        .trim_end_matches('/');
    let path = without_scheme.splitn(2, '/').nth(1).unwrap_or("");
    if path.starts_with("@") {
        let name = path.trim_start_matches('@').split('/').next().unwrap_or("");
        if !name.trim().is_empty() {
            return Some(name.trim().to_string());
        }
    }
    if let Some(rest) = path.strip_prefix("users/") {
        let name = rest.split('/').next().unwrap_or("");
        if !name.trim().is_empty() {
            return Some(name.trim().to_string());
        }
    }
    None
}

fn actor_stub_json(actor_url: &str, username: Option<&str>) -> String {
    let mut obj = serde_json::Map::new();
    obj.insert("@context".to_string(), serde_json::json!("https://www.w3.org/ns/activitystreams"));
    obj.insert("id".to_string(), serde_json::Value::String(actor_url.to_string()));
    obj.insert("type".to_string(), serde_json::Value::String("Person".to_string()));
    if let Some(u) = username {
        obj.insert(
            "preferredUsername".to_string(),
            serde_json::Value::String(u.to_string()),
        );
    }
    serde_json::Value::Object(obj).to_string()
}
