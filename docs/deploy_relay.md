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

## 4) Smoke test

```
scripts/relay_smoke_test.sh https://relay.fedi3.com <ADMIN_TOKEN>
```

## 5) Monitoring

- `/healthz`, `/readyz` con `Authorization: Bearer <ADMIN_TOKEN>`
- `/_fedi3/relay/metrics.prom` con `Authorization: Bearer <ADMIN_TOKEN>`

## 6) Note

In produzione:

- `FEDI3_RELAY_ALLOW_SELF_REGISTER=false`
- HSTS abilitato
- token admin conservato in vault
