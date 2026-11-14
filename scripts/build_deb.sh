#!/usr/bin/env bash
# Package a previously verified AppImage, without introducing host libraries.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="$(realpath "${1:-$root/dist/Fedi3-linux-x86_64.AppImage}")"
version="${2:-$(sed -n 's/^version: \([^+]*\).*/\1/p' "$root/app/pubspec.yaml")}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 2
command -v dpkg-deb >/dev/null || { echo 'Run on Debian/Ubuntu with dpkg-dev' >&2; exit 2; }
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
(cd "$stage" && "$image" --appimage-extract >/dev/null)
pkg="$stage/package"
install -d "$pkg/opt/fedi3" "$pkg/DEBIAN" "$pkg/usr/bin" "$pkg/usr/share/doc/fedi3"
cp -a "$stage/squashfs-root/." "$pkg/opt/fedi3/"
printf '#!/bin/sh\nexec /opt/fedi3/AppRun "$@"\n' > "$pkg/usr/bin/fedi3"
chmod 755 "$pkg/usr/bin/fedi3"
install -Dm644 "$root/deploy/appimage/fedi3.desktop" "$pkg/usr/share/applications/fedi3.desktop"
install -Dm644 "$root/app/web/icons/Icon-512.png" "$pkg/usr/share/icons/hicolor/512x512/apps/fedi3.png"
install -m644 "$root/LICENSE" "$pkg/usr/share/doc/fedi3/copyright"
cat > "$pkg/DEBIAN/control" <<CONTROL
Package: fedi3
Version: $version-1
Section: net
Priority: optional
Architecture: amd64
Maintainer: RedHunt07 <redhunt07@tutanota.com>
Homepage: https://github.com/redhunt07/Fedi3
Depends: libc6 (>= 2.34), libgtk-3-0, libsecret-1-0, libgl1, libegl1, libgles2, libgl1-mesa-dri, libgbm1, libasound2, libpulse0, libgstreamer1.0-0, libgstreamer-plugins-base1.0-0
Description: ActivityPub client with a local Rust core and peer synchronization
 Profiles, posts, encrypted chat and calls. Requires a running Secret Service.
 User identities and data are preserved during upgrades and package removal.
CONTROL
mkdir -p "$root/dist"
dpkg-deb --root-owner-group --build "$pkg" "$root/dist/fedi3_${version}-1_amd64.deb"
