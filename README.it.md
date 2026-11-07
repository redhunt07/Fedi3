# FEDI3

[![CI](https://github.com/redhunt07/Fedi3/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/redhunt07/Fedi3/actions/workflows/ci.yml) [![AGPL-3.0-or-later](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](LICENSE) [![Rust workspace](https://img.shields.io/badge/core-Rust-f97316.svg)](Cargo.toml) [![Flutter client](https://img.shields.io/badge/client-Flutter-38bdf8.svg)](app)

Autore: **[RedHunt07](https://github.com/redhunt07)** · [English](README.md) · **Italiano**

![FEDI3: client Flutter, core Rust e rete federata](docs/assets/header.svg)

FEDI3 è un social federato con componente P2P: privacy-first, interoperabile con
il fediverso e resiliente senza rinunciare alla compatibilità con le istanze
ActivityPub classiche.

> **Stato: pre-1.0.** Il progetto è in hardening verso la prima release stabile.
> I blocker aperti e il piano di rimedio sono in
> [audit di production readiness](docs/it/audit/production_readiness_audit.md) e
> [piano di rimedio](docs/it/audit/remediation_plan.md).

## Indice

- [Cos'è](#cosè)
- [Struttura del repository](#struttura-del-repository)
- [Requisiti](#requisiti)
- [Quick start: relay](#quick-start-relay)
- [Installazione del client](#installazione-del-client)
- [Sviluppo e test](#sviluppo-e-test)
- [Documentazione](#documentazione)
- [Sicurezza](#sicurezza)
- [Contribuire](#contribuire)
- [Licenza](#licenza)

## Cos'è

FEDI3 combina un core Rust locale, un client Flutter e un relay pubblico:

| Componente | Percorso | Responsabilità |
| :--- | :--- | :--- |
| Client Flutter | `app/` | UI, timeline, composer, chat, media, impostazioni |
| Core Rust locale | `crates/fedi3_core` | ActivityPub, firme HTTP, chat E2E, cache, trasporto P2P, tunnel verso il relay; caricato dall'app via `dart:ffi` |
| Relay pubblico | `crates/fedi3_relay` | Compatibilità legacy, routing, media, ricerca, spool per utenti offline |
| Protocollo condiviso | `crates/fedi3_protocol` | Tipi e modelli condivisi tra core e relay |
| Infrastruttura P2P | `crates/fedi3_p2p_infra` | Servizi mailbox e circuit-relay |
| Asset di deploy | `deploy/` | PostgreSQL, Meilisearch, Redis, coturn, nodo P2P, TLS |

Il relay non vede mai le chiavi private: firma e decifratura avvengono nel core
locale. Il traffico federato in ingresso richiede una firma HTTP valida legata
all'attore dichiarato; il core locale è protetto da un token interno e ascolta
su loopback per default.

## Struttura del repository

```text
app/                     Client Flutter (Linux, Android, iOS, macOS, Windows)
crates/fedi3_core/       Core locale (FFI, ActivityPub, P2P, chat, storage)
crates/fedi3_relay/      Relay e layer database
crates/fedi3_protocol/   Tipi di protocollo condivisi
crates/fedi3_p2p_infra/  Infrastruttura mailbox e circuit-relay
deploy/                  Compose, Dockerfile, asset TLS e TURN
docs/                    Documentazione inglese (italiano in docs/it/)
scripts/                 Script di build, installazione, backup e diagnostica
```

## Requisiti

- Rust stabile (il workspace è compilato e testato con il toolchain stabile corrente).
- Flutter 3.38 o successivo per il client.
- Dipendenze desktop Linux per l'AppImage: `clang`, `cmake`, `ninja`,
  `pkg-config`, `libgtk-3-dev`, `libsecret-1-dev`, `libjsoncpp-dev`, `libmpv-dev`,
  `libasound2-dev`, `libpulse-dev`. Vedi [guida di build Linux](docs/it/build_linux.md).
- Docker e Docker Compose per lo stack del relay.

## Quick start: relay

Installazione automatica del server per Debian/Ubuntu e derivate Arch (relay +
P2P, solo relay o solo P2P):

```bash
sudo git clone https://github.com/redhunt07/Fedi3.git /opt/fedi3
cd /opt/fedi3
sudo bash scripts/install_server.sh
```

Vedi [installazione del server](docs/it/deploy_server.md) per la guida passo
passo. La configurazione manuale segue.

```bash
cp .env.example .env
# Modifica .env: almeno FEDI3_DOMAIN, FEDI3_RELAY_ADMIN_TOKEN (>= 32 caratteri)
# e FEDI3_RELAY_PG_PASSWORD. Il template avvia relay,p2p; aggiungi turn a
# COMPOSE_PROFILES (e imposta TURN_*) per le chiamate, search per Meilisearch.
# Lascia FEDI3_RELAY_DB_URL vuoto: le credenziali PostgreSQL sono composte automaticamente.
docker compose up -d --build
scripts/relay_smoke_test.sh https://relay.example.com <ADMIN_TOKEN>
```

Configurazione completa, TLS e monitoraggio: [deploy del relay](docs/it/deploy_relay.md)
e [deploy su VPS](docs/it/deploy_vps.md).

## Installazione del client

Linux (Debian/Ubuntu):

```bash
curl -fsSL https://raw.githubusercontent.com/redhunt07/Fedi3/main/scripts/install_deb.sh | bash
```

Linux (Arch e derivate):

```bash
curl -fsSL https://raw.githubusercontent.com/redhunt07/Fedi3/main/scripts/install_arch.sh | bash
```

Entrambi gli installer accettano `--update-only` per aggiornare senza
reinstallare le dipendenze.

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy Bypass -Command "iex (iwr -useb https://raw.githubusercontent.com/redhunt07/Fedi3/main/scripts/install_windows.ps1); Install-Fedi3"
```

Aggiungi `-UpdateOnly` a `Install-Fedi3` per il solo aggiornamento. Servono
privilegi amministratore per installare le dipendenze via winget/BuildTools; il
core viene installato come Scheduled Task `Fedi3 Core`.

Il client salva i token nel portachiavi di sistema: le build Linux richiedono
`libsecret` e un Secret Service attivo. Un'AppImage può essere compilata in
locale con `scripts/build_appimage.sh release`.

Le build Android e iOS sono disponibili, ma la pipeline di release e la firma
mobile non sono ancora definitive; il target di produzione primario è
relay + desktop Linux.

## Sviluppo e test

```bash
# Rust
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

# Flutter
cd app
flutter pub get
flutter analyze --fatal-infos
dart format --output=none --set-exit-if-changed lib test
flutter test

# Configurazione di piattaforma (manifest Android, entitlements macOS, Info.plist iOS)
bash scripts/verify_platform_config.sh
```

La suite non richiede Docker o servizi esterni: il relay viene testato su SQLite
e i peer remoti sono server HTTP in-process. Il test opzionale della quota
PostgreSQL gira in CI con un database effimero.

## Documentazione

Italiano:

- [Primi passi](docs/it/getting_started.md)
- [Installazione del server, Debian e Arch](docs/it/deploy_server.md)
- [Deploy del relay](docs/it/deploy_relay.md)
- [Directory relay](docs/it/relay_directory.md)
- [Core locale e modalità P2P](docs/it/deploy_core.md)
- [Deploy su VPS](docs/it/deploy_vps.md)
- [Modello di decentralizzazione](docs/it/decentralization.md)
- [Rete P2P](docs/it/p2p_network.md)
- [Interoperabilità](docs/it/interop.md)
- [Build Linux](docs/it/build_linux.md)
- [Guida dell'app](docs/it/app_guide.md)
- [Piano UX del client](docs/it/client_ux_plan.md)
- [Backup e ripristino](docs/it/ops_backup_restore.md)
- [Diagnostica](docs/it/troubleshooting.md)
- [FAQ](docs/it/faq.md)
- [Piano dei test](docs/it/test_plan.md)
- [API di sync del relay](docs/it/relay_sync_api.md)
- [Specifica OpenAPI](docs/openapi.yaml)

Documentazione inglese (default): [`docs/`](docs/README.md).

## Sicurezza

- Le chiavi private restano sul dispositivo; il relay non le vede.
- Il traffico federato in ingresso richiede una firma HTTP valida legata all'attore dichiarato.
- Il core locale è protetto da un token interno e ascolta su loopback per default.
- La chat è cifrata end-to-end senza forward secrecy (limite documentato).
- I fetch federati in uscita applicano protezione SSRF, redirect limitati e cap sul corpo.

Segnala le vulnerabilità in privato: vedi [SECURITY.md](SECURITY.md).

## Contribuire

Leggi [CONTRIBUTING.md](CONTRIBUTING.md) prima di aprire una pull request. Sono
utili segnalazioni riproducibili, test di protocollo/interoperabilità, correzioni
alla documentazione e modifiche mirate e revisionabili. Il progetto segue il
[Contributor Covenant](CODE_OF_CONDUCT.md).

## Licenza

AGPL-3.0-or-later. Vedi [LICENSE](LICENSE) e [NOTICE](NOTICE). Le modifiche alle
parti esposte in rete devono restare open source.
