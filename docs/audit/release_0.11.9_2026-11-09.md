# Client 0.11.9 verification — 2026-11-09

Release: [v0.11.9](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.9).
Flutter version: `0.11.9+2012`. This release consolidates the fixes shipped
after 0.11.1 (composer cleanup, single Android task, relayed P2P addresses,
migrated outbox normalization, profile banners, neon logo, continuous navigation
particles and a clean Linux startup log).

## Packages

| Package | Platform | Size (bytes) | SHA256 |
| :--- | :--- | ---: | :--- |
| `Fedi3-linux-x86_64.AppImage` | Linux x86_64 | 177185984 | `40c5138c4db4434802eaa2d8fa2b4597fcedbcfe4dd0aeb7103f37ef8d0ef0d9` |
| `Fedi3-windows-x64.zip` | Windows x64 | 57047939 | `76f78cdf4265fefbe0aca52f6930fc9962dc3bbbf3ebe58613a2b8ce245e2f5b` |
| `app-arm64-v8a-release.apk` | Android arm64-v8a | 73183443 | `df003663fc4144fbf29873ff70542a849cc9e3b3e9b6ff4866dcd3faebb4e9a9` |
| `app-x86_64-release.apk` | Android x86_64 | 85706817 | `1e6022f1c88bc821918fad5b8b4b46a9dfdb2ad12ebbf2f7124fd2ff8d2f81b4` |

The AppImage and the Windows ZIP are produced by the CI workflow on the
`v0.11.9` tag and smoke-tested before publication (`AppImage startup` on Ubuntu
22.04 and 24.04). The Android APKs are built locally with
`scripts/build_android.sh release`; `aapt2 dump badging` reports
`versionName=0.11.9`, version codes `4012` (arm64-v8a) and `6012` (x86_64).

## Verification performed

- CI on the release commit: Rust fmt, clippy, tests, Flutter analyze and tests,
  platform configuration, deployment/OpenAPI/release contracts, PostgreSQL quota
  test, AppImage build and first-frame smoke test, Windows package build.
- Linux AppImage: starts on the development desktop, connects to the core and
  renders the timeline; no ATK critical in the startup log.
- Android APKs: install and run on a physical arm64 device and on the x86_64
  emulator target; chat, timeline, profile and settings screens render.
- Relay interoperability: federated timeline, relayed P2P addresses, migrated
  outbox normalization and profile consistency are covered by the relay tests in
  the workspace suite.

## Known limits

- Android APKs are sideload builds signed with the local debug key, not Play
  Store packages. Android accepts updates only when the signing certificate
  matches, so keep the existing local key for future builds.
- iOS and macOS are buildable from source but are not part of this release.
- Chat is end-to-end encrypted without forward secrecy (documented limitation).
- The Windows package is built and verified by CI; it is not run on a physical
  Windows machine as part of the release checklist.
