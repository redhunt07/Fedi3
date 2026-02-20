/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use hkdf::Hkdf;
use pqcrypto_kyber::kyber768;
use pqcrypto_traits::kem::{Ciphertext as _, PublicKey as _, SecretKey as _, SharedSecret as _};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use crate::ap::ApState;
use crate::http_sig::{sign_bytes_rsa_sha256, verify_bytes_rsa_sha256};
use crate::social_db::{ChatMessage, ChatThread, CollectionPage};

const CHAT_VERSION: u32 = 1;
const CHAT_PREKEY_TARGET: u32 = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatPreKey {
    pub id: String,
    pub public_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatBundle {
    pub v: u32,
    pub actor: String,
    pub peer_id: String,
    pub did: Option<String>,
    pub device_id: String,
    pub kem_public_b64: String,
    pub prekeys: Vec<ChatPreKey>,
    pub created_at_ms: i64,
    pub signature_b64: Option<String>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatPayload {
    pub op: String,
    pub text: Option<String>,
    pub reply_to: Option<String>,
    pub message_id: Option<String>,
    pub status: Option<String>,
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<ChatAttachment>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reaction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatAttachment {
    pub id: String,
    pub url: String,
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub name: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub blurhash: Option<String>,
}

pub fn ensure_chat_keys(state: &ApState) -> Result<()> {
    if state.social.get_chat_identity_key()?.is_none() {
        let (pk, sk) = kyber768::keypair();
        state
            .social
            .set_chat_identity_key(&B64.encode(pk.as_bytes()), &B64.encode(sk.as_bytes()))?;
    }
    let prekey_count = state.social.count_unused_chat_prekeys()?;
    if prekey_count >= CHAT_PREKEY_TARGET {
        return Ok(());
    }
    let missing = CHAT_PREKEY_TARGET - prekey_count;
    for _ in 0..missing {
        let (pk, sk) = kyber768::keypair();
        let key_id = format!("prekey-{}", random_id());
        state.social.insert_chat_prekey(
            &key_id,
            &B64.encode(pk.as_bytes()),
            &B64.encode(sk.as_bytes()),
        )?;
    }
    Ok(())
}

pub fn build_chat_bundle(state: &ApState) -> Result<ChatBundle> {
    ensure_chat_keys(state)?;
    let (pub_b64, _secret_b64) = state
        .social
        .get_chat_identity_key()?
        .context("missing chat identity key")?;
    let prekeys = state
        .social
        .list_chat_prekeys(CHAT_PREKEY_TARGET)?
        .into_iter()
        .map(|(id, public_b64, _)| ChatPreKey { id, public_b64 })
        .collect::<Vec<_>>();
    let device_id = get_or_create_device_id(state)?;
    let actor = format!(
        "{}/users/{}",
        state.cfg.public_base_url.trim_end_matches('/'),
        state.cfg.username
    );
    let did = state
        .cfg
        .also_known_as
        .iter()
        .find(|s| s.starts_with("did:fedi3:"))
        .cloned();
    let mut bundle = ChatBundle {
        v: CHAT_VERSION,
        actor,
        peer_id: state.cfg.p2p_peer_id.clone().unwrap_or_default(),
        did,
        device_id,
        kem_public_b64: pub_b64,
        prekeys,
        created_at_ms: now_ms(),
        signature_b64: None,
    };
    let sig = sign_bundle(state, &bundle)?;
    bundle.signature_b64 = Some(B64.encode(sig));
    Ok(bundle)
}

pub async fn verify_bundle(state: &ApState, bundle: &ChatBundle) -> Result<()> {
    let sig_b64 = bundle
        .signature_b64
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing bundle signature"))?;
    let sig = B64.decode(sig_b64.as_bytes())?;
    let mut clone = bundle.clone();
    clone.signature_b64 = None;
    let bytes = serde_json::to_vec(&clone)?;
    let summary = state
        .key_resolver
        .resolve_actor_summary_for_key_id(&bundle.actor)
        .await?;
    verify_bytes_rsa_sha256(&summary.public_key_pem, &bytes, &sig)?;
    Ok(())
}

pub fn encrypt_payload_for_bundle(
    state: &ApState,
    bundle: &ChatBundle,
    thread_id: &str,
    message_id: &str,
    payload: &ChatPayload,
) -> Result<ChatEnvelope> {
    let payload_bytes = serde_json::to_vec(payload)?;
    let (kem_key_id, kem_ct, shared) = select_kem_target(bundle)?;
    let key = derive_key(&shared, thread_id, message_id)?;
    let nonce = random_nonce();
    let cipher = Aes256Gcm::new_from_slice(&key)?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), payload_bytes.as_ref())
        .context("encrypt payload")?;
    let mut env = ChatEnvelope {
        v: CHAT_VERSION,
        thread_id: thread_id.to_string(),
        message_id: message_id.to_string(),
        sender_actor: format!(
            "{}/users/{}",
            state.cfg.public_base_url.trim_end_matches('/'),
            state.cfg.username
        ),
        sender_device: get_or_create_device_id(state)?,
        sender_peer_id: state.cfg.p2p_peer_id.clone().unwrap_or_default(),
        created_at_ms: now_ms(),
        kem_alg: "kyber768".to_string(),
        kem_ciphertext_b64: B64.encode(kem_ct.as_bytes()),
        kem_key_id,
        nonce_b64: B64.encode(nonce),
        ciphertext_b64: B64.encode(ciphertext),
        signature_b64: None,
    };
    let sig = sign_envelope(state, &env)?;
    env.signature_b64 = Some(B64.encode(sig));
    Ok(env)
}

pub async fn decrypt_envelope(state: &ApState, env: &ChatEnvelope) -> Result<ChatPayload> {
    verify_envelope(state, env).await?;
    let shared = if env.kem_key_id.starts_with("prekey-") {
        let Some(sec_b64) = state.social.get_chat_prekey_secret(&env.kem_key_id)? else {
            return Err(anyhow::anyhow!("missing prekey"));
        };
        state.social.mark_chat_prekey_used(&env.kem_key_id)?;
        let sk = kyber768::SecretKey::from_bytes(&B64.decode(sec_b64.as_bytes())?)?;
        kyber768::decapsulate(
            &kyber768::Ciphertext::from_bytes(&B64.decode(env.kem_ciphertext_b64.as_bytes())?)?,
            &sk,
        )
    } else {
        let (_pub, sec) = state
            .social
            .get_chat_identity_key()?
            .context("missing chat identity key")?;
        let sk = kyber768::SecretKey::from_bytes(&B64.decode(sec.as_bytes())?)?;
        kyber768::decapsulate(
            &kyber768::Ciphertext::from_bytes(&B64.decode(env.kem_ciphertext_b64.as_bytes())?)?,
            &sk,
        )
    };
    let key = derive_key(&shared, &env.thread_id, &env.message_id)?;
    let cipher = Aes256Gcm::new_from_slice(&key)?;
    let nonce = B64.decode(env.nonce_b64.as_bytes())?;
    if nonce.len() != 12 {
        return Err(anyhow::anyhow!("invalid nonce length"));
    }
    let ciphertext = B64.decode(env.ciphertext_b64.as_bytes())?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .context("decrypt payload")?;
    let payload = serde_json::from_slice::<ChatPayload>(&plaintext)?;
    Ok(payload)
}

pub async fn store_incoming_payload(
    state: &ApState,
    env: &ChatEnvelope,
    payload: &ChatPayload,
) -> Result<()> {
    let thread_id = env.thread_id.trim();
    if thread_id.is_empty() {
        return Err(anyhow::anyhow!("missing thread_id"));
    }
    if payload.op == "message" {
        let self_actor = format!(
            "{}/users/{}",
            state.cfg.public_base_url.trim_end_matches('/'),
            state.cfg.username
        );
        state.social.create_chat_thread(thread_id, "dm", None)?;
        state
            .social
            .upsert_chat_member(thread_id, &env.sender_actor, "member")?;
        state
            .social
            .upsert_chat_member(thread_id, &self_actor, "member")?;
        let msg = ChatMessage {
            message_id: env.message_id.clone(),
            thread_id: thread_id.to_string(),
            sender_actor: env.sender_actor.clone(),
            sender_device: env.sender_device.clone(),
            created_at_ms: env.created_at_ms,
            edited_at_ms: None,
            deleted: false,
            body_json: serde_json::to_string(payload)?,
        };
        state.social.insert_chat_message(&msg)?;
        state.social.touch_chat_thread(thread_id)?;
    } else if payload.op == "edit" {
        if let Some(message_id) = payload.message_id.as_deref() {
            state
                .social
                .update_chat_message_edit(message_id, &serde_json::to_string(payload)?)?;
        }
    } else if payload.op == "delete" {
        if let Some(message_id) = payload.message_id.as_deref() {
            state.social.mark_chat_message_deleted(message_id)?;
        }
    } else if payload.op == "system" {
        let title = payload.title.as_deref();
        state.social.create_chat_thread(thread_id, "group", title)?;
        if let Some(members) = payload.members.as_ref() {
            state.social.set_chat_members(thread_id, members)?;
        }
        if let Some(action) = payload.action.as_deref() {
            if action == "delete_thread" {
                let _ = state.social.delete_chat_thread(thread_id);
                return Ok(());
            }
            if action == "create_thread" {
                let _ = state
                    .social
                    .upsert_chat_member(thread_id, &env.sender_actor, "owner");
                return Ok(());
            }
            if action == "add_member" {
                if let Some(targets) = payload.targets.as_ref() {
                    for actor in targets {
                        let _ = state.social.upsert_chat_member(thread_id, actor, "member");
                    }
                }
            } else if action == "remove_member" {
                if let Some(targets) = payload.targets.as_ref() {
                    for actor in targets {
                        let _ = state.social.remove_chat_member(thread_id, actor);
                    }
                }
            } else if action == "rename" {
                let _ = state.social.update_chat_thread_title(thread_id, title);
            }
        }
        let msg = ChatMessage {
            message_id: env.message_id.clone(),
            thread_id: thread_id.to_string(),
            sender_actor: env.sender_actor.clone(),
            sender_device: env.sender_device.clone(),
            created_at_ms: env.created_at_ms,
            edited_at_ms: None,
            deleted: false,
            body_json: serde_json::to_string(payload)?,
        };
        state.social.insert_chat_message(&msg)?;
        state.social.touch_chat_thread(thread_id)?;
    } else if payload.op == "receipt" {
        if let (Some(message_id), Some(status)) =
            (payload.message_id.as_deref(), payload.status.as_deref())
        {
            state
                .social
                .upsert_chat_message_status(message_id, &env.sender_actor, status)?;
        }
    } else if payload.op == "react" {
        if let (Some(message_id), Some(reaction)) =
            (payload.message_id.as_deref(), payload.reaction.as_deref())
        {
            let action = payload.action.as_deref().unwrap_or("add");
            if action == "remove" {
                let _ = state
                    .social
                    .remove_chat_reaction(message_id, &env.sender_actor, reaction);
            } else {
                let _ = state
                    .social
                    .add_chat_reaction(message_id, &env.sender_actor, reaction);
            }
            if let Ok(Some(thread_id)) = state.social.get_chat_message_thread_id(message_id) {
                let _ = state.social.touch_chat_thread(&thread_id);
            }
        }
    }
    Ok(())
}

pub fn list_threads_for_actor(
    state: &ApState,
    actor_id: &str,
    archived: bool,
    limit: u32,
    cursor: Option<i64>,
) -> Result<CollectionPage<ChatThread>> {
    state
        .social
        .list_chat_threads_for_actor(actor_id, archived, limit, cursor)
}

pub fn list_messages(
    state: &ApState,
    thread_id: &str,
    limit: u32,
    cursor: Option<i64>,
) -> Result<CollectionPage<ChatMessage>> {
    state.social.list_chat_messages(thread_id, limit, cursor)
}

pub fn random_id() -> String {
    let mut bytes = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sign_bundle(state: &ApState, bundle: &ChatBundle) -> Result<Vec<u8>> {
    let mut clone = bundle.clone();
    clone.signature_b64 = None;
    let bytes = serde_json::to_vec(&clone)?;
    Ok(sign_bytes_rsa_sha256(&state.private_key_pem, &bytes)?)
}

fn sign_envelope(state: &ApState, env: &ChatEnvelope) -> Result<Vec<u8>> {
    let mut clone = env.clone();
    clone.signature_b64 = None;
    let bytes = serde_json::to_vec(&clone)?;
    Ok(sign_bytes_rsa_sha256(&state.private_key_pem, &bytes)?)
}

async fn verify_envelope(state: &ApState, env: &ChatEnvelope) -> Result<()> {
    let sig_b64 = env
        .signature_b64
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing signature"))?;
    let sig = B64.decode(sig_b64.as_bytes())?;
    let mut clone = env.clone();
    clone.signature_b64 = None;
    let bytes = serde_json::to_vec(&clone)?;
    let summary = state
        .key_resolver
        .resolve_actor_summary_for_key_id(&env.sender_actor)
        .await?;
    verify_bytes_rsa_sha256(&summary.public_key_pem, &bytes, &sig)?;
    Ok(())
}

fn derive_key(
    shared: &kyber768::SharedSecret,
    thread_id: &str,
    message_id: &str,
) -> Result<[u8; 32]> {
    let hk = Hkdf::<Sha256>::new(Some(thread_id.as_bytes()), shared.as_bytes());
    let mut out = [0u8; 32];
    hk.expand(message_id.as_bytes(), &mut out)
        .map_err(|_| anyhow::anyhow!("hkdf expand failed"))?;
    Ok(out)
}

fn select_kem_target(
    bundle: &ChatBundle,
) -> Result<(String, kyber768::Ciphertext, kyber768::SharedSecret)> {
    if let Some(pk) = bundle.prekeys.first() {
        let public = kyber768::PublicKey::from_bytes(&B64.decode(pk.public_b64.as_bytes())?)?;
        let (ss, ct) = kyber768::encapsulate(&public);
        return Ok((pk.id.clone(), ct, ss));
    }
    let public = kyber768::PublicKey::from_bytes(&B64.decode(bundle.kem_public_b64.as_bytes())?)?;
    let (ss, ct) = kyber768::encapsulate(&public);
    Ok(("identity".to_string(), ct, ss))
}

fn random_nonce() -> [u8; 12] {
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    nonce
}

fn get_or_create_device_id(state: &ApState) -> Result<String> {
    if let Some(v) = state.social.get_local_meta("chat_device_id")? {
        if !v.trim().is_empty() {
            return Ok(v);
        }
    }
    let id = format!("dev-{}", random_id());
    state.social.set_local_meta("chat_device_id", &id)?;
    Ok(id)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
