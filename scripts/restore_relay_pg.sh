#!/usr/bin/env bash
# Restore the relay PostgreSQL database from a dump produced by
# backup_relay_pg.sh. Destructive: it overwrites the current schema and data.
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Uso: $0 <backup.sql.gz> [--yes]" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BACKUP="$1"
ASSUME_YES=0
if [[ "${2:-}" == "--yes" || "${2:-}" == "-y" ]]; then
  ASSUME_YES=1
fi

if [[ ! -f "$BACKUP" ]]; then
  echo "Backup non trovato: $BACKUP" >&2
  exit 1
fi

if ! gzip -t "$BACKUP" 2>/dev/null; then
  echo "Archivio corrotto: $BACKUP" >&2
  exit 1
fi

cd "$ROOT_DIR" || exit 1

# shellcheck source=lib/dotenv.sh
source "$ROOT_DIR/scripts/lib/dotenv.sh"
load_dotenv "$ROOT_DIR/.env"

PG_USER="${FEDI3_RELAY_PG_USER:-fedi3}"
PG_DB="${FEDI3_RELAY_PG_DB:-fedi3_relay}"

if [[ -z "$(docker compose ps -q postgres 2>/dev/null)" ]]; then
  echo "restore_relay_pg: the postgres service is not running" >&2
  exit 1
fi

if [[ $ASSUME_YES -eq 0 ]]; then
  echo "Verranno SOVRASCRITTI schema e dati del database '$PG_DB' (utente '$PG_USER')."
  read -r -p "Continuare? [s/N] " answer
  case "$answer" in
    s|S|y|Y) ;;
    *) echo "Annullato."; exit 1 ;;
  esac
fi

# Stop the relay so nothing writes while the restore runs.
echo "Fermo il relay..."
docker compose stop relay >/dev/null

echo "Ripristino '$PG_DB'..."
set +e
gzip -dc "$BACKUP" | docker compose exec -T postgres \
  psql -v ON_ERROR_STOP=1 -U "$PG_USER" -d "$PG_DB"
rc=$?
set -e

if [[ $rc -ne 0 ]]; then
  echo "restore_relay_pg: psql failed (exit $rc); the relay is still stopped" >&2
  exit 1
fi

echo "Riavvio il relay..."
docker compose start relay >/dev/null

echo "Restore completato. Verifica con: scripts/relay_smoke_test.sh"
