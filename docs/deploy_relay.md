> For an automated install on Debian/Ubuntu or Arch derivatives, see
> [server installation](deploy_server.md). This guide describes the manual
> relay configuration.

# Deploy relay (plug & play)

## 1) Config

```bash
cp .env.example .env
```

The template sets `COMPOSE_PROFILES=relay,p2p,caddy` (full stack). For a
relay-only install remove `p2p`; behind YunoHost remove `caddy` as well; add
`turn` and/or `search` only if you need coturn or Meilisearch. `scripts/install_server.sh` writes this value automatically.

Fill in the essential fields:

- `FEDI3_DOMAIN=relay.example.org` — the real domain of the relay.
- `FEDI3_RELAY_ADMIN_TOKEN=<token>` — at least 32 characters (`openssl rand -hex 32`). The relay refuses to start if it is shorter.
- `FEDI3_RELAY_TELEMETRY_TOKEN=<token>` — optional but recommended; it separates metrics from the admin token.
- `FEDI3_RELAY_PG_PASSWORD=<password>` — mandatory, the compose stops if it is missing.
- `FEDI3_RELAY_DB_DRIVER=postgres`; leave `FEDI3_RELAY_DB_URL` empty for the Compose service.
  The URL is composed from `FEDI3_RELAY_PG_USER`, `FEDI3_RELAY_PG_PASSWORD` and
  `FEDI3_RELAY_PG_DB`, with escaping of special characters. A full URL is an
  advanced override for external databases; do not duplicate the password in the template.
- Media backend (S3/WebDAV or local).
- `TURN_REALM`, `TURN_PASS`, `TURN_EXTERNAL_IP` (public IP) — needed by coturn. TURN starts only when `turn` is added to `COMPOSE_PROFILES`.
  The TURN container receives only its own port parameter in the environment,
  not the DB or relay credentials; the TURN credentials are passed to the service.
  STUN healthcheck and memory limit are also present in the standalone compose.

## 2) Start

```bash
docker compose up -d --build
docker compose ps
```

The `relay` service has a healthcheck: Caddy starts only when the relay responds on `/healthz`. If the relay does not become healthy, `docker compose ps` shows the status and `docker compose logs relay` the cause.

## 3) TLS / ACME

Caddy requires a real domain (not `relay.example.com`).
If ACME fails, check:

- DNS A/AAAA
- ports 80/443
- correct `FEDI3_DOMAIN`

Logs: `docker compose logs --tail=200 caddy`

## 4) Smoke test

```bash
scripts/relay_smoke_test.sh https://relay.example.org <ADMIN_TOKEN>
```

## 5) Monitoring

- `GET /healthz` — public, no authentication, no DB access. Use it for the liveness probe.
- `GET /readyz` — public; responds 503 if the database is unreachable. Use it for the readiness probe.
- `GET /_fedi3/relay/metrics.prom` — requires `Authorization: Bearer <FEDI3_RELAY_TELEMETRY_TOKEN>` (or the admin token if the telemetry token is not set).

The health endpoints do not write audit rows: they are meant to be queried every few seconds.

## 6) Diagnostics

```bash
scripts/relay_diagnose.sh
```

The script does not assume that the relay image has `curl` or `wget`: it uses the container healthcheck status and, if an HTTP client is available on the host, queries `127.0.0.1:8787` (the port is published only on loopback).

## 7) Notes

In production:

- `FEDI3_RELAY_ALLOW_SELF_REGISTER=false`
- `FEDI3_RELAY_HSTS_MAX_AGE_SECS` greater than zero (the compose default is one year)
- admin token stored in a vault, never in git
- do not publish port 8787 on `0.0.0.0`: the compose exposes it only on `127.0.0.1` for local diagnostics
- run the relay behind Caddy and leave `FEDI3_RELAY_TRUST_PROXY_HEADERS=true` only because the proxy overwrites `X-Forwarded-For`
