#!/usr/bin/env bash
# Fedi3 relay installer for YunoHost.
#
# YunoHost already owns ports 80/443 and terminates TLS through its nginx.
# This wrapper runs the regular installer with --no-caddy, exposes the relay
# through a YunoHost nginx location, opens the P2P ports in the YunoHost
# firewall and verifies the public HTTPS endpoint.
#
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
IFS=$'\n\t'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALLER="$SCRIPT_DIR/install_server.sh"

DOMAIN=""
MODE="full"
TURN_CHOICE=""
SEARCH_CHOICE=""
SEED_RELAYS=""
P2P_BOOTSTRAP=""
PUBLIC_IP=""
ADMIN_TOKEN=""
DEPLOY_DIR_ARG=""
NON_INTERACTIVE=0
ASSUME_YES=0
SKIP_DOCKER_INSTALL=0
SKIP_FIREWALL=0
ROTATE_SECRETS=0
DRY_RUN=0

usage() {
  cat <<'EOF'
Usage: sudo bash scripts/install_yunohost.sh --domain relay.example.org [options]

YunoHost prerequisites:
  - the domain must already exist in YunoHost (yunohost domain add ...)
  - install a Let's Encrypt certificate for it (yunohost domain cert install ...)
  - use a dedicated subdomain: Fedi3 needs the whole domain root

Options:
  --domain DOMAIN         YunoHost domain for the relay (required for relay/full)
  --mode full|relay|p2p   full = relay + P2P, relay = relay only, p2p = P2P only
                          (default: full)
  --turn / --no-turn      install coturn for WebRTC calls
  --search db|meili       search backend (default: db)
  --seed-relays LIST      comma-separated relay URLs for the HTTP relay mesh
  --p2p-bootstrap LIST    comma-separated P2P multiaddrs of existing nodes
  --public-ip IP          public IPv4 (auto-detected when omitted)
  --admin-token TOKEN     reuse an admin token (>= 32 chars)
  --dir PATH              deployment directory (default: the checkout, else /opt/fedi3)
  --rotate-secrets        regenerate secrets instead of reusing .env values
  --non-interactive       never prompt
  --yes                   skip the confirmation prompt
  --skip-docker-install   do not install Docker (fail if missing)
  --skip-firewall         do not touch the YunoHost firewall
  --dry-run               write .env/nginx and validate, do not start or reload
  -h, --help              show this help

Examples:
  sudo bash scripts/install_yunohost.sh --domain relay.example.org
  sudo bash scripts/install_yunohost.sh --domain relay.example.org --mode full \
    --seed-relays https://relay1.example.org --turn --non-interactive --yes
EOF
}

die() {
  printf 'ERROR: %s\n' "$*" >&2
  exit 1
}

warn() {
  printf 'WARN: %s\n' "$*" >&2
}

info() {
  printf '==> %s\n' "$*"
}

need_arg() {
  [[ $# -ge 2 && -n "${2:-}" ]] || die "Option $1 requires a value"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --domain) need_arg "$@"; DOMAIN="$2"; shift 2 ;;
    --domain=*) DOMAIN="${1#*=}"; shift ;;
    --mode) need_arg "$@"; MODE="$2"; shift 2 ;;
    --mode=*) MODE="${1#*=}"; shift ;;
    --turn) TURN_CHOICE=yes; shift ;;
    --no-turn) TURN_CHOICE=no; shift ;;
    --search) need_arg "$@"; SEARCH_CHOICE="$2"; shift 2 ;;
    --search=*) SEARCH_CHOICE="${1#*=}"; shift ;;
    --seed-relays) need_arg "$@"; SEED_RELAYS="$2"; shift 2 ;;
    --seed-relays=*) SEED_RELAYS="${1#*=}"; shift ;;
    --p2p-bootstrap) need_arg "$@"; P2P_BOOTSTRAP="$2"; shift 2 ;;
    --p2p-bootstrap=*) P2P_BOOTSTRAP="${1#*=}"; shift ;;
    --public-ip) need_arg "$@"; PUBLIC_IP="$2"; shift 2 ;;
    --public-ip=*) PUBLIC_IP="${1#*=}"; shift ;;
    --admin-token) need_arg "$@"; ADMIN_TOKEN="$2"; shift 2 ;;
    --admin-token=*) ADMIN_TOKEN="${1#*=}"; shift ;;
    --dir) need_arg "$@"; DEPLOY_DIR_ARG="$2"; shift 2 ;;
    --dir=*) DEPLOY_DIR_ARG="${1#*=}"; shift ;;
    --rotate-secrets) ROTATE_SECRETS=1; shift ;;
    --non-interactive) NON_INTERACTIVE=1; shift ;;
    --yes|-y) ASSUME_YES=1; shift ;;
    --skip-docker-install) SKIP_DOCKER_INSTALL=1; shift ;;
    --skip-firewall) SKIP_FIREWALL=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *)
      case "$1" in
        full|relay|p2p) MODE="$1"; shift ;;
        *) die "Unknown option: $1 (use --help)" ;;
      esac
      ;;
  esac
