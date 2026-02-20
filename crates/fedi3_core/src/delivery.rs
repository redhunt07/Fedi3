/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use crate::crypto_envelope::encrypt_relay_http_request_body;
use crate::http_retry::send_with_retry;
use crate::http_sig::sign_request_rsa_sha256;
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use fedi3_protocol::{RelayHttpRequest, RelayHttpResponse};
use http::{HeaderMap, Method, Uri};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::p2p::DidDiscoveryRecord;
use crate::p2p::P2pHandle;
use crate::p2p::PeerDiscoveryRecord;
use crate::webrtc_p2p::WebrtcHandle;

#[derive(Clone)]
pub struct Delivery {
    client: reqwest::Client,
    p2p: Arc<RwLock<Option<P2pHandle>>>,
    webrtc: Arc<RwLock<Option<WebrtcHandle>>>,
    mailbox_targets: Arc<RwLock<Vec<MailboxTarget>>>,
}

impl Delivery {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            p2p: Arc::new(RwLock::new(None)),
            webrtc: Arc::new(RwLock::new(None)),
            mailbox_targets: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn set_p2p(&self, handle: Option<P2pHandle>) {
        *self.p2p.write().await = handle;
    }

    pub async fn set_webrtc(&self, handle: Option<WebrtcHandle>) {
        *self.webrtc.write().await = handle;
    }

    pub async fn set_mailbox_targets_from_relay_reserve(&self, relay_reserve: Vec<String>) {
        let mut by_peer: HashMap<String, Vec<String>> = HashMap::new();
        for s in relay_reserve {
            let s = s.trim().to_string();
            if s.is_empty() {
                continue;
            }
            let Some(peer_id) = parse_peer_id_from_multiaddr_str(&s) else {
                continue;
            };
            by_peer.entry(peer_id).or_default().push(s);
        }
        let mut out = Vec::new();
        for (peer_id, addrs) in by_peer {
            out.push(MailboxTarget { peer_id, addrs });
        }
        *self.mailbox_targets.write().await = out;
    }

    pub async fn p2p_add_peer_addrs(&self, peer_id: &str, addrs: Vec<String>) -> Result<()> {
        let Some(handle) = self.p2p.read().await.clone() else {
            return Ok(());
        };
        handle.add_peer_addrs(peer_id, addrs).await?;
        Ok(())
    }

    pub async fn resolve_actor_info(&self, actor_url: &str) -> Result<ActorInfo> {
        let resp = self
            .client
            .get(actor_url)
            .header(
                ACCEPT,
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
        let public_key_pem = actor.publicKey.as_ref().map(|p| p.public_key_pem.clone());

        if let Some(endpoints) = actor.endpoints {
            if let Some(shared) = endpoints.shared_inbox {
                return Ok(ActorInfo {
                    inbox: shared,
                    p2p_peer_id: endpoints.fedi3_peer_id,
                    p2p_peer_addrs: endpoints.fedi3_peer_addrs.unwrap_or_default(),
                    public_key_pem,
                });
            }
        }
        let inbox = actor.inbox.ok_or_else(|| anyhow!("actor missing inbox"))?;
        Ok(ActorInfo {
            inbox,
            p2p_peer_id: actor.fedi3_peer_id,
            p2p_peer_addrs: actor.fedi3_peer_addrs.unwrap_or_default(),
            public_key_pem,
        })
    }

    pub async fn resolve_actor_info_signed(
        &self,
        private_key_pem: &str,
        key_id: &str,
        actor_url: &str,
    ) -> Result<ActorInfo> {
        let uri: Uri = actor_url.parse().context("parse actor url")?;

        let mut headers = HeaderMap::new();
        headers.insert(
            "Accept",
            "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\""
                .parse()
                .expect("static header"),
        );

        sign_request_rsa_sha256(
            private_key_pem,
            key_id,
            &Method::GET,
            &uri,
            &mut headers,
            &[],
            &["(request-target)", "host", "date"],
        )?;

        let mut req = self.client.get(actor_url).header(
            ACCEPT,
            "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"",
        );
        for (k, v) in headers.iter() {
            req = req.header(k.as_str(), v.to_str().unwrap_or_default());
        }

        let resp = req
            .send()
            .await
            .with_context(|| format!("fetch actor (signed): {actor_url}"))?
            .error_for_status()
            .with_context(|| format!("actor not ok (signed): {actor_url}"))?;

        let text = resp.text().await?;
        let actor: ActorDoc = serde_json::from_str(&text)
            .with_context(|| format!("parse actor json from {actor_url}"))?;
        let public_key_pem = actor.publicKey.as_ref().map(|p| p.public_key_pem.clone());

        if let Some(endpoints) = actor.endpoints {
            if let Some(shared) = endpoints.shared_inbox {
                return Ok(ActorInfo {
                    inbox: shared,
                    p2p_peer_id: endpoints.fedi3_peer_id,
                    p2p_peer_addrs: endpoints.fedi3_peer_addrs.unwrap_or_default(),
                    public_key_pem,
                });
            }
        }
        let inbox = actor.inbox.ok_or_else(|| anyhow!("actor missing inbox"))?;
        Ok(ActorInfo {
            inbox,
            p2p_peer_id: actor.fedi3_peer_id,
            p2p_peer_addrs: actor.fedi3_peer_addrs.unwrap_or_default(),
            public_key_pem,
        })
    }

    pub async fn deliver_json(
        &self,
        private_key_pem: &str,
        key_id: &str,
        inbox_url: &str,
        body: &[u8],
    ) -> Result<()> {
        let uri: Uri = inbox_url.parse().context("parse inbox url")?;

        let mut headers = HeaderMap::new();
        headers.insert(
            "Accept",
            "application/activity+json".parse().expect("static header"),
        );
        headers.insert(
            "Content-Type",
            "application/activity+json".parse().expect("static header"),
        );

        sign_request_rsa_sha256(
            private_key_pem,
            key_id,
            &Method::POST,
            &uri,
            &mut headers,
            body,
            &["(request-target)", "host", "date", "digest", "content-type"],
        )?;

        let mut req = self
            .client
            .post(inbox_url)
            .header(ACCEPT, "application/activity+json");
        for (k, v) in headers.iter() {
            req = req.header(k.as_str(), v.to_str().unwrap_or_default());
        }
        req = req.header(CONTENT_TYPE, "application/activity+json");

        let resp = send_with_retry(|| req.try_clone().unwrap().body(body.to_vec()), 3).await?;
        let status = resp.status();
        if !status.is_success() && status.as_u16() != 202 {
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("delivery failed: {} {}", status, text));
        }
        Ok(())
    }

