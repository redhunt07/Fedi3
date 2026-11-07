# Aggiornamenti del client (Windows + Linux)

Il client desktop controlla le GitHub Releases (latest).

- Windows: aggiornamento manuale tramite script PowerShell (compila in locale).
- Linux: l'installazione automatica funziona solo per le build AppImage. Le installazioni da sorgente
  mostrano un comando di aggiornamento manuale basato sulla distribuzione (Debian/Ubuntu o Arch).

## Asset della release

Pubblica questi asset in ogni release:

- Windows: nessun asset richiesto (lo script di aggiornamento manuale compila in locale)
- Linux: `Fedi3-linux-x86_64.AppImage`
- Checksum: `checksums.txt` (SHA256 per asset)

Formato di `checksums.txt` (una riga per file):

```
<sha256>  Fedi3-linux-x86_64.AppImage
```

## Versioning

- Etichetta le release come `vX.Y.Z`
- `pubspec.yaml` deve corrispondere allo stesso `X.Y.Z`

Il client confronta il tag della release con la propria versione locale e installa solo versioni più recenti.

## Build + publish (guida completa)

### 1) Aggiorna la versione app

`app/pubspec.yaml`

```
version: X.Y.Z+N
```

Usa lo stesso `X.Y.Z` del tag release. `N` è il build number.

### Script completo (consigliato)

Windows (aggiornamento manuale, nessun asset):

```
powershell -ExecutionPolicy Bypass -Command "iex (iwr -useb https://raw.githubusercontent.com/redhunt07/Fedi3/main/scripts/install_windows.ps1); Install-Fedi3 -UpdateOnly"
```

Lo script installa anche il core come servizio in background:

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
1) bump della versione in `app/pubspec.yaml`
2) build del core in release
3) build della release Flutter
4) crea asset + `checksums.txt` in `dist/` (include tutti i file presenti in `dist/`)

Se vuoi evitare il bump:

Windows: non serve (aggiornamento manuale).

Linux:

```
NO_BUMP=1 ./scripts/release_full_linux.sh patch
```

### 2) Build Windows (installazione locale)

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

# Crea l'AppImage con il tuo toolchain (appimagetool o script esistente).
# Output finale: Fedi3-linux-x86_64.AppImage
```

### 4) Calcola SHA256

Linux:

```
sha256sum Fedi3-linux-x86_64.AppImage > checksums.txt
```

Windows: nessun checksum necessario (aggiornamento manuale).

### 5) Crea la release su GitHub

- Tag: `vX.Y.Z`
- Titolo: `vX.Y.Z`
- Carica gli asset:
  - `Fedi3-linux-x86_64.AppImage`
  - `checksums.txt`

### 6) Verifica

Avvia il client: dovrebbe comparire il banner "Aggiornamento disponibile".
