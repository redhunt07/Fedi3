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

### Backups

YunoHost's backup does not include `/opt/fedi3` or the Docker volumes by
default. Back up:

- `/opt/fedi3/.env` and `/opt/fedi3/relay-secrets.txt` (mode 600);
- the `relay_data` (media), `relay_pg_data` (PostgreSQL), `p2p_infra_data` and,
  if Caddy was ever used, `caddy_data`/`caddy_config` volumes;
- optionally the repository scripts/configuration.

The project's `scripts/backup_relay_pg.sh` and `scripts/backup_relay_media.sh`
work with the Compose stack; add them to cron or include the volumes in the
YunoHost backup plan.

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
