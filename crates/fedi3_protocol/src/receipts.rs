//! Forward acknowledgments bind exact HTTP bytes and a fresh relay nonce.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CORE_ACK_HEADER: &str = "x-fedi3-core-receipt";
pub const FORWARD_NONCE_HEADER: &str = "x-fedi3-forward-nonce";

pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn request_digest(method: &str, path: &str, query: &str, nonce: &str, body: &[u8]) -> String {
    sha256(
        &serde_json::to_vec(&(
            "fedi3-forward-request/v1",
            method,
            path,
            query.trim_start_matches('?'),
            nonce,
            sha256(body),
        ))
        .expect("request binding JSON"),
    )
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreAcknowledgment {
    pub schema: String,
    pub actor: String,
    pub request_digest: String,
    pub response_digest: String,
    pub status: u16,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedEnvelope {
    pub algorithm: String,
    pub payload_b64: String,
    pub signature_b64: String,
    pub key_id: String,
}

/// Public links commit to the previous link and the private signed payload.
/// Anyone can check this chain without seeing account metadata.
pub fn chain_hash(sequence: u64, previous: &str, commitment: &str) -> String {
    sha256(
        &serde_json::to_vec(&("fedi3-forward-log-link/v1", sequence, previous, commitment))
            .expect("log link JSON"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binding_separates_fields_and_changes_with_nonce_or_body() {
        let digest = request_digest("POST", "/inbox", "?a=1", "nonce1", b"hello");
        assert_eq!(
            digest,
            request_digest("POST", "/inbox", "a=1", "nonce1", b"hello")
        );
        for other in [
            request_digest("POST", "/inbox", "a=1", "nonce2", b"hello"),
            request_digest("POST", "/inbox", "a=1", "nonce1", b"changed"),
            request_digest("PUT", "/inbox", "a=1", "nonce1", b"hello"),
            request_digest("POST", "/inbox2", "a=1", "nonce1", b"hello"),
        ] {
            assert_ne!(digest, other);
        }
    }
}
