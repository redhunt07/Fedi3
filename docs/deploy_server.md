# Server installation (Debian and Arch derivatives)

`scripts/install_server.sh` installs a Fedi3 server in one pass and lets you
choose what it runs:

| Mode | Services | ActivityPub federation | Public domain and TLS | Public ports |
| :--- | :--- | :--- | :--- | :--- |
| `full` | relay, PostgreSQL, Dragonfly, Caddy, P2P infrastructure | yes | required | 80/tcp, 443/tcp, 4001/tcp, 4001/udp |
| `relay` | relay, PostgreSQL, Dragonfly, Caddy | yes | required | 80/tcp, 443/tcp |
| `p2p` | P2P infrastructure only (mailbox, Kademlia, circuit relay) | no | not required | 4001/tcp, 4001/udp |

Optional add-ons:

- **TURN** (`coturn`): relays audio/video media when a direct WebRTC path is
  impossible. It requires a public IPv4 and UDP `49152-65535`; add `3478/tcp`
  and `3478/udp`.
- **Meilisearch**: faster full-text search for the relay. The default search
  backend is PostgreSQL; Meilisearch needs about 1 GB of extra RAM.

The installer supports Debian and Ubuntu derivatives (Debian, Ubuntu, Linux
Mint, Pop!_OS, Zorin OS, Kali, Raspbian, Devuan, MX Linux, ...) and Arch
derivatives (Arch Linux, Manjaro, EndeavourOS, Garuda, CachyOS, ...). It
installs Docker from the official repositories on Debian-family systems and
from `pacman` on Arch-family systems.

## What each mode is for

- **full** is the recommended public server: it federates with classic
  ActivityPub instances and also runs the P2P infrastructure that keeps
  messages for offline peers and offers circuit-relay capacity.
- **relay** is for operators who only need federation and the relay tunnel.
  Clients can still connect directly to each other; there is no mailbox or
  circuit-relay node on this server.
- **p2p** is a federation-free bootstrap node. It is useful for a community
  that wants an independent rendezvous/mailbox point without exposing an
  ActivityPub domain or TLS certificate.

A server can be switched between modes later: the installer stops the old
containers, keeps the named volumes (relay database, media, P2P identity) and
starts the new selection.

## Requirements

- A fresh VPS or dedicated server with systemd and root/sudo access.
- Debian 12+ / Ubuntu 22.04+ (or a compatible derivative) with `apt`, or an
  Arch derivative with `pacman`.
- At least 1 GB RAM for `p2p`, 4 GB for `relay`, 6 GB recommended for `full`
  (the Compose file applies memory limits). At least 20 GB free disk for the
  first image build and the database/media volumes.
- For `full` and `relay`: a real domain with an A/AAAA record pointing to the
  server, ports 80 and 443 reachable from the Internet, and no other web server
  bound to those ports. Caddy obtains and renews the certificate through ACME.
- For `p2p`: a public IPv4 address and ports 4001/tcp and 4001/udp reachable.
- Open the same ports in the cloud provider's security group/firewall.

DNS must be configured **before** starting a relay mode; otherwise Caddy cannot
complete the ACME challenge. The installer warns when the domain does not
resolve to the detected public address, but it does not block the installation.

## Step-by-step: Debian and derivatives

```bash
# 1. Update the system and install git/curl
sudo apt update
sudo apt full-upgrade -y
sudo apt install -y git curl

# 2. Clone the repository into the deployment directory
sudo git clone https://github.com/redhunt07/Fedi3.git /opt/fedi3
cd /opt/fedi3

# 3. Run the installer
sudo bash scripts/install_server.sh
```

The installer asks, in order:

1. **Mode**: `1` relay + P2P, `2` relay only, `3` P2P only.
2. **Domain** for modes 1 and 2 (for example `relay.example.org`). For mode 3
   the domain is optional and can be left empty.
3. **TURN**: whether to install coturn. It defaults to yes for relay modes when
   a public IPv4 is detected.
4. **Meilisearch**: whether to use it instead of the PostgreSQL search backend.
5. **Seed relays**: optional comma-separated relay URLs for the HTTP relay mesh.
6. **Confirmation** of the plan.

Then it:

- adds the official Docker repository and installs Docker Engine, Buildx and
  the Compose plugin (or `docker`, `docker-compose` and `docker-buildx` on
  Arch);
- starts Docker and adds the invoking user to the `docker` group;
- detects the public IPv4 and checks DNS, ports, RAM and disk;
- writes `.env` and `relay-secrets.txt` with mode `600` and random secrets;
- selects the Compose profiles in `.env`, so `docker compose up -d` starts
  exactly the chosen mode;
- allows the required ports if `ufw` or `firewalld` is already active;
- builds the images, starts the stack and waits for the health checks;
- prints the relay URL, the P2P peer id/multiaddrs and the client settings.

The first Rust image build can take several minutes.

## Step-by-step: Arch and derivatives

```bash
# 1. Update the system and install git/curl
sudo pacman -Syu --needed git curl

# 2. Clone the repository into the deployment directory
sudo git clone https://github.com/redhunt07/Fedi3.git /opt/fedi3
cd /opt/fedi3

# 3. Run the installer
sudo bash scripts/install_server.sh
```

