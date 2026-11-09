# Client 0.11.10 verification — 2026-11-09

Release: [v0.11.10](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.10).
Flutter version: `0.11.10+2013`. This release fixes the relay reservation loop
that kept P2P clients from holding a circuit, and makes the Android core start
P2P even when the platform DNS resolver is unusable.

## What changed

- Relay: per-peer reservation budget raised to 8 (configurable up to 128) and
  the libp2p reservation limiter replaced with token buckets that refill over
  time, so reconnecting clients are no longer rejected in a loop.
- Core: a single reservation per relay peer (the response already carries both
  TCP and QUIC addresses), exponential retry from 5 seconds up to 2 minutes and
  periodic relay infrastructure re-discovery, so a phone that starts before its
  network is ready recovers without an app restart.
- Android: the system DNS resolver (hickory) panics when the NDK context is not
  initialised; the core catches the panic and keeps going with IP
  multiaddresses. mDNS and listen failures no longer abort the whole stack.
- Diagnostics: core tracing is mirrored to Android logcat under the
  `fedi3-core` tag and panics are reported through tracing.

## Packages

| Package | Platform | Size (bytes) | SHA256 |
| :--- | :--- | ---: | :--- |
| `Fedi3-linux-x86_64.AppImage` | Linux x86_64 | 177198272 | `c29d6edd9d0070f4b939d64f647ac4912c73730ac040477c099adcdb6a6c00c4` |
| `Fedi3-windows-x64.zip` | Windows x64 | 57068313 | `110c99f0f5544075dd314b684be3d9ed565f565a89d19e43c82b899232729fbb` |
| `app-arm64-v8a-release.apk` | Android arm64-v8a | 73216215 | `ead7db4e82055e98049c49a94a71f7f9a9e7fb9d6cef1d726d98fa8331c982ae` |
| `app-x86_64-release.apk` | Android x86_64 | 85739589 | `f581fdcc0780a8c05b9c698e2ebee2e7bb3c7e962b14e126f3d27717af00f57a` |

The AppImage and the Windows ZIP are produced by the CI workflow on the
release commit and smoke-tested before publication (`AppImage startup` on
Ubuntu 22.04 and 24.04). The Android APKs are built locally with
`scripts/build_android.sh release`; `aapt` reports `versionName=0.11.10`,
version codes `4013` (arm64-v8a) and `6013` (x86_64).

## Verification performed

- `cargo test --workspace --locked` passes; `cargo fmt --check` and
  `cargo clippy --workspace --all-targets --locked -- -D warnings` are clean.
- `flutter analyze --fatal-infos` reports no issues and the full Flutter test
  suite passes (205 tests).
- Relay redeployed from the release commit on both relays. The reservation
  limits now report `max_reservations_per_peer: 8` with two refilling rate
  limiters, and the infrastructure log shows no `relay reservation denied`
  entries for either client over the observation window.
- Live clients: the Linux client holds a single reservation (`renewed=false`,
  then no churn) and the Android client obtains its own reservation and
  publishes its relayed address. Circuits between the two clients are accepted
  by the relay in both directions.
- Android: `adb logcat -s fedi3-core` shows `p2p configuration enable=true`,
  `p2p task starting`, `p2p transport ready`, `p2p enabled` and
  `p2p listening` with the relayed multiaddress; before the fix the P2P task
  died on the hickory resolver panic and no listener existed.

## Known limits

- Android APKs are sideload builds signed with the local debug key, not Play
  Store packages. Android accepts updates only when the signing certificate
  matches, so keep the existing local key for future builds.
- DNS-based multiaddresses (`/dns4`, `/dnsaddr`) are not resolved on Android
  while the system resolver stays unusable; IP multiaddresses, including the
  relay infrastructure address, keep working.
- iOS and macOS are buildable from source but are not part of this release.
- The Windows package is built and verified by CI; it is not run on a physical
  Windows machine as part of the release checklist.
