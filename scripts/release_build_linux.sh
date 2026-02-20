#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="${ROOT_DIR}/app"
DIST_DIR="${ROOT_DIR}/dist"

if [[ -n "${VERSION}" ]]; then
  sed -i -E "s/^version: .*/version: ${VERSION}/" "${APP_DIR}/pubspec.yaml"
fi

pushd "${APP_DIR}" >/dev/null
flutter clean
flutter pub get
flutter build linux --release
popd >/dev/null

mkdir -p "${DIST_DIR}"

APPIMAGE_OUT="${DIST_DIR}/Fedi3-linux-x86_64.AppImage"

if [[ -z "${FEDI3_APPIMAGE_TOOL:-}" ]]; then
  echo "FEDI3_APPIMAGE_TOOL not set. Build your AppImage and place it at ${APPIMAGE_OUT}"
  echo "Then re-run to generate checksums.txt"
else
  "${FEDI3_APPIMAGE_TOOL}" "${APP_DIR}" "${APPIMAGE_OUT}"
fi

if [[ ! -f "${APPIMAGE_OUT}" ]]; then
  echo "AppImage missing: ${APPIMAGE_OUT}"
  exit 1
fi

(
  cd "${DIST_DIR}"
  find . -maxdepth 1 -type f ! -name "checksums.txt" -print0 \
    | xargs -0 sha256sum \
    | sed 's|^\([a-f0-9]\+\)  \./|\1  |' \
    | sort > checksums.txt
)

echo "Linux update ready in ${DIST_DIR}"
