# Deploy relay (plug & play)

## 1) Config

```
cp .env.example .env
```

Compila i campi essenziali:

- `FEDI3_DOMAIN=relay.fedi3.com`
- `FEDI3_RELAY_ADMIN_TOKEN=<token>`
- `FEDI3_RELAY_TELEMETRY_TOKEN=<token>` (opzionale ma consigliato)
- `FEDI3_RELAY_DB_DRIVER=postgres`
- `FEDI3_RELAY_DB_URL=postgres://...`
- Media backend (S3/WebDAV o local)
- Relay mesh (server-side P2P):
  - `FEDI3_RELAY_MESH_ENABLE=true`
  - `FEDI3_RELAY_MESH_KEY=/data/fedi3_relay_mesh_keypair.pb` (persistente su volume)
  - `FEDI3_RELAY_MESH_LISTEN=` (opzionale, default auto)
  - `FEDI3_RELAY_MESH_BOOTSTRAP=` (opzionale, default: p2p_infra)

## 2) Avvio

```
docker compose up -d --build
docker compose ps
```

## 3) TLS / ACME

Caddy richiede un dominio reale (no `relay.example.com`).  
Se ACME fallisce, controlla:

- DNS A/AAAA
- porte 80/443
- `FEDI3_DOMAIN` corretto

Logs: `docker compose logs --tail=200 caddy`

## 3b) TURN su VPS

La configurazione TURN usa `network_mode: host` per evitare problemi con il mapping
di migliaia di porte UDP. Su VPS Linux e' la scelta consigliata.
Se usi Docker Desktop (non Linux), valuta un range piu' piccolo in `TURN_MIN_PORT`
e `TURN_MAX_PORT` oppure avvia TURN separatamente con `deploy/turn/docker-compose.yml`.

## 4) Smoke test

```
scripts/relay_smoke_test.sh https://relay.fedi3.com <ADMIN_TOKEN>
```

## 5) Monitoring

- `/healthz`, `/readyz` con `Authorization: Bearer <ADMIN_TOKEN>`
- `/_fedi3/relay/metrics.prom` con `Authorization: Bearer <ADMIN_TOKEN>`

## 5b) Verifica relay mesh

- `/_fedi3/relay/stats` deve includere `relay_p2p_peer_id`
- log relay: "relay mesh enabled" + "relay mesh sync applied"

## 6) Note

In produzione:

- `FEDI3_RELAY_ALLOW_SELF_REGISTER=false`
- HSTS abilitato
- token admin conservato in vault
