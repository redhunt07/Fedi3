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
- **Documentation:** English is the default (`docs/`); Italian counterparts are
  in `docs/it/`. Historical audits are in `docs/audit/`.

## Commands

- Build core (Linux/macOS): `scripts/build_core.sh [release|debug]` (defaults to `release`).
- Build core (Windows): `scripts/build_core.ps1 -Profile release`.
- The build scripts compile `crates/fedi3_core` and copy the produced
  `fedi3_core.dll` / `libfedi3_core.so` into `app/` and the Flutter output paths.
- Run the app: `cd app && flutter run` (Linux desktop is the primary target).
- Relay quick start: `cp .env.example .env && docker compose up -d --build`,
  then `scripts/relay_smoke_test.sh https://<domain> <ADMIN_TOKEN>`.
- AppImage: `scripts/build_appimage.sh release`.

## Important code pointers

- Rust core: `crates/fedi3_core/Cargo.toml` (crate-type `cdylib`, FFI entrypoints in `src/`).
- Workspace: root `Cargo.toml` lists the members; use `cargo build -p <crate>` for a single crate.
- Flutter app: `app/pubspec.yaml`; the native library is expected at
  `app/fedi3_core.dll` (Windows) or `app/libfedi3_core.so` (Linux).
- Relay/infra compose: root `docker-compose.yml` (environment-driven; P2P data
  under `deploy/p2p_infra/data`, ignored by git).
- OpenAPI: `docs/openapi.yaml`.

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

## Testing and debugging

- Rust: `cargo test --workspace --locked` (use `--test-threads=4` to match CI).
- Flutter: `cd app && flutter test`, `flutter analyze --fatal-infos`,
  `dart format --output=none --set-exit-if-changed lib test`.
- Deployment contracts: `python3 scripts/tests/test_deploy_config.py` and
  `python3 scripts/tests/test_backup_scripts.py`.
- When copying the native library fails, the build script prints an explicit
  message and suggests closing any running Flutter app.

## Safety and licensing

- License: AGPL-3.0-or-later; network-facing changes must remain open source.
- Do not commit credentials, `.env`, runtime databases or generated build output.
- Do not document or add the local-only `tools/team/` tooling to the public
  repository: it is excluded through `.git/info/exclude`.

## Where to look first

- `crates/fedi3_core/Cargo.toml` — native crate configuration.
- `scripts/build_core.sh` and `scripts/build_core.ps1` — canonical build steps.
- `app/pubspec.yaml` and `app/` — Flutter integration and UI.
- `docker-compose.yml` and `deploy/` — relay deployment and environment variables.
- `docs/` — English documentation; `docs/it/` — Italian counterparts.
