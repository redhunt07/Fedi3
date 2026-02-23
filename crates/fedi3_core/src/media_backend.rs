/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use crate::http_retry::send_with_retry;
use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::{rngs::OsRng, RngCore};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct MediaConfig {
    /// "local" (default), "relay" oppure "webdav"
    pub backend: Option<String>,
    pub max_local_cache_bytes: Option<u64>,

    // WebDAV (S3-like support can be added later).
    pub webdav_base_url: Option<String>,
    pub webdav_username: Option<String>,
    pub webdav_password: Option<String>,
    pub webdav_bearer_token: Option<String>,

    // Relay media backend.
    pub relay_base_url: Option<String>,
    pub relay_token: Option<String>,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            backend: Some("local".to_string()),
            max_local_cache_bytes: Some(50 * 1024 * 1024 * 1024),
            webdav_base_url: None,
            webdav_username: None,
            webdav_password: None,
            webdav_bearer_token: None,
            relay_base_url: None,
            relay_token: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MediaUploadResponse {
    pub id: String,
    pub url: String,
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blurhash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MediaSaved {
    pub response: MediaUploadResponse,
    /// Nome file locale salvato in `data_dir/media/` (se presente).
    pub local_name: Option<String>,
}

#[async_trait]
pub trait MediaBackend: Send + Sync {
    async fn save_upload(
        &self,
        username: &str,
        public_base_url: &str,
        filename: &str,
        content_type: Option<&str>,
        bytes: &[u8],
    ) -> Result<MediaSaved>;
}

pub fn media_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("media")
}

pub fn load_media(data_dir: &Path, stored_name: &str) -> Result<(Vec<u8>, String)> {
    let path = media_dir(data_dir).join(stored_name);
    let bytes = std::fs::read(&path).with_context(|| format!("read media {path:?}"))?;
    let mime = mime_guess::from_path(&path)
        .first()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());
    Ok((bytes, mime))
}

pub fn probe_image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    // PNG: signature + IHDR
    if bytes.len() >= 24 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
        let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
        if w > 0 && h > 0 {
            return Some((w, h));
        }
    }

    // GIF: "GIF87a"/"GIF89a" little-endian width/height at offset 6.
    if bytes.len() >= 10 && (&bytes[0..6] == b"GIF87a" || &bytes[0..6] == b"GIF89a") {
        let w = u16::from_le_bytes(bytes[6..8].try_into().ok()?) as u32;
        let h = u16::from_le_bytes(bytes[8..10].try_into().ok()?) as u32;
        if w > 0 && h > 0 {
            return Some((w, h));
        }
    }

    // JPEG: scan for SOF0/SOF2 markers.
    if bytes.len() >= 4 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        let mut i = 2usize;
        while i + 4 < bytes.len() {
            if bytes[i] != 0xFF {
                i += 1;
                continue;
            }
            // Skip fill bytes 0xFF.
            while i < bytes.len() && bytes[i] == 0xFF {
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }
            let marker = bytes[i];
            i += 1;

            // Standalone markers without length.
            if marker == 0xD9 || marker == 0xDA {
                break;
            }
            if i + 2 > bytes.len() {
                break;
            }
            let seg_len = u16::from_be_bytes(bytes[i..i + 2].try_into().ok()?) as usize;
            if seg_len < 2 || i + seg_len > bytes.len() {
                break;
            }

            // SOF0 (0xC0) / SOF2 (0xC2) contain dimensions.
            if marker == 0xC0 || marker == 0xC2 {
                if i + 7 >= bytes.len() {
                    break;
                }
                let h = u16::from_be_bytes(bytes[i + 3..i + 5].try_into().ok()?) as u32;
                let w = u16::from_be_bytes(bytes[i + 5..i + 7].try_into().ok()?) as u32;
                if w > 0 && h > 0 {
                    return Some((w, h));
                }
                break;
            }
            i += seg_len;
        }
    }

    // WebP: "RIFF....WEBP" + chunk VP8X/VP8L/VP8.
    if bytes.len() >= 30 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        // Look for VP8X chunk for extended dims.
        let mut i = 12usize;
        while i + 8 <= bytes.len() {
            let chunk = &bytes[i..i + 4];
            let size = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().ok()?) as usize;
            i += 8;
            if i + size > bytes.len() {
                break;
            }
            if chunk == b"VP8X" && size >= 10 && i + 10 <= bytes.len() {
                // bytes[i+4..i+7] width-1 (24-bit LE), bytes[i+7..i+10] height-1
                let w = (bytes[i + 4] as u32)
                    | ((bytes[i + 5] as u32) << 8)
                    | ((bytes[i + 6] as u32) << 16);
                let h = (bytes[i + 7] as u32)
                    | ((bytes[i + 8] as u32) << 8)
                    | ((bytes[i + 9] as u32) << 16);
                return Some((w + 1, h + 1));
            }
            // Chunks are padded to even size.
            i += size + (size % 2);
        }
    }

    None
}

