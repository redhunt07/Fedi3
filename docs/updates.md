# Client Updates (Windows + Linux)

The desktop client checks GitHub Releases (latest).

- Windows: manual update via PowerShell script (builds locally).
- Linux: auto-install works only for AppImage builds. Source installs show a
  manual update command based on the distro (Debian/Ubuntu or Arch).

## Release assets

Publish these assets in each release:

- Windows: no asset required (manual update script builds locally)
- Linux: `Fedi3-linux-x86_64.AppImage`
- Checksums: `checksums.txt` (SHA256 per asset)

`checksums.txt` format (one per line):

```
<sha256>  Fedi3-linux-x86_64.AppImage
```

## Versioning

- Tag releases as `vX.Y.Z`
- `pubspec.yaml` must match the same `X.Y.Z`

The client compares the release tag with its local version and installs only newer versions.

## Build + publish (guida completa)

### 1) Aggiorna la versione app

`app/pubspec.yaml`

```
version: X.Y.Z+N
```

Usa lo stesso `X.Y.Z` del tag release. `N` e' il build number.

### Script completo (consigliato)

Windows (manual update, no asset):

```
powershell -ExecutionPolicy Bypass -Command "iex (iwr -useb https://raw.githubusercontent.com/redhunt07/Fedi3/main/scripts/install_windows.ps1); Install-Fedi3 -UpdateOnly"
```

La script installa anche il core come servizio in background:

- Windows: task schedulata `Fedi3 Core`
- Linux: systemd user service `fedi3-core.service`

Comandi utili:

Windows (PowerShell):

```
Get-ScheduledTask -TaskName "Fedi3 Core"
Start-ScheduledTask -TaskName "Fedi3 Core"
```

Linux:

```
systemctl --user status fedi3-core.service
systemctl --user restart fedi3-core.service
```

Linux:

```
./scripts/release_full_linux.sh patch
```

Lo script fa:
1) bump versione in `app/pubspec.yaml`
2) build core in release
3) build Flutter release
4) crea asset + `checksums.txt` in `dist/` (include tutti i file presenti in `dist/`)

Se vuoi evitare il bump:

Windows: non serve (aggiornamento manuale).

Linux:

```
NO_BUMP=1 ./scripts/release_full_linux.sh patch
```

### 2) Build Windows (local install)

Esempio (Windows PowerShell):

```
scripts\build_core.ps1 -Profile release
cd app
flutter pub get
flutter build windows --release
```

### 3) Build Linux (AppImage)

Esempio (Linux):

```
flutter clean
flutter pub get
flutter build linux --release

# Crea AppImage con il tuo toolchain (appimagetool o script esistente).
# Output finale: Fedi3-linux-x86_64.AppImage
```

### 4) Calcola SHA256

Linux:

```
sha256sum Fedi3-linux-x86_64.AppImage > checksums.txt
```

Windows: non serve checksum (aggiornamento manuale).

### 5) Crea release su GitHub

- Tag: `vX.Y.Z`
- Titolo: `vX.Y.Z`
- Carica asset:
  - `Fedi3-linux-x86_64.AppImage`
  - `checksums.txt`

### 6) Verifica

Avvia il client: dovrebbe comparire il banner “Aggiornamento disponibile”.
