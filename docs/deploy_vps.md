# Deploy relay and P2P infrastructure on a VPS

This guide describes the active deployment on `fedi3.redhunt07.ovh`
(IPv4 `51.210.42.24`). It complements the automated
[server installation](deploy_server.md) and the manual
[relay configuration](deploy_relay.md).

The relay is reachable via HTTPS through Caddy; public DNS is proxied by
Cloudflare (managed WAF, TLS 1.2 minimum, HSTS), while the origin stays on
`51.210.42.24`. PostgreSQL, Dragonfly (Redis-compatible) and Meilisearch stay on
the internal Docker network. P2P and TURN are exposed publicly.

## DNS and TLS

- A record: `fedi3.redhunt07.ovh -> 51.210.42.24`.
- The certificate is issued and renewed automatically by Caddy (Let's Encrypt).
- AAAA record: `fedi3.redhunt07.ovh -> 2001:41d0:404:200::9721`, also proxied.
  Public resolvers see Cloudflare anycast addresses (for example
  `188.114.96.3` and `2a06:98c1:3120::3`), not the origin.
- The zone is proxied by Cloudflare (see [Security](#security)); the origin
  stays on `51.210.42.24` and is still reachable directly with SNI.
- `FEDI3_RELAY_BASE_DOMAIN` is empty: actors live on the main domain
  (`https://fedi3.redhunt07.ovh/users/<user>`). This way no wildcard DNS
  `*.fedi3.redhunt07.ovh` is needed.

## Stack

The deployment uses `docker-compose.yml` in the directory
`/home/rocky/fedi3` on the VPS:

- `caddy`: TLS and reverse proxy to the relay.
- `relay`: Fedi3 service and local media.
- `postgres`, `dragonfly`, `meilisearch`: internal backends (database,
  Redis-compatible cache and optional search).
- `p2p_infra`: libp2p node with relay server, Kademlia and mailbox.
- `turn`: coturn with credentials, external IP and UDP range.
- `push_gateway` and `ntfy` (profile `push`): self-hosted Web Push gateway and
  UnifiedPush/ntfy endpoint for background notifications.

Compose services are gated by profiles. This deployment sets
`COMPOSE_PROFILES=relay,caddy,p2p,turn,search,push` in `.env`; without that
value a plain `docker compose up -d` starts no service.

Ports open in `firewalld`: `22`, `80`, `443`, `4001/tcp`, `4001/udp`,
`3478/tcp`, `3478/udp`, `49152-65535/udp` (TURN), plus `4444/tcp` for the
support agent hub (see [Security](#security)). Only Caddy publishes 80/443;
`relay` listens on `127.0.0.1:8787` and PostgreSQL/Dragonfly/Meilisearch stay on
the internal Docker network. `rpcbind`/port 111 is disabled because NFS is not
used.

## Configuration and secrets

- Configuration: `/home/rocky/fedi3/.env` (permissions `600`).
- Application tokens: `/home/rocky/fedi3/relay-secrets.txt` (permissions `600`).
- Do not commit `.env` or `relay-secrets.txt`.
- The user `alice` is registered on the relay with the application token present in
  `relay-secrets.txt`; the local client uses the same token.
- TURN uses `relay` and the password in `relay-secrets.txt`; the local client has
  the same values in `~/.config/fedi3/config.json`.

## Security

The host runs Rocky Linux 9 with SELinux enforcing and `firewalld` active.
The current posture is verified live, not assumed:

- **SSH**: key-only. `PasswordAuthentication no`, `PermitRootLogin no`,
  `KbdInteractiveAuthentication no`, `GSSAPIAuthentication no`,
  `MaxAuthTries 3`, `LoginGraceTime 30`. The hardening lives in
  `/etc/ssh/sshd_config.d/00-fedi3-hardening.conf`; keep it alphabetically
  first so later drop-ins (`50-cloud-init.conf`, `50-redhat.conf`) cannot
  re-enable password login. The `rocky` user has passwordless sudo; root has
  no authorized key.
- **Login from a workstation**: the private key is passphrase-protected. Use
  the askpass helper (the passphrase is stored in
  `~/.ssh/askpass_fedi3.sh`, mode 600):
  ```bash
  export SSH_ASKPASS=$HOME/.ssh/askpass_fedi3.sh SSH_ASKPASS_REQUIRE=force DISPLAY=:0
  ssh -i ~/.ssh/id_ed25519_fedi3_vps_redhunt07 rocky@51.210.42.24
  ```
- **fail2ban**: installed and running, with **no SSH jail by design**. The
  login IP is dynamic and SSH already rejects passwords, so an IP ban would
  only lock the operator out. On this host the jail list is intentionally
  empty; brute force cannot succeed because only public keys are accepted.
  Relay HTTP endpoints are token-based, not password-based.
- **Firewall**: only the ports listed in [Stack](#stack) are open. The Docker
  socket is `root:docker 660`; no database port is published on the host.
- **Support agent hub**: `fedi3-agent-server` listens on `4444/tcp` for
  outbound agent connections and `127.0.0.1:4445` for the admin socket. The
  agent token is in `/etc/fedi3-agent.env` (mode 640 `root:rocky`). Traffic is
  plain TCP by design: agents connect from arbitrary user networks, and the
  token is verified on `HELLO`.
- **Updates**: `dnf-automatic-install.timer` applies security updates
  (`upgrade_type = security`, `apply_updates = yes`). `reboot = never`: after
  kernel/glibc updates run `sudo needs-restarting -r` and reboot in a chosen
  window. The relay restarts with the host.
- **TLS and CDN**: public traffic goes through Cloudflare — proxied A/AAAA,
  `ssl=strict`, `min_tls_version=1.2`, TLS 1.3, managed WAF rulesets (Free
  Managed Ruleset + DDoS L7) and HSTS preload enabled on the edge. The origin
  Caddy serves TLS 1.2/1.3 with a Let's Encrypt certificate; it is also
  reachable directly on `51.210.42.24:443`, which is useful for debugging but
  also a potential DDoS bypass if the IP leaks.
- **Cloudflare API token**: the token is valid. It is an **account-owned**
  token, so `GET /user/tokens/verify` answers 401 by design; verify it with
  `GET /accounts/1881cf0f52c1c5acabb570f924fca5c6/tokens/verify`
  (`status: active`). It carries DNS, zone settings, SSL, WAF and cache
  permissions.
- **Local testing caveat**: this workstation maps
  `51.210.42.24 fedi3.redhunt07.ovh` in `/etc/hosts`, so `curl`/`openssl`
  tests here hit the origin and skip Cloudflare. Remove that line or use
  `curl --resolve fedi3.redhunt07.ovh:443:188.114.96.3` to test the public
  path.

## Push notifications (optional)

The `push` Compose profile starts `push_gateway` and `ntfy`. This deployment
uses dedicated hostnames configured in `.env`:

```text
FEDI3_NTFY_DOMAIN=ntfy.redhunt07.ovh
FEDI3_PUSH_DOMAIN=push.redhunt07.ovh
FEDI3_RELAY_PUSH_GATEWAY_URL=https://push.redhunt07.ovh/push
FEDI3_RELAY_PUSH_GATEWAY_TOKEN=<at least 32 characters>
FEDI3_RELAY_PUSH_PROVIDERS=unifiedpush
FEDI3_PUSH_GATEWAY_PATH=/push
FEDI3_NTFY_BASE_URL=https://ntfy.redhunt07.ovh
```

The Caddyfile contains site blocks for `{$FEDI3_NTFY_DOMAIN}` and
`{$FEDI3_PUSH_DOMAIN}`. If the variables are unset the blocks fall back to
`*.invalid` and receive no traffic, so a deployment without push keeps working.
VAPID is optional for the plain ntfy transport; leave
`FEDI3_RELAY_PUSH_VAPID_PUBLIC_KEY` empty unless a Web Push distributor is used.

Verify the stack:

```bash
curl -fsS https://push.redhunt07.ovh/healthz
curl -fsS https://ntfy.redhunt07.ovh/v1/health
curl -fsS https://fedi3.redhunt07.ovh/_fedi3/relay/push-config
# end-to-end wake to an ntfy topic (token = FEDI3_RELAY_PUSH_GATEWAY_TOKEN)
curl -s -X POST -H "Authorization: Bearer <TOKEN>" -H 'content-type: application/json' \
  -d '{"provider":"unifiedpush","token":"https://ntfy.redhunt07.ovh/e2epush","event":"sync","ttl_secs":86400}' \
  https://push.redhunt07.ovh/push
curl -s "https://ntfy.redhunt07.ovh/e2epush/json?poll=1"
```

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
curl -fsS https://fedi3.redhunt07.ovh/healthz
curl -fsS https://fedi3.redhunt07.ovh/readyz
curl -fsS https://fedi3.redhunt07.ovh/_fedi3/relay/stats

curl -fsS -H "Authorization: Bearer <ADMIN_TOKEN>" \
  https://fedi3.redhunt07.ovh/_fedi3/relay/metrics.prom | head

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
# newest backup must be from today or yesterday
sudo ls -1t /home/rocky/backups | head -2
sudo tail -20 /var/log/fedi3-backup.log
```

The scripts must keep the executable bit, otherwise cron fails with
`Permission denied` while the cron entry still looks correct:

```bash
sudo chmod +x /home/rocky/fedi3/scripts/*.sh /home/rocky/fedi3/scripts/*.py
```

The cron entries invoke `bash ./scripts/backup_relay_*.sh` explicitly, so a
`git reset --hard` that restores the repository mode 644 cannot silently stop
backups again.

A silent backup gap happened on 2026-11-07 (scripts lost `+x` after a tree
update); always check the newest file date, not only the cron entry.

The backups are not encrypted: protect the directory or copy it to
encrypted storage. For a restore follow [ops_backup_restore.md](ops_backup_restore.md).

## Maintenance

- `dnf-automatic.timer` applies security updates.
- The server timezone is `Europe/Rome`.
- Swap: 2 GB in `/swapfile` to avoid OOM with the stack's memory limits.
- The containers have `restart: unless-stopped`, healthcheck and log rotation.
- After a code update: fetch the release commit, rebuild the affected service,
  `docker compose up -d`, then smoke test. Do not rely on a stale local HEAD:
  verify the deployed tree matches the intended release
  (`git log -1 --format='%h %s'`, `sha256sum crates/fedi3_relay/src/main.rs`).
- `deploy/relay/Dockerfile` must copy the `fedi3_push_gateway` crate: since
  0.12.0 it is a workspace member, and a Dockerfile without that `COPY` fails
  with `failed to read /src/crates/fedi3_push_gateway/Cargo.toml`.
- Verify the new relay answers `/_fedi3/account/<user>/state` with `401`
  (route present) instead of `404` (old build).

## Known limits

- The relay is behind Cloudflare for public DNS (proxied A/AAAA, managed WAF,
  TLS 1.2 minimum, HSTS preload). The origin IP `51.210.42.24` is still
  reachable directly, so it can be used to bypass the CDN.
- The certificate depends on the reachability of ports 80/443 during
  renewal.
- The relay is single-node: PostgreSQL and media are not replicated.
- Group calls use a P2P mesh; beyond ~5–6 participants it is worth
  evaluating an optional SFU.
