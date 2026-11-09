# Building FEDI3 on Linux

This page covers building the Linux desktop app and the native core. For the relay see [deploy_relay.md](deploy_relay.md); for client usage see [app_guide.md](app_guide.md).

## System requirements

On Arch/EndeavourOS:

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

On Debian/Ubuntu:

```bash
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  clang cmake ninja-build pkg-config libgtk-3-dev liblzma-dev \
  libwebkit2gtk-4.1-dev libmpv-dev libasound2-dev libpulse-dev \
  libsecret-1-dev libjsoncpp-dev \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  curl unzip xz-utils
```

Saving credentials also requires a Secret Service in the user session
(GNOME Keyring or KWallet), unlocked. If it is unavailable, the app shows
the recovery screen with **Retry** and keeps the existing profile.

## Toolchain

You need stable Flutter and stable Rust:

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

Do not install Flutter from distribution repositories if the version is old: the app requires Dart >= 3.10 and Flutter >= 3.38.

## Building the core

The core is a shared library loaded by the app via `dart:ffi`:

```bash
scripts/build_core.sh release
```

The produced file is copied next to the app executable.

## Building the app

```bash
cd app
flutter pub get
flutter build linux --release
```

The finished bundle is in `app/build/linux/x64/release/bundle/`.

## Creating the AppImage

The script builds the core and the Flutter bundle, creates the AppRun, bundles
`libmpv` and calls `appimagetool`. It downloads `appimagetool` into `.cache/`
when it is missing; `linuxdeploy` is not required.

```bash
scripts/build_appimage.sh release
```

A direct script run produces `dist/Fedi3-release-x86_64.AppImage`. The release
pipeline (`scripts/release_full_linux.sh` or `scripts/release_build_linux.sh`)
writes `dist/Fedi3-linux-x86_64.AppImage` and `checksums.txt` instead.

Notes:

- `libmpv` must be present on the build host: the script bundles it into the AppImage. Without it videos do not start.
- The app initializes `media_kit` at startup, so the bundled `libmpv` is used automatically.
- The generated AppRun enables software rendering by default (`FEDI3_SOFTWARE_RENDERING=1`). Set `FEDI3_SOFTWARE_RENDERING=0` to use the GPU.

## Bundle verification

```bash
ldd app/build/linux/x64/release/bundle/fedi3 | grep "not found"
```

If a "not found" library appears, install it or copy it into the bundle before creating the AppImage.

## Full cleanup

If the build fails due to badly generated platform files:

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

If the plugins do not have the `linux/` directory:

```bash
flutter pub cache repair
flutter pub get
```

## Tests

```bash
cd app
flutter analyze
flutter test
```

## Windows and macOS

The equivalent scripts are `scripts/build_core.ps1` and `scripts/bootstrap.ps1` for Windows. On macOS the build goes through Xcode and requires the network entitlements present in `app/macos/Runner/*.entitlements`.
