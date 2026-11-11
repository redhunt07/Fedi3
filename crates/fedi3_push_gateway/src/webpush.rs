/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

//! Web Push message encryption (RFC 8188 / RFC 8291) and VAPID authentication.
//!
//! UnifiedPush distributors fall into two groups:
//! - Web Push distributors (e.g. NextPush) return subscription keys; the
//!   gateway encrypts the wakeup and signs the request with VAPID.
//! - Plain HTTP distributors (e.g. ntfy) return a topic URL; the gateway POSTs
//!   the opaque JSON wakeup to it without encryption.
//!
//! Both modes carry only `{"event":"sync"}`; message contents stay in the
//! relay spool.

use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes128Gcm, Nonce};
use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use fedi3_protocol::net::is_public_ip;
use hkdf::Hkdf;
use p256::ecdh::EphemeralSecret;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::elliptic_curve::sec1::ToSec1Point;
use p256::elliptic_curve::Generate;
use p256::{PublicKey, SecretKey};
use rand::rand_core::UnwrapErr;
use rand::rngs::SysRng;
use rand::Rng;
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use sha2::Sha256;

/// Record size used by browsers and UnifiedPush distributors (RFC 8188).
const RECORD_SIZE: u32 = 4096;
const MAX_TTL_SECS: u64 = 86_400;
const VAPID_LIFETIME_SECS: u64 = 12 * 3600;
const WAKE_PAYLOAD: &[u8] = br#"{"event":"sync"}"#;

/// Subscription keys returned by a Web Push distributor.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebPushKeys {
    pub pub_key: String,
    pub auth: String,
}

pub struct WakeTarget {
    pub url: Url,
    client_pub: PublicKey,
    auth: [u8; 16],
}

pub struct Encrypted {
    pub body: Vec<u8>,
}

/// Validate a device endpoint.
///
/// The endpoint must be public HTTPS; the same policy as relay-side push
/// registration, so a compromised registration cannot reach private networks.
pub fn validate_endpoint(endpoint: &str) -> Result<Url> {
    let url = Url::parse(endpoint).context("invalid endpoint URL")?;
    if url.scheme() != "https" {
        bail!("endpoint must use https");
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("endpoint must not carry credentials");
    }
    let Some(host) = url.host_str() else {
        bail!("endpoint has no host");
    };
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !is_public_ip(&ip) {
            bail!("endpoint host is not public");
        }
    }
    Ok(url)
}

/// Validate Web Push subscription keys.
pub fn validate_keys(keys: &WebPushKeys) -> Result<(PublicKey, [u8; 16])> {
    let pub_bytes = URL_SAFE_NO_PAD
        .decode(&keys.pub_key)
        .context("pub_key is not base64url")?;
    if pub_bytes.len() != 65 || pub_bytes[0] != 4 {
        bail!("pub_key must be an uncompressed P-256 key");
    }
    let client_pub = PublicKey::from_sec1_bytes(&pub_bytes).context("invalid pub_key")?;
    let auth_bytes = URL_SAFE_NO_PAD
        .decode(&keys.auth)
        .context("auth is not base64url")?;
    if auth_bytes.len() != 16 {
        bail!("auth must be 16 bytes");
    }
    let mut auth = [0u8; 16];
    auth.copy_from_slice(&auth_bytes);
    Ok((client_pub, auth))
}

pub fn target(url: Url, keys: &WebPushKeys) -> Result<WakeTarget> {
    let (client_pub, auth) = validate_keys(keys)?;
    Ok(WakeTarget {
        url,
        client_pub,
        auth,
    })
}

/// Encrypt one `aes128gcm` record for the subscription (RFC 8291).
pub fn encrypt(target: &WakeTarget, payload: &[u8]) -> Result<Encrypted> {
    let mut salt = [0u8; 16];
    let mut rng = UnwrapErr(SysRng);
    rng.fill_bytes(&mut salt);
    let ephemeral = EphemeralSecret::generate();
    let server_pub = ephemeral.public_key();
    let shared = ephemeral.diffie_hellman(&target.client_pub);
    let client_pub_bytes = target.client_pub.to_sec1_point(false).as_bytes().to_vec();
    let server_pub_bytes = server_pub.to_sec1_point(false).as_bytes().to_vec();
    let mut info = b"WebPush: info\x00".to_vec();
    info.extend_from_slice(&client_pub_bytes);
    info.extend_from_slice(&server_pub_bytes);
    let mut ikm = [0u8; 32];
    Hkdf::<Sha256>::new(Some(&target.auth), shared.raw_secret_bytes())
        .expand(&info, &mut ikm)
        .map_err(|_| anyhow!("HKDF extract failed"))?;
    let hk = Hkdf::<Sha256>::new(Some(&salt), &ikm);
    let mut cek = [0u8; 16];
    hk.expand(b"Content-Encoding: aes128gcm\x00", &mut cek)
        .map_err(|_| anyhow!("HKDF CEK failed"))?;
    let mut nonce = [0u8; 12];
    hk.expand(b"Content-Encoding: nonce\x00", &mut nonce)
        .map_err(|_| anyhow!("HKDF nonce failed"))?;
    let cipher = Aes128Gcm::new_from_slice(&cek).map_err(|_| anyhow!("invalid AES key"))?;
    let mut plaintext = payload.to_vec();
    plaintext.push(0x02);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_slice())
        .map_err(|_| anyhow!("encryption failed"))?;
    let mut body = Vec::with_capacity(16 + 4 + 1 + server_pub_bytes.len() + ciphertext.len());
    body.extend_from_slice(&salt);
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(server_pub_bytes.len() as u8);
    body.extend_from_slice(&server_pub_bytes);
    body.extend_from_slice(&ciphertext);
    Ok(Encrypted { body })
}

