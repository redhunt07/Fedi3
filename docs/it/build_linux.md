# Compilare FEDI3 su Linux

Questa pagina riguarda la build dell'app desktop Linux e del core nativo. Per il relay vedi [deploy_relay.md](deploy_relay.md); per l'uso del client vedi [app_guide.md](app_guide.md).

## Requisiti di sistema

Su Arch/EndeavourOS:

```bash
sudo pacman -Syu --needed \
  base-devel git curl unzip xz zip \
  clang cmake ninja pkgconf \
  gtk3 gsettings-desktop-schemas libsecret jsoncpp \
  webkit2gtk-4.1 gstreamer gst-plugins-base gst-plugins-good \
  libxkbcommon libxss libxrandr libxcursor libxinerama libxi \
  mesa libglvnd \
  pango cairo gdk-pixbuf2 glib2 \
  alsa-lib pulseaudio \
  mpv ffmpeg
```

Su Debian/Ubuntu:

```bash
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  clang cmake ninja-build pkg-config libgtk-3-dev liblzma-dev \
  libwebkit2gtk-4.1-dev libmpv-dev libasound2-dev libpulse-dev \
  libsecret-1-dev libjsoncpp-dev \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  curl unzip xz-utils
```

Il salvataggio delle credenziali richiede anche un Secret Service nella sessione
utente (GNOME Keyring o KWallet), sbloccato. Se non è disponibile, l’app mostra
la schermata di recupero con **Riprova** e conserva il profilo esistente.

## Toolchain

Serve Flutter stabile e Rust stabile:

```bash
# Flutter
git clone https://github.com/flutter/flutter.git -b stable "$HOME/flutter"
export PATH="$HOME/flutter/bin:$PATH"
flutter config --enable-linux-desktop
flutter doctor

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
export PATH="$HOME/.cargo/bin:$PATH"
```

Non installare Flutter dai repository della distribuzione se la versione è vecchia: l'app richiede Dart >= 3.10 e Flutter >= 3.38.

## Compilare il core

Il core è una libreria condivisa caricata dall'app via `dart:ffi`:

```bash
scripts/build_core.sh release
```

Il file prodotto viene copiato accanto all'eseguibile dell'app.

## Compilare l'app

```bash
cd app
flutter pub get
flutter build linux --release
```

Il bundle finito è in `app/build/linux/x64/release/bundle/`.

## Creare l'AppImage

Lo script compila il core e il bundle Flutter, crea l'AppRun, include `libmpv`
e chiama `appimagetool`. Se `appimagetool` manca, lo scarica in `.cache/`;
`linuxdeploy` non è necessario.

```bash
scripts/build_appimage.sh release
```

Un'esecuzione diretta dello script produce `dist/Fedi3-release-x86_64.AppImage`.
La pipeline di release (`scripts/release_full_linux.sh` o
`scripts/release_build_linux.sh`) scrive invece
`dist/Fedi3-linux-x86_64.AppImage` e `checksums.txt`.

Note:

- `libmpv` deve essere presente sulla macchina di build: lo script lo include nell'AppImage. Senza di esso i video non partono.
- L'app inizializza `media_kit` all'avvio, quindi il `libmpv` incluso viene usato automaticamente.
- L'AppRun generato abilita il rendering software per impostazione predefinita (`FEDI3_SOFTWARE_RENDERING=1`). Imposta `FEDI3_SOFTWARE_RENDERING=0` per usare la GPU.

## Verifica del bundle

```bash
ldd app/build/linux/x64/release/bundle/fedi3 | grep "not found"
```

Se compare una libreria "not found", installala o copiala nel bundle prima di creare l'AppImage.

## Pulizia completa

Se la build fallisce per file di piattaforma generati male:

```bash
cd app
rm -rf .dart_tool build linux/flutter/ephemeral linux/build
find linux -name CMakeCache.txt -delete
find linux -name CMakeFiles -type d -prune -exec rm -rf {} +
flutter config --enable-linux-desktop
flutter create --platforms=linux .
flutter pub get
flutter build linux --release
```

Se i plugin non hanno la cartella `linux/`:

```bash
flutter pub cache repair
flutter pub get
```

## Test

```bash
cd app
flutter analyze
flutter test
```

## Windows e macOS

Gli script equivalenti sono `scripts/build_core.ps1` e `scripts/bootstrap.ps1` per Windows. Su macOS la build passa da Xcode e richiede gli entitlement di rete presenti in `app/macos/Runner/*.entitlements`.
