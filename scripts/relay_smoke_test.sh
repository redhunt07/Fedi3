#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Uso: $0 <base_url> [admin_token] [check_stats]"
  exit 1
fi

BASE_URL="${1%/}"
ADMIN_TOKEN="${2:-}"
CHECK_STATS="${3:-0}"

auth_headers=()
if [[ -n "$ADMIN_TOKEN" ]]; then
  auth_headers=(-H "Authorization: Bearer $ADMIN_TOKEN")
fi

expect_status() {
  local expected="$1"
  local url="$2"
  shift 2
  local code
  code="$(curl -sS -o /dev/null -w "%{http_code}" "$@" "$url")"
  if [[ "$code" != "$expected" ]]; then
    echo "Unexpected status for $url: got $code expected $expected" >&2
    exit 1
  fi
}

echo "Checking admin auth is enforced..."
expect_status "401" "$BASE_URL/healthz"

if [[ -z "$ADMIN_TOKEN" ]]; then
  echo "Admin token not provided; stopping after negative auth check."
  exit 0
fi

echo "Checking healthz..."
expect_status "200" "$BASE_URL/healthz" "${auth_headers[@]}"
echo "Checking readyz..."
expect_status "200" "$BASE_URL/readyz" "${auth_headers[@]}"
echo "Checking metrics..."
expect_status "200" "$BASE_URL/_fedi3/relay/metrics.prom" "${auth_headers[@]}"

if [[ "$CHECK_STATS" == "1" ]]; then
  echo "Checking relay stats..."
  expect_status "200" "$BASE_URL/_fedi3/relay/stats"
fi

echo "OK"
