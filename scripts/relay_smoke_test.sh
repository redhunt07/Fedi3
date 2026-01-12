#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Uso: $0 <base_url> [admin_token]"
  exit 1
fi

BASE_URL="${1%/}"
ADMIN_TOKEN="${2:-}"

auth_headers=()
if [[ -n "$ADMIN_TOKEN" ]]; then
  auth_headers=(-H "Authorization: Bearer $ADMIN_TOKEN")
fi

echo "Checking healthz..."
curl -fsS "${auth_headers[@]}" "$BASE_URL/healthz" >/dev/null
echo "Checking readyz..."
curl -fsS "${auth_headers[@]}" "$BASE_URL/readyz" >/dev/null
echo "Checking metrics..."
curl -fsS "${auth_headers[@]}" "$BASE_URL/_fedi3/relay/metrics.prom" >/dev/null

echo "OK"
