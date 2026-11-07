//! Compatibility-preserving RSA operations backed by AWS-LC.
//!
//! PKCS#8 private keys, SPKI public keys, PKCS#1 v1.5 SHA256 signatures and
//! OAEP SHA256/MGF1-SHA256 with an empty label keep their existing wire formats.
use anyhow::{anyhow, ensure, Result};
pub use aws_lc_rs::rsa::{PrivateDecryptingKey, PublicEncryptingKey};
use aws_lc_rs::{
    encoding::{AsDer, Pkcs8V1Der, PublicKeyX509Der},
    rand::SystemRandom,
    rsa::{KeySize, OaepPrivateDecryptingKey, OaepPublicEncryptingKey, OAEP_SHA256_MGF1SHA256},
    signature::{
        RsaKeyPair, UnparsedPublicKey, RSA_PKCS1_1024_8192_SHA256_FOR_LEGACY_USE_ONLY,
        RSA_PKCS1_SHA256,
    },
};

fn decode(pem: &str, expected: &str) -> Result<Vec<u8>> {
    let (label, der) =
        pem_rfc7468::decode_vec(pem.as_bytes()).map_err(|_| anyhow!("invalid RSA PEM"))?;
    ensure!(label == expected, "unexpected RSA PEM label");
    Ok(der)
}

pub fn rsa_private_key(pem: &str) -> Result<PrivateDecryptingKey> {
    PrivateDecryptingKey::from_pkcs8(&decode(pem, "PRIVATE KEY")?)
        .map_err(|_| anyhow!("invalid RSA private key"))
}

pub fn rsa_generate_private_pem() -> Result<String> {
    let key = PrivateDecryptingKey::generate(KeySize::Rsa2048)
        .map_err(|_| anyhow!("RSA key generation failed"))?;
    let der = AsDer::<Pkcs8V1Der>::as_der(&key)
        .map_err(|_| anyhow!("RSA private key encoding failed"))?;
    pem_rfc7468::encode_string("PRIVATE KEY", pem_rfc7468::LineEnding::LF, der.as_ref())
        .map_err(|_| anyhow!("RSA PEM encoding failed"))
}

pub fn rsa_public_pem(private_pem: &str) -> Result<String> {
    let key = rsa_private_key(private_pem)?.public_key();
    let der = AsDer::<PublicKeyX509Der>::as_der(&key)
        .map_err(|_| anyhow!("RSA public key encoding failed"))?;
    pem_rfc7468::encode_string("PUBLIC KEY", pem_rfc7468::LineEnding::LF, der.as_ref())
        .map_err(|_| anyhow!("RSA PEM encoding failed"))
}

pub fn rsa_sign_sha256(private_pem: &str, message: &[u8]) -> Result<Vec<u8>> {
    let key = RsaKeyPair::from_pkcs8(&decode(private_pem, "PRIVATE KEY")?)
        .map_err(|_| anyhow!("invalid RSA signing key"))?;
    let mut signature = vec![0; key.public_modulus_len()];
    key.sign(
        &RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        message,
        &mut signature,
    )
    .map_err(|_| anyhow!("RSA signing failed"))?;
    Ok(signature)
}

pub fn rsa_verify_sha256(public_pem: &str, message: &[u8], signature: &[u8]) -> Result<()> {
    let der = decode(public_pem, "PUBLIC KEY")?;
    UnparsedPublicKey::new(&RSA_PKCS1_1024_8192_SHA256_FOR_LEGACY_USE_ONLY, der)
        .verify(message, signature)
        .map_err(|_| anyhow!("RSA signature verification failed"))
}

pub fn rsa_oaep_encrypt(public_pem: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
    let key = PublicEncryptingKey::from_der(&decode(public_pem, "PUBLIC KEY")?)
        .map_err(|_| anyhow!("invalid RSA encryption key"))?;
    let key = OaepPublicEncryptingKey::new(key).map_err(|_| anyhow!("invalid RSA OAEP key"))?;
    let mut output = vec![0; key.ciphertext_size()];
    let ciphertext = key
        .encrypt(&OAEP_SHA256_MGF1SHA256, plaintext, &mut output, None)
        .map_err(|_| anyhow!("RSA OAEP encryption failed"))?;
    Ok(ciphertext.to_vec())
}

pub fn rsa_oaep_decrypt(private_pem: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
    let key = OaepPrivateDecryptingKey::new(rsa_private_key(private_pem)?)
        .map_err(|_| anyhow!("invalid RSA OAEP key"))?;
    let mut output = vec![0; key.min_output_size()];
    let plaintext = key
        .decrypt(&OAEP_SHA256_MGF1SHA256, ciphertext, &mut output, None)
        .map_err(|_| anyhow!("RSA OAEP decryption failed"))?;
    Ok(plaintext.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    const PRIVATE: &str = include_str!("../tests/fixtures/legacy_rsa/private.pem");
    const PUBLIC: &str = include_str!("../tests/fixtures/legacy_rsa/public.pem");

    #[test]
    fn preserves_legacy_public_key_bytes_signatures_and_ciphertexts() {
        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/legacy_rsa/vectors.json"))
                .unwrap();
        let message = B64
            .decode(vectors["message_b64"].as_str().unwrap())
            .unwrap();
        let signature = B64
            .decode(vectors["signature_b64"].as_str().unwrap())
            .unwrap();
        let ciphertext = B64
            .decode(vectors["oaep_ciphertext_b64"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            rsa_public_pem(PRIVATE).unwrap(),
            PUBLIC,
            "a key encoding change would change the actor DID"
        );
        rsa_verify_sha256(PUBLIC, &message, &signature).unwrap();
        assert_eq!(rsa_sign_sha256(PRIVATE, &message).unwrap(), signature);
        assert_eq!(rsa_oaep_decrypt(PRIVATE, &ciphertext).unwrap(), message);
        assert!(rsa_verify_sha256(PUBLIC, b"tampered", &signature).is_err());
        let mut corrupted = ciphertext;
        corrupted[0] ^= 1;
        assert!(rsa_oaep_decrypt(PRIVATE, &corrupted).is_err());
    }

    #[test]
    fn generated_keys_roundtrip_and_bad_keys_fail_without_regeneration() {
        let private = rsa_generate_private_pem().unwrap();
        let public = rsa_public_pem(&private).unwrap();
        let cipher = rsa_oaep_encrypt(&public, b"aes-key-material").unwrap();
        assert_eq!(
            rsa_oaep_decrypt(&private, &cipher).unwrap(),
            b"aes-key-material"
        );
        assert!(rsa_private_key("corrupted private key").is_err());
    }
}
