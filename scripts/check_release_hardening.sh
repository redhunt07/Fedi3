#!/usr/bin/env bash
set -euo pipefail

echo "Checking required release files..."
test -f .env.example
test -f docker-compose.yml
test -f scripts/relay_smoke_test.sh

echo "Checking Android placeholders..."
if rg -n 'com\.example\.' app/android app/lib >/dev/null; then
  echo "Found placeholder Android application id" >&2
  exit 1
fi
if rg -n 'signingConfigs\.getByName\("debug"\)' app/android/app/build.gradle.kts >/dev/null; then
  echo "Android release build still uses debug signing" >&2
  exit 1
fi

echo "Checking release checksum support..."
rg -n 'checksums\.txt' docs/updates.md scripts/release_build_linux.sh scripts/release_build_windows.ps1 >/dev/null

echo "Checking relay env example coverage..."
rg -n '^FEDI3_RELAY_ADMIN_TOKEN=' .env.example >/dev/null
rg -n '^FEDI3_RELAY_PG_PASSWORD=' .env.example >/dev/null
rg -n '^TURN_PASS=' .env.example >/dev/null

echo "Release hardening checks passed."
