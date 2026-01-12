/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use http::{HeaderMap, Method, Uri};
use httpdate::parse_http_date;
use rsa::{
    pkcs1v15::{SigningKey, VerifyingKey},
    pkcs8::{DecodePrivateKey, DecodePublicKey},
    signature::{RandomizedSigner, SignatureEncoding, Verifier},
    RsaPrivateKey, RsaPublicKey,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct KeyResolver {
    client: reqwest::Client,
    cache: Arc<RwLock<HashMap<String, CachedActor>>>,
}

#[derive(Clone)]
struct CachedActor {
    pem: String,
    key_id: String,
    is_fedi3: bool,
    expires_at: std::time::Instant,
}

impl KeyResolver {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn resolve_actor_summary_for_key_id(&self, key_id: &str) -> Result<ActorSummary> {
        let (actor_url, wanted_key_id) = match key_id.split_once('#') {
            Some((actor, _frag)) => (actor.to_string(), Some(key_id.to_string())),
            None => (key_id.to_string(), None),
        };

        if let Some(hit) = self.get_cached(&actor_url).await {
            if wanted_key_id.as_deref().map(|k| k == hit.key_id).unwrap_or(true) {
                return Ok(ActorSummary {
                    actor_url,
                    key_id: hit.key_id,
                    public_key_pem: hit.pem,
                    is_fedi3: hit.is_fedi3,
                });
            }
        }

        let resp = self
            .client
            .get(&actor_url)
            .header(
                "Accept",
                "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"",
            )
            .send()
            .await
            .with_context(|| format!("fetch actor: {actor_url}"))?
            .error_for_status()
            .with_context(|| format!("actor not ok: {actor_url}"))?;

        let text = resp.text().await?;
        let actor: ActorDoc = serde_json::from_str(&text)
            .with_context(|| format!("parse actor json from {actor_url}"))?;

        let pk = actor.public_key.ok_or_else(|| anyhow!("actor missing publicKey"))?;
        let pem = pk.public_key_pem;
        let actual_key_id = pk.id;

        let is_fedi3 = actor.fedi3_peer_id.is_some()
            || actor
                .endpoints
                .as_ref()
                .and_then(|e| e.fedi3_peer_id.as_ref())
                .is_some();

        self.put_cached(&actor_url, pem.clone(), actual_key_id.clone(), is_fedi3, Duration::from_secs(300))
            .await;

        Ok(ActorSummary {
            actor_url,
            key_id: actual_key_id,
            public_key_pem: pem,
            is_fedi3,
        })
    }

    pub async fn resolve_public_key_pem(&self, key_id: &str) -> Result<String> {
        let summary = self.resolve_actor_summary_for_key_id(key_id).await?;
        Ok(summary.public_key_pem)
    }

    async fn get_cached(&self, actor_url: &str) -> Option<CachedActor> {
        let mut cache = self.cache.write().await;
        let now = std::time::Instant::now();
        if let Some(v) = cache.get(actor_url) {
            if v.expires_at > now {
                return Some(v.clone());
            }
        }
        cache.remove(actor_url);
        None
    }

    async fn put_cached(&self, actor_url: &str, pem: String, key_id: String, is_fedi3: bool, ttl: Duration) {
        let mut cache = self.cache.write().await;
        cache.insert(
            actor_url.to_string(),
            CachedActor {
                pem,
                key_id,
                is_fedi3,
                expires_at: std::time::Instant::now() + ttl,
            },
        );
    }
}

#[derive(Debug, Clone)]
pub struct ActorSummary {
    pub actor_url: String,
    pub key_id: String,
    pub public_key_pem: String,
    pub is_fedi3: bool,
}

#[derive(Debug)]
pub struct SignatureParams {
    pub key_id: String,
    pub headers: Vec<String>,
    pub signature: Vec<u8>,
}

pub fn parse_signature_header(value: &str) -> Result<SignatureParams> {
    // Signature: keyId="...",headers="(request-target) host date",signature="base64..."
    let mut map = HashMap::<String, String>::new();
    for part in value.split(',') {
        let part = part.trim();
        let Some((k, v)) = part.split_once('=') else { continue };
        let v = v.trim().trim_matches('"');
        map.insert(k.trim().to_string(), v.to_string());
    }

    let key_id = map
        .get("keyId")
        .cloned()
        .ok_or_else(|| anyhow!("Signature missing keyId"))?;
    let headers = map
        .get("headers")
        .cloned()
        .unwrap_or_else(|| "date".to_string());
    let signature_b64 = map
        .get("signature")
        .cloned()
        .ok_or_else(|| anyhow!("Signature missing signature"))?;

    let signature = B64
        .decode(signature_b64.as_bytes())
        .context("decode signature")?;

    Ok(SignatureParams {
        key_id,
        headers: headers
            .split_whitespace()
            .map(|s| s.to_ascii_lowercase())
            .collect(),
        signature,
    })
}

pub fn build_signing_string(method: &Method, uri: &Uri, headers: &HeaderMap, signed_headers: &[String]) -> Result<String> {
    let mut out = String::new();
    for (i, name) in signed_headers.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if name == "(request-target)" {
            let path = uri.path();
            let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
            out.push_str("(request-target): ");
            out.push_str(&method.as_str().to_ascii_lowercase());
            out.push(' ');
            out.push_str(path);
            out.push_str(&query);
            continue;
        }

        let header_name = http::header::HeaderName::from_bytes(name.as_bytes())
            .with_context(|| format!("bad signed header name: {name}"))?;
        let value = headers
            .get(&header_name)
            .ok_or_else(|| anyhow!("missing signed header: {name}"))?
            .to_str()
            .with_context(|| format!("invalid header value for {name}"))?;
        out.push_str(name);
        out.push_str(": ");
        out.push_str(value.trim());
    }
    Ok(out)
}

