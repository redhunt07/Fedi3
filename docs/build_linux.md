# Compilare FEDI3 su Linux

Questa pagina riguarda la build dell'app desktop Linux e del core nativo. Per il relay vedi [deploy_relay.md](deploy_relay.md).

## Requisiti di sistema

Su Arch/EndeavourOS:

```bash
sudo pacman -Syu --needed \
  base-devel git curl unzip xz zip \
  clang cmake ninja pkgconf \
  gtk3 gsettings-desktop-schemas \
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
  libmpv-dev libasound2-dev libpulse-dev curl unzip xz-utils
```

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

Lo script ufficiale fa tutto: core, bundle Flutter, AppRun, icone e AppImage.

```bash
yay -S --needed linuxdeploy linuxdeploy-plugin-gtk appimagetool
scripts/build_appimage.sh release
```

Il risultato è in `dist/Fedi3-release-x86_64.AppImage`.

Note:

- `linuxdeploy` va eseguito solo su una macchina con le dipendenze GTK installate.
- `libmpv` deve essere presente nel bundle o nel sistema: senza di esso i video non partono.
- L'AppRun generato dallo script abilita il backend media (`FEDI3_ENABLE_MEDIA_KIT_LINUX=1`).

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
