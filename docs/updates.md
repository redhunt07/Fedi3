# Client Updates (Windows + Linux)

The desktop client checks GitHub Releases (latest).

- Windows: auto-install supported (zip asset).
- Linux: auto-install works only for AppImage builds. Source installs show a
  manual update command based on the distro (Debian/Ubuntu or Arch).

## Release assets

Publish these assets in each release:

- Windows: `Fedi3-windows-x64.zip` (contiene **tutta** la cartella `Release`, non solo `Fedi3.exe`)
- Linux: `Fedi3-linux-x86_64.AppImage`
- Checksums: `checksums.txt` (SHA256 per asset)

`checksums.txt` format (one per line):

```
<sha256>  Fedi3-windows-x64.zip
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

Windows:

```
scripts\release_full_windows.ps1 -Bump patch
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

Windows:

```
scripts\release_full_windows.ps1 -NoBump
```

Linux:

```
NO_BUMP=1 ./scripts/release_full_linux.sh patch
```

### 2) Build Windows (Release + zip)

Esempio (Windows PowerShell):

```
flutter clean
flutter pub get
flutter build windows --release

mkdir dist
Compress-Archive -Path build\\windows\\x64\\runner\\Release\\* -DestinationPath dist\\Fedi3-windows-x64.zip -Force
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
sha256sum Fedi3-windows-x64.zip Fedi3-linux-x86_64.AppImage > checksums.txt
```

Windows (PowerShell):

```
CertUtil -hashfile Fedi3-windows-x64.zip SHA256
CertUtil -hashfile Fedi3-linux-x86_64.AppImage SHA256
```

Poi crea `checksums.txt` manualmente:

```
<sha256_zip>  Fedi3-windows-x64.zip
<sha256_appimage>  Fedi3-linux-x86_64.AppImage
```

### 5) Crea release su GitHub

- Tag: `vX.Y.Z`
- Titolo: `vX.Y.Z`
- Carica asset:
  - `Fedi3-windows-x64.zip`
  - `Fedi3-linux-x86_64.AppImage`
  - `checksums.txt`

### 6) Verifica

Avvia il client: dovrebbe comparire il banner “Aggiornamento disponibile”.
