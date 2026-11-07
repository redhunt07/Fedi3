//! Test-only oracle for the previously shipped webrtc 0.12 data channel.
//! Built in a disposable project by verify_legacy_webrtc.py, never in a release.
use std::{io::{self, Write}, sync::Arc};
use webrtc::{api::APIBuilder, data_channel::{RTCDataChannel, data_channel_message::DataChannelMessage},
    peer_connection::{configuration::RTCConfiguration, sdp::session_description::RTCSessionDescription}};

fn emit(value: &impl serde::Serialize) {
    println!("{}", serde_json::to_string(value).unwrap());
    io::stdout().flush().unwrap();
}
async fn read_sdp() -> RTCSessionDescription {
    tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        io::stdin().read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }).await.unwrap()
}
fn echo(channel: Arc<RTCDataChannel>) {
    let writer = channel.clone();
    channel.on_message(Box::new(move |message: DataChannelMessage| {
        let writer = writer.clone();
        Box::pin(async move {
            assert!(message.data.len() > 25 && message.data[0] == 1, "expected a legacy Fedi3 v1 binary frame");
            writer.send(&message.data).await.unwrap();
        })
    }));
}
fn encrypted_message_fixture() {
    use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    use rsa::{pkcs8::DecodePublicKey, RsaPublicKey, Oaep};
    let mut public = String::new();
    std::io::Read::read_to_string(&mut io::stdin(), &mut public).unwrap();
    let public = RsaPublicKey::from_public_key_pem(&public).unwrap();
    let key = [31u8;32];
    let nonce = [17u8;12];
    let message = br#"{"type":"Create","object":{"type":"Note","content":"messaggio precedente"}}"#;
    let wrapped = public.encrypt(&mut rand::rngs::OsRng, Oaep::new::<sha2::Sha256>(), &key).unwrap();
    let encrypted = Aes256Gcm::new_from_slice(&key).unwrap().encrypt(Nonce::from_slice(&nonce), message.as_ref()).unwrap();
    let envelope = serde_json::json!({"v":1,"alg":"rsa-oaep-sha256+aes-256-gcm",
        "ek_b64":B64.encode(wrapped),"nonce_b64":B64.encode(nonce),"ct_b64":B64.encode(encrypted)});
    emit(&serde_json::json!({"id":"legacy-message-1","method":"POST","path":"/users/alice/inbox","query":"",
        "headers":[["x-fedi3-encrypted","1"],["content-type","application/json"]],
        "body_b64":B64.encode(serde_json::to_vec(&envelope).unwrap())}));
}

fn rsa_oracle() {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    use rsa::{pkcs8::{DecodePrivateKey, DecodePublicKey}, RsaPrivateKey, RsaPublicKey, Oaep,
        pkcs1v15::VerifyingKey, signature::Verifier};
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let value: serde_json::Value = serde_json::from_str(&input).unwrap();
    let private = RsaPrivateKey::from_pkcs8_pem(value["private"].as_str().unwrap()).unwrap();
    let public = RsaPublicKey::from_public_key_pem(value["public"].as_str().unwrap()).unwrap();
    let message = B64.decode(value["message"].as_str().unwrap()).unwrap();
    let encrypted = B64.decode(value["encrypted"].as_str().unwrap()).unwrap();
    let signature = B64.decode(value["signature"].as_str().unwrap()).unwrap();
    let signature = rsa::pkcs1v15::Signature::try_from(signature.as_slice()).unwrap();
    VerifyingKey::<sha2::Sha256>::new(public).verify(&message, &signature).unwrap();
    let decrypted = private.decrypt(Oaep::new::<sha2::Sha256>(), &encrypted).unwrap();
    emit(&serde_json::json!({"decrypted": B64.encode(decrypted), "signature_valid": true}));
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    if std::env::args().nth(1).as_deref() == Some("fixture") {
        encrypted_message_fixture();
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("rsa") {
        rsa_oracle();
        return;
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    let offerer = std::env::args().nth(1).as_deref() == Some("offer");
    let pc = APIBuilder::new().build().new_peer_connection(RTCConfiguration::default()).await.unwrap();
    pc.on_data_channel(Box::new(|channel| Box::pin(async move { echo(channel); })));
    if offerer {
        echo(pc.create_data_channel("fedi3", None).await.unwrap());
        let offer = pc.create_offer(None).await.unwrap();
        let mut gathering = pc.gathering_complete_promise().await;
        pc.set_local_description(offer).await.unwrap();
        gathering.recv().await;
        emit(&pc.local_description().await.unwrap());
        pc.set_remote_description(read_sdp().await).await.unwrap();
    } else {
        pc.set_remote_description(read_sdp().await).await.unwrap();
        let answer = pc.create_answer(None).await.unwrap();
        let mut gathering = pc.gathering_complete_promise().await;
        pc.set_local_description(answer).await.unwrap();
        gathering.recv().await;
        emit(&pc.local_description().await.unwrap());
    }
    tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        io::stdin().read_line(&mut line).unwrap();
    }).await.unwrap();
    pc.close().await.unwrap();
}