The prompts and the resulting stack are identical to the Debian path. The
installer uses `pacman` for Docker, Docker Compose and Buildx and enables
`docker.service`.

## Non-interactive installation

Useful for automation, Ansible or a cloud-init script:

```bash
# Full server (relay + P2P + TURN)
sudo bash scripts/install_server.sh \
  --mode full --domain relay.example.org --turn \
  --non-interactive --yes

# Relay only, no TURN
sudo bash scripts/install_server.sh \
  --mode relay --domain relay.example.org --no-turn \
  --non-interactive --yes

# P2P-only bootstrap node
sudo bash scripts/install_server.sh \
  --mode p2p --non-interactive --yes
```

Add `--public-ip 203.0.113.10` when public-IP detection fails (common behind
some NAT or VPN setups), `--seed-relays https://relay1.example,https://relay2.example`
to join a relay mesh, and `--search meili` to enable Meilisearch.

`--dry-run` writes `.env` and `relay-secrets.txt`, validates the Compose file
and prints the plan without starting containers.

## Generated files and secrets

- `.env` (mode `600`): Compose profiles, domain, database password, admin and
  telemetry tokens, TURN credentials, P2P external address and seed relays.
  A timestamped `.env.bak.*` copy is kept on every run.
- `relay-secrets.txt` (mode `600`): the values needed by clients and operators,
  including the admin token, telemetry token, PostgreSQL password, TURN
  credentials, Meilisearch key and the P2P peer id/multiaddrs after the first
  start.
- Named Docker volumes: relay database, media, Caddy certificates and P2P
  identity. They survive `docker compose down`; they are deleted only by
  `docker compose down -v`.

Re-running the installer preserves existing secrets unless `--rotate-secrets`
is passed. The admin token can also be set explicitly with `--admin-token`.

Do not commit `.env` or `relay-secrets.txt`; `.gitignore` already excludes
`.env` and `.env.*`.

## Verify the installation

Relay modes:

```bash
cd /opt/fedi3
sudo docker compose ps
curl -fsS https://relay.example.org/healthz
curl -fsS https://relay.example.org/readyz

# Admin metrics (token from relay-secrets.txt)
curl -fsS -H "Authorization: Bearer <ADMIN_TOKEN>" \
  https://relay.example.org/_fedi3/relay/metrics.prom | head

# Optional end-to-end smoke test
sudo bash scripts/relay_smoke_test.sh https://relay.example.org <ADMIN_TOKEN>
```

P2P modes:

```bash
cd /opt/fedi3
sudo docker compose ps p2p_infra
sudo docker compose logs --tail=100 p2p_infra
sudo grep FEDI3_P2P_PEER_ID relay-secrets.txt
sudo grep FEDI3_P2P_MULTIADDRS relay-secrets.txt
```

If Caddy does not obtain a certificate, check DNS, ports 80/443 and
`docker compose logs --tail=200 caddy`. For a local test with the domain
`localhost`, Caddy uses an internal CA: use `curl -k`.

## Connect the app

### Relay modes (`full` and `relay`)

Self-registration is disabled by default, so register the user before expecting
the relay tunnel to connect:

1. In onboarding use:
   - public base URL: `https://<your-domain>`
   - relay WebSocket: `wss://<your-domain>`
   - relay token: at least 16 characters (the app can generate one; keep it).
2. After the profile exists, open **Settings → Network → Relay admin** and paste
   the admin token from `relay-secrets.txt`.
3. Register the same username with the relay token used in onboarding. The
   admin token is only used for administration; the user token belongs to the
   account.
4. The tunnel connects after registration. If the relay is unreachable, check
   `wss://<your-domain>` and the registered token in **Settings → Security**.

### P2P bootstrap (`full` and `p2p`)

In the app go to **Settings → Network → Independent Fedi3 bootstrap nodes** and
add the multiaddrs printed by the installer (also stored in
`relay-secrets.txt` as `FEDI3_P2P_MULTIADDRS`), for example:

```text
/ip4/203.0.113.10/tcp/4001/p2p/12D3KooW...
/ip4/203.0.113.10/udp/4001/quic-v1/p2p/12D3KooW...
```

A relay-only server does not run a P2P infrastructure node; clients can still
use their own direct P2P connections or another bootstrap node.

### TURN

If TURN is enabled, use the values in `relay-secrets.txt` in the app's ICE
configuration:

```text
turn:<your-domain>:3478?transport=udp
turn:<your-domain>:3478?transport=tcp
user: relay
credential: <TURN_PASS>
```

STUN is not required when TURN is configured, but public STUN servers can still
help find a direct path.

## Operations

```bash
cd /opt/fedi3

# Status and logs
sudo docker compose ps
sudo docker compose logs --tail=200 relay
sudo docker compose logs --tail=200 caddy
sudo docker compose logs --tail=200 p2p_infra

# Restart one service
sudo docker compose restart relay

# Stop the stack (volumes and data are preserved)
sudo docker compose down

# Update the code and rebuild
sudo git pull --ff-only
sudo docker compose up -d --build
# or re-run the installer to refresh .env/profile selection:
sudo bash scripts/install_server.sh
```

