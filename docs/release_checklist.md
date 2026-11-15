# Reproducible release verification

This procedure separates source checks from tests of distributed packages.
Passing automated tests does not replace real-device testing. Code comments and
docstrings use English; maintained guides have English and Italian editions.

## Environment and automated gates

Use the Rust version in `rust-toolchain.toml`, Flutter **3.50.0** (as in CI),
Python 3 with PyYAML, Docker with Compose, ShellCheck and `cargo-audit`.
Install the Linux prerequisites in [build_linux.md](build_linux.md), then run
from the repository root:

```bash
export PATH="$HOME/.cargo/bin:$HOME/flutter/bin:$PATH"
cargo install cargo-audit --locked
(cd app && flutter pub get)
bash scripts/verify_release.sh
```

The script does not bump versions, commit, tag or publish. It stops at the first
failure and prints the temporary log directory; override it with
`FEDI3_VERIFY_LOG_DIR`. Each `scripts/tests/test_*.py` runs directly: unittest
discovery alone does not execute contracts implemented with `main()`.
The link check verifies local files, not external URLs or anchors.

Coverage includes federation with real relay/core processes, account sync,
spools, ActivityPub security, RSA/WebRTC with a previous client, and Flutter
widgets. Responsive checks cover first run, account creation, settings and
pairing at 320/390/1280 pixels, English/Italian, light/dark themes and 200% text.
Widget HTTP is simulated; this does not verify cameras, OS notifications or
Internet federation.

For PostgreSQL, create an **isolated disposable test database** with a dedicated
user and pass its URL as `FEDI3_TEST_POSTGRES_URL` to the same script. Never use
production: the test writes data and exercises reservations and deletion.
Without the variable the script explicitly reports the test as not run; CI
uses its own PostgreSQL service. Public IPFS bootstrap testing is separate and
requires external connectivity. `cargo audit` must succeed; review maintenance
warnings even when they are not blocking vulnerabilities.

## Protocol and data preservation

- Update [multi-device sync](multi_device_sync.md), [P2P](p2p_network.md),
  [interoperability](interop.md) and [OpenAPI](openapi.yaml) when contracts change.
- Upgrade the relay before clients negotiating new replication records. Identity
  means the complete actor URL and RSA key; a relay token does not replace key
  proof. Do not recreate an identity to recover an existing account.
- Test upgrade and restore using copied test data: identity, keys, own posts,
  chat, relationships and credentials must remain readable.
- On desktop, also test a locked keyring: show recovery and retain the profile.
  The plaintext private cache for the Android service must not be created or
  used on desktop. Legacy desktop copies are removed after successful vault
  access; unavailable vaults leave those files recoverable without using them.

## Git and packages

1. Review `git status --short`, `git diff --check` and the complete diff. Distinguish
   pre-existing edits from new work. Exclude dotenv secrets, keys, databases,
   logs and generated output from commits.
2. Align `app/pubspec.yaml` version/build, changelog and notes. The `vX.Y.Z` tag
   must match the version and identify the tested commit. Repeat checks after
   further source changes.
3. Build core and client from the same commit. A plain Linux Flutter build does
   not automatically bundle the newly built core: use packaging scripts, or run
   `scripts/build_core.sh release` after `flutter build linux --release` to copy it.
4. A tag creates a draft in CI; `codex/release/vX.Y.Z` requests automatic
   publication and must point to current `main`. Do not use that branch as an
   ordinary working area. See [updates](updates.md).
5. CI builds AppImage, Windows ZIP, Android APKs and DEB. Android signing secrets
   must exist and preserve the signing identity of earlier updates. Debug-key
   APKs do not constitute distribution packages signed for that channel.
6. Verify downloaded checksums and exercise clean install, upgrade and restart
   on Linux/Windows/Android. CI checks the AppImage's first frame on Ubuntu
   22.04/24.04. Other systems and Flatpak/AUR/F-Droid channels need the specific
   checks in [distribution](distribution.md).

## Real tests before publication

Use controlled test accounts. Follow [interoperability](interop.md) with
Mastodon/Misskey and at least two paired devices: follow/accept/unfollow,
posts/replies, private visibility, edits/deletions, attachments, chat, calls and
notifications. Exercise network loss, restart, Android suspension and recovery
while another paired device stays connected. Check camera, microphone, audio,
keyboard, focus, scroll and large text on the actual packages.

Record commit, environment, commands, outcomes and limits in a dated
`docs/audit/` report with its Italian counterpart in `docs/it/audit/`.
Previous tests are historical evidence and must not be reported as new tests
of a different package. Publish only after resolving blockers for the selected
channels and verifying backup/restore.

For the full pre-release CI graph without publication, push
`codex/verify-release/vX.Y.Z` matching pubspec. This runs Windows, Android,
AppImage and both Ubuntu startup jobs with a blocking audit gate; the publish
job stays disabled. These branches are for validation only.
