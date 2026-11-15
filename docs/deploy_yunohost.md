# Fedi3 relay on YunoHost

YunoHost already runs nginx on ports 80/443 and manages TLS certificates, so
the normal Fedi3 installer must not start its own Caddy instance. The wrapper
`scripts/install_yunohost.sh` runs the server installer with `--no-caddy`,
publishes the loopback relay through a YunoHost nginx location, bypasses the
YunoHost SSO for ActivityPub/WebSocket traffic, opens the P2P ports in the
YunoHost firewall and verifies the public HTTPS endpoint.

A `full` installation makes the server contribute to Fedi3 in two ways:

- the relay joins the HTTP relay mesh (`FEDI3_RELAY_SEED_RELAYS`) and stores
  posts, media and relay traffic for its users;
- `fedi3_p2p_infra` runs a libp2p node with mailbox storage, Kademlia and
  circuit-relay v2, so clients can use it as a bootstrap and keep messages for
  offline peers.

## Prerequisites

- A YunoHost server with root/sudo access.
- A dedicated subdomain, for example `relay.example.org`, added in YunoHost:
  `yunohost domain add relay.example.org`.
- A valid Let's Encrypt certificate for that domain:
  `yunohost domain cert install relay.example.org` (or the equivalent action in
  the YunoHost admin UI). The DNS record must already point to the server.
- No other YunoHost app installed at the root of that subdomain: Fedi3 needs
  `/` for ActivityPub, WebFinger, NodeInfo, media and the relay WebSocket.
- Docker and Docker Compose: the installer adds Docker's official Debian
  repository and installs the Compose plugin. Use `--skip-docker-install` if
  Docker is already installed and should not be touched.
- Public IPv4 reachable from the Internet, ports 4001/tcp and 4001/udp open,
  and optionally TURN ports.

## Install

```bash
sudo git clone https://github.com/redhunt07/Fedi3.git /opt/fedi3
cd /opt/fedi3

# Full node: relay + P2P infrastructure
sudo bash scripts/install_yunohost.sh --domain relay.example.org --mode full
```

The wrapper asks the same questions as the normal installer (TURN, search
backend, seed relays) and then:

1. writes `.env` with `COMPOSE_PROFILES=relay,p2p` and
   `FEDI3_EDGE_PROXY=external`;
2. starts relay, PostgreSQL, Dragonfly and `p2p_infra` without Caddy;
3. writes `/etc/nginx/conf.d/relay.example.org.d/fedi3.conf`;
4. runs `nginx -t` and reloads nginx;
5. opens 4001/tcp and 4001/udp in the YunoHost firewall;
6. checks `https://relay.example.org/healthz` and `/readyz`.

Non-interactive example with TURN and a relay seed:

```bash
sudo bash scripts/install_yunohost.sh \
  --domain relay.example.org --mode full \
  --seed-relays https://relay.example.net \
  --turn --non-interactive --yes
```

Useful options:

- `--mode relay`: relay only, no P2P infrastructure node.
- `--mode p2p`: P2P-only; no nginx/TLS is configured, only port 4001 is opened.
- `--seed-relays LIST`: HTTP relay mesh seeds. When they run a full node, the
  wrapper/installer also discovers their `/_fedi3/relay/p2p_infra` multiaddrs
  and sets `FEDI3_P2P_BOOTSTRAP`.
- `--p2p-bootstrap LIST`: explicit P2P multiaddrs of existing Fedi3 nodes.
- `--public-ip IP`: override public-IP detection.
- `--dir PATH`: deployment directory (default: the checkout, else `/opt/fedi3`).
- `--dry-run`: write `.env` and the nginx config, validate, but do not start or
  reload.
- `--skip-firewall`: do not modify the YunoHost firewall.

## Nginx and SSO

The generated location is:

```nginx
location / {
    # Fedi3 authenticates its own users and ActivityPub clients.
    access_by_lua_file /dev/null;

    proxy_pass http://127.0.0.1:8787;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_set_header X-Forwarded-Host $host;
    proxy_set_header X-Forwarded-Port $server_port;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection $connection_upgrade;
    proxy_read_timeout 3600s;
    proxy_send_timeout 3600s;
    client_body_timeout 3600s;
    client_max_body_size 64m;
    proxy_buffering off;
    proxy_request_buffering off;
    proxy_redirect off;
}
```

