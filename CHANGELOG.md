# Changelog

All notable changes to FEDI3 are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Test infrastructure for the core and the relay: in-process mock ActivityPub servers, request signers, ephemeral-port harnesses.
- CI pipeline: formatting, linting, build, tests, Flutter analyze/test, platform configuration checks.
- `scripts/verify_platform_config.sh` to catch release-breaking platform regressions.
- `SECURITY.md` with disclosure policy and the security model.

### Changed
- The relay now verifies the HTTP signature on `/inbox` and requires a `Digest` bound to the signed headers.
- Inbound activities must be signed by the actor they claim to come from.
- `POST /users/{u}/outbox` requires the internal token or a valid signature.
- The tunnel reconnects with exponential backoff instead of dying on the first network error.
- `fedi3_core_start` returns success only after the local server is listening.
- The relay token is optional: local-only and P2P-only instances start without a relay.
- Health and readiness endpoints are unauthenticated and side-effect free.
- Client IP is derived from the proxy-appended `X-Forwarded-For` entry; rate limits are per route.
- SQLite uses `busy_timeout`; delivery workers survive transient database errors.
- The app uses bounded HTTP timeouts, typed API errors, atomic configuration writes, and supervises the core with a health watchdog.
- Post text is always converted to escaped HTML before publishing.

### Fixed
- Android release builds had no `INTERNET` permission.
- Android cleartext traffic to the loopback core was blocked on API 28+.
- macOS release entitlements granted no network or file access.
- iOS lacked the ATS exception for the local core.
- Editing the configuration stopped the core and never restarted it.
- The widget test asserted a string the app never rendered.
- `relay_diagnose.sh` used `wget`, which is not present in the relay image.
- Backup scripts ignored the configured PostgreSQL user/database and nested media archives incorrectly.

### Security
- Panics no longer cross the FFI boundary.
- Actor impersonation through unbound HTTP signatures is rejected.
- Admin and user tokens are compared in constant time.
- Private key files are created with `0600` permissions on Unix.
- P2P frames are length-capped before allocation.
- Media identifiers received from the relay are sanitized before touching the filesystem.

## [0.1.0] - 2026-01-13

### Added
- Initial public snapshot: Flutter client, Rust core (ActivityPub, P2P, chat, media), public relay, Docker Compose deployment, operational documentation.
