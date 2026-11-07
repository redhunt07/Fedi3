#!/usr/bin/env bash
# Diagnose a running relay deployment. Works with the stock relay image,
# which ships neither wget nor curl: health checks go through the container
# healthcheck status or a host-side HTTP client.
set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMPOSE_DIR="${ROOT_DIR}"

if [[ ! -f "${COMPOSE_DIR}/docker-compose.yml" ]]; then
  echo "relay_diagnose: docker-compose.yml not found at ${COMPOSE_DIR}" >&2
  exit 1
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "relay_diagnose: docker not found" >&2
  exit 1
fi

cd "${COMPOSE_DIR}" || exit 1

if ! docker compose ps >/dev/null 2>&1; then
  echo "relay_diagnose: docker compose not ready in ${COMPOSE_DIR}" >&2
  exit 1
fi

# Load .env for port and token lookups without exporting secrets to the terminal.
RELAY_PORT=8787
# shellcheck source=lib/dotenv.sh
source "$ROOT_DIR/scripts/lib/dotenv.sh"
load_dotenv "$ROOT_DIR/.env"
if [[ -n "${FEDI3_RELAY_BIND:-}" ]]; then
  RELAY_PORT="${FEDI3_RELAY_BIND##*:}"
fi
BASE_URL="http://127.0.0.1:${RELAY_PORT}"

http_get() {
  local url="$1"
  if command -v curl >/dev/null 2>&1; then
    curl -fsS -m 5 "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO- -T 5 "$url"
  else
    return 127
  fi
}

container_health() {
  local name="$1"
  docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}' "$name" 2>/dev/null || echo "unknown"
}

echo "== Relay Diagnose =="
echo "Working dir: ${COMPOSE_DIR}"
echo "Relay port:  ${RELAY_PORT}"
echo

echo "-- docker compose ps"
docker compose ps
echo

echo "-- Container health status"
for svc in relay postgres meilisearch dragonfly turn; do
  id="$(docker compose ps -q "$svc" 2>/dev/null || true)"
  if [[ -n "$id" ]]; then
    printf '%-14s %s\n' "$svc" "$(container_health "$id")"
  else
    printf '%-14s %s\n' "$svc" "(not running)"
  fi
done
echo

echo "-- Env sanity"
if [[ -f .env ]]; then
  grep -E 'FEDI3_RELAY_ADMIN_TOKEN|FEDI3_RELAY_MEILI_API_KEY|MEILI_MASTER_KEY|FEDI3_DOMAIN|FEDI3_RELAY_DB_URL|TURN_REALM|TURN_PORT|TURN_USER' .env | sed -E 's/^([^=]*(TOKEN|KEY|PASSWORD|DB_URL)[^=]*)=.*/\1=<redacted>/' || true
  admin_token="${FEDI3_RELAY_ADMIN_TOKEN:-}"
  token_len="${#admin_token}"
  if [[ "$token_len" -lt 32 ]]; then
    echo "WARNING: FEDI3_RELAY_ADMIN_TOKEN is ${token_len} characters (should be >= 32)"
  fi
else
  echo ".env missing"
fi
echo

echo "-- Relay health (${BASE_URL}/healthz)"
if out="$(http_get "${BASE_URL}/healthz")"; then
  echo "$out"
else
  rc=$?
  if [[ $rc -eq 127 ]]; then
    echo "no curl/wget on the host: relying on the container healthcheck above"
  else
    echo "unreachable from the host (is the port published?)"
  fi
fi
echo

echo "-- Relay readiness (${BASE_URL}/readyz)"
if out="$(http_get "${BASE_URL}/readyz")"; then
  echo "$out"
fi
echo

echo "-- Relay logs (tunnel connected/disconnected)"
docker compose logs --tail=200 relay 2>/dev/null | grep -E 'tunnel (connected|disconnected)' || echo "(none)"
echo

echo "-- Relay logs (5xx responses)"
docker compose logs --tail=200 relay 2>/dev/null | grep -E ' 5[0-9]{2} ' || echo "(none)"
echo

echo "-- Relay logs (signature rejections)"
docker compose logs --tail=200 relay 2>/dev/null | grep -iE 'signature|digest' | tail -20 || echo "(none)"
echo

echo "-- P2P infra logs (peer id)"
docker compose logs --tail=200 p2p_infra 2>/dev/null | grep -E 'peer_id|starting' || echo "(none)"
echo

echo "-- TURN logs (recent)"
docker compose logs --tail=50 turn 2>/dev/null || echo "(none)"
echo

echo "-- Meilisearch logs (auth errors)"
docker compose logs --tail=200 meilisearch 2>/dev/null | grep -i 'authorization header is missing' || echo "(none)"
echo

echo "-- Postgres logs (recent)"
docker compose logs --tail=50 postgres 2>/dev/null || echo "(none)"
echo

echo "-- Dragonfly logs (recent)"
docker compose logs --tail=50 dragonfly 2>/dev/null || echo "(none)"
echo

echo "== Done =="