`access_by_lua_file /dev/null;` disables YunoHost SSO for this location: classic
ActivityPub servers and the app's WebSocket tunnel cannot log in through the
YunoHost portal. Fedi3 has its own admin/user tokens, so this is intentional.
Do not add `include conf.d/yunohost_panel.conf.inc;` here unless you want the
YunoHost user panel on Fedi3 pages.

The relay port 8787 is published only on `127.0.0.1`; do not expose it directly.

## Join the P2P/relay network

A full node is immediately usable as a bootstrap by clients that add its
multiaddrs, but joining the wider Fedi3 DHT requires at least one existing
peer. Add seed relays or explicit bootstrap multiaddrs:

```bash
sudo bash scripts/install_yunohost.sh \
  --domain relay.example.org --mode full \
  --seed-relays https://relay1.example.org,https://relay2.example.org
```

or edit `.env`:

```dotenv
FEDI3_RELAY_SEED_RELAYS=https://relay1.example.org,https://relay2.example.org
FEDI3_P2P_BOOTSTRAP=/ip4/203.0.113.10/tcp/4001/p2p/12D3KooW...,/ip4/203.0.113.10/udp/4001/quic-v1/p2p/12D3KooW...
```

then restart:

```bash
cd /opt/fedi3
sudo docker compose up -d
```

The node's own peer id and multiaddrs are printed by the installer and stored in
`relay-secrets.txt`:

```bash
sudo grep FEDI3_P2P_PEER_ID relay-secrets.txt
sudo grep FEDI3_P2P_MULTIADDRS relay-secrets.txt
```

Share those multiaddrs with other operators so they can use this node as a
bootstrap. The HTTP relay mesh distributes posts/media between relays; the
libp2p node handles chat mailbox and circuit relay for clients.

## Verify

```bash
cd /opt/fedi3
sudo docker compose ps
curl -fsS https://relay.example.org/healthz
curl -fsS https://relay.example.org/readyz

# Public relay statistics
curl -fsS https://relay.example.org/_fedi3/relay/stats

# Admin metrics
TOKEN=$(sudo grep '^FEDI3_RELAY_ADMIN_TOKEN=' relay-secrets.txt | cut -d= -f2-)
curl -fsS -H "Authorization: Bearer $TOKEN" \
  https://relay.example.org/_fedi3/relay/metrics.prom | head

# P2P identity
sudo grep -E '^FEDI3_P2P_(PEER_ID|MULTIADDRS|BOOTSTRAP)=' relay-secrets.txt .env
```

`nginx -t` and `systemctl status nginx` help when the public endpoint fails.
A 502 means the relay container is down; check
`sudo docker compose logs --tail=200 relay`. A redirect to the YunoHost portal
means the SSO bypass is missing or the location is shadowed by another app at
`/`.

## Client configuration

Relay modes:

1. Onboarding: public base `https://relay.example.org`, relay WebSocket
   `wss://relay.example.org`, and a user relay token of at least 16 characters.
2. **Settings → Network → Relay admin**: paste the admin token from
   `relay-secrets.txt` and register the same username with the user token.
3. The tunnel connects after registration.

P2P bootstrap:

- **Settings → Network → Independent Fedi3 bootstrap nodes**: add the
  multiaddrs from `relay-secrets.txt`.

TURN, if enabled:

```text
turn:relay.example.org:3478?transport=udp
turn:relay.example.org:3478?transport=tcp
user: relay
credential: <TURN_PASS>
```

## Operations

```bash
cd /opt/fedi3

sudo docker compose ps
sudo docker compose logs --tail=200 relay
sudo docker compose logs --tail=200 p2p_infra
sudo docker compose down          # volumes preserved
sudo docker compose up -d --build # update/rebuild
```

Re-run the wrapper after changing mode, domain, TURN or seed relays. It keeps
existing secrets unless `--rotate-secrets` is passed. Switching mode stops the
previous containers and preserves the named volumes.

### Security

The reference deployment (`fedi3.palermo.nohost.me`) runs Debian 12 with
YunoHost and its generated nftables firewall. Verified posture:

- **SSH is not exposed directly**: the YunoHost host sits behind a dynamic
  public IP, so access goes through the reverse tunnel opened by
  `fedi3-tunnel.service` on the support VPS. From the VPS:
  ```bash
  ssh -i /home/rocky/.ssh/fedi3_support -p 2223 panormus@127.0.0.1
  ```
  From a workstation the same hop is:
  ```bash
  export SSH_ASKPASS=$HOME/.ssh/askpass_fedi3.sh SSH_ASKPASS_REQUIRE=force DISPLAY=:0
  ssh -i ~/.ssh/id_ed25519_fedi3_vps_redhunt07 rocky@51.210.42.24 \
    'ssh -i /home/rocky/.ssh/fedi3_support -p 2223 panormus@127.0.0.1'
  ```
  SSH listens on port 2200, `PermitRootLogin no`, `MaxAuthTries 3`,
  `LoginGraceTime 30`. Password authentication stays enabled for the `panormus`
  user (the support key is also authorized); keep the password strong because
  the tunnel is the only ingress.
