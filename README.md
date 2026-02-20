# FEDI3

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

## Installazione client Linux (Debian/Ubuntu)

Install (build + setup):

```
curl -fsSL https://raw.githubusercontent.com/redhunt07/Fedi3/main/scripts/install_deb.sh | bash
```

Update-only (senza reinstallare dipendenze):

```
curl -fsSL https://raw.githubusercontent.com/redhunt07/Fedi3/main/scripts/install_deb.sh | bash -s -- --update-only
```

## Installazione client Linux (Arch/derivate)

Dipendenze:

```
sudo pacman -S --needed git curl unzip xz zip clang cmake ninja pkgconf gtk3
```

Flutter SDK (opzionale, se non installato):

```
sudo pacman -S --needed flutter
```

Build + install:

```
sudo mkdir -p /opt/fedi3
sudo git clone https://github.com/redhunt07/Fedi3.git /opt/fedi3/src
cd /opt/fedi3/src
./scripts/build_core.sh release
cd app
flutter pub get
flutter build linux --release
sudo rm -rf /opt/fedi3/app
sudo cp -r build/linux/x64/release/bundle /opt/fedi3/app
```

## Docs

- `docs/getting_started.md`
- `docs/deploy_relay.md`
- `docs/deploy_core.md`
- `docs/app_guide.md`
- `docs/ops_backup_restore.md`
- `docs/troubleshooting.md`
- `docs/faq.md`

## Licenza

AGPLv3. Modifiche network-facing devono rimanere open.
