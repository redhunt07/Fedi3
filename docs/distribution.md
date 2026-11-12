# Distribution packages

Fedi3 is published on GitHub Releases as an AppImage, a Windows ZIP and two
Android APKs. This page covers the store-style channels: F-Droid for Android,
Flatpak for Linux desktops and the AUR package `fedi3-bin`.

The packaging files live in this repository:

- `fastlane/metadata/android/`: F-Droid store metadata and screenshots.
- `packaging/fdroiddata/org.fedi3.client.yml`: F-Droid build recipe.
- `packaging/flatpak/`: Flatpak manifest, mpv module, desktop file and AppStream
  metadata.
- `packaging/aur/`: `fedi3-bin` PKGBUILD and `.SRCINFO`.

## Version and version-code mapping

`app/pubspec.yaml` carries the Flutter build number `4015`. Flutter's
`--split-per-abi` adds the ABI offset, so the release APKs use:

| ABI | Version code |
| --- | --- |
| arm64-v8a | 6015 (4015 + 2000) |
| x86_64 | 8015 (4015 + 4000) |

The F-Droid recipe encodes the same mapping through `VercodeOperation` and
`UpdateCheckData`; `CurrentVersionCode` is the highest code, 8015. Bump the
pubspec version and build number, then update the recipe's `CurrentVersion`,
`CurrentVersionCode`, `commit` and both `Builds` entries for each release.

## Android: F-Droid

### Build locally

The recipe uses Flutter 3.50.0, Rust 1.99.1, NDK r28c and JDK 21. The helper
script builds one ABI at a time:

```bash
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk
export FEDI3_ANDROID_NDK=$HOME/Android/Sdk/ndk/28.2.13676358
bash scripts/build_android_fdroid.sh arm64-v8a
bash scripts/build_android_fdroid.sh x86_64
```

It compiles `fedi3_core` for the NDK target, copies the resulting
`libfedi3_core.so` into `app/android/app/src/main/jniLibs/<abi>/` and runs
`flutter build apk --release --split-per-abi`. The output is
`app/build/app/outputs/flutter-apk/app-<abi>-release.apk`.

### Submit to F-Droid

1. Fork <https://gitlab.com/fdroid/fdroiddata>.
2. Copy `packaging/fdroiddata/org.fedi3.client.yml` to `metadata/org.fedi3.client.yml`.
3. Push the branch and open a merge request against `master`.

The recipe builds both ABIs from source and F-Droid signs the result with its
own key. The APKs attached to GitHub Releases are signed with the local debug
key (no `app/android/key.properties` in CI), so the two signatures differ:
users moving from a sideloaded release APK to the F-Droid build must uninstall
the old app once. Keep the same signing key for future GitHub releases to avoid
this for sideload updates.

Notes for reviewers and future releases:

- `mobile_scanner` uses the prebuilt Google MLKit barcode AAR from Maven; it is
  not rebuilt in the recipe. The rest of the native code (the Rust core and the
  Flutter plugins) is built from source or comes from Gradle dependencies.
- `flutter_webrtc` ships prebuilt WebRTC libraries inside its Android AAR; the
  recipe does not copy them into the repository.
- The recipe pins the Flutter SDK through the `flutter@stable` srclib and
  checks out tag `3.50.0`; `.flutter` and `.pub-cache` are excluded from the
  scanner, while `.flutter/bin/cache` is ignored as binary cache data.

## Linux: Flatpak

The manifest targets the GNOME 50 runtime and SDK, which provide GTK3,
libsecret, GStreamer and `webkit2gtk-4.1` (needed by the embedded web view).
`libmpv` is not part of the runtime, so the manifest builds mpv 0.41.0 from
source. Flutter 3.50.0 is downloaded from the official stable archive with a
pinned SHA256; the Rust core is compiled with the `rust-stable` SDK extension.

### Build and test locally

```bash
flatpak-builder --user --force-clean --repo=/tmp/fedi3-flatpak-repo \
  /tmp/fedi3-flatpak-build packaging/flatpak/org.fedi3.client.yml
flatpak build-export /tmp/fedi3-flatpak-repo /tmp/fedi3-flatpak-build
flatpak install --user /tmp/fedi3-flatpak-repo org.fedi3.client
flatpak run org.fedi3.client
```

Requirements: `org.gnome.Platform//50`, `org.gnome.Sdk//50` and
`org.freedesktop.Sdk.Extension.rust-stable//25.08` installed from Flathub.

### Flathub

Flathub hosts each app in its own repository under the `flathub` GitHub
organization. Submit the app first through the new-app flow at
<https://github.com/flathub/flathub/issues/new?template=new-app.yml>; the
reviewers create `flathub/org.fedi3.client`. Then open a pull request adding:

- `org.fedi3.client.yml`
- `mpv.json`
- `org.fedi3.client.desktop`
- `org.fedi3.client.metainfo.xml`

The app ID `org.fedi3.client` matches the GTK application ID in
`app/linux/CMakeLists.txt`. The manifest installs the icon and the AppStream
metadata, and the app has no proprietary runtime dependency.

## Linux: AUR

`packaging/aur/PKGBUILD` builds the binary package `fedi3-bin` from the published
AppImage. It extracts the AppImage under `/opt/fedi3`, installs a wrapper in
`/usr/bin/fedi3` and adds the desktop file and icon. `fedi3-bin` provides
`fedi3` and conflicts with a source-built `fedi3` package.

Build locally:

```bash
cd packaging/aur
makepkg -f
sudo pacman -U fedi3-bin-0.11.11-1-x86_64.pkg.tar.zst
```

Publish to the AUR:

```bash
git clone ssh://aur@aur.archlinux.org/fedi3-bin.git
cp packaging/aur/PKGBUILD packaging/aur/.SRCINFO fedi3-bin/
cd fedi3-bin
git add PKGBUILD .SRCINFO
git commit -m "fedi3-bin 0.11.11-1"
git push
```

Users then install it with `yay -S fedi3-bin` or `paru -S fedi3-bin`.

## Release checklist

1. Bump `app/pubspec.yaml` (`version: X.Y.Z+NNNN`).
2. Build and publish the GitHub release assets with `scripts/release_build_linux.sh`
   and `scripts/build_android.sh release`.
3. Update `packaging/fdroiddata/org.fedi3.client.yml` (`commit`,
   `versionName`, `versionCode`, `CurrentVersion`, `CurrentVersionCode`).
4. Update `packaging/aur/PKGBUILD` (`pkgver`, AppImage `sha256sums`) and regenerate
   `.SRCINFO` with `makepkg --printsrcinfo > .SRCINFO`.
5. Update the Flatpak manifest source commit and run the local Flatpak build.
6. Update the fastlane changelogs under
   `fastlane/metadata/android/<locale>/changelogs/<versionCode>.txt`.