- **fail2ban**: active with 11 jails (`yunohost`, `recidive`, `postfix`,
  `dovecot`, `nginx-http-auth`, `sasl`, `pam-generic`, `yunohost-portal`,
  `immich`, `bonfire`, `friendica`). The `sshd` jail is disabled on purpose:
  tunneled logins appear as `127.0.0.1` and a dynamic-IP ban would lock the
  operator out. Other services stay protected.
- **TLS**: nginx serves TLS 1.2/1.3 only (`ssl_protocols TLSv1.2 TLSv1.3;` in
  `/etc/nginx/nginx.conf`) with HSTS, CSP `upgrade-insecure-requests`,
  `X-Content-Type-Options`, `X-Frame-Options` and `Referrer-Policy`. Keep
  `/etc/nginx/nginx.conf` in mind on YunoHost upgrades: re-check the
  `ssl_protocols` line if a package update restores the old template.
- **Updates**: `unattended-upgrades` is enabled and active; Debian security
  updates are applied automatically. YunoHost apps and the core are upgraded
  with `yunohost tools upgrade --system` in a chosen window.
- **Secrets**: `/opt/fedi3/.env` is mode 600 (`panormus`), and
  `/opt/fedi3/relay-secrets.txt` is mode 600. The relay port 8787 is published
  on `127.0.0.1` only; PostgreSQL, Dragonfly and TURN are internal.

### Push notifications (same-domain layout)

The reference deployment runs the `push` Compose profile and mounts both
services on paths of the relay domain: ntfy under `/ntfy` and the gateway under
`/_fedi3/push`. Add to `.env`:

```text
COMPOSE_PROFILES=relay,p2p,turn,push
FEDI3_RELAY_PUSH_GATEWAY_URL=https://<domain>/_fedi3/push
FEDI3_RELAY_PUSH_GATEWAY_TOKEN=<at least 32 characters>
FEDI3_RELAY_PUSH_PROVIDERS=unifiedpush
FEDI3_RELAY_PUSH_VAPID_PUBLIC_KEY=
FEDI3_PUSH_GATEWAY_PATH=/_fedi3/push
FEDI3_PUSH_GATEWAY_VAPID_SUBJECT=mailto:admin@example.org
FEDI3_PUSH_GATEWAY_VAPID_PRIVATE_KEY=
FEDI3_NTFY_BASE_URL=https://<domain>
```

`FEDI3_NTFY_BASE_URL` must stay at the host root: ntfy rejects a base URL that
contains a path (`/ntfy`). The nginx location still serves ntfy under `/ntfy`,
and the mobile distributor is configured with that server URL explicitly.

`docker-compose.override.yml` publishes the two services on loopback for nginx
and gives the relay and gateway public resolvers:

```yaml
services:
  relay:
    dns:
      - "9.9.9.9"
      - "1.1.1.1"
  push_gateway:
    dns:
      - "9.9.9.9"
      - "1.1.1.1"
    ports:
      - "127.0.0.1:8090:8090"
  ntfy:
    ports:
      - "127.0.0.1:8080:80"
```

YunoHost's local resolver maps its own domains to loopback (`::1`). The relay
validates push endpoints as public HTTPS (`ensure_fetch_target_allowed`) and the
gateway's SSRF guard rejects private addresses, so both containers need public
resolvers (or an `/etc/hosts` entry with the public IP).

nginx locations in `/etc/nginx/conf.d/<domain>.d/fedi3.conf`, before
`location /`:

```nginx
location = /_fedi3/push {
    access_by_lua_file /dev/null;
    proxy_pass http://127.0.0.1:8090/_fedi3/push;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_read_timeout 60s;
}
location = /_fedi3/push/healthz {
    access_by_lua_file /dev/null;
    proxy_pass http://127.0.0.1:8090/healthz;
    proxy_set_header Host $host;
}
location /ntfy/ {
    access_by_lua_file /dev/null;
    proxy_pass http://127.0.0.1:8080/;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection $connection_upgrade;
    proxy_buffering off;
    proxy_read_timeout 3600s;
}
```

