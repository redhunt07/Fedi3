# FEDI3

FEDI3 è un social federato con componente P2P.
Privacy-first, interoperabile e resiliente senza sacrificare la compatibilità con il fediverso.

## Cosa include

- **App Flutter** (`app/`): UI, timeline, composer, chat, media, impostazioni.
- **Core Rust locale** (`crates/fedi3_core`): ActivityPub, firme HTTP, crittografia della chat, cache, trasporto P2P e tunnel verso il relay. Esposto all'app via `dart:ffi`.
- **Relay pubblico** (`crates/fedi3_relay`): compatibilità legacy, routing, media, ricerca, spool per utenti offline.
- **Infrastruttura opzionale** (`deploy/`): PostgreSQL, Meilisearch, Redis, coturn, nodo P2P.

## Stato

Pre-1.0, in hardening verso la prima release stabile. La lista dei blocker e il piano di rimedio sono in [`docs/production_readiness_audit.md`](docs/production_readiness_audit.md) e [`docs/remediation_plan.md`](docs/remediation_plan.md).

## Quick start (relay)

```bash
cp .env.example .env
# Modifica .env: almeno FEDI3_DOMAIN, FEDI3_RELAY_ADMIN_TOKEN (>=32 caratteri),
# FEDI3_RELAY_PG_PASSWORD, TURN_PASS.
docker compose up -d --build
scripts/relay_smoke_test.sh
```

## Sviluppo e test

Prerequisiti: Rust stabile e Flutter >= 3.38 (vedi [`docs/build_linux.md`](docs/build_linux.md)).

```bash
# Rust
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked

# App
cd app
flutter pub get
flutter analyze
flutter test

# Configurazione di piattaforma (manifest Android, entitlements macOS, Info.plist iOS)
scripts/verify_platform_config.sh
```

Nessun test richiede Docker o servizi esterni: il relay viene testato su SQLite e i peer remoti sono server HTTP in-process.

## Documentazione

- [`docs/getting_started.md`](docs/getting_started.md) — primi passi
- [`docs/deploy_relay.md`](docs/deploy_relay.md) — mettere in produzione il relay
- [`docs/deploy_core.md`](docs/deploy_core.md) — core locale e modalità P2P
- [`docs/decentralization.md`](docs/decentralization.md) — identità separata dal relay, spool cifrato, mesh e moderazione
- [`docs/interop.md`](docs/interop.md) — interoperabilità verificata con Mastodon e Misskey reali
- [`docs/build_linux.md`](docs/build_linux.md) — compilare app e AppImage su Linux
- [`docs/app_guide.md`](docs/app_guide.md) — guida utente dell'app
- [`docs/client_ux_plan.md`](docs/client_ux_plan.md) — piano di rilancio del client (IA, auth, step)
- [`docs/openapi.yaml`](docs/openapi.yaml) — specifica OpenAPI 3.1 di core e relay
- [`docs/ops_backup_restore.md`](docs/ops_backup_restore.md) — backup e ripristino
- [`docs/troubleshooting.md`](docs/troubleshooting.md) — diagnosi dei problemi
- [`docs/faq.md`](docs/faq.md) — domande frequenti
- [`docs/test_plan.md`](docs/test_plan.md) — piano dei test
- [`SECURITY.md`](SECURITY.md) — come segnalare una vulnerabilità

## Sicurezza in breve

- Le chiavi private restano sul dispositivo; il relay non le vede.
- Le richieste federate in ingresso richiedono una firma HTTP valida legata all'attore dichiarato.
- Il core locale è protetto da un token interno e ascolta su loopback per default.
- La chat usa cifratura end-to-end senza forward secrecy (limite documentato).

## Licenza

AGPL-3.0. Vedi [`LICENSE`](LICENSE) e [`NOTICE`](NOTICE).
Le modifiche alle parti esposte in rete devono restare open source.