/// VAPID key pair used to authenticate Web Push requests.
pub struct VapidKey {
    signing: SigningKey,
    private_b64: String,
    public_b64: String,
}

impl VapidKey {
    pub fn from_private_b64(private: &str) -> Result<Self> {
        let bytes = URL_SAFE_NO_PAD
            .decode(private)
            .context("VAPID private key is not base64url")?;
        if bytes.len() != 32 {
            bail!("VAPID private key must be 32 bytes");
        }
        let secret = SecretKey::from_slice(&bytes).context("invalid VAPID private key")?;
        let signing = SigningKey::from_slice(&bytes).context("invalid VAPID signing key")?;
        let public = secret.public_key();
        let public_b64 = URL_SAFE_NO_PAD.encode(public.to_sec1_point(false).as_bytes());
        Ok(Self {
            signing,
            private_b64: private.to_owned(),
            public_b64,
        })
    }

    pub fn generate() -> Self {
        let secret = SecretKey::generate();
        let signing = SigningKey::from_slice(secret.to_bytes().as_slice()).expect("valid scalar");
        let public = secret.public_key();
        let public_b64 = URL_SAFE_NO_PAD.encode(public.to_sec1_point(false).as_bytes());
        let private_b64 = URL_SAFE_NO_PAD.encode(secret.to_bytes().as_slice());
        Self {
            signing,
            private_b64,
            public_b64,
        }
    }

    pub fn private_b64(&self) -> &str {
        &self.private_b64
    }

    pub fn public_b64(&self) -> &str {
        &self.public_b64
    }

    /// Build the `Authorization: vapid t=..., k=...` header for an endpoint.
    pub fn authorization(&self, endpoint: &Url, subject: &str) -> Result<String> {
        let aud = endpoint.origin().ascii_serialization();
        let exp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock before epoch")?
            .as_secs()
            + VAPID_LIFETIME_SECS;
        let header = serde_json::json!({"typ": "JWT", "alg": "ES256"});
        let claims = serde_json::json!({"aud": aud, "exp": exp, "sub": subject});
        let h = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header)?);
        let p = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims)?);
        let input = format!("{h}.{p}");
        let signature: Signature = self.signing.sign(input.as_bytes());
        let sig = URL_SAFE_NO_PAD.encode(signature.to_bytes().as_slice());
        Ok(format!("vapid t={input}.{sig}, k={}", self.public_b64))
    }
}

/// Send an encrypted wakeup to a Web Push distributor endpoint.
pub async fn send_webpush(
    client: &Client,
    target: &WakeTarget,
    encrypted: &Encrypted,
    vapid: &VapidKey,
    subject: &str,
    ttl_secs: u64,
) -> Result<StatusCode> {
    let ttl = ttl_secs.clamp(1, MAX_TTL_SECS);
    let authorization = vapid.authorization(&target.url, subject)?;
    let response = client
        .post(target.url.clone())
        .header("Content-Encoding", "aes128gcm")
        .header("Content-Type", "application/octet-stream")
        .header("TTL", ttl.to_string())
        .header("Authorization", authorization)
        .body(encrypted.body.clone())
        .send()
        .await
        .context("push endpoint request failed")?;
    Ok(response.status())
}

