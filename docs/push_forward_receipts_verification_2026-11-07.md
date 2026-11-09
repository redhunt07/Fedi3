# Native push and forward receipts — verification of 7 November 2026

Linux client and relay updated; identity, relay signing key and private
configuration preserved. The client reconnected after the relay replacement.

## Result

- Android: official native UnifiedPush 3.5.0 connector, distributor selection,
  endpoint renewal/revocation, Web Push keys and encrypted message handling.
  Headless startup with a generic notification; opening the UI synchronizes the
  queue without starting competing cores in different Flutter engines.
- iOS: APNs callbacks for token, rotation/errors and background fetch; single
  completion with a native timeout; Dart bridge and short synchronization.
  Configurable entitlement, background modes and APNs environment. Added an
  Xcode phase to compile/link the static Rust core, preserving FFI symbols.
- Opt-in push settings in mobile security. Separate scope per account and relay;
  credentials in the keychain. Renewal on app resume and limited retry on the
  relay; push sending separate from message acceptance.
- Ed25519 receipts for mutating attempts to local cores and spool insertions;
  independent RSA-SHA256 confirmation from the core with nonce and request and
  response digests. Distinct states for queueing, verified response and rejection.
- Public log of chained commitments, signed checkpoints and authenticated private
  receipts. Retention with anchoring of the removed prefix, atomic writes and
  protection against late events during account deletion.
- Independent verifier `scripts/verify_forward_receipt.py` for receipts and
  complete public logs, with independently pinned keys.

## Completed proofs

- Rust workspace: **291 passed**, 2 ignored (public IPFS and PostgreSQL fixture).
- PostgreSQL 16 fixture run separately: **passed**, including media quota,
  concurrent spool, concurrent creation of the signing key, log chain and absence
  of private receipts after deletion/late events. Production database separate
  from tests; temporary container and tunnel removed.
- Flutter: **158 passed**; a further 7 targeted tests on the latest connector/API
  changes passed. Analyze fatal-infos and Clippy all-targets passed.
- Python verifier: 2 tests passed; forged signatures/unexpected keys and wrong
  bindings rejected. Full chain verified also on candidate and production.
- E2E with real relay and core: 202 response and 404 rejection, both with
  verified relay/core signatures. Tests for different nonce, altered body and
  different actor.
- Android ARM64 APK built with the included core; binary inside the APK equal to
  the Rust core after Gradle stripping. Desugaring and Tink desktop/Android
  conflict fixed. Debug APK for validation, with minimum API 24.
- iOS: plist and entitlement valid; Xcode project parsed with the OpenStep parser;
  Rust phase and linking references present; scripts passed ShellCheck. APNs
  bridge verified with Dart tests. UIKit, Apple toolchain, signing and APNs
  delivery **still require compilation and a test on macOS/a provisioned device**.
- Compose and OpenAPI validated. Docker candidate verified before deployment:
  push JSON limits, authentication, disabled configuration, private receipts,
  public log and independent signatures.

## Production verification

`https://fedi3.redhunt07.ovh/transparency` and
`https://fedi3.redhunt07.ovh/_fedi3/relay/forward-log` are available.

A private probe of the local account produced 404 and `core_rejected`: verified
Ed25519 signature with the key pinned before deployment, RSA signature with the
public key derived from the local identity, request and response byte digests and
the complete public chain. The private receipts endpoint requires the account
token. Relay/P2P/TURN healthy, no restart or panic observed.

## Operational limits and configuration

External providers remain **disabled**: a gateway compliant with the contract, a
UnifiedPush distributor and/or APNs credentials/provisioning are required. The
repository implements the gateway contract; it does not activate a provider
deployment. Actual delivery to phones was not proven with real services. The APK
was not installed on the user's device. A signed Android release requires the
project signing key; the debug build serves verification.

Receipts attest the operations observed by the relay and the core responses.
Signatures are not human read receipts or proof of processing by another
ActivityPub instance. GET/HEAD/OPTIONS and transports other than the local
tunnel/spool have separate scope. Independent witnesses and automatic comparison
between checkpoints of different relays remain further work.

Configuration and protocol: [push_forward_receipts.md](push_forward_receipts.md).

## Artifacts and rollback

- Active client: `dist/Fedi3-release-x86_64.AppImage`.
- Test APK: `app/build/app/outputs/flutter-apk/app-debug.apk`.
- Relay: `fedi3-relay:push-receipts-20261107`.
- Rollback: `fedi3-relay:before-push-receipts-20261107`.
- VPS backup: `/home/rocky/fedi3-backups/push-receipts-20261107/`, with verified
  PostgreSQL dump, previous configuration, sources and manifest.
- Previous client: `~/.cache/fedi3-audit/Fedi3-before-push-receipts.AppImage`.
- Encrypted profile backup: `~/.cache/fedi3-audit/alice-before-push-receipts-20261107.aesgcm`;
  12-byte nonce followed by AES-GCM ciphertext/tag, AAD
  `fedi3-push-receipts-20261107`. Key in the Secret Service with attributes
  application=fedi3, backup=fedi3-push-receipts-20261107.
- Journal tables are additive; binary rollback does not require deleting data or
  restoring the dump.

SHA256 fingerprint of the preserved relay public key: `510f37d2a922a0518089961729b51ebdb50b90c5569fe7a8fffeff615201e90c`.

- core_sha256: `401fd528a0a12cc576f2f825fd5610e32f752e3e99a9b18140ac15670d1f3607`
- appimage_sha256: `83a5645053bc0cdbd1fec0edd4aaa1fdb29f1592962ce90997f8de674bce3be2`
- android_core_sha256: `b227840c01927be8c02f6249df9959e29480933234e1443b66b3c1fb76c34eeb`
- android_apk_sha256: `f4f4126001e1dda57a08ab09b19e326b47a18e130ebb0f6137a786cbb5de864a`
- relay_sha256: `af7407611e0ddb5d867a2cf24c5680ce6a8ace9394942082e5b851d0e10b1620`