Verify:

```bash
curl -fsS https://<domain>/_fedi3/relay/push-config   # enabled=true, providers
curl -fsS https://<domain>/_fedi3/push/healthz        # ok
curl -fsS https://<domain>/ntfy/v1/health             # healthy
```

Register an installation with the account Bearer token:

```bash
curl -X PUT -H "Authorization: Bearer <USER_TOKEN>" -H 'content-type: application/json' \
  -d '{"provider":"unifiedpush","token":"https://<domain>/ntfy/<topic>","expires_at_ms":<ms>}' \
  https://<domain>/_fedi3/relay/push/<user>
```

Send a wake through the gateway (token = `FEDI3_RELAY_PUSH_GATEWAY_TOKEN`):

```bash
curl -X POST -H "Authorization: Bearer <GATEWAY_TOKEN>" -H 'content-type: application/json' \
  -d '{"provider":"unifiedpush","token":"https://<domain>/ntfy/<topic>","event":"sync","ttl_secs":86400}' \
  https://<domain>/_fedi3/push
curl -s "https://<domain>/ntfy/<topic>/json?poll=1"   # {"event":"sync"} delivered
```

### Backupss

YunoHost's backup does not include `/opt/fedi3` or the Docker volumes by
default. Back up:

- `/opt/fedi3/.env` and `/opt/fedi3/relay-secrets.txt` (mode 600);
- the `relay_data` (media), `relay_pg_data` (PostgreSQL), `p2p_infra_data` and,
  if Caddy was ever used, `caddy_data`/`caddy_config` volumes;
- optionally the repository scripts/configuration.

The project's `scripts/backup_relay_pg.sh` and `scripts/backup_relay_media.sh`
work with the Compose stack. The reference deployment schedules them with
`/etc/cron.d/fedi3-backups`:

```bash
sudo mkdir -p /var/backups/fedi3-relay
sudo chmod 700 /var/backups/fedi3-relay
sudo tee /etc/cron.d/fedi3-backups >/dev/null <<'EOF'
SHELL=/bin/bash
PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
15 3 * * * root cd /opt/fedi3 && bash ./scripts/backup_relay_pg.sh /var/backups/fedi3-relay >> /var/log/fedi3-backup.log 2>&1
45 3 * * * root cd /opt/fedi3 && bash ./scripts/backup_relay_media.sh /var/backups/fedi3-relay >> /var/log/fedi3-backup.log 2>&1
EOF
sudo chmod +x /opt/fedi3/scripts/*.sh /opt/fedi3/scripts/*.py
```

Keep the executable bit on the scripts: without it cron logs
`Permission denied` and no backup is written. Verify the newest file date:

```bash
sudo ls -1t /var/backups/fedi3-relay | head -2
sudo tail -20 /var/log/fedi3-backup.log
```

Include `/opt/fedi3/.env`, `relay-secrets.txt` and the named volumes in the
YunoHost backup plan as well.

## Troubleshooting

| Symptom | Fix |
| :--- | :--- |
| `Domain ... is not configured in YunoHost` | Add the subdomain first: `yunohost domain add relay.example.org`. |
| Certificate warning | Install the certificate: `yunohost domain cert install relay.example.org`; check DNS. |
| Another app serves `/` | Use a dedicated subdomain or remove/move the app from the root. |
| nginx test fails | Restore the `.bak.*` file in `/etc/nginx/conf.d/<domain>.d/`; check `nginx -t`. |
| Redirect to YunoHost portal | Ensure `access_by_lua_file /dev/null;` is present in `fedi3.conf`. |
| 502 Bad Gateway | Relay container down or unhealthy: `docker compose logs relay`. |
| WebSocket fails | Ensure `proxy_http_version 1.1`, `Upgrade`/`Connection` headers and long timeouts are present. |
| Ports blocked | Check `yunohost firewall list`; allow 4001 and TURN ports; also check the provider firewall. |
| P2P node isolated | Add `FEDI3_P2P_BOOTSTRAP` multiaddrs or seed relays; verify 4001 reachability. |

## Uninstall

```bash
cd /opt/fedi3
sudo docker compose down -v   # -v deletes the named volumes: back up first
sudo rm -f /etc/nginx/conf.d/<domain>.d/fedi3.conf
sudo nginx -t && sudo systemctl reload nginx
```

Optionally close the ports with `yunohost firewall disallow Both 4001` and the
TURN ports.
