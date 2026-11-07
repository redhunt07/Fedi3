# FEDI3

[![CI](https://github.com/redhunt07/Fedi3/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/redhunt07/Fedi3/actions/workflows/ci.yml) [![AGPL-3.0-or-later](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](LICENSE) [![Rust workspace](https://img.shields.io/badge/core-Rust-f97316.svg)](Cargo.toml) [![Flutter client](https://img.shields.io/badge/client-Flutter-38bdf8.svg)](app)

Maintainer: **[RedHunt07](https://github.com/redhunt07)** · **English** · [Italiano](README.it.md)

![FEDI3: Flutter client, Rust core and federated network](docs/assets/header.svg)

FEDI3 is a federated social platform with a peer-to-peer layer: privacy-first,
interoperable with the fediverse and resilient without giving up compatibility
with classic ActivityPub instances.

> **Status: pre-1.0.** The project is in hardening toward its first stable
> release. Open blockers and the remediation plan are tracked in
> [production readiness audit](docs/audit/production_readiness_audit.md) and
> [remediation plan](docs/audit/remediation_plan.md).

## Contents

- [What it is](#what-it-is)
- [Repository layout](#repository-layout)
- [Requirements](#requirements)
- [Quick start: relay](#quick-start-relay)
- [Client installation](#client-installation)
- [Development and testing](#development-and-testing)
- [Documentation](#documentation)
- [Security](#security)
- [Contributing](#contributing)
- [License](#license)

## What it is

FEDI3 combines a local Rust core, a Flutter client and a public relay:

| Component | Path | Responsibility |
| :--- | :--- | :--- |
| Flutter client | `app/` | UI, timelines, composer, chat, media, settings |
| Local Rust core | `crates/fedi3_core` | ActivityPub, HTTP signatures, E2E chat, cache, P2P transport, relay tunnel; loaded by the app through `dart:ffi` |
| Public relay | `crates/fedi3_relay` | Legacy federation compatibility, routing, media, search, spool for offline users |
| Shared protocol | `crates/fedi3_protocol` | Types and models shared by core and relay |
| P2P infrastructure | `crates/fedi3_p2p_infra` | Mailbox and circuit-relay services |
| Deployment assets | `deploy/` | PostgreSQL, Meilisearch, Redis, coturn, P2P node, TLS |

The relay never sees private keys: signing and decryption happen in the local
core. Federated ingress requires a valid HTTP signature bound to the declared
actor, and the local core is protected by an internal token and listens on
loopback by default.

## Repository layout

```text
app/                     Flutter client (Linux, Android, iOS, macOS, Windows)
crates/fedi3_core/       Local core (FFI, ActivityPub, P2P, chat, storage)
crates/fedi3_relay/      Relay service and database layer
crates/fedi3_protocol/   Shared protocol types
crates/fedi3_p2p_infra/  Mailbox and circuit-relay infrastructure
deploy/                  Compose files, Dockerfiles, TLS and TURN assets
docs/                    English documentation (Italian in docs/it/)
scripts/                 Build, install, backup and diagnostic scripts
```

## Requirements

- Rust stable (the workspace is built and tested with the current stable toolchain).
- Flutter 3.38 or newer for the client.
- Linux desktop dependencies for the AppImage: `clang`, `cmake`, `ninja`,
  `pkg-config`, `libgtk-3-dev`, `libsecret-1-dev`, `libjsoncpp-dev`, `libmpv-dev`,
  `libasound2-dev`, `libpulse-dev`. See [Linux build guide](docs/build_linux.md).
- Docker and Docker Compose for the relay stack.

## Quick start: relay

```bash
cp .env.example .env
# Edit .env: at least FEDI3_DOMAIN, FEDI3_RELAY_ADMIN_TOKEN (>= 32 chars),
# FEDI3_RELAY_PG_PASSWORD, TURN_PASS, TURN_REALM and TURN_EXTERNAL_IP.
# Leave FEDI3_RELAY_DB_URL empty: PostgreSQL credentials are composed automatically.
docker compose up -d --build
scripts/relay_smoke_test.sh https://relay.example.com <ADMIN_TOKEN>
```

Full configuration, TLS and monitoring: [relay deployment](docs/deploy_relay.md)
and [VPS deployment](docs/deploy_vps.md).

## Client installation

Linux (Debian/Ubuntu):

```bash
curl -fsSL https://raw.githubusercontent.com/redhunt07/Fedi3/main/scripts/install_deb.sh | bash
```

Linux (Arch and derivatives):

```bash
curl -fsSL https://raw.githubusercontent.com/redhunt07/Fedi3/main/scripts/install_arch.sh | bash
```

Both installers accept `--update-only` to update without reinstalling
dependencies.

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy Bypass -Command "iex (iwr -useb https://raw.githubusercontent.com/redhunt07/Fedi3/main/scripts/install_windows.ps1); Install-Fedi3"
```

Add `-UpdateOnly` to `Install-Fedi3` for an update-only run. Administrator
privileges are required to install dependencies through winget/BuildTools; the
core is installed as the Scheduled Task `Fedi3 Core`.

The client stores tokens in the OS keyring: Linux builds require `libsecret`
and a running Secret Service. An AppImage can be built locally with
`scripts/build_appimage.sh release`.

Android and iOS builds are available, but the mobile release pipeline and
signing are not final yet; the primary production target is relay + Linux
desktop.

## Development and testing

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

# Platform configuration (Android manifest, macOS entitlements, iOS Info.plist)
bash scripts/verify_platform_config.sh
```

The suite does not require Docker or external services: the relay is tested
against SQLite and remote peers are in-process HTTP servers. The optional
PostgreSQL quota test runs in CI with an ephemeral database.

## Documentation

English (default):

- [Getting started](docs/getting_started.md)
- [Relay deployment](docs/deploy_relay.md)
- [Relay directory](docs/relay_directory.md)
- [Local core and P2P mode](docs/deploy_core.md)
- [VPS deployment](docs/deploy_vps.md)
- [Decentralization model](docs/decentralization.md)
- [P2P network](docs/p2p_network.md)
- [Interoperability](docs/interop.md)
- [Linux build](docs/build_linux.md)
- [App guide](docs/app_guide.md)
- [Client UX plan](docs/client_ux_plan.md)
- [Backup and restore](docs/ops_backup_restore.md)
- [Troubleshooting](docs/troubleshooting.md)
- [FAQ](docs/faq.md)
- [Test plan](docs/test_plan.md)
- [Relay sync API](docs/relay_sync_api.md)
- [OpenAPI specification](docs/openapi.yaml)

Italian counterparts: [`docs/it/`](docs/it/README.md).

## Security

- Private keys stay on the device; the relay never sees them.
- Federated ingress requires a valid HTTP signature bound to the declared actor.
- The local core is protected by an internal token and binds to loopback by default.
- Chat is end-to-end encrypted without forward secrecy (documented limitation).
- Outbound federation fetches enforce SSRF protection, bounded redirects and body caps.

Please report vulnerabilities privately: see [SECURITY.md](SECURITY.md).

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Useful
contributions include reproducible bug reports, protocol/interop tests,
documentation fixes and focused, reviewable changes. This project follows the
[Contributor Covenant](CODE_OF_CONDUCT.md).

## License

AGPL-3.0-or-later. See [LICENSE](LICENSE) and [NOTICE](NOTICE). Modifications to
network-exposed parts must remain open source.
