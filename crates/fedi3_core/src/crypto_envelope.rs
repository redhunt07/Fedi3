/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use fedi3_protocol::RelayHttpRequest;

pub fn decrypt_relay_http_request_body(
    private_key_pem: &str,
    mut req: RelayHttpRequest,
) -> RelayHttpRequest {
    let enc = req
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("x-fedi3-encrypted"))
        .map(|(_, v)| v.clone());
    if enc.as_deref() != Some("1") {
        return req;
    }

    use aes_gcm::{aead::Aead, aead::KeyInit, Aes256Gcm, Nonce};
    use rsa::pkcs8::DecodePrivateKey;
    use rsa::{Oaep, RsaPrivateKey};
    use sha2::Sha256;

    let Ok(env_bytes) = B64.decode(req.body_b64.as_bytes()) else {
        return req;
    };
    let Ok(env) = serde_json::from_slice::<serde_json::Value>(&env_bytes) else {
        return req;
    };
    let Some(ek_b64) = env.get("ek_b64").and_then(|v| v.as_str()) else {
        return req;
    };
    let Some(nonce_b64) = env.get("nonce_b64").and_then(|v| v.as_str()) else {
        return req;
    };
    let Some(ct_b64) = env.get("ct_b64").and_then(|v| v.as_str()) else {
        return req;
    };

    let Ok(ek) = B64.decode(ek_b64.as_bytes()) else {
        return req;
    };
    let Ok(nonce_bytes) = B64.decode(nonce_b64.as_bytes()) else {
        return req;
    };
    let Ok(ct) = B64.decode(ct_b64.as_bytes()) else {
        return req;
    };
    if nonce_bytes.len() != 12 {
        return req;
    }

    let Ok(privkey) = RsaPrivateKey::from_pkcs8_pem(private_key_pem) else {
        return req;
    };
    let Ok(key) = privkey.decrypt(Oaep::new::<Sha256>(), &ek) else {
        return req;
    };
    if key.len() != 32 {
        return req;
    }
    let Ok(cipher) = Aes256Gcm::new_from_slice(&key) else {
        return req;
    };
    let nonce = Nonce::from_slice(&nonce_bytes);
    let Ok(pt) = cipher.decrypt(nonce, ct.as_ref()) else {
        return req;
    };

    // Remove encryption marker header before handing to HTTP handler.
    req.headers
        .retain(|(k, _)| !k.eq_ignore_ascii_case("x-fedi3-encrypted"));
    req.body_b64 = B64.encode(pt);
    req
}

pub fn encrypt_relay_http_request_body(
    public_key_pem: &str,
    mut req: RelayHttpRequest,
) -> Result<RelayHttpRequest> {
    // Encrypt only the body; keep headers/method/path for routing.
    // Envelope: {v, alg, ek_b64, nonce_b64, ct_b64}
    use aes_gcm::{aead::Aead, aead::KeyInit, Aes256Gcm, Nonce};
    use rand::RngCore as _;
    use rsa::pkcs8::DecodePublicKey;
    use rsa::{Oaep, RsaPublicKey};
    use sha2::Sha256;

    let plaintext = B64.decode(req.body_b64.as_bytes()).unwrap_or_default();
    if plaintext.is_empty() {
        return Ok(req);
    }

    let pubkey = RsaPublicKey::from_public_key_pem(public_key_pem)?;

    let mut key_bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key_bytes);
    let cipher = Aes256Gcm::new_from_slice(&key_bytes).expect("aes key");
    let mut nonce_bytes = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|e| anyhow::anyhow!("aes-gcm encrypt: {e}"))?;

    let ek = pubkey
        .encrypt(&mut rand::rngs::OsRng, Oaep::new::<Sha256>(), &key_bytes)
        .map_err(|e| anyhow::anyhow!("rsa encrypt: {e}"))?;

    let env = serde_json::json!({
      "v": 1,
      "alg": "rsa-oaep-sha256+aes-256-gcm",
      "ek_b64": B64.encode(ek),
      "nonce_b64": B64.encode(nonce_bytes),
      "ct_b64": B64.encode(ciphertext),
    });
    let env_bytes = serde_json::to_vec(&env).unwrap_or_default();
    req.headers
        .push(("x-fedi3-encrypted".to_string(), "1".to_string()));
    req.headers
        .push(("content-type".to_string(), "application/json".to_string()));
    req.body_b64 = B64.encode(env_bytes);
    Ok(req)
}
