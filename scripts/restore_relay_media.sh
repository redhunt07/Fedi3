#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Uso: $0 <media.tar.gz>"
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARCHIVE="$1"

if [[ ! -f "$ARCHIVE" ]]; then
  echo "Archivio non trovato: $ARCHIVE"
  exit 1
fi

cd "$ROOT_DIR"
gzip -dc "$ARCHIVE" | docker compose exec -T relay tar -xzf - -C /data/media
echo "Restore media completato."
