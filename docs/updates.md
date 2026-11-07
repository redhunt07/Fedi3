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

## Build + publish (complete guide)

### 1) Update the app version

`app/pubspec.yaml`

```
version: X.Y.Z+N
```

Use the same `X.Y.Z` as the release tag. `N` is the build number.

### Complete script (recommended)

Windows (manual update, no asset):

```
powershell -ExecutionPolicy Bypass -Command "iex (iwr -useb https://raw.githubusercontent.com/redhunt07/Fedi3/main/scripts/install_windows.ps1); Install-Fedi3 -UpdateOnly"
```

The script also installs the core as a background service:

- Windows: scheduled task `Fedi3 Core`
- Linux: systemd user service `fedi3-core.service`

Useful commands:

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

The script does:
1) bump the version in `app/pubspec.yaml`
2) build the core in release
3) build the Flutter release
4) create assets + `checksums.txt` in `dist/` (includes all files present in `dist/`)

If you want to avoid the bump:

Windows: not needed (manual update).

Linux:

```
NO_BUMP=1 ./scripts/release_full_linux.sh patch
```

### 2) Windows build (local install)

Example (Windows PowerShell):

```
scripts\build_core.ps1 -Profile release
cd app
flutter pub get
flutter build windows --release
```

### 3) Linux build (AppImage)

Example (Linux):

```
flutter clean
flutter pub get
flutter build linux --release

# Create the AppImage with your toolchain (appimagetool or existing script).
# Final output: Fedi3-linux-x86_64.AppImage
```

### 4) Compute SHA256

Linux:

```
sha256sum Fedi3-linux-x86_64.AppImage > checksums.txt
```

Windows: no checksum needed (manual update).

### 5) Create the GitHub release

- Tag: `vX.Y.Z`
- Title: `vX.Y.Z`
- Upload assets:
  - `Fedi3-linux-x86_64.AppImage`
  - `checksums.txt`

### 6) Verify

Start the client: the "Update available" banner should appear.
