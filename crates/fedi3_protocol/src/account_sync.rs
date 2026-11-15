//! Durable, opaque account replication records. Signatures bind metadata and ciphertext.
use serde::{Deserialize, Serialize};
/// One signed, encrypted revision shared by devices holding the same identity.
/// The relay can merge metadata but cannot decrypt the account record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountEnvelope {
    /// Complete canonical actor URL; usernames alone do not identify accounts.
    pub actor: String,
    /// Allowlisted record family, negotiated for older clients at page retrieval.
    pub kind: String,
    /// Stable opaque key for a row or independently merged profile field.
    pub key: String,
    /// Logical revision clock. Equal clocks are ordered by device identifier.
    pub clock: i64,
    /// An irreversible owned-post deletion takes priority over a later edit.
    #[serde(default)]
    pub terminal: bool,
    pub device: String,
    pub nonce_b64: String,
    pub ciphertext_b64: String,
    pub signature_b64: Option<String>,
}
impl AccountEnvelope {
    /// Serialize the exact wire fields with a null signature before RSA signing.
    /// Field order and the null field are part of the existing signing contract.
    pub fn signing_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut value = self.clone();
        value.signature_b64 = None;
        serde_json::to_vec(&value)
    }
    /// Reject unsupported record families before attempting to apply a row.
    pub fn valid_kind(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "personal_notifications"
                | "profile"
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
/// Relay-local sequence numbers are separate from each record's logical clock.
/// Clients persist progress only after verifying and applying received records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountPage {
    pub items: Vec<(i64, AccountEnvelope)>,
    pub cursor: i64,
}
/// Fresh challenge response proving ownership of the relay's pinned RSA key.
/// A relay bearer token alone must not authorize replacing an existing identity.
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
    /// Use the same null-signature serialization contract as account envelopes.
    pub fn signing_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut p = self.clone();
        p.signature_b64 = None;
        serde_json::to_vec(&p)
    }
}
