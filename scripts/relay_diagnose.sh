#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMPOSE_DIR="${ROOT_DIR}"

if [[ ! -f "${COMPOSE_DIR}/docker-compose.yml" ]]; then
  echo "relay_diagnose: docker-compose.yml not found at ${COMPOSE_DIR}"
  exit 1
fi

cd "${COMPOSE_DIR}"

echo "== Relay Diagnose =="
echo "Working dir: ${COMPOSE_DIR}"
echo

if ! command -v docker >/dev/null 2>&1; then
  echo "docker not found"
  exit 1
fi

if ! docker compose ps >/dev/null 2>&1; then
  echo "docker compose not ready in ${COMPOSE_DIR}"
  exit 1
fi

echo "-- docker compose ps"
docker compose ps
echo

echo "-- Env sanity"
if [[ -f .env ]]; then
  grep -E 'FEDI3_RELAY_MEILI_API_KEY|MEILI_MASTER_KEY|FEDI3_DOMAIN|FEDI3_RELAY_DB_URL|TURN_REALM|TURN_PORT|TURN_USER' .env || true
else
  echo ".env missing"
fi
echo

echo "-- Meili health"
docker compose exec -T meilisearch sh -lc 'wget -qO- http://127.0.0.1:7700/health || true'
echo

echo "-- Relay health"
docker compose exec -T relay sh -lc 'wget -qO- http://127.0.0.1:8787/healthz || true'
docker compose exec -T relay sh -lc 'wget -qO- http://127.0.0.1:8787/readyz || true'
echo

echo "-- Relay logs (tunnel connected/disconnected)"
docker compose logs --tail=200 relay | grep -E 'tunnel (connected|disconnected)' || true
echo

echo "-- Relay logs (errors)"
docker compose logs --tail=200 relay | grep -E ' 5[0-9]{2} ' || true
echo

echo "-- P2P infra logs (peer id)"
docker compose logs --tail=200 p2p_infra | grep -E 'peer_id|starting' || true
echo

echo "-- TURN logs (recent)"
docker compose logs --tail=50 turn || true
echo

echo "-- Meili logs (auth errors)"
docker compose logs --tail=200 meilisearch | grep -i 'authorization header is missing' || true
echo

echo "-- Postgres logs (recent)"
docker compose logs --tail=50 postgres || true
echo

echo "-- Dragonfly logs (recent)"
docker compose logs --tail=50 dragonfly || true
echo

echo "== Done =="
