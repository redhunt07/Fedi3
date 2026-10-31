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

/// Restrict a directory to the owner (0700) on Unix. Best-effort on other platforms.
pub fn secure_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let perms = fs::Permissions::from_mode(0o700);
        fs::set_permissions(path, perms)
            .with_context(|| format!("chmod 0700 {}", path.display()))?;
    }
    Ok(())
}

/// Restrict a file to the owner (0600) on Unix. Best-effort on other platforms.
pub fn secure_file(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(path, perms)
            .with_context(|| format!("chmod 0600 {}", path.display()))?;
    }
    Ok(())
}

/// Write a secret file atomically (temp file + rename) and with 0600 permissions.
pub fn write_secret_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).with_context(|| format!("create dir {}", dir.display()))?;
    secure_dir(dir)?;
    let tmp = dir.join(format!(
        ".{}.tmp.{}",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("secret"),
        std::process::id()
    ));
    fs::write(&tmp, bytes).with_context(|| format!("write {}", tmp.display()))?;
    secure_file(&tmp)?;
    fs::rename(&tmp, path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    secure_file(path)?;
    Ok(())
}

pub fn load_or_generate_identity(dir: impl AsRef<Path>) -> Result<Identity> {
    let dir = dir.as_ref();
    fs::create_dir_all(dir).with_context(|| format!("create data dir: {}", dir.display()))?;
    secure_dir(dir)?;

    let priv_path = dir.join("identity_private_key.pem");
    let priv_pem = if priv_path.exists() {
        let pem = fs::read_to_string(&priv_path)
            .with_context(|| format!("read {}", priv_path.display()))?;
        // A corrupted file must fail loudly instead of silently regenerating a new identity
        // (which would change the actor DID and break federation).
        RsaPrivateKey::from_pkcs8_pem(&pem)
            .with_context(|| format!("parse {}", priv_path.display()))?;
        secure_file(&priv_path)?;
        pem
    } else {
        let mut rng = OsRng;
        let priv_key = RsaPrivateKey::new(&mut rng, 2048)?;
        let priv_pem = priv_key.to_pkcs8_pem(LineEnding::LF)?.to_string();
        write_secret_atomic(&priv_path, priv_pem.as_bytes())?;
        priv_pem
    };

    let private_key = RsaPrivateKey::from_pkcs8_pem(&priv_pem).context("parse private key pem")?;
    let public_key = RsaPublicKey::from(&private_key);
    let public_key_pem = public_key.to_public_key_pem(LineEnding::LF)?.to_string();

    Ok(Identity {
        private_key,
        public_key,
        public_key_pem,
        private_key_pem: priv_pem,
    })
}

/// Normalize a PEM document so that whitespace/newline differences do not change the DID.
pub fn normalize_pem(pem: &str) -> String {
    let normalized = pem.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    lines.join("\n")
}

pub fn did_from_public_key_pem(public_key_pem: &str) -> String {
    let mut h = sha2::Sha256::new();
    h.update(normalize_pem(public_key_pem).as_bytes());
    let hex = hex::encode(h.finalize());
    // 128-bit (32 hex) identifier is enough for UX and avoids huge strings.
    format!("did:fedi3:{}", &hex[..32])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    #[test]
    fn did_is_stable_across_whitespace() {
        let pem = "-----BEGIN PUBLIC KEY-----\nAAAA\n-----END PUBLIC KEY-----";
        let padded = "-----BEGIN PUBLIC KEY-----\r\n  AAAA  \r\n\r\n-----END PUBLIC KEY-----\n";
        assert_eq!(
            did_from_public_key_pem(pem),
            did_from_public_key_pem(padded)
        );
    }

    #[test]
    fn identity_files_have_restricted_permissions() {
        let dir = TempDir::new("keys");
        let path = dir.path().join("app");
        let identity = load_or_generate_identity(&path).unwrap();
        assert!(!identity.private_key_pem.is_empty());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = fs::metadata(path.join("identity_private_key.pem"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
            let dir_mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(dir_mode, 0o700);
        }
    }

    #[test]
    fn corrupted_key_file_is_an_error_not_a_regeneration() {
        let dir = TempDir::new("keys");
        let path = dir.path().join("app");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("identity_private_key.pem"), "not a pem").unwrap();
        assert!(load_or_generate_identity(&path).is_err());
    }
}
