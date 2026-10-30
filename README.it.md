# FEDI3

[![CI](https://github.com/redhunt07/Fedi3/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/redhunt07/Fedi3/actions/workflows/ci.yml) [![AGPL-3.0-or-later](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](LICENSE) [![Rust workspace](https://img.shields.io/badge/core-Rust-f97316.svg)](Cargo.toml) [![Flutter client](https://img.shields.io/badge/client-Flutter-38bdf8.svg)](app)

[English](README.md) · **Italiano** · Autore: [RedHunt07](https://github.com/redhunt07)

![FEDI3: client Flutter, core Rust e rete federata](docs/assets/header.svg)

FEDI3 è un social federato con componente P2P.  
Privacy-first, interoperabile e resiliente senza sacrificare la compatibilità.

## Cosa include

- App Flutter (UI/UX + impostazioni)
- Core Rust locale (AP + P2P + cache)
- Relay pubblico (compatibilità legacy + routing)

## Quick start (relay)

```
cp .env.example .env
docker compose up -d --build
```

Note:
- `.env.example` is a development-safe template; replace all secret values before public deployment.
- The primary production target in this repo is `relay + Linux desktop`.
- Android is not considered release-ready until dedicated signing and mobile release pipeline are defined.

## Installazione client Linux (Debian/Ubuntu)

Install (build + setup):

```
curl -fsSL https://raw.githubusercontent.com/redhunt07/Fedi3/main/scripts/install_deb.sh | bash
```

Update-only (senza reinstallare dipendenze):

```
curl -fsSL https://raw.githubusercontent.com/redhunt07/Fedi3/main/scripts/install_deb.sh | bash -s -- --update-only
```

Note:
- Richiede privilegi amministratore (usa `sudo`) per installare in `/opt` e scrivere in `/usr/share`.
- Il core viene installato come servizio user systemd (`fedi3-core.service`).

## Installazione client Linux (Arch/derivate)

Install (build + setup):

```
curl -fsSL https://raw.githubusercontent.com/redhunt07/Fedi3/main/scripts/install_arch.sh | bash
```

Update-only (senza reinstallare dipendenze):

```
curl -fsSL https://raw.githubusercontent.com/redhunt07/Fedi3/main/scripts/install_arch.sh | bash -s -- --update-only
```

Dipendenze principali (pacman):

```
base-devel git curl unzip xz zip python clang cmake ninja pkgconf gtk3 webkit2gtk
gstreamer gst-plugins-base gst-plugins-good libsecret libnotify mpv
```

Note:
- Richiede privilegi amministratore (usa `sudo`) per installare in `/opt` e scrivere in `/usr/share`.
- Il core viene installato come servizio user systemd (`fedi3-core.service`).

## Installazione client Windows

Install (build + setup):

```
powershell -ExecutionPolicy Bypass -Command "iex (iwr -useb https://raw.githubusercontent.com/redhunt07/Fedi3/main/scripts/install_windows.ps1); Install-Fedi3"
```

Update-only:

```
powershell -ExecutionPolicy Bypass -Command "iex (iwr -useb https://raw.githubusercontent.com/redhunt07/Fedi3/main/scripts/install_windows.ps1); Install-Fedi3 -UpdateOnly"
```

Note:
- Richiede privilegi amministratore per installare le dipendenze via winget/BuildTools.
- Il core viene installato come Scheduled Task (`Fedi3 Core`).

## Docs

- [docs/getting_started.md](docs/getting_started.md)
- [docs/deploy_relay.md](docs/deploy_relay.md)
- [docs/deploy_core.md](docs/deploy_core.md)
- [docs/app_guide.md](docs/app_guide.md)
- [docs/ops_backup_restore.md](docs/ops_backup_restore.md)
- [docs/troubleshooting.md](docs/troubleshooting.md)
- [docs/faq.md](docs/faq.md)

## Licenza

AGPLv3. Modifiche network-facing devono rimanere open.

## Contribuire

Leggi [CONTRIBUTING.md](CONTRIBUTING.md) e [GOVERNANCE.md](GOVERNANCE.md). Sono utili test di federazione riproducibili, miglioramenti di accessibilità, traduzioni e documentazione. Per segnalare problemi includi piattaforma, versione e passaggi riproducibili, rimuovendo token e dati personali.
