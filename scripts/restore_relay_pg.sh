#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Uso: $0 <backup.sql.gz>"
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BACKUP="$1"

if [[ ! -f "$BACKUP" ]]; then
  echo "Backup non trovato: $BACKUP"
  exit 1
fi

cd "$ROOT_DIR"
gzip -dc "$BACKUP" | docker compose exec -T postgres psql -U fedi3 -d fedi3_relay

echo "Restore completato."
