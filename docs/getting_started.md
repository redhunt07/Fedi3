# Getting started

This guide is as short as possible so you can start right away.

## What FEDI3 is

FEDI3 is a federated social network with a P2P component. The app uses a local core (Rust) and talks to a public relay for ActivityPub compatibility. The relay is optional: without it you still have P2P and chat, but no federation with traditional instances.

## Prerequisites

- Docker + Docker Compose (for the relay).
- Flutter >= 3.38 and stable Rust (to build the app and the core).

## Quick start (relay)

```bash
cp .env.example .env
```

Fill in at least: `FEDI3_DOMAIN`, `FEDI3_RELAY_ADMIN_TOKEN` (>= 32 characters), `FEDI3_RELAY_PG_PASSWORD`, `TURN_REALM`, `TURN_PASS`, `FEDI3_RELAY_DB_DRIVER` and the media backend. Leave `FEDI3_RELAY_DB_URL` empty for the Compose service: the relay derives the PostgreSQL URL from `FEDI3_RELAY_PG_USER`, `FEDI3_RELAY_PG_PASSWORD` and `FEDI3_RELAY_PG_DB`.

```bash
docker compose up -d --build
scripts/relay_smoke_test.sh https://<your-domain> <ADMIN_TOKEN>
```

## Quick start (app)

```bash
cd app
flutter pub get
flutter run
```

To build the native core and the AppImage see [build_linux.md](build_linux.md).

## Environment verification

```bash
cargo test --workspace --locked
cd app && flutter analyze && flutter test
scripts/verify_platform_config.sh
```

## Main docs

- [deploy_relay.md](deploy_relay.md) — putting the relay into production
- [deploy_core.md](deploy_core.md) — local core and P2P mode
- [build_linux.md](build_linux.md) — building the app and AppImage
- [app_guide.md](app_guide.md) — user guide
- [ops_backup_restore.md](ops_backup_restore.md) — backup and restore
- [troubleshooting.md](troubleshooting.md) — diagnostics
- [faq.md](faq.md) — frequently asked questions
- [test_plan.md](test_plan.md) — test plan
- [SECURITY.md](../SECURITY.md) — vulnerability reporting
