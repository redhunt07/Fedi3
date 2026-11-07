#!/usr/bin/env bash
# Archive the relay media directory. Paths are stored relative to the media
# root so that a restore does not create /data/media/data/media/... nesting.
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

MEDIA_DIR="${FEDI3_RELAY_MEDIA_DIR:-/data/media}"
MEDIA_PARENT="$(dirname "$MEDIA_DIR")"
MEDIA_BASENAME="$(basename "$MEDIA_DIR")"

if [[ -z "$(docker compose ps -q relay 2>/dev/null)" ]]; then
  echo "backup_relay_media: the relay service is not running" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
ts="$(date -u +"%Y%m%dT%H%M%SZ")"
out_file="$OUT_DIR/relay_media_$ts.tar.gz"

echo "Archiving ${MEDIA_DIR}..."
set +e
docker compose exec -T relay tar -czf - -C "$MEDIA_PARENT" "$MEDIA_BASENAME" > "$out_file"
rc=$?
set -e

if [[ $rc -ne 0 ]]; then
  rm -f "$out_file"
  echo "backup_relay_media: tar failed (exit $rc)" >&2
  exit 1
fi

if [[ ! -s "$out_file" ]]; then
  rm -f "$out_file"
  echo "backup_relay_media: the archive is empty" >&2
  exit 1
fi

if ! gzip -t "$out_file" 2>/dev/null; then
  echo "backup_relay_media: the archive is corrupt" >&2
  exit 1
fi

first_path="$(tar -tzf "$out_file" 2>/dev/null | head -n1 || true)"
case "$first_path" in
  "$MEDIA_BASENAME"/*|"$MEDIA_BASENAME"/) ;;
  *)
    echo "backup_relay_media: unexpected archive layout (first entry: '$first_path')" >&2
    exit 1
    ;;
esac

echo "Backup media: $out_file ($(du -h "$out_file" | cut -f1), $(tar -tzf "$out_file" | wc -l) entries)"

# shellcheck disable=SC2012
mapfile -t old < <(ls -1t "$OUT_DIR"/relay_media_*.tar.gz 2>/dev/null | tail -n +"$((KEEP + 1))")
for f in "${old[@]:-}"; do
  [[ -n "$f" ]] || continue
  rm -f "$f"
  echo "Rimosso backup vecchio: $f"
done
