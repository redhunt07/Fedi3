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
use axum::body::Body;
use bytes::Bytes;
use reqwest::Client as HttpClient;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;

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
#[allow(clippy::double_must_use)]
pub trait MediaBackend: Send + Sync {
    async fn save_upload(&self, key: &str, media_type: &str, bytes: &Bytes) -> Result<MediaSaved>;
    async fn load(&self, key: &str, range: Option<(u64, u64)>) -> Result<Body>;
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
    async fn save_upload(&self, key: &str, media_type: &str, bytes: &Bytes) -> Result<MediaSaved> {
        let path = self.dir.join(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("create media dir")?;
        }
        let mut file = tokio::fs::File::create(&path)
            .await
            .context("create media file")?;
        file.write_all(bytes).await.context("write media file")?;
        file.flush().await.context("flush media file")?;
        Ok(MediaSaved {
            storage_key: key.to_string(),
            media_type: media_type.to_string(),
            size: bytes.len() as u64,
        })
    }

    async fn load(&self, key: &str, range: Option<(u64, u64)>) -> Result<Body> {
        let path = self.dir.join(key);
        let mut file = tokio::fs::File::open(&path)
            .await
            .with_context(|| format!("read media {path:?}"))?;
        if let Some((start, end)) = range {
            anyhow::ensure!(
                end < file.metadata().await?.len(),
                "media range outside file"
            );
            file.seek(std::io::SeekFrom::Start(start)).await?;
            Ok(Body::from_stream(ReaderStream::new(
                file.take(end - start + 1),
            )))
        } else {
            Ok(Body::from_stream(ReaderStream::new(file)))
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.dir.join(key);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).with_context(|| format!("delete media {path:?}")),
        }
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
    async fn save_upload(&self, key: &str, media_type: &str, bytes: &Bytes) -> Result<MediaSaved> {
        let url = format!("{}/{}", self.base_url, key);
        let mut req = self.http.put(&url).body(bytes.clone());
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

    async fn load(&self, key: &str, range: Option<(u64, u64)>) -> Result<Body> {
        let url = format!("{}/{}", self.base_url, key);
        let mut req = self.http.get(&url);
        if let Some((start, end)) = range {
            req = req.header("Range", format!("bytes={start}-{end}"));
        }
        if let Some(tok) = &self.bearer_token {
            req = req.header("Authorization", format!("Bearer {}", tok));
        } else if let (Some(u), Some(p)) = (&self.username, &self.password) {
            req = req.basic_auth(u, Some(p));
        }
        let resp = req.send().await.context("webdav get")?;
        if let Some((start, end)) = range {
            anyhow::ensure!(
                resp.status() == reqwest::StatusCode::PARTIAL_CONTENT,
                "WebDAV backend ignored range"
            );
            let expected = format!("bytes {start}-{end}/");
            anyhow::ensure!(
                resp.headers()
                    .get("content-range")
                    .and_then(|h| h.to_str().ok())
                    .map(|h| h.starts_with(&expected))
                    .unwrap_or(false),
                "invalid WebDAV range response"
            );
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("webdav get failed: {} {}", status, text);
        }
        let stream = futures_util::stream::try_unfold(resp, |mut resp| async move {
            let chunk = resp.chunk().await?;
            Ok::<_, reqwest::Error>(chunk.map(|chunk| (chunk, resp)))
        });
        Ok(Body::from_stream(stream))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let url = format!("{}/{}", self.base_url, key);
        let mut req = self.http.delete(&url);
        if let Some(tok) = &self.bearer_token {
            req = req.header("Authorization", format!("Bearer {}", tok));
        } else if let (Some(u), Some(p)) = (&self.username, &self.password) {
            req = req.basic_auth(u, Some(p));
        }
        let resp = req.send().await.context("webdav delete")?;
        if !resp.status().is_success() && resp.status().as_u16() != 404 {
            anyhow::bail!("webdav delete failed: {}", resp.status());
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
    async fn save_upload(&self, key: &str, media_type: &str, bytes: &Bytes) -> Result<MediaSaved> {
        let body = ByteStream::from(bytes.clone());
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

    async fn load(&self, key: &str, range: Option<(u64, u64)>) -> Result<Body> {
        let mut request = self.client.get_object().bucket(&self.bucket).key(key);
        if let Some((start, end)) = range {
            request = request.range(format!("bytes={start}-{end}"));
        }
        let resp = request.send().await.context("s3 get")?;
        Ok(Body::from_stream(ReaderStream::new(
            resp.body.into_async_read(),
        )))
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
    let normalized = trimmed.replace('\\', "/");
    // Explicitly reject traversal segments instead of relying on callers to
    // never generate them (L-15).
    let cleaned: Vec<&str> = normalized
        .split('/')
        .filter(|seg| !seg.is_empty() && *seg != "." && *seg != "..")
        .collect();
    cleaned.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn s3_current_http_client_sends_signed_uploads_without_legacy_tls() {
        use axum::{routing::put, Router};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let sender = std::sync::Arc::new(tokio::sync::Mutex::new(Some(sender)));
        let router = Router::new().route(
            "/test-bucket/media.bin",
            put(move |request: axum::http::Request<Body>| {
                let sender = sender.clone();
                async move {
                    let signed = request
                        .headers()
                        .get("authorization")
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .starts_with("AWS4-HMAC-SHA256 ");
                    let body = axum::body::to_bytes(request.into_body(), 4096)
                        .await
                        .unwrap();
                    let _ = sender.lock().await.take().unwrap().send((signed, body));
                    axum::http::StatusCode::OK
                }
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let config = S3Config::builder()
            .region(Region::new("us-east-1"))
            .credentials_provider(Credentials::new(
                "test-access",
                "test-secret",
                None,
                None,
                "test",
            ))
            .force_path_style(true)
            .endpoint_url(endpoint)
            .build();
        let backend = S3MediaBackend::new(S3Client::from_conf(config), "test-bucket".into());
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            backend.save_upload(
                "media.bin",
                "application/octet-stream",
                &Bytes::from_static(b"media"),
            ),
        )
        .await;
        server.abort();
        assert_eq!(result.unwrap().unwrap().size, 5);
        let (signed, body) = receiver.await.unwrap();
        assert!(
            signed,
            "S3 requests must still be signed after removing legacy TLS"
        );
        assert_eq!(body.as_ref(), b"media");
    }

    #[tokio::test]
    async fn webdav_returns_a_stream_before_the_remote_file_finishes() {
        use axum::{routing::get, Router};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (sender, receiver) = tokio::sync::mpsc::channel::<std::io::Result<Bytes>>(4);
        sender.send(Ok(Bytes::from_static(b"first"))).await.unwrap();
        let receiver = std::sync::Arc::new(tokio::sync::Mutex::new(Some(receiver)));
        let router = Router::new().route(
            "/media",
            get(move || {
                let receiver = receiver.clone();
                async move {
                    let receiver = receiver.lock().await.take().unwrap();
                    let stream =
                        futures_util::stream::unfold(receiver, |mut receiver| async move {
                            receiver.recv().await.map(|chunk| (chunk, receiver))
                        });
                    Body::from_stream(stream)
                }
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let backend = WebDavMediaBackend::new(base, None, None, None, HttpClient::new());
        let body = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            backend.load("media", None),
        )
        .await
        .expect("backend buffered the entire response")
        .unwrap();
        let mut chunks = body.into_data_stream();
        assert_eq!(chunks.next().await.unwrap().unwrap().as_ref(), b"first");
        sender.send(Ok(Bytes::from_static(b"last"))).await.unwrap();
        drop(sender);
        let mut tail = Vec::new();
        while let Some(chunk) = chunks.next().await {
            tail.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(tail, b"last");
        server.abort();
    }
}