done

case "$MODE" in
  full|relay+p2p|relay_p2p|both) MODE=full ;;
  relay|relay-only|relay_only) MODE=relay ;;
  p2p|p2p-only|p2p_only) MODE=p2p ;;
  *) die "Invalid --mode '$MODE' (expected full, relay or p2p)" ;;
esac

case "$SEARCH_CHOICE" in
  ""|db) SEARCH_CHOICE=db ;;
  meili|meilisearch) SEARCH_CHOICE=meili ;;
  *) die "Invalid --search '$SEARCH_CHOICE' (expected db or meili)" ;;
esac

[[ -f "$INSTALLER" ]] || die "$INSTALLER not found"

if [[ $EUID -ne 0 ]] && ! command -v sudo >/dev/null 2>&1; then
  die "sudo is required when the wrapper is not run as root"
fi

as_root() {
  if [[ $EUID -eq 0 ]]; then
    "$@"
  else
    sudo "$@"
  fi
}

INTERACTIVE=1
if [[ $NON_INTERACTIVE -eq 1 || ! -r /dev/tty ]]; then
  INTERACTIVE=0
fi

normalize_domain() {
  local d="$1"
  d="${d#http://}"
  d="${d#https://}"
  d="${d%%/*}"
  d="${d,,}"
  d="${d%.}"
  printf '%s' "$d"
}

