//! Signed chat transport envelope shared by core and relay.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatEnvelope {
    pub v: u32,
    pub thread_id: String,
    pub message_id: String,
    pub sender_actor: String,
    pub sender_device: String,
    pub sender_peer_id: String,
    pub created_at_ms: i64,
    pub kem_alg: String,
    pub kem_ciphertext_b64: String,
    pub kem_key_id: String,
    pub nonce_b64: String,
    pub ciphertext_b64: String,
    pub signature_b64: Option<String>,
}
