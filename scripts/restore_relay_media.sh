#!/usr/bin/env bash
# Restore relay media from an archive produced by backup_relay_media.sh.
# Supports both the current layout (media/...) and legacy archives that stored
# absolute paths (/data/media/...).
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Uso: $0 <media.tar.gz> [--yes]" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARCHIVE="$1"
ASSUME_YES=0
if [[ "${2:-}" == "--yes" || "${2:-}" == "-y" ]]; then
  ASSUME_YES=1
fi

if [[ ! -f "$ARCHIVE" ]]; then
  echo "Archivio non trovato: $ARCHIVE" >&2
  exit 1
fi

if ! gzip -t "$ARCHIVE" 2>/dev/null; then
  echo "Archivio corrotto: $ARCHIVE" >&2
  exit 1
fi

cd "$ROOT_DIR" || exit 1

if [[ -f .env ]]; then
  set -a
  # shellcheck disable=SC1091
  . ./.env
  set +a
fi

MEDIA_DIR="${FEDI3_RELAY_MEDIA_DIR:-/data/media}"
MEDIA_PARENT="$(dirname "$MEDIA_DIR")"
MEDIA_BASENAME="$(basename "$MEDIA_DIR")"

if [[ -z "$(docker compose ps -q relay 2>/dev/null)" ]]; then
  echo "restore_relay_media: the relay service is not running" >&2
  exit 1
fi

first_path="$(tar -tzf "$ARCHIVE" 2>/dev/null | head -n1 || true)"
case "$first_path" in
  /*)
    echo "Rilevato archivio con percorsi assoluti (formato legacy)."
    EXTRACT_DIR="/"
    ;;
  *)
    echo "Rilevato archivio con percorsi relativi: estrazione in ${MEDIA_PARENT}."
    EXTRACT_DIR="$MEDIA_PARENT"
    ;;
esac

if [[ $ASSUME_YES -eq 0 ]]; then
  echo "I file in ${MEDIA_DIR} verranno sovrascritti dai contenuti di '${ARCHIVE}'."
  read -r -p "Continuare? [s/N] " answer
  case "$answer" in
    s|S|y|Y) ;;
    *) echo "Annullato."; exit 1 ;;
  esac
fi

echo "Estrazione in ${EXTRACT_DIR}..."
set +e
docker compose exec -T relay tar -xzf - -C "$EXTRACT_DIR" < "$ARCHIVE"
rc=$?
set -e

if [[ $rc -ne 0 ]]; then
  echo "restore_relay_media: tar failed (exit $rc)" >&2
  exit 1
fi

echo "Restore media completato."
echo "Suggerimento: esegui 'docker compose exec relay du -sh ${MEDIA_DIR}' e verifica che il conteggio corrisponda all'archivio."
