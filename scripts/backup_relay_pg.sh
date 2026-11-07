#!/usr/bin/env bash
# Dump the relay PostgreSQL database. Honours the deployment's configured
# user/database instead of hardcoding them, verifies the archive and applies
# retention.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$ROOT_DIR/backups}"

cd "$ROOT_DIR" || exit 1

# shellcheck source=lib/dotenv.sh
source "$ROOT_DIR/scripts/lib/dotenv.sh"
load_dotenv "$ROOT_DIR/.env"
KEEP="${BACKUP_KEEP:-7}"
if [[ ! "$KEEP" =~ ^[1-9][0-9]{0,3}$ ]]; then
  echo "BACKUP_KEEP must be an integer between 1 and 9999" >&2
  exit 1
fi

PG_USER="${FEDI3_RELAY_PG_USER:-fedi3}"
PG_DB="${FEDI3_RELAY_PG_DB:-fedi3_relay}"

mkdir -p "$OUT_DIR"
ts="$(date -u +"%Y%m%dT%H%M%SZ")"
out_file="$OUT_DIR/relay_pg_$ts.sql.gz"

if ! docker compose ps -q postgres >/dev/null 2>&1 || [[ -z "$(docker compose ps -q postgres)" ]]; then
  echo "backup_relay_pg: the postgres service is not running" >&2
  exit 1
fi

echo "Dumping database '$PG_DB' as user '$PG_USER'..."
set +e
docker compose exec -T postgres pg_dump -U "$PG_USER" -d "$PG_DB" --no-owner --no-privileges \
  | gzip -9 > "$out_file"
rc=$?
set -e

if [[ $rc -ne 0 ]]; then
  rm -f "$out_file"
  echo "backup_relay_pg: pg_dump failed (exit $rc)" >&2
  exit 1
fi

if [[ ! -s "$out_file" ]]; then
  rm -f "$out_file"
  echo "backup_relay_pg: the dump is empty" >&2
  exit 1
fi

if ! gzip -t "$out_file" 2>/dev/null; then
  echo "backup_relay_pg: the archive is corrupt" >&2
  exit 1
fi

echo "Backup scritto: $out_file ($(du -h "$out_file" | cut -f1))"

# Retention: keep the newest $KEEP dumps.
# shellcheck disable=SC2012
mapfile -t old < <(ls -1t "$OUT_DIR"/relay_pg_*.sql.gz 2>/dev/null | tail -n +"$((KEEP + 1))")
for f in "${old[@]:-}"; do
  [[ -n "$f" ]] || continue
  rm -f "$f"
  echo "Rimosso backup vecchio: $f"
done
