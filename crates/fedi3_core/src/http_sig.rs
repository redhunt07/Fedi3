/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use fedi3_protocol::crypto;
use http::{HeaderMap, Method, Uri};
use httpdate::parse_http_date;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::RwLock;

/// Hard cap on the key cache so an attacker using endless `keyId` paths cannot grow it forever.
const KEY_CACHE_MAX: usize = 10_000;
/// Hard cap on the actor document size fetched while resolving a key.
const MAX_ACTOR_DOC_BYTES: usize = crate::net_policy::MAX_ACTOR_DOC_BYTES;

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
    /// Last time this entry was served, used for LRU eviction (H-46).
    last_used: std::time::Instant,
}

impl KeyResolver {
    pub fn new() -> Self {
        // Shared federation client: bounded timeouts and redirects, explicit user agent.
        let client = crate::net_policy::federation_client();
        Self {
            client,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn resolve_actor_summary_for_key_id(&self, key_id: &str) -> Result<ActorSummary> {
        let key_id = key_id.trim();
        if key_id.is_empty() {
            return Err(anyhow!("empty keyId"));
        }
        let (actor_url, wanted_key_id) = match key_id.split_once('#') {
            Some((actor, _frag)) => (actor.to_string(), Some(key_id.to_string())),
            None => (key_id.to_string(), None),
        };
        if !(actor_url.starts_with("http://") || actor_url.starts_with("https://")) {
            return Err(anyhow!("keyId actor is not an http(s) URL: {actor_url}"));
        }

        if let Some(hit) = self.get_cached(&actor_url).await {
            if wanted_key_id
                .as_deref()
                .map(|k| k == hit.key_id)
                .unwrap_or(true)
            {
                return Ok(ActorSummary {
                    actor_url,
                    key_id: hit.key_id,
                    public_key_pem: hit.pem,
                    is_fedi3: hit.is_fedi3,
                });
            }
        }

        // SSRF guard: the actor URL comes from an attacker-controlled keyId.
        crate::net_policy::check_url_allowed(&actor_url).await?;

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

        // Canonical URL after redirects: the relay may redirect its own actor URL to the
        // identity anchor, so the identity check must use where the document actually came from.
        let final_url = resp.url().to_string();

        // Streamed, capped read: a hostile server cannot make us buffer gigabytes.
        let body = crate::net_policy::read_capped(resp, MAX_ACTOR_DOC_BYTES).await?;
        let text = String::from_utf8_lossy(&body).into_owned();
        if text.len() > MAX_ACTOR_DOC_BYTES {
            return Err(anyhow!("actor document too large: {} bytes", text.len()));
        }
        let actor: ActorDoc = serde_json::from_str(&text)
            .with_context(|| format!("parse actor json from {actor_url}"))?;

        // The actor document must identify itself with the URL we fetched (or at least the same
        // origin): otherwise a server could serve a valid key belonging to someone else.
        if let Some(id) = actor.id.as_deref() {
            let same_origin = match (Uri::try_from(id), Uri::try_from(final_url.as_str())) {
                (Ok(a), Ok(b)) => {
                    a.scheme_str() == b.scheme_str() && a.host() == b.host() && a.port() == b.port()
                }
                _ => false,
            };
            if !same_origin && id.trim_end_matches('/') != final_url.trim_end_matches('/') {
                return Err(anyhow!(
                    "actor id mismatch: document id={id} fetched={final_url}"
                ));
            }
        }

        let pk = actor
            .public_key
            .ok_or_else(|| anyhow!("actor missing publicKey"))?;
        // If the caller asked for a specific key id, it must match the advertised one.
        if let Some(wanted) = wanted_key_id.as_deref() {
            if pk.id.trim() != wanted.trim() {
                return Err(anyhow!(
                    "publicKey id mismatch: requested={wanted} advertised={}",
                    pk.id
                ));
            }
        }
        let pem = pk.public_key_pem;
        let actual_key_id = pk.id;

        let is_fedi3 = actor.fedi3_peer_id.is_some()
            || actor
                .endpoints
                .as_ref()
                .and_then(|e| e.fedi3_peer_id.as_ref())
                .is_some();

        self.put_cached(
            &actor_url,
            pem.clone(),
            actual_key_id.clone(),
            is_fedi3,
            Duration::from_secs(300),
        )
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
        if let Some(v) = cache.get_mut(actor_url) {
            if v.expires_at > now {
                v.last_used = now;
                return Some(v.clone());
            }
        }
        cache.remove(actor_url);
        None
    }

    async fn put_cached(
        &self,
        actor_url: &str,
        pem: String,
        key_id: String,
        is_fedi3: bool,
        ttl: Duration,
    ) {
        let mut cache = self.cache.write().await;
        let now = std::time::Instant::now();
        if cache.len() >= KEY_CACHE_MAX {
            // Drop expired entries first, then the least recently used ones (H-46: LRU, not
            // arbitrary eviction, so hot actors survive a keyId flood).
            cache.retain(|_, v| v.expires_at > now);
            if cache.len() >= KEY_CACHE_MAX {
                let overflow = cache.len() - KEY_CACHE_MAX + 1;
                let mut victims: Vec<(String, std::time::Instant)> = cache
                    .iter()
                    .map(|(k, v)| (k.clone(), v.last_used))
                    .collect();
                victims.sort_by_key(|(_, used)| *used);
                for (k, _) in victims.into_iter().take(overflow) {
                    cache.remove(&k);
                }
            }
        }
        cache.insert(
            actor_url.to_string(),
            CachedActor {
                pem,
                key_id,
                is_fedi3,
                expires_at: now + ttl,
                last_used: now,
            },
        );
    }
}

impl Default for KeyResolver {
    fn default() -> Self {
        Self::new()
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
    pub algorithm: Option<String>,
}

/// Parse a `Signature` header following the HTTP Signatures grammar closely enough for
/// Mastodon/Misskey interoperability: quoted strings may contain commas and escaped quotes,
/// parameter names are case-insensitive.
pub fn parse_signature_header(value: &str) -> Result<SignatureParams> {
    let mut map = HashMap::<String, String>::new();
    let bytes = value.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i] == b',' || bytes[i].is_ascii_whitespace()) {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let name_start = i;
        while i < bytes.len() && bytes[i] != b'=' {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let name = value[name_start..i].trim().to_ascii_lowercase();
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut val = String::new();
        if i < bytes.len() && bytes[i] == b'"' {
            i += 1;
            let mut escaped = false;
            while i < bytes.len() {
                let b = bytes[i];
                i += 1;
                if escaped {
                    val.push(b as char);
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'"' {
                    break;
                } else {
                    val.push(b as char);
                }
            }
        } else {
            let val_start = i;
            while i < bytes.len() && bytes[i] != b',' {
                i += 1;
            }
            val = value[val_start..i].trim().to_string();
        }
        if !name.is_empty() {
            map.insert(name, val);
        }
    }

    let key_id = map
        .get("keyid")
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
    let algorithm = map.get("algorithm").cloned();

    let signature = B64
        .decode(signature_b64.trim().as_bytes())
        .context("decode signature")?;

    Ok(SignatureParams {
        key_id,
        headers: headers
            .split_whitespace()
            .map(|s| s.to_ascii_lowercase())
            .collect(),
        signature,
        algorithm,
    })
}

pub fn build_signing_string(
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
    signed_headers: &[String],
) -> Result<String> {
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

/// The minimum set of signed headers required for a POST in Fedi3.
pub const REQUIRED_POST_SIGNED_HEADERS: &[&str] = &["(request-target)", "host", "date", "digest"];

pub fn require_min_signed_headers(signed_headers: &[String]) -> Result<()> {
    for required in REQUIRED_POST_SIGNED_HEADERS {
        if !signed_headers.iter().any(|h| h == required) {
            return Err(anyhow!("signed headers missing: {required}"));
        }
    }
    Ok(())
}

/// Verify a `Digest` header. When `required` is true a missing header is an error (POST);
/// when false a missing header is tolerated (GET requests in the wild).
pub fn verify_digest(headers: &HeaderMap, body: &[u8], required: bool) -> Result<()> {
    let Some(digest) = headers.get("Digest") else {
        return if required {
            Err(anyhow!("missing Digest header"))
        } else {
            Ok(())
        };
    };
    let digest = digest.to_str().context("Digest header not utf8")?;
    let actual = Sha256::digest(body);
    let mut saw_sha256 = false;
    let mut matched = false;
    for part in digest.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((alg, value)) = part.split_once('=') else {
            return Err(anyhow!("invalid Digest header: {part}"));
        };
        if alg.trim().eq_ignore_ascii_case("SHA-256") {
            saw_sha256 = true;
            let expected = B64
                .decode(value.trim().as_bytes())
                .context("decode digest")?;
            if expected.as_slice() == actual.as_slice() {
                matched = true;
            }
        }
    }
    if !saw_sha256 {
        return Err(anyhow!("Digest has no SHA-256 entry: {digest}"));
    }
    if !matched {
        return Err(anyhow!("digest mismatch"));
    }
    Ok(())
}

pub fn verify_digest_if_present(headers: &HeaderMap, body: &[u8]) -> Result<()> {
    verify_digest(headers, body, false)
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

pub fn verify_signature_rsa_sha256(
    public_key_pem: &str,
    signing_string: &str,
    signature: &[u8],
) -> Result<()> {
    crypto::rsa_verify_sha256(public_key_pem, signing_string.as_bytes(), signature)
}

pub fn verify_bytes_rsa_sha256(public_key_pem: &str, bytes: &[u8], signature: &[u8]) -> Result<()> {
    crypto::rsa_verify_sha256(public_key_pem, bytes, signature)
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

    let signed_headers_lower: Vec<String> = signed_headers
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();
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

    let signature = crypto::rsa_sign_sha256(private_key_pem, signing_string.as_bytes())?;
    let sig_b64 = B64.encode(signature);

    let headers_list = signed_headers_lower.join(" ");
    let sig_header = format!(
        "keyId=\"{key_id}\",algorithm=\"rsa-sha256\",headers=\"{headers_list}\",signature=\"{sig_b64}\""
    );
    headers.insert("Signature", sig_header.parse().context("set Signature")?);
    Ok(())
}

pub fn sign_bytes_rsa_sha256(private_key_pem: &str, bytes: &[u8]) -> Result<Vec<u8>> {
    crypto::rsa_sign_sha256(private_key_pem, bytes)
}

#[derive(Debug, Deserialize)]
struct ActorDoc {
    id: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_keypair() -> (String, String) {
        let private_pem = crypto::rsa_generate_private_pem().unwrap();
        let public_pem = crypto::rsa_public_pem(&private_pem).unwrap();
        (private_pem, public_pem)
    }

    fn sample_headers() -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("host", "example.test".parse().unwrap());
        h.insert("date", "Sun, 06 Nov 1994 08:49:37 GMT".parse().unwrap());
        h.insert(
            "digest",
            "SHA-256=47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU="
                .parse()
                .unwrap(),
        );
        h
    }

    #[test]
    fn parse_signature_fields() {
        let p = parse_signature_header(
            "keyId=\"https://a.test/users/a#main-key\",headers=\"(request-target) host date\",signature=\"AAAA\"",
        )
        .unwrap();
        assert_eq!(p.key_id, "https://a.test/users/a#main-key");
        assert_eq!(p.headers, vec!["(request-target)", "host", "date"]);
        assert_eq!(p.signature, vec![0, 0, 0]);
    }

    #[test]
    fn parse_signature_hs2019_with_quoted_comma() {
        let p = parse_signature_header(
            "keyId=\"https://a.test/u#k\",algorithm=\"hs2019\",headers=\"date digest\",signature=\"AA==\"",
        )
        .unwrap();
        assert_eq!(p.algorithm.as_deref(), Some("hs2019"));
        assert_eq!(p.headers, vec!["date", "digest"]);
    }

    #[test]
    fn parse_signature_missing_signature_is_error() {
        assert!(parse_signature_header("keyId=\"x\"").is_err());
    }

    #[test]
    fn build_signing_string_request_target() {
        let uri: Uri = "https://a.test/users/a/outbox?page=true&cursor=1"
            .parse()
            .unwrap();
        let h = sample_headers();
        let s = build_signing_string(
            &Method::POST,
            &uri,
            &h,
            &["(request-target)".to_string(), "host".to_string()],
        )
        .unwrap();
        assert_eq!(
            s,
            "(request-target): post /users/a/outbox?page=true&cursor=1\nhost: example.test"
        );
    }

    #[test]
    fn build_signing_string_missing_header_is_error() {
        let uri: Uri = "https://a.test/x".parse().unwrap();
        let h = HeaderMap::new();
        assert!(build_signing_string(
            &Method::GET,
            &uri,
            &h,
            &["(request-target)".to_string(), "date".to_string()]
        )
        .is_err());
    }

    #[test]
    fn signature_round_trip_and_tamper() {
        let (priv_pem, pub_pem) = test_keypair();
        let uri: Uri = "https://a.test/users/a/inbox".parse().unwrap();
        let mut headers = sample_headers();
        let body = br#"{"type":"Create"}"#;
        sign_request_rsa_sha256(
            &priv_pem,
            "https://a.test/users/a#main-key",
            &Method::POST,
            &uri,
            &mut headers,
            body,
            REQUIRED_POST_SIGNED_HEADERS,
        )
        .unwrap();
        let sig_header = headers.get("Signature").unwrap().to_str().unwrap();
        let parsed = parse_signature_header(sig_header).unwrap();
        let signing = build_signing_string(&Method::POST, &uri, &headers, &parsed.headers).unwrap();
        verify_signature_rsa_sha256(&pub_pem, &signing, &parsed.signature).unwrap();

        let mut tampered = parsed.signature.clone();
        tampered[0] ^= 0xff;
        assert!(verify_signature_rsa_sha256(&pub_pem, &signing, &tampered).is_err());
    }

    #[test]
    fn signature_with_other_key_fails() {
        let (priv_pem, _) = test_keypair();
        let (_, other_pub) = test_keypair();
        let uri: Uri = "https://a.test/inbox".parse().unwrap();
        let mut headers = sample_headers();
        sign_request_rsa_sha256(
            &priv_pem,
            "https://a.test/u#k",
            &Method::POST,
            &uri,
            &mut headers,
            b"body",
            &["(request-target)", "host", "date", "digest"],
        )
        .unwrap();
        let parsed =
            parse_signature_header(headers.get("Signature").unwrap().to_str().unwrap()).unwrap();
        let signing = build_signing_string(&Method::POST, &uri, &headers, &parsed.headers).unwrap();
        assert!(verify_signature_rsa_sha256(&other_pub, &signing, &parsed.signature).is_err());
    }

    #[test]
    fn digest_required_on_post() {
        let headers = HeaderMap::new();
        assert!(verify_digest(&headers, b"x", true).is_err());
        assert!(verify_digest(&headers, b"x", false).is_ok());
    }

    #[test]
    fn digest_mismatch_fails() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "digest",
            "SHA-256=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
                .parse()
                .unwrap(),
        );
        assert!(verify_digest(&headers, b"hello", true).is_err());
    }

    #[test]
    fn digest_multi_algorithm_accepts_sha256() {
        use sha2::Digest as _;
        let body = b"hello";
        let sha = B64.encode(Sha256::digest(body));
        let mut headers = HeaderMap::new();
        headers.insert(
            "digest",
            format!("SHA-512=abcd,SHA-256={sha}").parse().unwrap(),
        );
        verify_digest(&headers, body, true).unwrap();
    }

    #[test]
    fn digest_correct_passes() {
        use sha2::Digest as _;
        let body = b"payload";
        let mut headers = HeaderMap::new();
        headers.insert(
            "digest",
            format!("SHA-256={}", B64.encode(Sha256::digest(body)))
                .parse()
                .unwrap(),
        );
        verify_digest(&headers, body, true).unwrap();
    }

    #[test]
    fn date_skew_is_bounded() {
        let mut headers = HeaderMap::new();
        let now = std::time::SystemTime::now();
        headers.insert("date", httpdate::fmt_http_date(now).parse().unwrap());
        assert!(verify_date(&headers, Duration::from_secs(300)).is_ok());
        headers.insert(
            "date",
            httpdate::fmt_http_date(now - Duration::from_secs(3600))
                .parse()
                .unwrap(),
        );
        assert!(verify_date(&headers, Duration::from_secs(300)).is_err());
        assert!(verify_date(&headers, Duration::from_secs(7200)).is_ok());
    }

    #[test]
    fn unparsable_date_is_error() {
        let mut headers = HeaderMap::new();
        headers.insert("date", "not a date".parse().unwrap());
        assert!(verify_date(&headers, Duration::from_secs(300)).is_err());
    }

    #[test]
    fn signed_header_set_minimum() {
        let ok = vec![
            "(request-target)".to_string(),
            "host".to_string(),
            "date".to_string(),
            "digest".to_string(),
        ];
        require_min_signed_headers(&ok).unwrap();
        let missing = vec!["date".to_string()];
        assert!(require_min_signed_headers(&missing).is_err());
    }

    /// H-46: the key cache must evict the least recently used entry, never a hot one.
    #[tokio::test]
    async fn key_cache_evicts_lru_entry() {
        let resolver = KeyResolver::new();
        let ttl = Duration::from_secs(300);
        for i in 0..KEY_CACHE_MAX {
            resolver
                .put_cached(
                    &format!("https://peer{i}.example/users/u"),
                    "pem".to_string(),
                    format!("https://peer{i}.example/users/u#main-key"),
                    false,
                    ttl,
                )
                .await;
        }
        // Touch the first entry so it becomes the most recently used.
        let hot = "https://peer0.example/users/u";
        assert!(resolver.get_cached(hot).await.is_some());

        resolver
            .put_cached(
                "https://new.example/users/u",
                "pem".to_string(),
                "https://new.example/users/u#main-key".to_string(),
                false,
                ttl,
            )
            .await;

        let cache = resolver.cache.read().await;
        assert!(cache.len() <= KEY_CACHE_MAX, "cache must stay bounded");
        assert!(cache.contains_key(hot), "recently used entry must survive");
        assert!(
            !cache.contains_key("https://peer1.example/users/u"),
            "the least recently used entry must be evicted"
        );
    }
}
