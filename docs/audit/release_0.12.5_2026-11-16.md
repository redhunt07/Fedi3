# Fedi3 0.12.5 verification — 16 November 2026

Release source: `319bdedcaf07ca742f39db1115cc71402c35004b`.
Client version: `0.12.5+6022` (split APK codes 8022/10022).

## Changes and source gates

Restored profile history, replies, media and public outbox pagination; excluded
existing/pending follows from suggestions; combined search accepts partial coverage
instead of returning 503. Persisted media filenames survive restart. Mobile P2P
supports WSS through HTTPS port 443, preserves DNS/TLS hostname validation and
initializes Android's native DNS context before Flutter/background engines start.

The full local release gate passed: workspace format, Clippy with warnings denied,
build, dependency audit, Python contracts, platform checks, ShellCheck, Dart format,
Flutter analysis and **285 Flutter tests**. **367 Rust tests** passed in the ordinary
suite; the four normally ignored cases also passed separately: two legacy oracle
cases, isolated PostgreSQL atomic quota and public IPFS bootstrap (**371 total**).
Dependency audit retains the documented maintenance warnings for paste,
pqcrypto-internals/kyber/traits and rustls-pemfile.

The complete candidate workflow [43928880328](https://github.com/redhunt07/Fedi3/actions/runs/43928880328)
succeeded, including Windows/Android/Linux packages and Ubuntu startup checks.

## Live deployment and data

The primary relay and P2P infrastructure were upgraded after PostgreSQL, media,
configuration and peer identity backups. Nodeinfo reports 0.12.5; readiness,
health and authenticated metrics smoke checks pass. The infrastructure peer ID
is unchanged. Caddy proxies `/_fedi3/p2p` to the internal WebSocket listener.

The user's installed desktop Flatpak and physical Android phone run 0.12.5.
The Flatpak version metadata was corrected as well. Updates preserve account DID,
actor, peer identity, signing identity, follows and existing database. Desktop
SQLite integrity checks pass and the six followed accounts persist. All five
persisted media files match their HTTPS originals byte for byte after restart.

On the physical phone after restart, the profile shows “Ciao mondo!”, peppenamir's
posts and replies, Panormus's posts/replies and populated media tab. Searching
`s` with source “Tutti” returns posts without the former 503.

Read-only requests over TCP and WSS circuits to the desktop returned the same
object and media bytes as HTTPS. Object SHA256:
`37aa80c45ee44a49fd272ccae81039a880c30b536bb04666591062da372641d8`;
media SHA256: `cc047ad9882f55abc2af7c0e5f6c598d57c515433198eb0c3fdae1491a9a0b23`.
The phone accepted a WSS reservation over cellular and served the same object
through that circuit. The historical media fixture is absent from the phone's
local cache (404); this does not establish phone media-transfer coverage.
No test posts, messages or follow changes were submitted.

## Access limitation

The Palermo relay remains reachable over HTTPS, but SSH port 2200 and the VPS
reverse tunnel on 2223 are unavailable. The supplied root credential is rejected
by the YunoHost administrative API (401). Restoring the tunnel requires a valid
YunoHost administrator login or a local restart of `fedi3-tunnel`; no successful
update of that second server is claimed.

The official CI ARM64 artifact was installed in place on the phone. APK signature
verification succeeds and matches the existing installation certificate. Core
version, DID, peer identity, outbox and a fresh relay reservation were verified
after that installation.

## Published release

All jobs in the [official workflow](https://github.com/redhunt07/Fedi3/actions/runs/43932998624)
succeeded and [v0.12.5](https://github.com/redhunt07/Fedi3/releases/tag/v0.12.5)
was published with AppImage, Debian, Windows ZIP, ARM64/x86_64 APKs and checksums.
A subsequent desktop log inspection found a separate missing-metadata case:
a media uploaded from another device exists on the relay but the local URL
returns 404 when no media row exists. A follow-up patch is required for this case.
Disposable P2P probes can also encounter a cold-connection `ConnectionClosed`;
successful TCP/WSS reads are established, but uninterrupted delivery is not claimed.

SHA256: 5/5 published packages verified; installed ARM64 APK matches the public asset.