    pub async fn deliver_json_p2p(
        &self,
        peer_id: &str,
        private_key_pem: &str,
        key_id: &str,
        inbox_url: &str,
        public_key_pem: Option<&str>,
        body: &[u8],
    ) -> Result<()> {
        let Some(handle) = self.p2p.read().await.clone() else {
            return Err(anyhow!("p2p not enabled"));
        };

        let mut req = build_signed_post_relay_http_request(
            format!("p2p-{}", random_id()),
            private_key_pem,
            key_id,
            inbox_url,
            body,
        )?;
        if let Some(pem) = public_key_pem {
            if let Ok(enc) = encrypt_relay_http_request_body(pem, req.clone()) {
                req = enc;
            }
        }
        let resp = handle.request(peer_id, req).await?;
        if (200..300).contains(&resp.status) || resp.status == 202 {
            return Ok(());
        }
        Err(anyhow!("p2p delivery failed: status {}", resp.status))
    }

    pub async fn deliver_json_webrtc(
        &self,
        peer_actor_url: &str,
        peer_id: &str,
        private_key_pem: &str,
        key_id: &str,
        inbox_url: &str,
        body: &[u8],
    ) -> Result<()> {
        let Some(handle) = self.webrtc.read().await.clone() else {
            return Err(anyhow!("webrtc not enabled"));
        };
        let req = build_signed_post_relay_http_request(
            format!("wrtc-{}", random_id()),
            private_key_pem,
            key_id,
            inbox_url,
            body,
        )?;
        let resp = handle.request(peer_actor_url, peer_id, req).await?;
        if (200..300).contains(&resp.status) || resp.status == 202 {
            return Ok(());
        }
        Err(anyhow!("webrtc delivery failed: status {}", resp.status))
    }

    pub async fn store_in_mailboxes(
        &self,
        to_peer_id: &str,
        signed_req: RelayHttpRequest,
        ttl_secs: u64,
    ) -> Result<()> {
        let Some(handle) = self.p2p.read().await.clone() else {
            return Err(anyhow!("p2p not enabled"));
        };
        let targets = self.mailbox_targets.read().await.clone();
        if targets.is_empty() {
            return Err(anyhow!("no mailbox targets configured"));
        }

        for t in targets {
            if !t.addrs.is_empty() {
                let _ = handle.add_peer_addrs(&t.peer_id, t.addrs.clone()).await;
            }

            let put_body = serde_json::json!({
                "to_peer_id": to_peer_id,
                "req": signed_req,
                "ttl_secs": ttl_secs,
            });
            let put_bytes = serde_json::to_vec(&put_body).unwrap_or_default();
            let put_req = RelayHttpRequest {
                id: format!("mbx-put-{}", random_id()),
                method: "POST".to_string(),
                path: "/.fedi3/mailbox/put".to_string(),
                query: "".to_string(),
                headers: vec![("content-type".to_string(), "application/json".to_string())],
                body_b64: B64.encode(put_bytes),
            };

            let resp = match handle.request(&t.peer_id, put_req).await {
                Ok(r) => r,
                Err(_) => continue,
            };
            if (200..300).contains(&resp.status) || resp.status == 202 {
                return Ok(());
            }
        }
        Err(anyhow!("all mailboxes failed"))
    }

    pub async fn publish_gossip(&self, topic: &str, data: Vec<u8>) -> Result<()> {
        let Some(handle) = self.p2p.read().await.clone() else {
            return Ok(());
        };
        let _ = handle.publish(topic, data).await;
        Ok(())
    }

