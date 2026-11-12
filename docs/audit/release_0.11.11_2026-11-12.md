# Client 0.11.11 verification — 2026-11-12

Release: [v0.11.11](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.11).
Flutter version: `0.11.11+4015`. This release collects the work published since
0.11.10: multi-device chat sync with QR login, playable voice and video notes,
the push-only Android service with lower background drain, background P2P
suspension, and the optional self-hosted push gateway.

## What changed

- Android: push-only foreground service that polls the relay chat spool and
  starts the core only when new items exist; process-wide power profile and
  stretched network cadences; P2P suspension while backgrounded when a relay
  tunnel can deliver; connection caps, IPv6 filtering without a route and mDNS
  disabled on mobile; UI timers slowed and hidden-page tickers stopped.
- Multi-device: relay identity blobs, QR pair sessions, chat spool and
  endpoints; core identity export, chat snapshot and spool polling; QR login
  inverted so the logged-in client shows the code and the new device scans it;
  profile and moderation settings carried in the identity package; chat
  snapshots imported in one transaction.
- Media: voice and video notes load and play on both sides, with mediaType
  fallback, recipient play control, inline video overlay and audio track in the
  video recorder.
- Build and tooling: Flutter 3.50 with material_ui/cupertino_ui, cancellable SSE
  reconnect timers, Gradle 9.5.0 / AGP 9.3.1 / Kotlin 2.4.20 / desktop_drop
  0.8.4, optional push gateway, CI format/compose/OpenAPI/libmpv fixes.

## Packages

| Package | Platform | Size (bytes) | SHA256 |
| :--- | :--- | ---: | :--- |
| `Fedi3-linux-x86_64.AppImage` | Linux x86_64 | 185595072 | `fc0b48e2d4704d9d244993f5c2777ff968540d8080472b30282649c8733a00ba` |
| `Fedi3-windows-x64.zip` | Windows x64 | 57521500 | `c4ecbc5b88fc8ebc07dd011aa1a4da1065936d2905a32aae42305e55fdc5f4fd` |
| `app-arm64-v8a-release.apk` | Android arm64-v8a | 80936286 | `2e7624120cf44fdccb28f7ec0898b7197a446fb780a183fbfc64b13595a7d2a0` |
| `app-x86_64-release.apk` | Android x86_64 | 94305619 | `bb32d3949ca7ff397e5b9a54fa875ed6a25f6683e35459aba3b21bce6c3ad705` |

The AppImage and the Windows ZIP are produced by the CI workflow on the release
commit and smoke-tested before publication (`AppImage startup` on Ubuntu 22.04
and 24.04). The Android APKs are built locally with
`scripts/build_android.sh release`; `aapt` reports `versionName=0.11.11`,
version codes `6015` (arm64-v8a) and `8015` (x86_64). The release build needs
JDK 21 for the AGP 9 lint tasks; the local build used
`/usr/lib/jvm/java-21-openjdk`.

The Linux AppImage was rebuilt on 2026-11-12 with a Mesa EGL fallback in
`AppRun`. Flutter 3.50.0 has a Linux compositor regression
([flutter#195885](https://github.com/flutter/flutter/issues/195885)): on NVIDIA
proprietary drivers `glBlitFramebuffer` is blocklisted and the client content is
shifted by half a window. The fallback forces the Mesa EGL vendor (llvmpipe)
when available; it can be removed once Flutter 3.50.1 ships the upstream fix.

## Verification performed

- CI on the release commit `dbc76d6`: Rust fmt, clippy and workspace tests,
  Flutter analyze and tests, platform/deployment/release contracts, PostgreSQL
  quota test, AppImage build with first-frame smoke test on Ubuntu 22.04 and
  24.04, Windows package build, and draft publication — all jobs succeeded.
- Local: `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked
  -- -D warnings` and `cargo test --workspace --locked` pass; `flutter analyze
  --fatal-infos` reports no issues and the full Flutter test suite passes (236
  tests); `dart format --set-exit-if-changed lib test` is clean; shellcheck and
  the deploy/release/relay/backup/AppImage contracts pass; the legacy WebRTC
  compatibility test passes.
- Android: the app was installed on a Xiaomi 2201123G (Android 15), the core
  started against `wss://fedi3.redhunt07.ovh/tunnel/redhunt07`, the timeline
  rendered with posts, navigation bar and composer, and no Flutter exceptions
  appeared in logcat.
- Battery: on the same device the push-only service returns `items=0` on a
  settled spool and keeps the core stopped; idle CPU is about 0.1-1 % of a core.
  A wake cycle was exercised end to end: spool item → core start → notification
  → 30 s quiet → `[bg] core stopped (idle)`.

## Known limits

- Android APKs are sideload builds signed with the local debug key, not Play
  Store packages. Android accepts updates only when the signing certificate
  matches; users installing from a different build must uninstall first.
- Push-only mode needs a relay tunnel and the relay chat spool; P2P-only
  accounts keep the core running because there is no spool to poll.
- The optional push gateway requires its own per-relay configuration and is not
  needed for the push-only foreground service.
