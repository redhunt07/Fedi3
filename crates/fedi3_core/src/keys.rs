/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use anyhow::{Context, Result};
use directories::ProjectDirs;
use rand::rngs::OsRng;
use rsa::{
    pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey, LineEnding},
    RsaPrivateKey, RsaPublicKey,
};
use sha2::Digest as _;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Identity {
    pub private_key: RsaPrivateKey,
    pub public_key: RsaPublicKey,
    pub public_key_pem: String,
    pub private_key_pem: String,
}

pub fn default_data_dir() -> Result<PathBuf> {
    if let Ok(v) = std::env::var("FEDI3_DATA_DIR") {
        return Ok(PathBuf::from(v));
    }
    let proj = ProjectDirs::from("net", "fedi3", "Fedi3")
        .context("unable to determine platform data dir")?;
    Ok(proj.data_local_dir().to_path_buf())
}

pub fn load_or_generate_identity(dir: impl AsRef<Path>) -> Result<Identity> {
    let dir = dir.as_ref();
    fs::create_dir_all(dir).with_context(|| format!("create data dir: {}", dir.display()))?;

    let priv_path = dir.join("identity_private_key.pem");
    let priv_pem = if priv_path.exists() {
        fs::read_to_string(&priv_path).with_context(|| format!("read {}", priv_path.display()))?
    } else {
        let mut rng = OsRng;
        let priv_key = RsaPrivateKey::new(&mut rng, 2048)?;
        let priv_pem = priv_key.to_pkcs8_pem(LineEnding::LF)?.to_string();
        fs::write(&priv_path, &priv_pem).with_context(|| format!("write {}", priv_path.display()))?;
        priv_pem
    };

    let private_key = RsaPrivateKey::from_pkcs8_pem(&priv_pem)
        .context("parse private key pem")?;
    let public_key = RsaPublicKey::from(&private_key);
    let public_key_pem = public_key
        .to_public_key_pem(LineEnding::LF)?
        .to_string();

    Ok(Identity {
        private_key,
        public_key,
        public_key_pem,
        private_key_pem: priv_pem,
    })
}

pub fn did_from_public_key_pem(public_key_pem: &str) -> String {
    let mut h = sha2::Sha256::new();
    h.update(public_key_pem.as_bytes());
    let hex = hex::encode(h.finalize());
    // 128-bit (32 hex) identifier is enough for UX and avoids huge strings.
    format!("did:fedi3:{}", &hex[..32])
}