    pub async fn p2p_resolve_peer(&self, peer_id: &str) -> Result<Option<PeerDiscoveryRecord>> {
        let Some(handle) = self.p2p.read().await.clone() else {
            return Ok(None);
        };
        handle.kad_get_peer(peer_id).await
    }

    pub async fn p2p_resolve_did(&self, did: &str) -> Result<Option<DidDiscoveryRecord>> {
        let Some(handle) = self.p2p.read().await.clone() else {
            return Ok(None);
        };
        handle.kad_get_did(did).await
    }

    pub async fn p2p_request(
        &self,
        peer_id: &str,
        req: RelayHttpRequest,
    ) -> Result<RelayHttpResponse> {
        let Some(handle) = self.p2p.read().await.clone() else {
            return Err(anyhow!("p2p not enabled"));
        };
        handle.request(peer_id, req).await
    }

    pub async fn webrtc_request(
        &self,
        peer_actor_url: &str,
        peer_id: &str,
        req: RelayHttpRequest,
    ) -> Result<RelayHttpResponse> {
        let Some(handle) = self.webrtc.read().await.clone() else {
            return Err(anyhow!("webrtc not enabled"));
        };
        handle.request(peer_actor_url, peer_id, req).await
    }
}

pub fn extract_recipients(activity: &Value) -> Vec<String> {
    let mut out = Vec::new();
    collect_field(activity, "to", &mut out);
    collect_field(activity, "cc", &mut out);
    out.retain(|v| v != "https://www.w3.org/ns/activitystreams#Public");
    out.sort();
    out.dedup();
    out
}

fn collect_field(activity: &Value, field: &str, out: &mut Vec<String>) {
    let Some(v) = activity.get(field) else { return };
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(arr) => {
            for item in arr {
                if let Value::String(s) = item {
                    out.push(s.clone());
                }
            }
        }
        _ => {}
    }
}

pub fn is_public_activity(activity: &Value) -> bool {
    fn has_public(v: &Value) -> bool {
        match v {
            Value::String(s) => s == "https://www.w3.org/ns/activitystreams#Public",
            Value::Array(arr) => arr.iter().any(has_public),
            _ => false,
        }
    }
    activity.get("to").map(has_public).unwrap_or(false)
        || activity.get("cc").map(has_public).unwrap_or(false)
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct ActorDoc {
    inbox: Option<String>,
    endpoints: Option<ActorEndpoints>,
    publicKey: Option<ActorPublicKey>,
    #[serde(rename = "fedi3PeerId")]
    fedi3_peer_id: Option<String>,
    #[serde(rename = "fedi3PeerAddrs")]
    fedi3_peer_addrs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct ActorPublicKey {
    #[serde(rename = "publicKeyPem")]
    public_key_pem: String,
}

#[derive(Debug, Deserialize)]
struct ActorEndpoints {
    #[serde(rename = "sharedInbox")]
    shared_inbox: Option<String>,
    #[serde(rename = "fedi3PeerId")]
    fedi3_peer_id: Option<String>,
    #[serde(rename = "fedi3PeerAddrs")]
    fedi3_peer_addrs: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct ActorInfo {
    pub inbox: String,
    pub p2p_peer_id: Option<String>,
    pub p2p_peer_addrs: Vec<String>,
    pub public_key_pem: Option<String>,
}

fn headers_to_vec(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|vs| (k.to_string(), vs.to_string())))
        .collect()
}

fn random_id() -> String {
    // 16 random bytes -> 32 hex chars
    let mut b = [0u8; 16];
    use rand::RngCore as _;
    rand::rngs::OsRng.fill_bytes(&mut b);
    b.iter().map(|v| format!("{v:02x}")).collect()
}

#[derive(Debug, Clone)]
struct MailboxTarget {
    peer_id: String,
    addrs: Vec<String>,
}

fn parse_peer_id_from_multiaddr_str(addr: &str) -> Option<String> {
    let ma: libp2p::Multiaddr = addr.parse().ok()?;
    for p in ma.iter() {
        if let libp2p::multiaddr::Protocol::P2p(h) = p {
            return Some(h.to_string());
        }
    }
    None
}

pub fn build_signed_post_relay_http_request(
    id: String,
    private_key_pem: &str,
    key_id: &str,
    inbox_url: &str,
    body: &[u8],
) -> Result<RelayHttpRequest> {
    let uri: Uri = inbox_url.parse().context("parse inbox url")?;
    let mut headers = HeaderMap::new();
    headers.insert(
        "Accept",
        "application/activity+json".parse().expect("static header"),
    );
    headers.insert(
        "Content-Type",
        "application/activity+json".parse().expect("static header"),
    );
    sign_request_rsa_sha256(
        private_key_pem,
        key_id,
        &Method::POST,
        &uri,
        &mut headers,
        body,
        &["(request-target)", "host", "date", "digest", "content-type"],
    )?;

    let path = uri.path().to_string();
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    Ok(RelayHttpRequest {
        id,
        method: "POST".to_string(),
        path,
        query,
        headers: headers_to_vec(&headers),
        body_b64: B64.encode(body),
    })
}