/// Publish the plain wakeup to a topic-style UnifiedPush endpoint (e.g. ntfy).
///
/// The endpoint is public and only carries the opaque wakeup, so anonymous
/// write access is enough; no encryption key or VAPID credential is involved.
pub async fn send_plain(client: &Client, endpoint: &Url, ttl_secs: u64) -> Result<StatusCode> {
    let ttl = ttl_secs.clamp(1, MAX_TTL_SECS);
    let response = client
        .post(endpoint.clone())
        .header("Content-Type", "application/json")
        .header("TTL", ttl.to_string())
        .body(WAKE_PAYLOAD.to_vec())
        .send()
        .await
        .context("push endpoint request failed")?;
    Ok(response.status())
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::signature::Verifier;
    use p256::ecdsa::VerifyingKey;
    use p256::elliptic_curve::sec1::ToSec1Point;

    fn decode_jwt_part(part: &str) -> serde_json::Value {
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(part).unwrap()).unwrap()
    }

    #[test]
    fn round_trip_decrypts_with_the_subscription_private_key() {
        let client_secret = EphemeralSecret::generate();
        let client_pub = client_secret.public_key();
        let auth = [7u8; 16];
        let keys = WebPushKeys {
            pub_key: URL_SAFE_NO_PAD.encode(client_pub.to_sec1_point(false).as_bytes()),
            auth: URL_SAFE_NO_PAD.encode(auth),
        };
        let url = validate_endpoint("https://ntfy.example/up123").unwrap();
        let target = target(url, &keys).unwrap();
        let encrypted = encrypt(&target, WAKE_PAYLOAD).unwrap();

        let body = &encrypted.body;
        assert!(body.len() > 86);
        let salt = &body[0..16];
        assert_eq!(
            u32::from_be_bytes(body[16..20].try_into().unwrap()),
            RECORD_SIZE
        );
        let idlen = body[20] as usize;
        assert_eq!(idlen, 65);
        let server_pub_bytes = &body[21..21 + idlen];
        let ciphertext = &body[21 + idlen..];
        let server_pub = PublicKey::from_sec1_bytes(server_pub_bytes).unwrap();
        let shared = client_secret.diffie_hellman(&server_pub);
        let mut info = b"WebPush: info\x00".to_vec();
        info.extend_from_slice(client_pub.to_sec1_point(false).as_bytes());
        info.extend_from_slice(server_pub_bytes);
        let mut ikm = [0u8; 32];
        Hkdf::<Sha256>::new(Some(&auth), shared.raw_secret_bytes())
            .expand(&info, &mut ikm)
            .unwrap();
        let hk = Hkdf::<Sha256>::new(Some(salt), &ikm);
        let mut cek = [0u8; 16];
        hk.expand(b"Content-Encoding: aes128gcm\x00", &mut cek)
            .unwrap();
        let mut nonce = [0u8; 12];
        hk.expand(b"Content-Encoding: nonce\x00", &mut nonce)
            .unwrap();
        let plain = Aes128Gcm::new_from_slice(&cek)
            .unwrap()
            .decrypt(Nonce::from_slice(&nonce), ciphertext)
            .unwrap();
        assert_eq!(&plain[..plain.len() - 1], WAKE_PAYLOAD);
        assert_eq!(plain[plain.len() - 1], 0x02);
    }

    #[test]
    fn vapid_header_is_a_valid_es256_jwt_for_the_endpoint_origin() {
        let vapid = VapidKey::generate();
        let url = Url::parse("https://ntfy.example/up123").unwrap();
        let header = vapid
            .authorization(&url, "mailto:admin@example.com")
            .unwrap();
        let rest = header.strip_prefix("vapid t=").unwrap();
        let (jwt, k) = rest.split_once(", k=").unwrap();
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let jwt_header = decode_jwt_part(parts[0]);
        assert_eq!(jwt_header["alg"], "ES256");
        let claims = decode_jwt_part(parts[1]);
        assert_eq!(claims["aud"], "https://ntfy.example");
        assert_eq!(claims["sub"], "mailto:admin@example.com");
        assert!(claims["exp"].as_u64().unwrap() > 0);
        let public = VerifyingKey::from_sec1_bytes(&URL_SAFE_NO_PAD.decode(k).unwrap()).unwrap();
        let signature = Signature::from_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap()).unwrap();
        public
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
            .unwrap();
    }

    #[test]
    fn accepts_plain_topic_endpoints() {
        let url = validate_endpoint("https://ntfy.example/upABC").unwrap();
        assert_eq!(url.path(), "/upABC");
        assert!(validate_endpoint("http://ntfy.example/upABC").is_err());
        assert!(validate_endpoint("https://127.0.0.1/upABC").is_err());
        assert!(validate_endpoint("https://10.0.0.5/upABC").is_err());
    }

    #[test]
    fn rejects_wrong_key_shapes() {
        let auth = URL_SAFE_NO_PAD.encode([0u8; 16]);
        let short = WebPushKeys {
            pub_key: URL_SAFE_NO_PAD.encode([4u8; 64]),
            auth: auth.clone(),
        };
        assert!(validate_keys(&short).is_err());
        let long_auth = WebPushKeys {
            pub_key: URL_SAFE_NO_PAD.encode([4u8; 65]),
            auth: URL_SAFE_NO_PAD.encode([0u8; 17]),
        };
        assert!(validate_keys(&long_auth).is_err());
    }
}
