#!/usr/bin/env bash
set -euo pipefail

BUMP="${1:-patch}"
NO_BUMP="${NO_BUMP:-0}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="${ROOT_DIR}/app"
PUBSPEC="${APP_DIR}/pubspec.yaml"

get_version() {
  local line
  line="$(grep -E "^version:" "$PUBSPEC" | head -n 1)"
  if [[ -z "$line" ]]; then
    echo "Missing version in pubspec.yaml" >&2
    exit 1
  fi
  echo "$line"
}

bump_version() {
  local line="$1"
  local version
  version="$(echo "$line" | sed -E 's/^version:\s*//')"
  local base="${version%%+*}"
  local build="0"
  if [[ "$version" == *"+"* ]]; then
    build="${version##*+}"
  fi
  IFS='.' read -r major minor patch <<<"$base"
  major="${major:-0}"
  minor="${minor:-0}"
  patch="${patch:-0}"
  build=$((build + 1))
  case "$BUMP" in
    major) major=$((major + 1)); minor=0; patch=0 ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    patch) patch=$((patch + 1)) ;;
    *) echo "Invalid bump: $BUMP" >&2; exit 1 ;;
  esac
  echo "${major}.${minor}.${patch}+${build}"
}

line="$(get_version)"
version="${line#version: }"
if [[ "$NO_BUMP" != "1" ]]; then
  version="$(bump_version "$line")"
  sed -i -E "s/^version: .*/version: ${version}/" "$PUBSPEC"
fi

echo "Version: ${version}"

"${ROOT_DIR}/scripts/build_core.sh" release
"${ROOT_DIR}/scripts/release_build_linux.sh" "${version}"
