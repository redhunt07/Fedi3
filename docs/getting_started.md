# Getting started

Questa guida è la più breve possibile per partire subito.

## Cos'è FEDI3

FEDI3 è un social federato con componente P2P. L’app usa il core locale (Rust) e comunica con un relay pubblico per compatibilità ActivityPub.

## Prerequisiti

- Docker + Docker Compose (per il relay).
- Flutter (per l’app).

## Quick start (relay)

```
cp .env.example .env
# Compila almeno: FEDI3_DOMAIN, FEDI3_RELAY_ADMIN_TOKEN, FEDI3_RELAY_DB_DRIVER, FEDI3_RELAY_DB_URL, media backend
docker compose up -d --build
```

## Quick start (app)

```
cd app
flutter pub get
flutter run
```

## Docs principali

- `docs/deploy_relay.md`
- `docs/deploy_core.md`
- `docs/app_guide.md`
- `docs/ops_backup_restore.md`
- `docs/troubleshooting.md`
- `docs/faq.md`