pub struct LocalFsMediaBackend {
    data_dir: PathBuf,
}

impl LocalFsMediaBackend {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }
}

#[async_trait]
impl MediaBackend for LocalFsMediaBackend {
    async fn save_upload(
        &self,
        username: &str,
        public_base_url: &str,
        filename: &str,
        content_type: Option<&str>,
        bytes: &[u8],
    ) -> Result<MediaSaved> {
        let dir = media_dir(&self.data_dir);
        std::fs::create_dir_all(&dir).context("create media dir")?;

        let id = new_id();
        let ext = Path::new(filename)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let media_type = content_type
            .map(|s| s.to_string())
            .or_else(|| {
                mime_guess::from_path(filename)
                    .first()
                    .map(|m| m.to_string())
            })
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let stored_name = if ext.is_empty() {
            id.clone()
        } else {
            format!("{id}.{ext}")
        };
        let path = dir.join(&stored_name);
        std::fs::write(&path, bytes).context("write media file")?;

        let base = public_base_url.trim_end_matches('/');
        let url = format!("{base}/users/{username}/media/{stored_name}");

        Ok(MediaSaved {
            response: MediaUploadResponse {
                id: stored_name.clone(),
                url,
                media_type,
                size: bytes.len() as u64,
                width: None,
                height: None,
                blurhash: None,
            },
            local_name: Some(stored_name),
        })
    }
}

pub struct WebDavMediaBackend {
    base_url: String,
    username: Option<String>,
    password: Option<String>,
    bearer_token: Option<String>,
    http: reqwest::Client,
}

impl WebDavMediaBackend {
    pub fn new(
        base_url: String,
        username: Option<String>,
        password: Option<String>,
        bearer_token: Option<String>,
        http: reqwest::Client,
    ) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            username,
            password,
            bearer_token,
            http,
        }
    }
}

#[async_trait]
impl MediaBackend for WebDavMediaBackend {
    async fn save_upload(
        &self,
        _username: &str,
        _public_base_url: &str,
        filename: &str,
        content_type: Option<&str>,
        bytes: &[u8],
    ) -> Result<MediaSaved> {
        let id = new_id();
        let ext = Path::new(filename)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let stored_name = if ext.is_empty() {
            id.clone()
        } else {
            format!("{id}.{ext}")
        };

        let media_type = content_type
            .map(|s| s.to_string())
            .or_else(|| {
                mime_guess::from_path(filename)
                    .first()
                    .map(|m| m.to_string())
            })
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let url = format!("{}/{}", self.base_url, stored_name);
        let mut req = self.http.put(&url).body(bytes.to_vec());
        req = req.header("Content-Type", &media_type);
        if let Some(tok) = &self.bearer_token {
            req = req.header("Authorization", format!("Bearer {}", tok));
        } else if let (Some(u), Some(p)) = (&self.username, &self.password) {
            req = req.basic_auth(u, Some(p));
        }

        let resp = send_with_retry(|| req.try_clone().unwrap(), 3)
            .await
            .context("webdav put")?;
        let status = resp.status();
        if !status.is_success() && status.as_u16() != 201 && status.as_u16() != 204 {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("webdav upload failed: {} {}", status, text);
        }

        Ok(MediaSaved {
            response: MediaUploadResponse {
                id: stored_name.clone(),
                url: url.clone(),
                media_type,
                size: bytes.len() as u64,
                width: None,
                height: None,
                blurhash: None,
            },
            local_name: None,
        })
    }
}

