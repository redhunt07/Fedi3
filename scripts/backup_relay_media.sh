#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$ROOT_DIR/backups}"

mkdir -p "$OUT_DIR"
ts="$(date -u +"%Y%m%dT%H%M%SZ")"
out_file="$OUT_DIR/relay_media_$ts.tar.gz"

cd "$ROOT_DIR"
docker compose exec -T relay tar -czf - /data/media > "$out_file"
echo "Backup media: $out_file"
