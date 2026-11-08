# Client 0.11.0 verification — 2026-11-08

Release: [v0.11.0](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.0).
Flutter version: `0.11.0+3`. This release combines the Android work after 0.10.0
with every AppImage repair published in 0.10.1.

## Local source checks

- 309 standard Rust tests passed, zero failures; four special tests are ignored
  by the standard suite. PostgreSQL and interoperability are exercised separately.
- 189 Flutter tests passed; clean static analysis, Rust and Dart formatting.
- Workspace/all-target Clippy with `-D warnings` passed.
- Old/new RSA and WebRTC interoperability passed using the isolated legacy oracle.
- Concurrent PostgreSQL quota reservations passed on a disposable PostgreSQL 16 instance.
- Deployment/OpenAPI/release contracts, backup scripts, relay directory, platform
  configuration, ShellCheck and the AppImage host-library regression passed.
- `cargo audit`: zero vulnerabilities, five maintenance notices, no ignored advisories.

## Packages and limits

Linux x86_64 uses the stable name `Fedi3-linux-x86_64.AppImage`. The release
workflow builds on Ubuntu 22.04 and requires GUI startup on Ubuntu 22.04 and
24.04; the local build is also exercised on the maintainer's Arch desktop.
The release notes and associated workflow runs record package validation results.

Android `Fedi3-android-arm64-v8a.apk` and `Fedi3-android-x86_64.apk` include a freshly
compiled release Rust core for their ABI, require Android 7/API 24 or later and
are sideload builds. 32-bit ARM is unsupported. Android version is `0.11.0`,
build code `3`.

`app/android/key.properties` is not configured: APKs use the existing local debug
signing key. Retain that key for updates; changing certificates requires a backup
before reinstalling. These are not Play Store packages. Gradle warnings about
future AGP/Kotlin minimums do not represent build failures.

Package SHA256 values are provided by release `checksums.txt`. Existing profiles,
databases, core keys and keyring credentials are not migrated or deleted.
The full physical-phone testing described by earlier announcements was previous
maintainer validation, separate from this release's checks.
