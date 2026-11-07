#!/usr/bin/env python3
"""Test old/new RSA and DTLS interoperability without shipping vulnerable legacy crates.

The old peer is an isolated, disposable test oracle. Its Cargo.lock is outside
this workspace and its dependencies never enter the application/release graph.
Only ephemeral localhost test sessions are used; no profiles or real keys.
"""
from pathlib import Path
import os
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CARGO = shutil.which('cargo') or str(Path.home() / '.cargo/bin/cargo')


def main():
    with tempfile.TemporaryDirectory(prefix='fedi3-legacy-webrtc-') as tmp:
        project = Path(tmp)
        (project / 'src').mkdir()
        (project / 'Cargo.toml').write_text('''[package]
name = "fedi3-legacy-webrtc-peer"
version = "0.0.0"
edition = "2021"
[workspace]
[dependencies]
webrtc = "=0.12.0"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
rustls = { version = "0.23", default-features = false, features = ["ring", "std", "tls12"] }
serde = "1"
serde_json = "1"
rsa = "=0.9.10"
rand = "0.8"
aes-gcm = { version = "0.10", features = ["std"] }
sha2 = { version = "0.10", features = ["oid"] }
base64 = "0.22"
''')
        shutil.copyfile(ROOT / 'tools/compat/legacy_webrtc_peer.rs', project / 'src/main.rs')
        env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / 'target/legacy-compat'))
        subprocess.run([CARGO, 'build', '--manifest-path', str(project / 'Cargo.toml')],
                       cwd=ROOT, env=env, check=True)
        suffix = '.exe' if os.name == 'nt' else ''
        env = dict(os.environ, FEDI3_LEGACY_WEBRTC_PEER=str(
            ROOT / 'target/legacy-compat/debug' / ('fedi3-legacy-webrtc-peer' + suffix)))
        subprocess.run([CARGO, 'test', '-p', 'fedi3_core', '--lib',
                        'legacy_webrtc_012_interoperates_in_both_sdp_roles', '--locked',
                        '--', '--ignored', '--nocapture'], cwd=ROOT, env=env, check=True)
        subprocess.run([CARGO, 'test', '-p', 'fedi3_core', '--lib',
                        'legacy_rsa_09_reads_new_envelopes_and_signatures', '--locked',
                        '--', '--ignored', '--nocapture'], cwd=ROOT, env=env, check=True)


if __name__ == '__main__':
    main()