validate_domain() {
  local d="$1"
  [[ -n "$d" && ${#d} -le 253 ]] || return 1
  [[ "$d" =~ ^[A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?$ ]] || return 1
  [[ "$d" != *".."* && "$d" != .* && "$d" != *.-* && "$d" != *-. ]] || return 1
  return 0
}

if [[ -n "$DOMAIN" ]]; then
  DOMAIN="$(normalize_domain "$DOMAIN")"
fi

if [[ "$MODE" == "p2p" ]]; then
  if [[ -n "$DOMAIN" ]] && ! validate_domain "$DOMAIN"; then
    warn "Ignoring invalid domain '$DOMAIN' for a P2P-only installation"
    DOMAIN=""
  fi
else
  if [[ -z "$DOMAIN" ]]; then
    [[ $INTERACTIVE -eq 1 ]] || die "--domain is required for --mode $MODE"
    read -r -p "YunoHost domain for the relay (e.g. relay.example.org): " DOMAIN
    DOMAIN="$(normalize_domain "$DOMAIN")"
  fi
  validate_domain "$DOMAIN" || die "Invalid domain: $DOMAIN"
fi

command -v yunohost >/dev/null 2>&1 || die "YunoHost not found (yunohost command missing)"

if [[ "$MODE" != "p2p" ]]; then
  [[ -f "/etc/nginx/conf.d/${DOMAIN}.conf" ]] || \
    die "Domain $DOMAIN is not configured in YunoHost; run: yunohost domain add $DOMAIN"

  if ! as_root yunohost domain list 2>/dev/null | awk '{print $1}' | grep -qx "$DOMAIN"; then
    warn "Domain $DOMAIN is not listed by 'yunohost domain list'; verify it in the YunoHost admin"
  fi

  if [[ ! -f "/etc/yunohost/certs/${DOMAIN}/crt.pem" ]]; then
    warn "No certificate found for $DOMAIN; install Let's Encrypt first:"
    warn "  yunohost domain cert install $DOMAIN"
  fi
fi

# Resolve the deployment directory the same way install_server.sh does.
if [[ -n "$DEPLOY_DIR_ARG" ]]; then
  DEPLOY_DIR="$DEPLOY_DIR_ARG"
elif [[ -f "$SCRIPT_DIR/../docker-compose.yml" ]]; then
  DEPLOY_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
else
  DEPLOY_DIR="/opt/fedi3"
fi

install_args=("--mode=$MODE" --no-caddy --skip-firewall)
[[ -n "$DOMAIN" ]] && install_args+=("--domain=$DOMAIN")
case "$TURN_CHOICE" in
  yes) install_args+=(--turn) ;;
  no) install_args+=(--no-turn) ;;
esac
[[ -n "$SEARCH_CHOICE" ]] && install_args+=(--search "$SEARCH_CHOICE")
[[ -n "$SEED_RELAYS" ]] && install_args+=(--seed-relays "$SEED_RELAYS")
[[ -n "$P2P_BOOTSTRAP" ]] && install_args+=(--p2p-bootstrap "$P2P_BOOTSTRAP")
[[ -n "$PUBLIC_IP" ]] && install_args+=(--public-ip "$PUBLIC_IP")
[[ -n "$ADMIN_TOKEN" ]] && install_args+=(--admin-token "$ADMIN_TOKEN")
[[ -n "$DEPLOY_DIR_ARG" ]] && install_args+=(--dir "$DEPLOY_DIR_ARG")
[[ $ROTATE_SECRETS -eq 1 ]] && install_args+=(--rotate-secrets)
[[ $NON_INTERACTIVE -eq 1 ]] && install_args+=(--non-interactive)
[[ $ASSUME_YES -eq 1 ]] && install_args+=(--yes)
[[ $SKIP_DOCKER_INSTALL -eq 1 ]] && install_args+=(--skip-docker-install)
[[ $DRY_RUN -eq 1 ]] && install_args+=(--dry-run)
# P2P-only has no domain/TLS prompt; keep it fully non-interactive.
[[ "$MODE" == "p2p" ]] && install_args+=(--non-interactive)

info "Running the Fedi3 installer without Caddy (YunoHost nginx will terminate TLS)"
as_root bash "$INSTALLER" "${install_args[@]}"

# The installer wrote .env; use the profiles it selected for the firewall.
ENV_FILE="$DEPLOY_DIR/.env"
SECRETS_FILE="$DEPLOY_DIR/relay-secrets.txt"
PROFILES="$(grep -E '^COMPOSE_PROFILES=' "$ENV_FILE" 2>/dev/null | cut -d= -f2- || true)"

if [[ "$MODE" != "p2p" ]]; then
  NGINX_DIR="/etc/nginx/conf.d/${DOMAIN}.d"
  NGINX_CONF="$NGINX_DIR/fedi3.conf"

  # Fedi3 needs the whole domain root; refuse to shadow another app at /.
  if [[ -d "$NGINX_DIR" ]]; then
    for file in "$NGINX_DIR"/*.conf; do
      [[ -e "$file" ]] || continue
      [[ "$file" == "$NGINX_CONF" ]] && continue
      if grep -qE '^[[:space:]]*location[[:space:]]+/[[:space:]]*\{' "$file"; then
        die "Another app already serves / on $DOMAIN ($file); use a dedicated subdomain for Fedi3"
      fi
    done
  fi

  info "Configuring the YunoHost nginx reverse proxy for $DOMAIN"
  as_root mkdir -p "$NGINX_DIR"
  nginx_tmp="$(mktemp)"
  nginx_lua=0
  if nginx -V 2>&1 | grep -qi lua \
    || nginx -T 2>/dev/null | grep -qi lua \
    || [[ -f /usr/lib/nginx/modules/ngx_http_lua_module.so ]] \
    || [[ -f /usr/share/nginx/modules/ngx_http_lua_module.so ]]; then
    nginx_lua=1
  fi
  {
    cat <<'EOF'
# Fedi3 relay reverse proxy for YunoHost.
# Managed by scripts/install_yunohost.sh; do not edit by hand.
location / {
EOF
    if [[ $nginx_lua -eq 1 ]]; then
      cat <<'EOF'
    # Fedi3 has its own authentication: bypass YunoHost SSO for ActivityPub
    # clients, media and the relay WebSocket tunnel.
    access_by_lua_file /dev/null;

EOF
    fi
    cat <<'EOF'
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

    # The relay tunnel and long-lived media transfers need long timeouts.
    proxy_read_timeout 3600s;
    proxy_send_timeout 3600s;
    client_body_timeout 3600s;
    client_max_body_size 64m;
    proxy_buffering off;
    proxy_request_buffering off;
    proxy_redirect off;
}
EOF
  } > "$nginx_tmp"

  nginx_backup=""
  nginx_changed=0
  if [[ -f "$NGINX_CONF" ]] && cmp -s "$nginx_tmp" "$NGINX_CONF"; then
    info "nginx reverse proxy already up to date"
  else
    if [[ -f "$NGINX_CONF" ]]; then
      nginx_backup="${NGINX_CONF}.bak.$(date -u +%Y%m%dT%H%M%SZ)"
      as_root cp -p "$NGINX_CONF" "$nginx_backup"
    fi
    as_root install -m 0644 "$nginx_tmp" "$NGINX_CONF"
    nginx_changed=1
    info "nginx reverse proxy written to $NGINX_CONF"
  fi
  rm -f "$nginx_tmp"

  if ! as_root nginx -t; then
    if [[ -n "$nginx_backup" ]]; then
      as_root cp -p "$nginx_backup" "$NGINX_CONF"
    else
      as_root rm -f "$NGINX_CONF"
    fi
    die "nginx configuration test failed; previous configuration restored"
  fi

  if [[ $nginx_changed -eq 1 && $DRY_RUN -eq 0 ]]; then
    if as_root systemctl reload nginx; then
      info "nginx reloaded"
    else
      warn "Could not reload nginx; run: systemctl reload nginx"
    fi
  fi
fi

open_firewall_port() {
  local protocol="$1" port="$2"
  if ! as_root yunohost firewall allow --no-upnp "$protocol" "$port" >/dev/null 2>&1; then
    warn "Could not open $protocol $port in the YunoHost firewall"
  fi
}

if [[ $SKIP_FIREWALL -eq 0 && $DRY_RUN -eq 0 ]]; then
  if [[ "$MODE" != "relay" ]]; then
    open_firewall_port Both 4001
  fi
  if [[ "$PROFILES" == *turn* ]]; then
    open_firewall_port Both 3478
    open_firewall_port UDP 49152:65535
  fi
  info "YunoHost firewall ports updated"
fi

verify_https() {
  local deadline=$((SECONDS + 90)) curl_opts=(-fsS -m 5)
  case "$DOMAIN" in
    localhost|*.localhost|127.*) curl_opts+=(-k) ;;
  esac
  while (( SECONDS < deadline )); do
    if curl "${curl_opts[@]}" "https://$DOMAIN/healthz" >/dev/null 2>&1; then
      info "Public HTTPS health check passed: https://$DOMAIN/healthz"
      if curl "${curl_opts[@]}" "https://$DOMAIN/readyz" >/dev/null 2>&1; then
        info "Readiness check passed: https://$DOMAIN/readyz"
      else
        warn "Readiness endpoint did not answer yet"
      fi
      return 0
    fi
    sleep 3
  done
  warn "Public HTTPS health check failed for https://$DOMAIN/healthz"
  warn "Check: nginx -t; systemctl status nginx; docker compose logs --tail=100 relay"
  return 1
}

if [[ "$MODE" != "p2p" && $DRY_RUN -eq 0 ]]; then
  verify_https || true
fi

# Summary: the installer already printed the full one; add YunoHost details.
if [[ "$MODE" != "p2p" ]]; then
  cat <<EOF

YunoHost integration:
  Domain:       https://$DOMAIN
  nginx config: $NGINX_CONF
  TLS:          YunoHost certificate (Let's Encrypt)
  Relay:        http://127.0.0.1:8787 (loopback only)
EOF
fi

if [[ "$MODE" != "relay" ]]; then
  peer_id="$(grep -E '^FEDI3_P2P_PEER_ID=' "$SECRETS_FILE" 2>/dev/null | cut -d= -f2- || true)"
  multiaddrs="$(grep -E '^FEDI3_P2P_MULTIADDRS=' "$SECRETS_FILE" 2>/dev/null | cut -d= -f2- || true)"
  cat <<EOF

P2P contribution:
  Peer id:      ${peer_id:-not available yet}
  Multiaddrs:   ${multiaddrs:-not available yet}
  Ports:        4001/tcp, 4001/udp (opened in the YunoHost firewall)
  Bootstrap:    $(grep -E '^FEDI3_P2P_BOOTSTRAP=' "$ENV_FILE" 2>/dev/null | cut -d= -f2- || true)
EOF
fi

if [[ "$MODE" != "p2p" ]]; then
  cat <<EOF

Next steps:
  - Add seed relays to join the HTTP relay mesh (and auto-discover P2P peers):
    sudo bash scripts/install_yunohost.sh --domain $DOMAIN --seed-relays https://relay1.example.org
  - Client onboarding: https://$DOMAIN, wss://$DOMAIN, user relay token.
  - Settings -> Network -> Relay admin: admin token from
    $SECRETS_FILE; register the same username with the user token.
  - YunoHost backups do not include Docker volumes by default: back up
    $DEPLOY_DIR and the fedi3 Docker volumes separately.
EOF
fi

if [[ $DRY_RUN -eq 1 ]]; then
  printf '\nDry run: containers were not started and nginx was not reloaded.\n'
fi
