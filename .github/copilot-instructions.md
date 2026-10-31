<!-- Auto-generated guidance for AI coding agents working on Fedi3 -->
# Fedi3 — AI Assistant Guidelines

Summary
- **Architecture:** Flutter UI in `app/` + Rust core in `crates/fedi3_core` (built as a cdylib) + relay and infra in `crates/fedi3_relay` and `crates/fedi3_p2p_infra`.
- **Primary data flow:** UI ⇄ Rust core via FFI (dll/.so copied into `app/`), core ↔ network via libp2p/webrtc, relay exposed via Docker (see `deploy/p2p_infra/docker-compose.yml`).

Quick tasks & commands
- Build core (Linux/macOS): `scripts/build_core.sh [release|debug]` (defaults to `release`).
- Build core (Windows PowerShell): `scripts/build_core.ps1 -Profile release`.
- The build scripts compile `crates/fedi3_core` and copy the produced `fedi3_core.dll` / `libfedi3_core.so` into `app/` and common Flutter output paths.
- Run the Flutter app: `cd app && flutter run -d windows` (or other supported platform).
- Relay quick start (deploy): copy env and run docker compose from repo root or `deploy/p2p_infra`: `cp .env.example .env && docker compose up -d --build` (see `deploy/p2p_infra/docker-compose.yml`).

Important code pointers
- Rust core: `crates/fedi3_core/Cargo.toml` (crate-type `cdylib`, FFI entrypoints live in `src/`).
- Workspace: root `Cargo.toml` lists the workspace members — use `cargo build -p <crate>` for single crates.
- Flutter app: `app/pubspec.yaml` — FFI `ffi` dependency is used; the native library is expected at `app/fedi3_core.dll` (Windows) or `app/libfedi3_core.so` (Linux).
- Relay/infra compose: `deploy/p2p_infra/docker-compose.yml` (environment-driven; data stored under `deploy/p2p_infra/data`).

Project-specific conventions
- Native integration: Always use the provided `scripts/build_core.*` helpers — they set platform-specific paths and copy the native artifact into Flutter expected locations.
- Windows build: `build_core.ps1` tries `VsDevCmd.bat` for correct toolchain; ensure MSVC toolchain is available when targeting Windows.
- Artifact names: expect `fedi3_core.dll` on Windows, `libfedi3_core.so` on Linux, and the target folder under `target/{release|debug}`.
- Database: `rusqlite` is built with `bundled` feature; local data is used by default (see docs in `docs/`).

Testing & debugging notes
- Rust: `cargo test -p fedi3_core` (runs crate tests if present).
- Flutter: `cd app && flutter test` for Dart tests; use `flutter run` for interactive debugging.
- When copying the native DLL fails, the build script prints an explicit message and suggests closing any running Flutter app (scripts handle this case).

Safety & licensing
- License: AGPLv3 (network-facing changes must remain open). Respect this when suggesting code that affects protocols or network behavior.

Where to look first (examples)
- `crates/fedi3_core/Cargo.toml` — native crate configuration.
- `scripts/build_core.sh` and `scripts/build_core.ps1` — canonical build and copy steps.
- `app/pubspec.yaml` and `app/` — Flutter integration and UI.
- `deploy/p2p_infra/docker-compose.yml` — relay deployment and environment variables.
- `docs/` — operational guides (deploy_core, deploy_relay, app_guide).

How to propose code changes
- Keep Rust/FFI surface minimal and preserve the cdylib ABI.
- When modifying network behavior, update `docs/` and note compatibility implications in PR description.
- For UI changes, run `flutter analyze` and `flutter test` locally.

If something's ambiguous
- Search for examples in `crates/` and `app/` first; prefer using the build scripts rather than ad-hoc cargo/flutter commands.
- Ask: which platform target (Windows/Linux/macOS) is intended — build steps differ.

End of agent guidance — ask the repo owner if additional CI or build-server credentials are required.