pub fn verify_digest_if_present(headers: &HeaderMap, body: &[u8]) -> Result<()> {
    let Some(digest) = headers.get("Digest") else {
        return Ok(());
    };
    let digest = digest.to_str().context("Digest header not utf8")?;
    // Digest: SHA-256=base64
    let Some((alg, value)) = digest.split_once('=') else {
        return Err(anyhow!("invalid Digest header"));
    };
    if alg.trim().eq_ignore_ascii_case("SHA-256") {
        let expected = B64.decode(value.trim().as_bytes()).context("decode digest")?;
        let actual = Sha256::digest(body);
        if expected.as_slice() != actual.as_slice() {
            return Err(anyhow!("digest mismatch"));
        }
        return Ok(());
    }
    Err(anyhow!("unsupported digest alg: {alg}"))
}

pub fn verify_date(headers: &HeaderMap, max_skew: Duration) -> Result<()> {
    let date = headers
        .get("Date")
        .ok_or_else(|| anyhow!("missing Date header"))?
        .to_str()
        .context("Date header not utf8")?;
    let ts = parse_http_date(date).context("parse Date header")?;
    let now = std::time::SystemTime::now();
    let diff = if now > ts {
        now.duration_since(ts).unwrap_or_default()
    } else {
        ts.duration_since(now).unwrap_or_default()
    };
    if diff > max_skew {
        return Err(anyhow!("Date skew too large: {}s", diff.as_secs()));
    }
    Ok(())
}

pub fn verify_signature_rsa_sha256(public_key_pem: &str, signing_string: &str, signature: &[u8]) -> Result<()> {
    let public_key = RsaPublicKey::from_public_key_pem(public_key_pem)
        .context("parse public key pem")?;
    let verifying_key = VerifyingKey::<Sha256>::new(public_key);
    let sig = rsa::pkcs1v15::Signature::try_from(signature)
        .context("invalid rsa signature bytes")?;
    verifying_key
        .verify(signing_string.as_bytes(), &sig)
        .context("signature verify failed")?;
    Ok(())
}

pub fn verify_bytes_rsa_sha256(public_key_pem: &str, bytes: &[u8], signature: &[u8]) -> Result<()> {
    let public_key = RsaPublicKey::from_public_key_pem(public_key_pem)
        .context("parse public key pem")?;
    let verifying_key = VerifyingKey::<Sha256>::new(public_key);
    let sig = rsa::pkcs1v15::Signature::try_from(signature)
        .context("invalid rsa signature bytes")?;
    verifying_key
        .verify(bytes, &sig)
        .context("signature verify failed")?;
    Ok(())
}

pub fn sign_request_rsa_sha256(
    private_key_pem: &str,
    key_id: &str,
    method: &Method,
    uri: &Uri,
    headers: &mut HeaderMap,
    body: &[u8],
    signed_headers: &[&str],
) -> Result<()> {
    if !headers.contains_key("Date") {
        let date = httpdate::fmt_http_date(std::time::SystemTime::now());
        headers.insert("Date", date.parse().context("set Date")?);
    }

    let signed_headers_lower: Vec<String> = signed_headers.iter().map(|s| s.to_ascii_lowercase()).collect();
    let want_digest = headers.contains_key("Digest")
        || signed_headers_lower.iter().any(|h| h == "digest")
        || !body.is_empty();
    if want_digest && !headers.contains_key("Digest") {
        let digest = Sha256::digest(body);
        let digest_b64 = B64.encode(digest);
        headers.insert(
            "Digest",
            format!("SHA-256={digest_b64}")
                .parse()
                .context("set Digest")?,
        );
    }

    if !headers.contains_key("Host") {
        if let Some(auth) = uri.authority() {
            headers.insert("Host", auth.as_str().parse().context("set Host")?);
        }
    }

    let signing_string = build_signing_string(method, uri, headers, &signed_headers_lower)?;

    let private_key = RsaPrivateKey::from_pkcs8_pem(private_key_pem)
        .context("parse private key pem")?;
    let signing_key = SigningKey::<Sha256>::new(private_key);
    let mut rng = rand::rngs::OsRng;
    let signature = signing_key.sign_with_rng(&mut rng, signing_string.as_bytes());
    let sig_b64 = B64.encode(signature.to_bytes());

    let headers_list = signed_headers_lower.join(" ");
    let sig_header = format!(
        "keyId=\"{key_id}\",algorithm=\"rsa-sha256\",headers=\"{headers_list}\",signature=\"{sig_b64}\""
    );
    headers.insert("Signature", sig_header.parse().context("set Signature")?);
    Ok(())
}

pub fn sign_bytes_rsa_sha256(private_key_pem: &str, bytes: &[u8]) -> Result<Vec<u8>> {
    let private_key = RsaPrivateKey::from_pkcs8_pem(private_key_pem)
        .context("parse private key pem")?;
    let signing_key = SigningKey::<Sha256>::new(private_key);
    let mut rng = rand::rngs::OsRng;
    let signature = signing_key.sign_with_rng(&mut rng, bytes);
    Ok(signature.to_bytes().to_vec())
}

#[derive(Debug, Deserialize)]
struct ActorDoc {
    #[serde(rename = "publicKey")]
    public_key: Option<ActorPublicKey>,
    endpoints: Option<ActorEndpoints>,
    #[serde(rename = "fedi3PeerId")]
    fedi3_peer_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ActorPublicKey {
    id: String,
    #[serde(rename = "publicKeyPem")]
    public_key_pem: String,
}

#[derive(Debug, Deserialize)]
struct ActorEndpoints {
    #[serde(rename = "fedi3PeerId")]
    fedi3_peer_id: Option<String>,
}
