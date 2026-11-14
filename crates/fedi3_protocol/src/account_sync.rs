//! Durable, opaque account replication records. Signatures bind metadata and ciphertext.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountEnvelope {
    pub actor: String,
    pub kind: String,
    pub key: String,
    pub clock: i64,
    #[serde(default)]
    pub terminal: bool,
    pub device: String,
    pub nonce_b64: String,
    pub ciphertext_b64: String,
    pub signature_b64: Option<String>,
}
impl AccountEnvelope {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut value = self.clone();
        value.signature_b64 = None;
        serde_json::to_vec(&value)
    }
    pub fn valid_kind(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "profile"
                | "followers"
                | "following"
                | "inbox_follows"
                | "follow_requests"
                | "reactions"
                | "note_replies"
                | "objects"
                | "outbox_items"
                | "object_meta"
                | "object_tags"
                | "object_attachments"
        )
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountPage {
    pub items: Vec<(i64, AccountEnvelope)>,
    pub cursor: i64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct TunnelProof {
    pub actor: String,
    pub user: String,
    pub nonce: String,
    pub timestamp: i64,
    pub public_key_pem: String,
    pub signature_b64: Option<String>,
}
impl TunnelProof {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut p = self.clone();
        p.signature_b64 = None;
        serde_json::to_vec(&p)
    }
}
