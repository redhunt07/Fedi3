# Deploy relay (plug & play)

> For an automated install on Debian/Ubuntu or Arch derivatives, see
> [server installation](deploy_server.md). This guide describes the manual
> relay configuration.

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
- `FEDI3_RELAY_TELEMETRY_TOKEN=<token>` — optional; it authenticates relay-to-relay telemetry exchanges (`X-Fedi3-Telemetry-Token`). It does not replace the admin token for metrics.
- `FEDI3_RELAY_PG_PASSWORD=<password>` — mandatory, the compose stops if it is missing.
- `FEDI3_RELAY_DB_DRIVER=postgres`; leave `FEDI3_RELAY_DB_URL` empty for the Compose service.
  The URL is composed from `FEDI3_RELAY_PG_USER`, `FEDI3_RELAY_PG_PASSWORD` and
  `FEDI3_RELAY_PG_DB`, with escaping of special characters. A full URL is an
  advanced override for external databases; do not duplicate the password in the template.
- Media backend (S3/WebDAV or local).
- Relay mesh (optional): `FEDI3_RELAY_SEED_RELAYS`, `FEDI3_RELAY_PEER_SYNC_SECS` and
  `FEDI3_RELAY_PEER_MAX_HOPS`; peers can also be managed through `/admin/peers`.
- `TURN_REALM`, `TURN_PASS`, `TURN_EXTERNAL_IP` (public IP) — needed by coturn. TURN starts only when `turn` is added to `COMPOSE_PROFILES`.
  When the `turn` profile is enabled, the relay also publishes the coturn
  credentials to authenticated clients (`GET /_fedi3/relay/turn/{username}`),
  deriving the URLs from `FEDI3_DOMAIN` and `TURN_PORT`. Override them with
  `FEDI3_RELAY_TURN_URLS`/`FEDI3_RELAY_TURN_PORT`/`FEDI3_RELAY_TURN_USERNAME`/
  `FEDI3_RELAY_TURN_CREDENTIAL`, or mount a multi-operator pool with
  `FEDI3_RELAY_TURN_POOL_FILE`. Without TURN, calls fail whenever both peers
  sit behind symmetric NAT or a VPN.
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
- `GET /_fedi3/relay/metrics.prom` — requires `Authorization: Bearer <FEDI3_RELAY_ADMIN_TOKEN>`.
- `GET /_fedi3/relay/stats` — public relay mesh statistics.
- `POST /_fedi3/relay/telemetry` — relay-to-relay telemetry; when `FEDI3_RELAY_TELEMETRY_TOKEN` is set, the sender must include `X-Fedi3-Telemetry-Token`.

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
- keep `.env` and `relay-secrets.txt` at mode 600 and outside git

### Public landing page and nodeinfo

The relay binary serves its own landing page, so a link posted in the fediverse
does not end on a 404:

- `GET /` — English page;
- `GET /it` — Italian page;
- `GET /en` — permanent redirect to `/`;
- `GET /assets/landing/*` — stylesheet, banner, favicon and screenshots embedded
  in the binary (no volume or static mount needed);
- `GET /nodeinfo/2.0` — reports `software.name = fedi3`, the release version and
  `metadata.repository`, so directories such as FediDB and fediverse.observer
  can classify the relay as Fedi3 instead of an unknown product.
- `GET /_fedi3/relay/live` — aggregate snapshot for the live view: online users,
  peers, relay reachability and process-wide traffic counters. No usernames or
  peer ids are included; responses are `Cache-Control: no-store` and rate-limited
  per client IP.

In base-domain mode (`FEDI3_RELAY_BASE_DOMAIN`), requests to `<user>.<domain>`
keep being forwarded to the user core, including `/`. Nothing needs to change
in Caddy: it already proxies the site to the relay. The page loads no external
fonts; the live mesh view uses one same-origin script (`/assets/landing/live.js`)
and falls back to the raw `/_fedi3/relay/live` JSON when JavaScript is disabled.
The CSP documented in `.env.example` allows it because `script-src` falls back to
`default-src 'self'`; a stricter policy that blocks same-origin scripts only
disables the animation, not the page.

Host hardening checklist (verified on the reference VPS, see
[deploy_vps.md](deploy_vps.md#security) and
[deploy_yunohost.md](deploy_yunohost.md#security)):

- SSH key-only (`PasswordAuthentication no`, `PermitRootLogin no`,
  `MaxAuthTries 3`) with the hardening drop-in kept alphabetically first;
- no SSH IP bans when the operator IP is dynamic and SSH already rejects
  passwords; keep fail2ban for the other exposed services;
- firewall allows only 80/443, 4001, TURN and the support-agent port;
- automatic security updates enabled; verify with
  `dnf needs-restarting -r` / `apt-get -s upgrade` and reboot only in a chosen
  window;
- backups scheduled **and** executable (`chmod +x scripts/*.sh`), with a check
  that the newest archive is from today or yesterday;
- TLS 1.2+ only at the proxy; verify the deployed tree matches the release
  commit before rebuilding.

## 8) Related documentation

- [server installation](deploy_server.md) — automated Debian/Arch installer
- [VPS deployment](deploy_vps.md) — reference deployment on `fedi3.redhunt07.ovh`
- [backup and restore](ops_backup_restore.md) — PostgreSQL and media backups
- [promotion and launch kit](promotion.md) — directory submissions and fediverse presence
