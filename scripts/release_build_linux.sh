#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="${ROOT_DIR}/app"
DIST_DIR="${ROOT_DIR}/dist"
FLUTTER_BIN="${FLUTTER_BIN:-}"

require_cmd() {
  local cmd="$1"
  if ! command -v "${cmd}" >/dev/null 2>&1; then
    echo "Missing required command: ${cmd}"
    exit 1
  fi
}

require_pkgconf() {
  local mod="$1"
  if ! pkg-config --exists "${mod}"; then
    echo "Missing required pkg-config module: ${mod}"
    exit 1
  fi
}

preflight_linux_deps() {
  require_cmd pkg-config
  require_cmd cmake
  require_cmd ninja
  require_cmd clang

  if [[ -z "${FLUTTER_BIN}" ]]; then
    if command -v flutter >/dev/null 2>&1; then
      FLUTTER_BIN="$(command -v flutter)"
    elif [[ -x "${HOME}/flutter/bin/flutter" ]]; then
      FLUTTER_BIN="${HOME}/flutter/bin/flutter"
    elif [[ -x "${HOME}/.local/flutter/bin/flutter" ]]; then
      FLUTTER_BIN="${HOME}/.local/flutter/bin/flutter"
    else
      echo "Missing Flutter SDK. Install Flutter or set FLUTTER_BIN=/path/to/flutter."
      exit 1
    fi
  fi

  # Flutter Linux desktop + plugins used by this app.
  require_pkgconf gtk+-3.0
  require_pkgconf webkit2gtk-4.1
  require_pkgconf libsecret-1
  require_pkgconf mpv

  # media_kit/mpv on some distros requires ffmpeg/cdio headers.
  # Accept either pkg-config-provided include flags or known include roots.
  if ! pkg-config --exists libavcodec && [[ ! -d /usr/include/ffmpeg ]]; then
    echo "Missing ffmpeg development headers (pkg-config libavcodec or /usr/include/ffmpeg)."
    echo "Install matching ffmpeg/mpv devel packages for your distro before building."
    exit 1
  fi
  if ! pkg-config --exists libcdio && [[ ! -d /usr/include/cdio ]]; then
    echo "Missing libcdio development headers (pkg-config libcdio or /usr/include/cdio)."
    echo "Install libcdio development package before building."
    exit 1
  fi
}

if [[ -n "${VERSION}" ]]; then
  sed -i -E "s/^version: .*/version: ${VERSION}/" "${APP_DIR}/pubspec.yaml"
fi

preflight_linux_deps

if [[ "${FEDI3_PREFLIGHT_ONLY:-0}" == "1" ]]; then
  echo "Linux preflight checks passed."
  exit 0
fi

mkdir -p "${DIST_DIR}"
APPIMAGE_OUT="${DIST_DIR}/Fedi3-linux-x86_64.AppImage"
PATH="$(dirname "${FLUTTER_BIN}"):$PATH" \
  FEDI3_APPIMAGE_OUT="${APPIMAGE_OUT}" \
  bash "${ROOT_DIR}/scripts/build_appimage.sh" release

# Hash exactly this build, never unrelated or stale files in dist/.
(cd "${DIST_DIR}" && sha256sum Fedi3-linux-x86_64.AppImage > checksums.txt)
echo "Linux update ready in ${DIST_DIR}"
