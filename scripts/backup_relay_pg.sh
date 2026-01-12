#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$ROOT_DIR/backups}"

mkdir -p "$OUT_DIR"
ts="$(date -u +"%Y%m%dT%H%M%SZ")"
out_file="$OUT_DIR/relay_pg_$ts.sql.gz"

cd "$ROOT_DIR"
docker compose exec -T postgres pg_dump -U fedi3 -d fedi3_relay | gzip -9 > "$out_file"

echo "Backup scritto: $out_file"
