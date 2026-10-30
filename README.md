# FEDI3

[![CI](https://github.com/redhunt07/Fedi3/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/redhunt07/Fedi3/actions/workflows/ci.yml) [![AGPL-3.0-or-later](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](LICENSE) [![Rust workspace](https://img.shields.io/badge/core-Rust-f97316.svg)](Cargo.toml) [![Flutter client](https://img.shields.io/badge/client-Flutter-38bdf8.svg)](app)

**English** · [Italiano](README.it.md) · Maintainer: [RedHunt07](https://github.com/redhunt07)

![FEDI3: a Flutter client, a Rust core and a federated network](docs/assets/header.svg)

FEDI3 is federated social software combining a **Flutter client**, a **local Rust core** and a **public relay**, with ActivityPub compatibility and a peer-to-peer layer.

[Get started](docs/getting_started.md) · [Deployment](docs/deploy_relay.md) · [Troubleshooting](docs/troubleshooting.md) · [Contribute](CONTRIBUTING.md)

## Choose your starting point

| Goal | Start here |
| --- | --- |
| Run a relay | [Relay deployment](docs/deploy_relay.md) |
| Run the desktop client | [App guide](docs/app_guide.md) and the installers below |
| Understand the local runtime | [Core deployment](docs/deploy_core.md) |
| Operate an existing installation | [Backup and restore](docs/ops_backup_restore.md) |
| Explore project rules | [Governance](GOVERNANCE.md), [ethics](ETHICS.md), [protocol compatibility](PROTOCOL_COMPATIBILITY.md) |

## Architecture

| Component | Role | Source |
| --- | --- | --- |
| Flutter app | Client interface and settings | [app/](app/) |
| Rust core | Local federation, P2P and cache runtime | [crates/fedi3_core/](crates/fedi3_core/) |
| Public relay | Federation compatibility and routing | [crates/fedi3_relay/](crates/fedi3_relay/) |
| Shared protocol | Protocol types and models | [crates/fedi3_protocol/](crates/fedi3_protocol/) |
| P2P infrastructure | Peer and mailbox services | [crates/fedi3_p2p_infra/](crates/fedi3_p2p_infra/) |

## Current platform scope

The primary deployment target is **relay + Linux desktop**. Windows installation scripts are included. Android is not release-ready until its dedicated signing and mobile release pipeline are defined. Platform folders in the source tree do not by themselves establish release support.

## Quick start: relay

Clone the repository and prepare the environment before starting the stack:

```sh
git clone https://github.com/redhunt07/Fedi3.git
cd Fedi3
cp .env.example .env
# Edit .env for your domain, database, media backend and secrets.
docker compose up -d --build
```

The example environment is a development template. Follow the [relay deployment guide](docs/deploy_relay.md) for public deployment, TLS and configuration.

## Desktop installers

The installers build from source and can install dependencies, services and desktop integration. Review the script for your platform before running it.

| Platform | Installer | Installed core |
| --- | --- | --- |
| Debian / Ubuntu | [scripts/install_deb.sh](scripts/install_deb.sh) | User systemd service |
| Arch and derivatives | [scripts/install_arch.sh](scripts/install_arch.sh) | User systemd service |
| Windows | [scripts/install_windows.ps1](scripts/install_windows.ps1) | Scheduled Task |

For Debian / Ubuntu, after cloning:

```sh
bash scripts/install_deb.sh
# Update an existing installation:
bash scripts/install_deb.sh --update-only
```

The [Italian guide](README.it.md) includes the existing commands for each platform. Linux installation uses administrative privileges for dependency installation and writes under `/opt` and `/usr/share`.

## Development

See [AGENTS.md](AGENTS.md) for the repository map and [CONTRIBUTING.md](CONTRIBUTING.md) for contribution rules.

```sh
cargo check -p fedi3_core
cargo check -p fedi3_relay
cargo test --workspace

cd app
flutter pub get
flutter analyze
flutter test
```

The [CI workflow](.github/workflows/ci.yml) includes Rust, Flutter, release-hardening and relay smoke checks. Its badge reports the current result.

## Documentation

- [Getting started](docs/getting_started.md)
- [Relay deployment](docs/deploy_relay.md)
- [Core runtime](docs/deploy_core.md)
- [App guide](docs/app_guide.md)
- [Backup and restore](docs/ops_backup_restore.md)
- [Troubleshooting](docs/troubleshooting.md)
- [Frequently asked questions](docs/faq.md)

## Help improve FEDI3

Reproducible federation tests, accessibility work, translations and focused documentation fixes are welcome. Read the [contribution guide](CONTRIBUTING.md); preserve protocol compatibility and existing attribution.

[Report a bug](https://github.com/redhunt07/Fedi3/issues/new?template=bug_report.yml) · [Suggest an improvement](https://github.com/redhunt07/Fedi3/issues/new?template=feature_request.yml) · [Contribute](CONTRIBUTING.md)

If the project is useful to you, a star helps others discover it. Use **Watch → Custom → Releases** to follow releases.

## License

Copyright © 2026 RedHunt07. **AGPL-3.0-or-later**, as specified in [LICENSE](LICENSE). See the [fork policy](FORK_POLICY.md) and [trademark and name policy](TRADEMARK_AND_NAME.md) for project-specific guidance.
