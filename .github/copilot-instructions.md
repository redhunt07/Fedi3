<!-- Guidance for AI coding agents working on Fedi3 -->
# Fedi3 — AI assistant guidelines

## Summary

- **Architecture:** Flutter client in `app/`, local Rust core in
  `crates/fedi3_core` (built as a cdylib and loaded through FFI), public relay in
  `crates/fedi3_relay`, shared protocol types in `crates/fedi3_protocol` and P2P
  infrastructure in `crates/fedi3_p2p_infra`.
- **Primary data flow:** UI ⇄ local core over HTTP/FFI; the core federates
  through the relay tunnel and P2P transport. The relay is deployed with the
  root `docker-compose.yml`.
- **Project state:** pre-1.0; current release 0.11.9 (build 2012), supported
  line 0.11.x. Read `README.md` and `AGENTS.md` for the authoritative layout and
  commands; do not duplicate them here.
- **Documentation:** English is the default (`docs/`); every document has an
  Italian counterpart in `docs/it/`. Historical audits are in `docs/audit/`.
  Root policy files (`CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`,
  `GOVERNANCE.md`, `FORK_POLICY.md`, `ETHICS.md`, `NETWORK_INTEGRITY.md`,
  `PROTOCOL_COMPATIBILITY.md`, `TRADEMARK_AND_NAME.md`) are English only.
- **License:** AGPL-3.0-or-later. Network-facing changes must remain open
  source.

## Commands

- Build core (Linux/macOS): `scripts/build_core.sh [release|debug]` (defaults to
  `release`).
- Build core (Windows): `scripts/build_core.ps1 -Profile release`.
- The build scripts compile `crates/fedi3_core` and copy the produced
  `fedi3_core.dll` / `libfedi3_core.so` into `app/` and the Flutter output paths.
- Run the app: `cd app && flutter run` (Linux desktop is the primary target).
- Relay quick start: `cp .env.example .env && docker compose up -d --build`,
  then `scripts/relay_smoke_test.sh https://<domain> <ADMIN_TOKEN>`.
- AppImage: `scripts/build_appimage.sh release`.

## Important code pointers

- Rust core: `crates/fedi3_core/Cargo.toml` (crate-type `cdylib`, FFI entrypoints
  in `src/`).
- Workspace: root `Cargo.toml` lists the members; use `cargo build -p <crate>`
  for a single crate.
- Flutter app: `app/pubspec.yaml`; the native library is expected at
  `app/fedi3_core.dll` (Windows) or `app/libfedi3_core.so` (Linux).
- Relay/infra compose: root `docker-compose.yml` (environment-driven; P2P data
  under `deploy/p2p_infra/data`, ignored by git).
- OpenAPI: `docs/openapi.yaml`.
- Compatibility evidence: `docs/audit/activitypub_compatibility_matrix.md`,
  `docs/interop.md` and `docs/audit/crypto_compatibility_2026-11-08.md`.

## Conventions

- Native integration: always use `scripts/build_core.*`; it sets platform paths
  and copies the artifact where Flutter expects it.
- Windows build: `build_core.ps1` tries `VsDevCmd.bat`; ensure the MSVC toolchain
  is available.
- Artifact names: `fedi3_core.dll` on Windows, `libfedi3_core.so` on Linux,
  under `target/{release|debug}`.
- Database: `rusqlite` is built with the `bundled` feature; local SQLite is the
  default, PostgreSQL is available for the relay.
- Keep the Rust/FFI surface minimal and preserve the cdylib ABI.
- Keep panics unwinding: `catch_unwind` is used at the FFI boundary, so
  `panic = "abort"` must not be enabled.
- Commit messages follow Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`,
  `refactor:`, `ci:`, `chore:`), with an imperative subject.
- Keep the client visually consistent with the existing Aurora design system.
- Relay-to-relay peering uses HTTP. Do not document removed libp2p mesh
  prototypes as a running subsystem.
- Do not edit generated output (`dist/`, `target/`, `build/`, `AppDir/`,
  `*.so`, `*.dll`) and do not edit `Cargo.lock` by hand.

## Testing and debugging

- Rust: `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo test --workspace --locked` (CI uses `-- --test-threads=4`).
- Flutter: `cd app && flutter test`, `flutter analyze --fatal-infos`,
  `dart format --output=none --set-exit-if-changed lib test`.
- Platform configuration: `bash scripts/verify_platform_config.sh`.
- Deployment, release, relay directory and backup contracts:
  `python3 scripts/tests/test_deploy_config.py`,
  `python3 scripts/tests/test_release_contract.py`,
  `python3 scripts/tests/test_relay_directory.py`,
  `python3 scripts/tests/test_backup_scripts.py`,
  `python3 scripts/tests/test_appimage_host_libs.py`.
- ShellCheck: use the same file list as `.github/workflows/ci.yml`.
- Crypto/interop: `python3 scripts/verify_legacy_webrtc.py` (CI release gate),
  `python3 scripts/tests/test_forward_receipts.py` (unit checks) and the
  `scripts/verify_forward_receipt.py` CLI, which needs a receipt file and a
  pinned key (see `docs/push_forward_receipts.md`).
- When copying the native library fails, the build script prints an explicit
  message and suggests closing any running Flutter app.

## Safety and licensing

- License: AGPL-3.0-or-later; network-facing changes must remain open source.
- Do not commit credentials, `.env`, runtime databases or generated build output.
- Preserve SPDX headers and copyright notices; add attribution when modifying a
  file substantially.
- Federated HTML and media are untrusted input: keep sanitisation, size caps and
  SSRF protections intact.
- Do not add tracking, profiling or advertising. Telemetry stays opt-in.
- Do not document or add the local-only `tools/team/` tooling to the public
  repository: it is excluded through `.git/info/exclude`.

## Where to look first

- `crates/fedi3_core/Cargo.toml` — native crate configuration.
- `scripts/build_core.sh` and `scripts/build_core.ps1` — canonical build steps.
- `app/pubspec.yaml` and `app/` — Flutter integration and UI.
- `docker-compose.yml` and `deploy/` — relay deployment and environment variables.
- `docs/` — English documentation; `docs/it/` — Italian counterparts.
