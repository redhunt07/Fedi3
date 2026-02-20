/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use anyhow::{Context, Result};
use async_trait::async_trait;
use aws_sdk_s3::{
    config::{Credentials, Region},
    primitives::ByteStream,
    Client as S3Client, Config as S3Config,
};
use reqwest::Client as HttpClient;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct MediaConfig {
    pub backend: String,
    pub local_dir: PathBuf,
    pub webdav_base_url: Option<String>,
    pub webdav_username: Option<String>,
    pub webdav_password: Option<String>,
    pub webdav_bearer_token: Option<String>,
    pub s3_region: Option<String>,
    pub s3_bucket: Option<String>,
    pub s3_endpoint: Option<String>,
    pub s3_access_key: Option<String>,
    pub s3_secret_key: Option<String>,
    pub s3_path_style: bool,
}

pub struct MediaSaved {
    pub storage_key: String,
    pub media_type: String,
    pub size: u64,
}

#[async_trait]
pub trait MediaBackend: Send + Sync {
    async fn save_upload(&self, key: &str, media_type: &str, bytes: &[u8]) -> Result<MediaSaved>;
    async fn load(&self, key: &str) -> Result<Vec<u8>>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn health_check(&self) -> Result<()>;
}

pub struct LocalMediaBackend {
    dir: PathBuf,
}

impl LocalMediaBackend {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

#[async_trait]
impl MediaBackend for LocalMediaBackend {
    async fn save_upload(&self, key: &str, media_type: &str, bytes: &[u8]) -> Result<MediaSaved> {
        let path = self.dir.join(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("create media dir")?;
        }
        std::fs::write(&path, bytes).context("write media file")?;
        Ok(MediaSaved {
            storage_key: key.to_string(),
            media_type: media_type.to_string(),
            size: bytes.len() as u64,
        })
    }

    async fn load(&self, key: &str) -> Result<Vec<u8>> {
        let path = self.dir.join(key);
        let bytes = std::fs::read(&path).with_context(|| format!("read media {path:?}"))?;
        Ok(bytes)
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.dir.join(key);
        if path.exists() {
            std::fs::remove_file(&path).with_context(|| format!("delete media {path:?}"))?;
        }
        Ok(())
    }

    async fn health_check(&self) -> Result<()> {
        std::fs::create_dir_all(&self.dir).context("ensure media dir")?;
        Ok(())
    }
}

pub struct WebDavMediaBackend {
    base_url: String,
    username: Option<String>,
    password: Option<String>,
    bearer_token: Option<String>,
    http: HttpClient,
}

impl WebDavMediaBackend {
    pub fn new(
        base_url: String,
        username: Option<String>,
        password: Option<String>,
        bearer_token: Option<String>,
        http: HttpClient,
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
    async fn save_upload(&self, key: &str, media_type: &str, bytes: &[u8]) -> Result<MediaSaved> {
        let url = format!("{}/{}", self.base_url, key);
        let mut req = self.http.put(&url).body(bytes.to_vec());
        req = req.header("Content-Type", media_type);
        if let Some(tok) = &self.bearer_token {
            req = req.header("Authorization", format!("Bearer {}", tok));
        } else if let (Some(u), Some(p)) = (&self.username, &self.password) {
            req = req.basic_auth(u, Some(p));
        }
        let resp = req.send().await.context("webdav put")?;
        let status = resp.status();
        if !status.is_success() && status.as_u16() != 201 && status.as_u16() != 204 {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("webdav upload failed: {} {}", status, text);
        }
        Ok(MediaSaved {
            storage_key: key.to_string(),
            media_type: media_type.to_string(),
            size: bytes.len() as u64,
        })
    }

    async fn load(&self, key: &str) -> Result<Vec<u8>> {
        let url = format!("{}/{}", self.base_url, key);
        let mut req = self.http.get(&url);
        if let Some(tok) = &self.bearer_token {
            req = req.header("Authorization", format!("Bearer {}", tok));
        } else if let (Some(u), Some(p)) = (&self.username, &self.password) {
            req = req.basic_auth(u, Some(p));
        }
        let resp = req.send().await.context("webdav get")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("webdav get failed: {} {}", status, text);
        }
        Ok(resp.bytes().await?.to_vec())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let url = format!("{}/{}", self.base_url, key);
        let mut req = self.http.request(reqwest::Method::DELETE, &url);
        if let Some(tok) = &self.bearer_token {
            req = req.header("Authorization", format!("Bearer {}", tok));
        } else if let (Some(u), Some(p)) = (&self.username, &self.password) {
            req = req.basic_auth(u, Some(p));
        }
        let resp = req.send().await.context("webdav delete")?;
        if !resp.status().is_success() && resp.status().as_u16() != 404 {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("webdav delete failed: {} {}", status, text);
        }
        Ok(())
    }

