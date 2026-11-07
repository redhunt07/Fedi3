# Deploy relay and P2P infrastructure on a VPS

This guide describes the active deployment on `fedi3.redhunt07.ovh`
(IPv4 `51.210.42.24`). The relay is reachable only via HTTPS through Caddy;
PostgreSQL, Redis/Dragonfly and Meilisearch stay on the internal Docker network.
P2P and TURN are exposed publicly.

## DNS and TLS

- A record: `fedi3.redhunt07.ovh -> 51.210.42.24`.
- The certificate is issued and renewed automatically by Caddy (Let's Encrypt).
- No AAAA record is configured: the server has IPv6, but the domain uses only
  IPv4. Add `AAAA -> 2001:41d0:404:200::9721` only after verifying
  that Caddy and the firewall are reachable over IPv6.
- `FEDI3_RELAY_BASE_DOMAIN` is empty: actors live on the main domain
  (`https://fedi3.redhunt07.ovh/users/<user>`). This way no wildcard DNS
  `*.fedi3.redhunt07.ovh` is needed.

## Stack

The deployment uses `docker-compose.yml` in the directory
`/home/rocky/fedi3` on the VPS:

- `caddy`: TLS and reverse proxy to the relay.
- `relay`: Fedi3 service, PostgreSQL database, local media.
- `postgres`, `dragonfly`, `meilisearch`: internal backends.
- `p2p_infra`: libp2p node with relay server, Kademlia and mailbox.
- `turn`: coturn with credentials, external IP and UDP range.

Compose services are gated by profiles. This deployment sets
`COMPOSE_PROFILES=relay,caddy,p2p,turn,search` in `.env`; without that value a
plain `docker compose up -d` starts no service.

Ports open in `firewalld`: `22`, `80`, `443`, `4001/tcp`, `4001/udp`,
`3478/tcp`, `3478/udp`, `49152-65535/udp`.

## Configuration and secrets

- Configuration: `/home/rocky/fedi3/.env` (permissions `600`).
- Application tokens: `/home/rocky/fedi3/relay-secrets.txt` (permissions `600`).
- Do not commit `.env` or `relay-secrets.txt`.
- The user `alice` is registered on the relay with the application token present in
  `relay-secrets.txt`; the local client uses the same token.
- TURN uses `relay` and the password in `relay-secrets.txt`; the local client has
  the same values in `~/.config/fedi3/config.json`.

## Client

The local client is configured with:

- `publicBaseUrl = https://fedi3.redhunt07.ovh`
- `relayWs = wss://fedi3.redhunt07.ovh`
- ICE: `stun:fedi3.redhunt07.ovh:3478`,
  `turn:fedi3.redhunt07.ovh:3478?transport=udp`,
  `turn:fedi3.redhunt07.ovh:3478?transport=tcp`
- `p2pEnable = true`, `webrtcEnable = true`

After a change to the relay or the token, restart the core/client and verify
that the log shows `tunnel connected`.

## Operations

```bash
cd /home/rocky/fedi3

# status
sudo docker compose ps
sudo docker compose logs --tail=100 relay caddy p2p_infra turn

# targeted restart
sudo docker compose restart relay
sudo docker compose up -d --force-recreate caddy

# public smoke test
bash scripts/relay_smoke_test.sh https://fedi3.redhunt07.ovh "$(grep ADMIN_TOKEN relay-secrets.txt | cut -d= -f2)"
```

Quick checks:

```bash
curl -fsS -H "Authorization: Bearer <ADMIN_TOKEN>" \
  https://fedi3.redhunt07.ovh/healthz

curl -fsS -H "Authorization: Bearer <ADMIN_TOKEN>" \
  https://fedi3.redhunt07.ovh/_fedi3/relay/stats

curl -fsS -H 'Accept: application/activity+json' \
  https://fedi3.redhunt07.ovh/users/alice
```

## Backup

Daily backups are active with the script's retention:

- `03:15` PostgreSQL;
- `03:45` local media;
- destination `/home/rocky/backups`;
- log `/var/log/fedi3-backup.log`, with weekly logrotate.

```bash
sudo cat /etc/cron.d/fedi3-backups
ls -lh /home/rocky/backups
```

The backups are not encrypted: protect the directory or copy it to
encrypted storage. For a restore follow `docs/ops_backup_restore.md`.

## Maintenance

- `dnf-automatic.timer` applies security updates.
- The server timezone is `Europe/Rome`.
- Swap: 2 GB in `/swapfile` to avoid OOM with the stack's memory limits.
- The containers have `restart: unless-stopped`, healthcheck and log rotation.
- After a code update: `git pull --ff-only`, rebuild of the affected service,
  `docker compose up -d`, then smoke test.

## Known limits

- The relay is not yet behind a WAF/CDN: Caddy applies TLS, security headers
  and body limits, but there is no upstream DDoS protection.
- The certificate depends on the reachability of ports 80/443 during
  renewal.
- The relay is single-node: PostgreSQL and media are not replicated.
- Group calls use a P2P mesh; beyond ~5–6 participants it is worth
  evaluating an optional SFU.