pub struct RelayMediaBackend {
    data_dir: PathBuf,
    http: reqwest::Client,
    relay_base_url: String,
    relay_token: String,
}

impl RelayMediaBackend {
    pub fn new(
        data_dir: PathBuf,
        http: reqwest::Client,
        relay_base_url: String,
        relay_token: String,
    ) -> Self {
        Self {
            data_dir,
            http,
            relay_base_url: relay_base_url.trim_end_matches('/').to_string(),
            relay_token: relay_token.trim().to_string(),
        }
    }
}

#[async_trait]
impl MediaBackend for RelayMediaBackend {
    async fn save_upload(
        &self,
        username: &str,
        _public_base_url: &str,
        filename: &str,
        content_type: Option<&str>,
        bytes: &[u8],
    ) -> Result<MediaSaved> {
        let dir = media_dir(&self.data_dir);
        std::fs::create_dir_all(&dir).context("create media dir")?;
        let ext = Path::new(filename)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("bin");
        let media_type = content_type
            .map(|s| s.to_string())
            .or_else(|| {
                mime_guess::from_path(filename)
                    .first()
                    .map(|m| m.to_string())
            })
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let id = new_id();
        let stored_name = format!("{id}.{ext}");
        let path = dir.join(&stored_name);
        std::fs::write(&path, bytes).context("write media file")?;

        let url = format!("{}/users/{}/media", self.relay_base_url, username);
        let req = self
            .http
            .post(url)
            .header("Authorization", format!("Bearer {}", self.relay_token))
            .header("X-Filename", filename)
            .header("Content-Type", &media_type)
            .body(bytes.to_vec());
        let resp = send_with_retry(|| req.try_clone().unwrap(), 3)
            .await
            .context("relay upload request failed")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("relay upload failed: {} {}", status, text);
        }
        let response = resp
            .json::<MediaUploadResponse>()
            .await
            .context("relay upload json")?;
        Ok(MediaSaved {
            response,
            local_name: Some(stored_name),
        })
    }
}

pub fn build_media_backend(
    cfg: MediaConfig,
    data_dir: PathBuf,
    http: reqwest::Client,
) -> Result<(MediaConfig, Box<dyn MediaBackend>)> {
    let backend = cfg
        .backend
        .clone()
        .unwrap_or_else(|| "local".to_string())
        .to_lowercase();
    match backend.as_str() {
        "local" => Ok((cfg, Box::new(LocalFsMediaBackend::new(data_dir)))),
        "webdav" => {
            let base = cfg.webdav_base_url.clone().ok_or_else(|| {
                anyhow::anyhow!("media.backend=webdav requires media.webdav_base_url")
            })?;
            Ok((
                cfg.clone(),
                Box::new(WebDavMediaBackend::new(
                    base,
                    cfg.webdav_username.clone(),
                    cfg.webdav_password.clone(),
                    cfg.webdav_bearer_token.clone(),
                    http,
                )),
            ))
        }
        "relay" => {
            let base = cfg.relay_base_url.clone().ok_or_else(|| {
                anyhow::anyhow!("media.backend=relay requires media.relay_base_url")
            })?;
            let token = cfg
                .relay_token
                .clone()
                .ok_or_else(|| anyhow::anyhow!("media.backend=relay requires media.relay_token"))?;
            Ok((
                cfg.clone(),
                Box::new(RelayMediaBackend::new(data_dir, http, base, token)),
            ))
        }
        _ => anyhow::bail!("unsupported media.backend: {backend}"),
    }
}

fn new_id() -> String {
    let mut b = [0u8; 16];
    OsRng.fill_bytes(&mut b);
    b.iter().map(|v| format!("{v:02x}")).collect()
}