Changing mode:

```bash
sudo bash scripts/install_server.sh --mode p2p --no-turn
```

The installer stops the containers of the previous mode and starts the new
one. Named volumes are preserved, so switching back restores the relay
database and media.

### Backups

The repository includes PostgreSQL and media backup scripts:

```bash
sudo bash scripts/backup_relay_pg.sh
sudo bash scripts/backup_relay_media.sh
```

For daily backups, add a cron file such as `/etc/cron.d/fedi3-backups`:

```cron
15 3 * * * root cd /opt/fedi3 && bash scripts/backup_relay_pg.sh
45 3 * * * root cd /opt/fedi3 && bash scripts/backup_relay_media.sh
```

The scripts apply retention (`BACKUP_KEEP`, default 7) and write to
`backups/` by default. Backups are not encrypted: protect the directory or copy
it to encrypted storage. See [ops_backup_restore.md](ops_backup_restore.md).

## Firewall

The installer configures `ufw` or `firewalld` only when one of them is already
active. Otherwise it prints the ports and the commands to run. Use
`--enable-firewall` to enable `ufw`/`firewalld` automatically; `ufw` is enabled
with SSH port 22 allowed, so verify your SSH port before using it.

Ports by mode:

- `full`: 80/tcp, 443/tcp, 4001/tcp, 4001/udp
- `relay`: 80/tcp, 443/tcp
- `p2p`: 4001/tcp, 4001/udp
- TURN add-on: 3478/tcp, 3478/udp, 49152-65535/udp

Always open the same ports in the cloud provider's security group. The relay
port 8787 is published only on `127.0.0.1` and does not need to be opened.

## Troubleshooting

| Symptom | Cause and fix |
| :--- | :--- |
| `Unsupported distribution` | The installer only supports Debian/Ubuntu and Arch derivatives. Install Docker manually and run with `--skip-docker-install`. |
| `Docker Compose >= 2.20 is required` | Upgrade `docker-compose-plugin` (Debian) or `docker-compose` (Arch); profiles and `required: false` dependencies need a recent Compose. |
| `apt-get update` fails after adding the Docker repository | Some derivatives report a codename Docker does not publish. The installer maps unknown Debian codenames to `bookworm`; if it still fails, use Docker's official instructions and re-run with `--skip-docker-install`. |
| DNS warning / ACME failure | Create the A/AAAA record first, wait for propagation, verify ports 80/443 are reachable and not used by another server. |
| Port already in use | Stop the existing web server or container bound to 80/443, then re-run the installer. |
| `permission denied` on Docker | Log out and back in after the installer adds your user to the `docker` group, or use `sudo docker`. |
| P2P node unhealthy | Check `docker compose logs p2p_infra`; do not delete the P2P identity volume. A public IPv4 is needed for reachable multiaddrs. |
| TURN unhealthy | Verify `TURN_EXTERNAL_IP` in `.env`, the public IPv4 and that the UDP range is open. |
| Secrets lost | They are in `relay-secrets.txt`; restore from a backup or rotate with `--rotate-secrets`. |

For relay-specific diagnostics see [deploy_relay.md](deploy_relay.md) and
[troubleshooting.md](troubleshooting.md).

## Security notes

- Keep `relay-secrets.txt` and `.env` at mode `600`; back them up separately
  from the database.
- Self-registration is disabled by default (`FEDI3_RELAY_ALLOW_SELF_REGISTER=false`):
  an administrator registers users from the app.
- The relay is reachable only through Caddy; PostgreSQL, Dragonfly and
  Meilisearch stay on the internal Docker network.
- Keep the host updated and rebuild after a `git pull`.
- TLS, HSTS and body limits are applied by Caddy; there is no upstream WAF/CDN
  by default.

## Installer reference

| Flag | Effect |
| :--- | :--- |
| `--mode full\|relay\|p2p` | Select the installation mode. |
| `--domain DOMAIN` | Public relay domain. Required for `full`/`relay`. |
| `--turn` / `--no-turn` | Enable or disable coturn. |
| `--search db\|meili` | Search backend. |
| `--seed-relays LIST` | Comma-separated relay URLs for the mesh. |
| `--public-ip IP` | Override public IPv4 detection. |
| `--dir PATH` | Deployment directory (default: the checkout, else `/opt/fedi3`). |
| `--admin-token TOKEN` | Reuse an admin token of at least 32 characters. |
| `--rotate-secrets` | Regenerate all secrets instead of reusing `.env`. |
| `--non-interactive` | Never prompt; omitted options use defaults. |
| `--yes` | Skip the confirmation prompt. |
| `--skip-docker-install` | Do not install Docker; fail if it is missing. |
| `--skip-firewall` | Do not touch the host firewall. |
| `--enable-firewall` | Enable `ufw`/`firewalld` if installed but inactive. |
| `--skip-dns-check` | Ignore DNS mismatch warnings. |
| `--dry-run` | Write `.env`/secrets and validate Compose without starting. |
