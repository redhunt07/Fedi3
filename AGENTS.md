# Fedi3 - Agent notes

## Index
- [Overview](#overview)
- [Key paths](#key-paths)
- [Common commands](#common-commands)
- [Docs map](#docs-map)
- [Testing](#testing)
- [Notes for agents](#notes-for-agents)
- [Workflows](#workflows)
- [Module inventory (summary)](#module-inventory-summary)
- [Entry points](#entry-points)
- [Configs](#configs)
- [HTTP endpoints (quick map)](#http-endpoints-quick-map)
- [Glossary](#glossary)
- [Debugging tips](#debugging-tips)
- [Quick setup checklist](#quick-setup-checklist)
- [Troubleshooting quick hits](#troubleshooting-quick-hits)
- [Dependency map (high level)](#dependency-map-high-level)
- [Relay env key notes](#relay-env-key-notes)
- [Core and app config notes](#core-and-app-config-notes)

## Overview
Fedi3 is a federated social app with a local Rust core (AP + P2P + cache),
a public relay for compatibility, and a Flutter UI client. The repo is a Rust
workspace plus a Flutter app and deployment assets.

## Key paths
- `app/`: Flutter client (UI, state, services, platform runners).
- `crates/fedi3_core/`: local Rust core (FFI to app).
- `crates/fedi3_relay/`: public relay service.
- `crates/fedi3_protocol/`: shared protocol types.
- `crates/fedi3_p2p_infra/`: P2P infra mailbox/peer services.
- `deploy/`: Dockerfiles, compose, entrypoints, infra assets.
- `scripts/`: build and ops scripts (core build, relay smoke test, backups).
- `dist/`, `target/`: build outputs. Avoid editing.
- `.env.example`: relay config template (used by docker compose).

## Common commands
### Relay (Docker)
```
cp .env.example .env
docker compose up -d --build
```

Smoke test:
```
scripts/relay_smoke_test.sh https://relay.example.com <ADMIN_TOKEN>
```

### Core (local Rust)
```
scripts/build_core.ps1   # Windows
scripts/build_core.sh    # Linux
```

The app loads the core binary via FFI at runtime.

### App (Flutter)
```
cd app
flutter pub get
flutter run
```

## Docs map
- `docs/getting_started.md`: quick start (relay + app).
- `docs/deploy_relay.md`: full relay config, TLS, monitoring.
- `docs/deploy_core.md`: local core runtime and health endpoints.
- `docs/app_guide.md`: app usage and Linux build steps.
- `docs/ops_backup_restore.md`: backup/restore guidance.
- `docs/troubleshooting.md`: common issues.

## Testing
- Flutter: `app/test/` (run via `flutter test` in `app/`).
- Rust: standard `cargo test` from repo root (workspace).

## Notes for agents
- Prefer editing source under `app/` and `crates/`; avoid touching `dist/` and
  `target/` (generated artifacts).
- Relay settings are driven by `.env` and `docker-compose.yml`.
- Health endpoints for core and relay are documented in `docs/deploy_core.md`
  and `docs/deploy_relay.md`.

## Workflows
### Debug core (local)
- Build core: `scripts/build_core.ps1` or `scripts/build_core.sh`
- Run the app and watch the core logs from the app console.
- Health checks (local): `/healthz`, `/readyz`, `/_fedi3/health`

### Develop relay (Docker)
- Configure `.env` then `docker compose up -d --build`
- Logs: `docker compose logs --tail=200 fedi3-relay` (service name in compose)
- Smoke test: `scripts/relay_smoke_test.sh https://relay.example.com <TOKEN>`

### Release notes (high level)
- Windows build scripts in `scripts/release_build_windows.ps1`
- Linux build scripts in `scripts/release_build_linux.sh` and AppImage helpers
  in `scripts/build_appimage.sh` and `build_appimage.sh`

## Module inventory (summary)
### Rust
- `crates/fedi3_core/`: local core (AP, P2P, storage, HTTP, FFI bridge).
- `crates/fedi3_relay/`: relay service, media storage, relay mesh.
- `crates/fedi3_protocol/`: protocol types and shared models.
- `crates/fedi3_p2p_infra/`: P2P mailbox and infra services.

### Flutter
- `app/lib/core/`: FFI and core API bindings.
- `app/lib/services/`: app services (telemetry, encryption, backup, relay admin).
- `app/lib/state/`: state stores.
- `app/lib/ui/`: UI root, screens, widgets, theme, and utils.

## Entry points
### Rust
- `crates/fedi3_relay/src/main.rs`: relay service main.
- `crates/fedi3_p2p_infra/src/main.rs`: P2P infra main.
- `crates/fedi3_core/src/lib.rs`: core library entry used by FFI.
- `crates/fedi3_core/src/bin/`: dev tools (client, post note, resolve actor).

### Flutter
- `app/lib/main.dart`: Flutter app entry.
- `app/lib/ui/app_root.dart`: UI root widget wiring.
- `app/lib/ui/shell.dart`: app shell and layout composition.

## Configs
- `.env.example` -> `.env`: relay configuration template.
- `docker-compose.yml`: relay + infra orchestration.
- `app/lib/model/core_config.dart`: app-side core config model.
- `app/l10n.yaml`: Flutter localization configuration.

## HTTP endpoints (quick map)
### Core (local)
- `GET /healthz`
- `GET /readyz`
- `GET /_fedi3/health` (requires `X-Fedi3-Internal`)
- `GET /_fedi3/net/metrics` or `/_fedi3/net/metrics.prom` (requires `X-Fedi3-Internal`)

### Relay
- `GET /healthz` (requires `Authorization: Bearer <ADMIN_TOKEN>`)
- `GET /readyz` (requires `Authorization: Bearer <ADMIN_TOKEN>`)
- `GET /_fedi3/relay/metrics.prom` (requires `Authorization: Bearer <ADMIN_TOKEN>`)
- `GET /_fedi3/relay/stats` (relay mesh visibility)

## Glossary
- Core: local Rust runtime used by the app (AP + P2P + cache).
- Relay: public service for federation compatibility and routing.
- P2P: peer-to-peer layer for direct sync.
- Relay mesh: server-side P2P mesh among relays.
- Mailbox: P2P infra component for message routing.

## Debugging tips
- Relay logs: `docker compose logs --tail=200 fedi3-relay` (service name in compose).
- Caddy logs: `docker compose logs --tail=200 caddy`.
- Core health: use endpoints in "HTTP endpoints" with the internal header.
- Check relay mesh: `/_fedi3/relay/stats` includes `relay_p2p_peer_id`.

## Quick setup checklist
### Windows
- Install Flutter SDK and required Windows build tools.
- `cd app && flutter pub get && flutter run`
- Build core: `scripts/build_core.ps1`

### Linux
- Install Flutter and desktop deps (see `docs/app_guide.md`).
- `cd app && flutter pub get && flutter run`
- Build core: `scripts/build_core.sh`

## Troubleshooting quick hits
- Relay not reachable: confirm DNS, ports 80/443, and `FEDI3_DOMAIN`.
- ACME/TLS errors: check `docker compose logs --tail=200 caddy`.
- Core not responding: verify core build ran and check `/healthz`.
- Media issues: validate media backend config in `.env`.
- Relay mesh not syncing: check `/_fedi3/relay/stats` and relay logs.

## Dependency map (high level)
- Flutter app -> FFI -> `fedi3_core` (local runtime).
- Core -> relay (ActivityPub compatibility + routing).
- Relay -> DB + media backend + optional mesh + TURN.

## Relay env key notes
- `FEDI3_DOMAIN`: public relay domain used by Caddy and public URLs.
- `FEDI3_RELAY_ADMIN_TOKEN`: admin auth for health/metrics and admin ops.
- `FEDI3_RELAY_TELEMETRY_TOKEN`: optional token for telemetry endpoints.
- `FEDI3_RELAY_DB_DRIVER`: database driver (typically `postgres`).
- `FEDI3_RELAY_DB_URL`: connection string for the relay DB.
- Media backend keys: S3/WebDAV/local settings (see `.env.example`).
- Relay mesh keys: `FEDI3_RELAY_MESH_ENABLE`, `FEDI3_RELAY_MESH_KEY`,
  `FEDI3_RELAY_MESH_LISTEN`, `FEDI3_RELAY_MESH_BOOTSTRAP`.

## Core and app config notes
- `internal_token`: core internal auth token used for protected endpoints.
- `public_base_url`: core public base URL used for links.
- `relay_ws`: relay websocket URL used by the core.
- Core start payload fields (app -> core) in `app/lib/model/core_config.dart`:
  `username`, `domain`, `public_base_url`, `relay_ws`, `relay_token`, `bind`,
  `internal_token`, plus optional `display_name`, `summary`, `icon_url`,
  `icon_media_type`, `image_url`, `image_media_type`, `profile_fields`,
  `manually_approves_followers`, `blocked_domains`, `blocked_actors`,
  `ap_relays`, `bootstrap_follow_actors`, `previous_public_base_url`,
  `previous_relay_token`, `upnp_port_start`, `upnp_port_end`,
  `upnp_lease_secs`, `upnp_timeout_secs`.
- App settings: surfaced in UI and stored via state stores under `app/lib/state/`.
### Defaults and constraints
- If `public_base_url` is empty, core infers it from `relay_ws`.
- `relay_ws` must include a scheme; core accepts `ws://`, `wss://`, `http://`,
  `https://` for base inference (tunnel expects websocket in practice).
- Defaults when creating a local profile: `http://127.0.0.1:8787` and
  `ws://127.0.0.1:8787`.
- If `internal_token` is empty, core generates a random token.
- `relay_token` is required and must be at least 16 chars (core runtime check).
- UI validation enforces relay token length >= 16 on onboarding/edit.
- Relay switch guard: core may require `previous_public_base_url` unless
  `allow_relay_switch_without_migration=true`.
  This flag is not exposed in the app UI (core-only config).
### UI mapping
- Onboarding (public base, relay ws, relay token, internal token):
  `app/lib/ui/screens/onboarding_screen.dart`
- Edit relay/config (public base, relay ws, relay token, internal token):
  `app/lib/ui/screens/edit_config_screen.dart`
- Security (internal token): `app/lib/ui/screens/security_settings_screen.dart`
- Moderation (blocked domains/actors): `app/lib/ui/screens/moderation_settings_screen.dart`
- Profile fields (display name, summary, fields, icons):
  `app/lib/ui/screens/profile_edit_screen.dart`
- Networking view (read-only summary): `app/lib/ui/screens/networking_settings_screen.dart`

### Advanced core options (core-only)
- `p2p`: P2P config block (mailbox/cache options).
- `media`: media backend config (relay/local/S3/WebDAV).
- `storage`: storage GC and cache limits.
- `data_dir`: override default data directory.
- `max_date_skew_secs`: HTTP signature clock skew tolerance.
- `http_timeout_secs`: outbound request timeout.
- `max_body_bytes`: inbound request size cap.
- `post_delivery_mode`: `p2p_only` or `p2p_relay`.
- `p2p_relay_fallback_secs`: delay before relay fallback.
- `p2p_cache_ttl_secs`: mailbox cache TTL.
- `global_ingest_max_items_per_actor_per_min` and
  `global_ingest_max_bytes_per_actor_per_min`: rate limits.
- `legacy_aliases`: list of legacy actor URLs for migration.

## Config examples
### Core start payload (minimal)
```json
{
  "username": "alice",
  "domain": "example.invalid",
  "public_base_url": "http://127.0.0.1:8787",
  "relay_ws": "ws://127.0.0.1:8787",
  "relay_token": "example-token-16chars",
  "bind": "127.0.0.1:8788",
  "internal_token": "example-internal-token"
}
```

### Relay .env (minimal)
```
FEDI3_DOMAIN=relay.example.com
FEDI3_RELAY_ADMIN_TOKEN=change_me_admin_token
FEDI3_RELAY_DB_DRIVER=postgres
FEDI3_RELAY_DB_URL=postgres://user:pass@db:5432/fedi3
```

### Core start payload (full example)
```json
{
  "username": "alice",
  "domain": "example.invalid",
  "public_base_url": "https://relay.example.com",
  "previous_public_base_url": "https://old-relay.example.com",
  "previous_relay_token": "old-relay-token-16chars",
  "relay_ws": "wss://relay.example.com",
  "relay_token": "new-relay-token-16chars",
  "bind": "127.0.0.1:8788",
  "internal_token": "example-internal-token",
  "display_name": "Alice",
  "summary": "<p>hello world</p>",
  "icon_url": "https://relay.example.com/media/avatar.png",
  "icon_media_type": "image/png",
  "image_url": "https://relay.example.com/media/banner.png",
  "image_media_type": "image/png",
  "profile_fields": [
    {"name": "site", "value": "<p>https://example.com</p>"}
  ],
  "manually_approves_followers": true,
  "blocked_domains": ["spam.example"],
  "blocked_actors": ["https://bad.example/users/evil"],
  "ap_relays": ["https://relay.misskey.io/actor"],
  "bootstrap_follow_actors": ["@announce@relay.example.com"],
  "upnp_port_start": 40000,
  "upnp_port_end": 40100,
  "upnp_lease_secs": 3600,
  "upnp_timeout_secs": 10,
  "http_timeout_secs": 30,
  "max_body_bytes": 20971520,
  "post_delivery_mode": "p2p_relay",
  "p2p_relay_fallback_secs": 10,
  "p2p_cache_ttl_secs": 3600,
  "legacy_aliases": ["https://mastodon.example/@alice"],
  "p2p": {
    "enable": true,
    "mailbox_cache_ttl_secs": 3600
  },
  "media": {
    "backend": "relay",
    "relay_base_url": "https://relay.example.com",
    "relay_token": "new-relay-token-16chars"
  },
  "storage": {
    "media_max_local_cache_bytes": 2147483648
  },
  "data_dir": "C:/Users/alice/AppData/Local/Fedi3/app/alice"
}
```

### Relay .env (with media + mesh)
```
FEDI3_DOMAIN=relay.example.com
FEDI3_RELAY_ADMIN_TOKEN=change_me_admin_token
FEDI3_RELAY_TELEMETRY_TOKEN=change_me_telemetry_token
FEDI3_RELAY_DB_DRIVER=postgres
FEDI3_RELAY_DB_URL=postgres://user:pass@db:5432/fedi3
FEDI3_RELAY_MEDIA_BACKEND=local
FEDI3_RELAY_MESH_ENABLE=true
FEDI3_RELAY_MESH_KEY=/data/fedi3_relay_mesh_keypair.pb
```
