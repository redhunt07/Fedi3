> Per un'installazione automatica su Debian/Ubuntu o derivate Arch, vedi
> [installazione del server](deploy_server.md). Questa guida descrive la
> configurazione manuale del relay.

# Deploy relay (plug & play)

## 1) Config

```bash
cp .env.example .env
```

Il template imposta `COMPOSE_PROFILES=relay,p2p,caddy` (stack completo). Per una
installazione solo relay rimuovi `p2p`; su YunoHost rimuovi anche `caddy`;
aggiungi `turn` e/o `search` solo se servono coturn o Meilisearch. `scripts/install_server.sh` scrive questo valore
automaticamente.

Compila i campi essenziali:

- `FEDI3_DOMAIN=relay.example.org` — il dominio reale del relay.
- `FEDI3_RELAY_ADMIN_TOKEN=<token>` — almeno 32 caratteri (`openssl rand -hex 32`). Il relay rifiuta di partire se è più corto.
- `FEDI3_RELAY_TELEMETRY_TOKEN=<token>` — opzionale ma consigliato, separa le metriche dal token admin.
- `FEDI3_RELAY_PG_PASSWORD=<password>` — obbligatoria, il compose si ferma se manca.
- `FEDI3_RELAY_DB_DRIVER=postgres`; lasciare `FEDI3_RELAY_DB_URL` vuoto per il servizio Compose.
  L’URL viene composto da `FEDI3_RELAY_PG_USER`, `FEDI3_RELAY_PG_PASSWORD` e
  `FEDI3_RELAY_PG_DB`, con escaping dei caratteri speciali. Un URL completo è
  un override avanzato per database esterni; non duplicare la password nel template.
- Media backend (S3/WebDAV o local).
- `TURN_REALM`, `TURN_PASS`, `TURN_EXTERNAL_IP` (IP pubblico) — necessarie per coturn. TURN parte solo quando `turn` è aggiunto a `COMPOSE_PROFILES`.
  Con il profilo `turn` attivo, il relay pubblica anche le credenziali coturn
  agli utenti autenticati (`GET /_fedi3/relay/turn/{username}`), ricavando gli
  URL da `FEDI3_DOMAIN` e `TURN_PORT`. Si possono sovrascrivere con
  `FEDI3_RELAY_TURN_URLS`/`FEDI3_RELAY_TURN_PORT`/`FEDI3_RELAY_TURN_USERNAME`/
  `FEDI3_RELAY_TURN_CREDENTIAL`, oppure montare un pool multi-operatore con
  `FEDI3_RELAY_TURN_POOL_FILE`. Senza TURN le chiamate falliscono quando
  entrambi gli interlocutori sono dietro NAT simmetrico o VPN.
  Il container TURN riceve soltanto il proprio parametro di porta nell’ambiente,
  non le credenziali del DB o del relay; le credenziali TURN sono passate al servizio.
  Healthcheck STUN e limite memoria sono presenti anche nel compose standalone.

## 2) Avvio

```bash
docker compose up -d --build
docker compose ps
```

Il servizio `relay` ha un healthcheck: Caddy parte solo quando il relay risponde su `/healthz`. Se il relay non diventa healthy, `docker compose ps` mostra lo stato e `docker compose logs relay` la causa.

## 3) TLS / ACME

Caddy richiede un dominio reale (no `relay.example.com`).
Se ACME fallisce, controlla:

- DNS A/AAAA
- porte 80/443
- `FEDI3_DOMAIN` corretto

Logs: `docker compose logs --tail=200 caddy`

## 4) Smoke test

```bash
scripts/relay_smoke_test.sh https://relay.example.org <ADMIN_TOKEN>
```

## 5) Monitoring

- `GET /healthz` — pubblico, nessuna autenticazione, nessun accesso al DB. Usalo per il probe di liveness.
- `GET /readyz` — pubblico; risponde 503 se il database non è raggiungibile. Usalo per il probe di readiness.
- `GET /_fedi3/relay/metrics.prom` — richiede `Authorization: Bearer <FEDI3_RELAY_TELEMETRY_TOKEN>` (o il token admin se il token telemetria non è impostato).

Gli endpoint di salute non scrivono righe di audit: sono pensati per essere interrogati ogni pochi secondi.

## 6) Diagnosi

```bash
scripts/relay_diagnose.sh
```

Lo script non assume che l'immagine del relay abbia `curl` o `wget`: usa lo stato di healthcheck del container e, se sul host è disponibile un client HTTP, interroga `127.0.0.1:8787` (la porta è pubblicata solo su loopback).

## 7) Note

In produzione:

- `FEDI3_RELAY_ALLOW_SELF_REGISTER=false`
- `FEDI3_RELAY_HSTS_MAX_AGE_SECS` maggiore di zero (il default del compose è un anno)
- token admin conservato in un vault, mai in git
- non pubblicare la porta 8787 su `0.0.0.0`: il compose la espone solo su `127.0.0.1` per la diagnosi locale
- fai girare il relay dietro Caddy e lascia `FEDI3_RELAY_TRUST_PROXY_HEADERS=true` solo perché il proxy sovrascrive `X-Forwarded-For`
