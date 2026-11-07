//! Real relay, tunnel and core; verify both signatures without trusting the issuer's embedded keys.
mod common;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use common::TestRelay;
use fedi3_core::runtime::{self, CoreStartConfig};
use fedi3_protocol::receipts::{sha256, CoreAcknowledgment, SignedEnvelope};
use serde_json::Value;
use std::time::{Duration, Instant};

struct Core(u64);
impl Drop for Core {
    fn drop(&mut self) {
        let _ = runtime::stop(self.0);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn live_forward_receipt_has_valid_relay_and_core_signatures() {
    std::env::set_var("FEDI3_ALLOW_PRIVATE_ACTOR_FETCH", "true");
    std::env::set_var("FEDI3_RELAY_DIRECTORY_URLS", "");
    let relay = TestRelay::start_with_env(&[("FEDI3_RELAY_DIRECTORY_URLS", "")]).await;
    let dir = fedi3_core::test_support::TempDir::new("signed-forward");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let token = "test-forward-token-with-32-characters";
    let internal = "test-forward-internal-token";
    let _core = Core(
        runtime::start(CoreStartConfig {
            username: "alice".into(),
            domain: "127.0.0.1".into(),
            public_base_url: relay.base_url(),
            relay_ws: relay.ws_url(""),
            relay_token: Some(token.into()),
            bind: format!("127.0.0.1:{port}"),
            data_dir: Some(dir.path().to_string_lossy().into()),
            internal_token: Some(internal.into()),
            p2p: None,
            ..Default::default()
        })
        .unwrap(),
    );
    let client = reqwest::Client::new();
    let deadline = Instant::now() + Duration::from_secs(20);
    let actor: Value = loop {
        let response = client
            .get(format!("{}/users/alice", relay.base_url()))
            .header("Accept", "application/activity+json")
            .send()
            .await
            .unwrap();
        if response.status() == 200 {
            break response.json().await.unwrap();
        }
        assert!(Instant::now() < deadline, "core did not connect");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let disk_public = fedi3_protocol::crypto::rsa_public_pem(
        &std::fs::read_to_string(dir.path().join("identity_private_key.pem")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        actor["publicKey"]["publicKeyPem"].as_str().unwrap(),
        disk_public,
        "actor key differs from core identity"
    );
    // A rejected HTTP operation is still a verifiable handler acknowledgment;
    // it must never be presented as successful delivery or as a human read receipt.
    let response = client
        .post(format!(
            "{}/users/alice/_fedi3/receipt-probe",
            relay.base_url()
        ))
        .header("X-Fedi3-Internal", internal)
        .body("receipt-body")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    let header = response
        .headers()
        .get("x-fedi3-forward-receipt")
        .expect("relay receipt header")
        .to_str()
        .unwrap();
    let envelope: Value = serde_json::from_slice(&B64.decode(header).unwrap()).unwrap();
    let payload = B64
        .decode(envelope["payload_b64"].as_str().unwrap())
        .unwrap();
    let signature = B64
        .decode(envelope["signature_b64"].as_str().unwrap())
        .unwrap();
    // Pin from the relay's independent transparency endpoint for this isolated fixture.
    let checkpoint: Value = client
        .get(format!("{}/_fedi3/relay/transparency", relay.base_url()))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let key: [u8; 32] = B64
        .decode(checkpoint["public_key_b64"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    ed25519_dalek::VerifyingKey::from_bytes(&key)
        .unwrap()
        .verify_strict(
            &payload,
            &ed25519_dalek::Signature::from_slice(&signature).unwrap(),
        )
        .unwrap();
    let receipt: Value = serde_json::from_slice(&payload).unwrap();
    assert_eq!(receipt["event"]["outcome"], "core_rejected");
    let ack: SignedEnvelope = serde_json::from_value(receipt["event"]["core_ack"].clone()).unwrap();
    let ack_bytes = B64.decode(&ack.payload_b64).unwrap();
    fedi3_core::http_sig::verify_bytes_rsa_sha256(
        actor["publicKey"]["publicKeyPem"].as_str().unwrap(),
        &ack_bytes,
        &B64.decode(ack.signature_b64).unwrap(),
    )
    .unwrap();
    let core: CoreAcknowledgment = serde_json::from_slice(&ack_bytes).unwrap();
    assert_eq!(
        core.request_digest,
        receipt["event"]["request_digest"].as_str().unwrap()
    );
    assert_eq!(core.status, 404);
    let accepted=client.post(format!("{}/users/alice/outbox",relay.base_url()))
        .header("X-Fedi3-Internal",internal).json(&serde_json::json!({
            "type":"Create","actor":actor["id"],"to":["https://www.w3.org/ns/activitystreams#Public"],
            "object":{"type":"Note","content":"isolated receipt fixture"}
        })).send().await.unwrap();
    assert_eq!(accepted.status(), 202);
    let accepted: Value = serde_json::from_slice(
        &B64.decode(
            accepted.headers()["x-fedi3-forward-receipt"]
                .to_str()
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let accepted: Value = serde_json::from_slice(
        &B64.decode(accepted["payload_b64"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(accepted["event"]["outcome"], "core_acknowledged");
    let ack: SignedEnvelope =
        serde_json::from_value(accepted["event"]["core_ack"].clone()).unwrap();
    fedi3_core::http_sig::verify_bytes_rsa_sha256(
        actor["publicKey"]["publicKeyPem"].as_str().unwrap(),
        &B64.decode(ack.payload_b64).unwrap(),
        &B64.decode(ack.signature_b64).unwrap(),
    )
    .unwrap();
    let public: Value = client
        .get(format!("{}/_fedi3/relay/forward-log", relay.base_url()))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(public["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["commitment"] == sha256(&payload)));
    let denied = client
        .get(format!("{}/_fedi3/relay/receipts/alice", relay.base_url()))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 401);
    let records: Value = client
        .get(format!("{}/_fedi3/relay/receipts/alice", relay.base_url()))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(!records["items"].as_array().unwrap().is_empty());
}