    async fn health_check(&self) -> Result<()> {
        let mut req = self.http.request(reqwest::Method::OPTIONS, &self.base_url);
        if let Some(tok) = &self.bearer_token {
            req = req.header("Authorization", format!("Bearer {}", tok));
        } else if let (Some(u), Some(p)) = (&self.username, &self.password) {
            req = req.basic_auth(u, Some(p));
        }
        let resp = req.send().await.context("webdav options")?;
        if !resp.status().is_success() {
            anyhow::bail!("webdav health failed: {}", resp.status());
        }
        Ok(())
    }
}

pub struct S3MediaBackend {
    client: S3Client,
    bucket: String,
}

impl S3MediaBackend {
    pub fn new(client: S3Client, bucket: String) -> Self {
        Self { client, bucket }
    }
}

#[async_trait]
impl MediaBackend for S3MediaBackend {
    async fn save_upload(&self, key: &str, media_type: &str, bytes: &[u8]) -> Result<MediaSaved> {
        let body = ByteStream::from(bytes.to_vec());
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(media_type)
            .body(body)
            .send()
            .await
            .context("s3 put")?;
        Ok(MediaSaved {
            storage_key: key.to_string(),
            media_type: media_type.to_string(),
            size: bytes.len() as u64,
        })
    }

    async fn load(&self, key: &str) -> Result<Vec<u8>> {
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .context("s3 get")?;
        let data = resp.body.collect().await.context("s3 body")?;
        Ok(data.into_bytes().to_vec())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .context("s3 delete")?;
        Ok(())
    }

    async fn health_check(&self) -> Result<()> {
        self.client
            .head_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .context("s3 head_bucket")?;
        Ok(())
    }
}

pub async fn build_media_backend(
    cfg: &MediaConfig,
    http: HttpClient,
) -> Result<Box<dyn MediaBackend>> {
    match cfg.backend.as_str() {
        "local" => Ok(Box::new(LocalMediaBackend::new(cfg.local_dir.clone()))),
        "webdav" => {
            let base = cfg.webdav_base_url.clone().ok_or_else(|| {
                anyhow::anyhow!("media.backend=webdav requires FEDI3_RELAY_MEDIA_WEBDAV_BASE_URL")
            })?;
            Ok(Box::new(WebDavMediaBackend::new(
                base,
                cfg.webdav_username.clone(),
                cfg.webdav_password.clone(),
                cfg.webdav_bearer_token.clone(),
                http,
            )))
        }
        "s3" => {
            let region = cfg.s3_region.clone().ok_or_else(|| {
                anyhow::anyhow!("media.backend=s3 requires FEDI3_RELAY_MEDIA_S3_REGION")
            })?;
            let bucket = cfg.s3_bucket.clone().ok_or_else(|| {
                anyhow::anyhow!("media.backend=s3 requires FEDI3_RELAY_MEDIA_S3_BUCKET")
            })?;
            let access = cfg.s3_access_key.clone().ok_or_else(|| {
                anyhow::anyhow!("media.backend=s3 requires FEDI3_RELAY_MEDIA_S3_ACCESS_KEY")
            })?;
            let secret = cfg.s3_secret_key.clone().ok_or_else(|| {
                anyhow::anyhow!("media.backend=s3 requires FEDI3_RELAY_MEDIA_S3_SECRET_KEY")
            })?;
            let credentials = Credentials::new(access, secret, None, None, "relay");
            let mut builder = S3Config::builder()
                .region(Region::new(region))
                .credentials_provider(credentials)
                .force_path_style(cfg.s3_path_style);
            if let Some(endpoint) = cfg.s3_endpoint.clone() {
                builder = builder.endpoint_url(endpoint);
            }
            let client = S3Client::from_conf(builder.build());
            Ok(Box::new(S3MediaBackend::new(client, bucket)))
        }
        other => anyhow::bail!("unsupported media.backend: {other}"),
    }
}

pub fn sanitize_key(key: &str) -> String {
    let trimmed = key.trim().trim_start_matches('/');
    trimmed.replace('\\', "/")
}
