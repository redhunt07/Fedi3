#!/usr/bin/env bash
# Fedi3 server installer for Debian derivatives (Debian, Ubuntu, Mint, Pop!_OS,
# Zorin, Kali, ...) and Arch derivatives (Arch, Manjaro, EndeavourOS, ...).
#
# It installs Docker, writes .env and relay-secrets.txt, configures an already
# active host firewall, selects the requested Compose profile and starts the
# stack. Supported modes:
#   full   relay + P2P infrastructure
#   relay  relay only
#   p2p    P2P infrastructure only
# TURN (coturn) and Meilisearch are optional add-ons. Caddy terminates TLS by
# default; use --no-caddy when an existing reverse proxy (YunoHost, nginx)
# already owns ports 80/443.
#
# SPDX-License-Identifier: AGPL-3.0-only
set -euo pipefail
IFS=$'\n\t'

REPO_URL="${FEDI3_REPO_URL:-https://github.com/redhunt07/Fedi3.git}"

# --------------------------------------------------------------------------- args
MODE=""
DOMAIN=""
TURN_CHOICE=""
SEARCH_CHOICE=""
SEARCH_SET=0
SEED_RELAYS=""
DEPLOY_DIR_ARG=""
NON_INTERACTIVE=0
ASSUME_YES=0
SKIP_DOCKER_INSTALL=0
SKIP_FIREWALL=0
SKIP_DNS_CHECK=0
ENABLE_FIREWALL=0
ROTATE_SECRETS=0
DRY_RUN=0
ADMIN_TOKEN_OVERRIDE=""
PUBLIC_IP_OVERRIDE=""
CADDY_ENABLED=1
CADDY_SET=0
P2P_BOOTSTRAP=""

usage() {
  cat <<'EOF'
Usage: sudo bash scripts/install_server.sh [options]

Installation mode (required in non-interactive runs):
  --mode full|relay|p2p   full = relay + P2P, relay = relay only, p2p = P2P only
  --domain DOMAIN         public relay domain (required for relay/full; optional for p2p)
  --turn / --no-turn      install coturn for WebRTC calls (default: ask; no when non-interactive)
  --search db|meili       search backend (default: db)
  --seed-relays LIST      comma-separated relay URLs for the HTTP relay mesh
  --p2p-bootstrap LIST    comma-separated P2P multiaddrs of existing nodes
                          (auto-discovered from --seed-relays when omitted)
  --public-ip IP          public IPv4 (auto-detected when omitted)

Other options:
  --dir PATH              deployment directory (default: the checkout, else /opt/fedi3)
  --admin-token TOKEN     reuse an admin token (>= 32 chars)
  --rotate-secrets        regenerate every secret instead of reusing .env values
  --non-interactive       never prompt; defaults are used for omitted options
  --yes                   skip the confirmation prompt
  --skip-docker-install   do not install Docker (fail if missing)
  --skip-firewall         do not touch the host firewall
  --enable-firewall       enable ufw/firewalld if installed but inactive
  --skip-dns-check        ignore DNS mismatch warnings
  --no-caddy              do not run Caddy; use an external TLS proxy
  --caddy                 run Caddy (default; ignored for --mode p2p)
  --dry-run               write .env/secrets and validate Compose, do not start
  -h, --help              show this help

Examples:
  sudo bash scripts/install_server.sh
  sudo bash scripts/install_server.sh --mode full --domain relay.example.org --turn --non-interactive --yes
  sudo bash scripts/install_server.sh --mode p2p --non-interactive --yes

  # YunoHost or another host with nginx on 80/443:
  sudo bash scripts/install_server.sh --mode full --domain relay.example.org \
    --no-caddy --non-interactive --yes
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
    --mode) need_arg "$@"; MODE="$2"; shift 2 ;;
    --mode=*) MODE="${1#*=}"; shift ;;
    --domain) need_arg "$@"; DOMAIN="$2"; shift 2 ;;
    --domain=*) DOMAIN="${1#*=}"; shift ;;
    --turn) TURN_CHOICE=yes; shift ;;
    --no-turn) TURN_CHOICE=no; shift ;;
    --search) need_arg "$@"; SEARCH_CHOICE="$2"; SEARCH_SET=1; shift 2 ;;
    --search=*) SEARCH_CHOICE="${1#*=}"; SEARCH_SET=1; shift ;;
    --meili) SEARCH_CHOICE=meili; SEARCH_SET=1; shift ;;
    --seed-relays) need_arg "$@"; SEED_RELAYS="$2"; shift 2 ;;
    --seed-relays=*) SEED_RELAYS="${1#*=}"; shift ;;
    --p2p-bootstrap) need_arg "$@"; P2P_BOOTSTRAP="$2"; shift 2 ;;
    --p2p-bootstrap=*) P2P_BOOTSTRAP="${1#*=}"; shift ;;
    --no-caddy|--external-proxy) CADDY_ENABLED=0; CADDY_SET=1; shift ;;
    --caddy) CADDY_ENABLED=1; CADDY_SET=1; shift ;;
    --public-ip) need_arg "$@"; PUBLIC_IP_OVERRIDE="$2"; shift 2 ;;
    --public-ip=*) PUBLIC_IP_OVERRIDE="${1#*=}"; shift ;;
    --dir) need_arg "$@"; DEPLOY_DIR_ARG="$2"; shift 2 ;;
    --dir=*) DEPLOY_DIR_ARG="${1#*=}"; shift ;;
    --admin-token) need_arg "$@"; ADMIN_TOKEN_OVERRIDE="$2"; shift 2 ;;
    --admin-token=*) ADMIN_TOKEN_OVERRIDE="${1#*=}"; shift ;;
    --rotate-secrets) ROTATE_SECRETS=1; shift ;;
    --non-interactive) NON_INTERACTIVE=1; shift ;;
    --yes|-y) ASSUME_YES=1; shift ;;
    --skip-docker-install) SKIP_DOCKER_INSTALL=1; shift ;;
    --skip-firewall) SKIP_FIREWALL=1; shift ;;
    --enable-firewall) ENABLE_FIREWALL=1; shift ;;
    --skip-dns-check) SKIP_DNS_CHECK=1; shift ;;
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
  "") ;;
  *) die "Invalid --mode '$MODE' (expected full, relay or p2p)" ;;
esac

case "$SEARCH_CHOICE" in
  ""|db) SEARCH_CHOICE=db ;;
  meili|meilisearch) SEARCH_CHOICE=meili ;;
  *) die "Invalid --search '$SEARCH_CHOICE' (expected db or meili)" ;;
esac

# P2P-only has no TLS edge; ignore Caddy there.
if [[ "$MODE" == "p2p" ]]; then
  CADDY_ENABLED=0
fi

# --------------------------------------------------------------------------- host
[[ -r /etc/os-release ]] || die "/etc/os-release not found; unsupported host"
# shellcheck disable=SC1091
. /etc/os-release

OS_ID="${ID:-unknown}"
OS_LIKE="${ID_LIKE:-}"
OS_NAME="${PRETTY_NAME:-$OS_ID}"
FAMILY=""

case " $OS_ID $OS_LIKE " in
  *" debian "*|*" ubuntu "*) FAMILY=debian ;;
esac
case " $OS_ID $OS_LIKE " in
  *" arch "*) FAMILY=arch ;;
esac

if [[ -z "$FAMILY" ]]; then
  die "Unsupported distribution: $OS_NAME (supported: Debian/Ubuntu derivatives and Arch derivatives)"
fi

if [[ $EUID -ne 0 ]] && ! command -v sudo >/dev/null 2>&1; then
  die "sudo is required when the installer is not run as root"
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

tty_read() {
  local __var="$1" __prompt="$2" __default="${3:-}" __value=""
  if [[ $INTERACTIVE -eq 0 ]]; then
    printf -v "$__var" '%s' "$__default"
    return 0
  fi
  printf '%s' "$__prompt" > /dev/tty
  IFS= read -r __value < /dev/tty || __value=""
  if [[ -z "$__value" ]]; then
    __value="$__default"
  fi
  printf -v "$__var" '%s' "$__value"
}

ask_yes_no() {
  local prompt="$1" default="${2:-no}" answer
  if [[ $INTERACTIVE -eq 0 ]]; then
    [[ "$default" == "yes" ]]
    return
  fi
  printf '%s' "$prompt" > /dev/tty
  IFS= read -r answer < /dev/tty || answer=""
  if [[ -z "$answer" ]]; then
    answer="$default"
  fi
  case "${answer,,}" in
    y|yes|s|si) return 0 ;;
    *) return 1 ;;
  esac
}

is_ipv4() {
  local ip="$1" octet
  local -a octets=()
  [[ "$ip" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]] || return 1
  IFS=. read -r -a octets <<< "$ip"
  for octet in "${octets[@]}"; do
    (( octet >= 0 && octet <= 255 )) || return 1
  done
  return 0
}

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

ensure_base_tools() {
  info "Installing base tools (curl, git, openssl, iproute2)"
  case "$FAMILY" in
    debian)
      as_root apt-get update -qq
      as_root env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        ca-certificates curl gnupg git openssl iproute2
      ;;
    arch)
      as_root pacman -Syu --needed --noconfirm \
        ca-certificates curl git openssl iproute2
      ;;
  esac
}

detect_public_ip() {
  local url ip
  for url in https://api.ipify.org https://ifconfig.co/ip https://icanhazip.com; do
    ip="$(curl -fsS --max-time 6 "$url" 2>/dev/null | tr -d '[:space:]' || true)"
    if is_ipv4 "$ip"; then
      printf '%s' "$ip"
      return 0
    fi
  done

  ip="$(ip -4 route get 1.1.1.1 2>/dev/null | awk '{for (i=1;i<=NF;i++) if ($i=="src") {print $(i+1); exit}}' || true)"
  if is_ipv4 "$ip"; then
    printf '%s' "$ip"
    return 0
  fi
  return 1
}

resolve_deploy_dir() {
  local candidate
  if [[ -n "$DEPLOY_DIR_ARG" ]]; then
    candidate="$DEPLOY_DIR_ARG"
  elif [[ -f "$(dirname "${BASH_SOURCE[0]}")/../docker-compose.yml" ]]; then
    candidate="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  else
    candidate="/opt/fedi3"
  fi

  if [[ ! -f "$candidate/docker-compose.yml" ]]; then
    info "Cloning Fedi3 into $candidate"
    as_root mkdir -p "$(dirname "$candidate")"
    as_root git clone --depth 1 "$REPO_URL" "$candidate"
  fi

  DEPLOY_DIR="$(cd "$candidate" && pwd)"
  local required
  for required in docker-compose.yml .env.example deploy/relay/Dockerfile \
    deploy/p2p_infra/Dockerfile scripts/lib/dotenv.sh; do
    [[ -f "$DEPLOY_DIR/$required" ]] || die "$required not found in $DEPLOY_DIR"
  done

  cd "$DEPLOY_DIR"
  if [[ ! -w "$DEPLOY_DIR" ]]; then
    die "$DEPLOY_DIR is not writable; run the installer with sudo or pass --dir"
  fi
}

# --------------------------------------------------------------------------- docker
install_docker_debian() {
  local repo codename arch
  if [[ "$OS_ID" == "ubuntu" || " $OS_LIKE " == *" ubuntu "* ]]; then
    repo="ubuntu"
    codename="${UBUNTU_CODENAME:-${VERSION_CODENAME:-}}"
    [[ -n "$codename" ]] || die "Cannot determine the Ubuntu codename"
  else
    repo="debian"
    codename="${VERSION_CODENAME:-}"
    case "$codename" in
      bullseye|bookworm|trixie|forky) ;;
      daedalus) codename="bookworm" ;;
      excalibur) codename="trixie" ;;
      *) codename="bookworm" ;;
    esac
  fi

  info "Adding the official Docker repository ($repo/$codename)"
  as_root install -m 0755 -d /etc/apt/keyrings
  curl -fsSL "https://download.docker.com/linux/$repo/gpg" | as_root tee /etc/apt/keyrings/docker.asc >/dev/null
  as_root chmod a+r /etc/apt/keyrings/docker.asc
  arch="$(dpkg --print-architecture)"
  printf 'deb [arch=%s signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/%s %s stable\n' \
    "$arch" "$repo" "$codename" | as_root tee /etc/apt/sources.list.d/docker.list >/dev/null
  as_root apt-get update -qq
  as_root env DEBIAN_FRONTEND=noninteractive apt-get install -y \
    docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
}

install_docker_arch() {
  info "Installing Docker from the Arch repositories"
  as_root pacman -Syu --needed --noconfirm docker docker-compose docker-buildx
}

start_docker() {
  if as_root systemctl is-active --quiet docker; then
    return 0
  fi
  if as_root systemctl enable --now docker; then
    return 0
  fi
  if as_root service docker start; then
    return 0
  fi
  warn "Could not start Docker automatically; start it manually (systemctl start docker)"
}

ensure_docker() {
  if ! command -v docker >/dev/null 2>&1; then
    if [[ $SKIP_DOCKER_INSTALL -eq 1 ]]; then
      die "Docker is not installed and --skip-docker-install was used"
    fi
    case "$FAMILY" in
      debian) install_docker_debian ;;
      arch) install_docker_arch ;;
    esac
  fi

  start_docker

  if ! docker compose version >/dev/null 2>&1; then
    warn "Docker Compose plugin not found; installing it"
    case "$FAMILY" in
      debian)
        if ! as_root env DEBIAN_FRONTEND=noninteractive apt-get install -y \
          docker-compose-plugin docker-buildx-plugin; then
          warn "docker-compose-plugin not available; trying docker-compose-v2"
          as_root env DEBIAN_FRONTEND=noninteractive apt-get install -y docker-compose-v2
        fi
        ;;
      arch)
        as_root pacman -S --needed --noconfirm docker-compose docker-buildx
        ;;
    esac
  fi

  docker compose version >/dev/null 2>&1 || die "Docker Compose v2 is required"

  local compose_version major minor rest
  compose_version="$(docker compose version --short 2>/dev/null || docker compose version 2>/dev/null | head -n1 || true)"
  compose_version="$(printf '%s' "$compose_version" | grep -oE '[0-9]+(\.[0-9]+)+' | head -n1 || true)"
  [[ -n "$compose_version" ]] || die "Could not parse the Docker Compose version"
  major="${compose_version%%.*}"
  rest="${compose_version#*.}"
  minor="${rest%%.*}"
  if (( major < 2 || (major == 2 && minor < 20) )); then
    die "Docker Compose >= 2.20 is required (found $compose_version); upgrade docker-compose-plugin/docker-compose"
  fi

  local target_user=""
  if [[ $EUID -eq 0 && -n "${SUDO_USER:-}" && "${SUDO_USER}" != "root" ]]; then
    target_user="$SUDO_USER"
  elif [[ $EUID -ne 0 ]]; then
    target_user="$USER"
  fi
  if [[ -n "$target_user" ]] && ! id -nG "$target_user" | tr ' ' '\n' | grep -qx docker; then
    as_root usermod -aG docker "$target_user" || true
    info "Added $target_user to the docker group (log out and back in to use docker without sudo)"
  fi

  if docker info >/dev/null 2>&1; then
    DOCKER_CMD=(docker)
  else
    DOCKER_CMD=(sudo docker)
  fi
  "${DOCKER_CMD[@]}" info >/dev/null 2>&1 || die "Cannot reach the Docker daemon"
}

# --------------------------------------------------------------------------- prompts
choose_mode() {
  if [[ -n "$MODE" ]]; then
    return 0
  fi
  if [[ $INTERACTIVE -eq 0 ]]; then
    die "--mode is required with --non-interactive (full, relay or p2p)"
  fi

  cat > /dev/tty <<'EOF'

Choose what this server should run:
  1) Relay + P2P infrastructure  (federation, relay mesh, mailbox, circuit relay)
  2) Relay only                  (federation; no P2P infrastructure node)
  3) P2P only                    (no federation, no TLS/domain; mailbox + circuit relay)

The P2P node lets clients find each other and keeps messages while a peer is
offline. The relay adds interoperability with classic ActivityPub instances.
EOF

  local choice
  while :; do
    tty_read choice "Mode [1-3] (default 1): " "1"
    case "$choice" in
      1) MODE=full; return 0 ;;
      2) MODE=relay; return 0 ;;
      3) MODE=p2p; return 0 ;;
      *) warn "Please enter 1, 2 or 3" ;;
    esac
  done
}

choose_domain() {
  if [[ -n "$DOMAIN" ]]; then
    DOMAIN="$(normalize_domain "$DOMAIN")"
  fi

  if [[ "$MODE" == "p2p" ]]; then
    if [[ -z "$DOMAIN" && $INTERACTIVE -eq 1 ]]; then
      tty_read DOMAIN "Public domain (optional, press Enter to skip): " ""
      DOMAIN="$(normalize_domain "$DOMAIN")"
    fi
    if [[ -n "$DOMAIN" ]] && ! validate_domain "$DOMAIN"; then
      warn "Ignoring invalid domain '$DOMAIN' for a P2P-only installation"
      DOMAIN=""
    fi
    return 0
  fi

  while :; do
    if [[ -z "$DOMAIN" ]]; then
      [[ $INTERACTIVE -eq 1 ]] || die "--domain is required for --mode $MODE"
      tty_read DOMAIN "Public domain (e.g. relay.example.org): " ""
      DOMAIN="$(normalize_domain "$DOMAIN")"
    fi
    if validate_domain "$DOMAIN"; then
      if [[ "$DOMAIN" == *example.* ]]; then
        warn "The domain still looks like a placeholder: $DOMAIN"
      fi
      return 0
    fi
    warn "Invalid domain: '$DOMAIN' (expected a hostname such as relay.example.org)"
    DOMAIN=""
    [[ $INTERACTIVE -eq 1 ]] || die "--domain is invalid"
  done
}

choose_turn() {
  if [[ -n "$TURN_CHOICE" ]]; then
    [[ "$TURN_CHOICE" == "yes" ]] && TURN_ENABLED=1 || TURN_ENABLED=0
    return 0
  fi
  if [[ $INTERACTIVE -eq 0 ]]; then
    TURN_ENABLED=0
    return 0
  fi

  local default="yes"
  [[ "$MODE" == "p2p" ]] && default="no"
  if [[ -z "$PUBLIC_IP" ]]; then
    default="no"
  fi

  cat > /dev/tty <<EOF

TURN (coturn) relays audio/video media when a direct WebRTC path is impossible.
It needs a public IPv4 address and the UDP range 49152-65535 open.
Detected public IPv4: ${PUBLIC_IP:-unknown}
EOF
  if ask_yes_no "Install TURN? [$default] " "$default"; then
    TURN_ENABLED=1
  else
    TURN_ENABLED=0
  fi

  if [[ $TURN_ENABLED -eq 1 && -z "$PUBLIC_IP" ]]; then
    if [[ $INTERACTIVE -eq 0 ]]; then
      die "--turn requires a public IPv4 address; pass --public-ip"
    fi
    while :; do
      tty_read PUBLIC_IP "Public IPv4 address for TURN: " ""
      if is_ipv4 "$PUBLIC_IP"; then
        break
      fi
      warn "Invalid IPv4 address: '$PUBLIC_IP'"
    done
  fi
}

choose_search() {
  if [[ "$MODE" == "p2p" ]]; then
    SEARCH_CHOICE=db
    return 0
  fi
  if [[ $SEARCH_SET -eq 1 ]]; then
    return 0
  fi
  if [[ $INTERACTIVE -eq 0 ]]; then
    SEARCH_CHOICE=db
    return 0
  fi

  cat > /dev/tty <<'EOF'

Search backend:
  db     uses PostgreSQL (simplest, recommended for small and medium servers)
  meili  adds Meilisearch (faster full-text search, about 1 GB of RAM)
EOF
  if ask_yes_no "Use Meilisearch? [y/N] " "no"; then
    SEARCH_CHOICE=meili
  else
    SEARCH_CHOICE=db
  fi
}

choose_seed_relays() {
  if [[ -n "$SEED_RELAYS" ]]; then
    return 0
  fi
  [[ $INTERACTIVE -eq 1 ]] || return 0
  [[ "$MODE" == "p2p" ]] && return 0

  cat > /dev/tty <<'EOF'

Relay mesh: relays can be linked to route users hosted on other Fedi3 relays.
Enter comma-separated relay base URLs to seed the mesh, or press Enter to skip.
EOF
  tty_read SEED_RELAYS "Seed relays (optional): " ""
}

discover_p2p_bootstraps() {
  local seeds="$1" relay url json addr existing
  local -a relays=() found=()
  IFS=, read -r -a relays <<< "$seeds"

  for relay in "${relays[@]}"; do
    relay="$(printf '%s' "$relay" | tr -d '[:space:]')"
    [[ -n "$relay" ]] || continue
    url="${relay%/}"
    case "$url" in
      http://*|https://*) ;;
      *) url="https://$url" ;;
    esac
    json="$(curl -fsS --max-time 6 "$url/_fedi3/relay/p2p_infra" 2>/dev/null || true)"
    [[ -n "$json" ]] || continue
    while IFS= read -r addr; do
      [[ "$addr" == */p2p/* ]] || continue
      existing=0
      local item
      for item in "${found[@]}"; do
        [[ "$item" == "$addr" ]] && existing=1 && break
      done
      [[ $existing -eq 1 ]] && continue
      found+=("$addr")
      (( ${#found[@]} >= 32 )) && break 2
    done < <(grep -oE '"/[^"]+"' <<< "$json" | tr -d '"')
  done

  [[ ${#found[@]} -gt 0 ]] || return 1
  local IFS=,
  printf '%s' "${found[*]}"
}

confirm_plan() {
  if [[ $INTERACTIVE -eq 1 && $ASSUME_YES -eq 0 ]]; then
    cat > /dev/tty <<EOF

Installation plan
  Distribution:  $OS_NAME ($FAMILY family)
  Deployment:    $DEPLOY_DIR
  Mode:          $MODE
  Domain:        ${DOMAIN:-none (P2P only)}
  Public IPv4:   ${PUBLIC_IP:-unknown}
  TLS proxy:     $([[ $CADDY_ENABLED -eq 1 ]] && echo "Caddy" || echo "external (no Caddy)")
  TURN:          $([[ $TURN_ENABLED -eq 1 ]] && echo enabled || echo disabled)
  Search:        $SEARCH_CHOICE
  Seed relays:   ${SEED_RELAYS:-none}
  P2P bootstrap: ${P2P_BOOTSTRAP:-auto from seed relays}

EOF
    ask_yes_no "Proceed? [Y/n] " "yes" || die "Installation cancelled"
  fi
}

# --------------------------------------------------------------------------- preflight
check_dns() {
  local domain="$1" expected_ip="$2" resolved
  [[ $SKIP_DNS_CHECK -eq 1 ]] && return 0

  resolved="$(getent ahostsv4 "$domain" 2>/dev/null | awk '{print $1}' | sort -u || true)"
  if [[ -z "$resolved" ]]; then
    warn "DNS: no A record found for $domain yet"
    return 1
  fi
  if [[ -n "$expected_ip" ]] && ! grep -Fqx "$expected_ip" <<< "$resolved"; then
    warn "DNS: $domain resolves to $(tr '\n' ' ' <<< "$resolved")but the detected public IPv4 is $expected_ip"
    return 1
  fi
  info "DNS: $domain -> $(tr '\n' ' ' <<< "$resolved")"
  return 0
}

port_in_use() {
  local proto="$1" port="$2"
  if [[ "$proto" == "tcp" ]]; then
    ss -ltnH 2>/dev/null | awk '{print $4}' | grep -qE "[:.]${port}$"
  else
    ss -lunH 2>/dev/null | awk '{print $4}' | grep -qE "[:.]${port}$"
  fi
}

preflight() {
  local ports=()

  [[ "$MODE" != "p2p" && $CADDY_ENABLED -eq 1 ]] && ports+=("80/tcp" "443/tcp")
  [[ "$MODE" != "relay" ]] && ports+=("4001/tcp" "4001/udp")
  [[ $TURN_ENABLED -eq 1 ]] && ports+=("3478/tcp" "3478/udp" "49152:65535/udp")

  if [[ "$MODE" != "p2p" ]]; then
    check_dns "$DOMAIN" "$PUBLIC_IP" || true
  fi

  local entry proto port
  for entry in "${ports[@]}"; do
    proto="${entry#*/}"
    port="${entry%%/*}"
    port="${port%%:*}"
    if port_in_use "$proto" "$port"; then
      warn "Port $entry is already in use (an existing web server or stack?)"
    fi
  done

  if [[ "$MODE" != "relay" && -z "$PUBLIC_IP" ]]; then
    warn "Could not detect a public IPv4; pass --public-ip or set FEDI3_P2P_EXTERNAL_ADDR manually"
  fi

  local avail_kb mem_kb
  avail_kb="$(df -Pk "$DEPLOY_DIR" 2>/dev/null | awk 'NR==2 {print $4}' || echo 0)"
  if [[ "$avail_kb" =~ ^[0-9]+$ ]] && (( avail_kb < 10 * 1024 * 1024 )); then
    warn "Less than 10 GB free on $DEPLOY_DIR; the first image build may fail"
  fi

  mem_kb="$(awk '/MemTotal/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)"
  if [[ "$mem_kb" =~ ^[0-9]+$ ]]; then
    if [[ "$MODE" == "full" ]] && (( mem_kb < 4 * 1024 * 1024 )); then
      warn "Less than 4 GB RAM: the full stack may hit its container memory limits"
    elif [[ "$MODE" == "relay" ]] && (( mem_kb < 3 * 1024 * 1024 )); then
      warn "Less than 3 GB RAM: the relay stack may be tight"
    elif [[ "$MODE" == "p2p" ]] && (( mem_kb < 768 * 1024 )); then
      warn "Less than 768 MB RAM: the P2P node may be tight"
    fi
  fi
}

# --------------------------------------------------------------------------- secrets
gen_hex() {
  openssl rand -hex "$1"
}

env_get() {
  local key="$1" file="${2:-$ENV_FILE}"
  [[ -f "$file" ]] || return 1
  awk -v k="$key" 'BEGIN {FS="="} $1 == k {sub(/^[^=]*=/, ""); print; found=1; exit} END {if (!found) exit 1}' "$file"
}

set_env() {
  local key="$1" value="$2" file="${3:-$ENV_FILE}" tmp
  tmp="$(mktemp "${file}.XXXXXX")"
  awk -v k="$key" -v v="$value" '
    BEGIN {FS="="; done=0}
    $1 == k {print k "=" v; done=1; next}
    {print}
    END {if (!done) print k "=" v}
  ' "$file" > "$tmp"
  chmod --reference="$file" "$tmp" 2>/dev/null || chmod 600 "$tmp"
  mv "$tmp" "$file"
}

prepare_secrets() {
  local existing

  existing="$(env_get FEDI3_RELAY_ADMIN_TOKEN || true)"
  if [[ -n "$ADMIN_TOKEN_OVERRIDE" ]]; then
    (( ${#ADMIN_TOKEN_OVERRIDE} >= 32 )) || die "--admin-token must be at least 32 characters"
    ADMIN_TOKEN="$ADMIN_TOKEN_OVERRIDE"
  elif [[ $ROTATE_SECRETS -eq 0 && ${#existing} -ge 32 && "$existing" != REPLACE_ME* ]]; then
    ADMIN_TOKEN="$existing"
  else
    ADMIN_TOKEN="$(gen_hex 32)"
  fi

  existing="$(env_get FEDI3_RELAY_TELEMETRY_TOKEN || true)"
  if [[ $ROTATE_SECRETS -eq 0 && -n "$existing" && "$existing" != REPLACE_ME* ]]; then
    TELEMETRY_TOKEN="$existing"
  else
    TELEMETRY_TOKEN="$(gen_hex 32)"
  fi

  existing="$(env_get FEDI3_RELAY_PG_PASSWORD || true)"
  if [[ $ROTATE_SECRETS -eq 0 && ${#existing} -ge 16 && "$existing" != REPLACE_ME* ]]; then
    PG_PASSWORD="$existing"
  else
    PG_PASSWORD="$(gen_hex 24)"
  fi

  existing="$(env_get TURN_PASS || true)"
  if [[ $ROTATE_SECRETS -eq 0 && -n "$existing" && "$existing" != REPLACE_ME* ]]; then
    TURN_PASS="$existing"
  else
    TURN_PASS="$(gen_hex 16)"
  fi

  existing="$(env_get FEDI3_RELAY_MEILI_MASTER_KEY || true)"
  if [[ $ROTATE_SECRETS -eq 0 && -n "$existing" ]]; then
    MEILI_KEY="$existing"
  else
    MEILI_KEY="$(gen_hex 32)"
  fi
}

write_secrets_file() {
  local previous_umask old_peer_id old_multiaddrs
  old_peer_id="$(grep -E '^FEDI3_P2P_PEER_ID=' "$SECRETS_FILE" 2>/dev/null | cut -d= -f2- || true)"
  old_multiaddrs="$(grep -E '^FEDI3_P2P_MULTIADDRS=' "$SECRETS_FILE" 2>/dev/null | cut -d= -f2- || true)"

  previous_umask="$(umask)"
  umask 077
  cat > "$SECRETS_FILE" <<EOF
# Fedi3 server credentials. Keep this file private (mode 600).
# Generated by scripts/install_server.sh; do not commit it.
MODE=$MODE
DEPLOY_DIR=$DEPLOY_DIR
DOMAIN=${DOMAIN:-}
PUBLIC_IP=${PUBLIC_IP:-}
RELAY_URL=$([[ "$MODE" != "p2p" ]] && echo "https://$DOMAIN" || echo "")
FEDI3_RELAY_ADMIN_TOKEN=$ADMIN_TOKEN
FEDI3_RELAY_TELEMETRY_TOKEN=$TELEMETRY_TOKEN
FEDI3_RELAY_PG_PASSWORD=$PG_PASSWORD
TURN_USER=relay
TURN_PASS=$TURN_PASS
FEDI3_RELAY_MEILI_MASTER_KEY=$MEILI_KEY
EOF
  if [[ -n "$old_peer_id" ]]; then
    set_env FEDI3_P2P_PEER_ID "$old_peer_id" "$SECRETS_FILE"
    if [[ -n "$PUBLIC_IP" ]]; then
      set_env FEDI3_P2P_MULTIADDRS \
        "/ip4/$PUBLIC_IP/tcp/4001/p2p/$old_peer_id,/ip4/$PUBLIC_IP/udp/4001/quic-v1/p2p/$old_peer_id" \
        "$SECRETS_FILE"
    elif [[ -n "$old_multiaddrs" ]]; then
      set_env FEDI3_P2P_MULTIADDRS "$old_multiaddrs" "$SECRETS_FILE"
    fi
  fi
  chmod 600 "$SECRETS_FILE"
  umask "$previous_umask"
}

write_env() {
  local profiles=()
  [[ "$MODE" != "p2p" ]] && profiles+=("relay")
  [[ "$MODE" != "relay" ]] && profiles+=("p2p")
  [[ "$MODE" != "p2p" && $CADDY_ENABLED -eq 1 ]] && profiles+=("caddy")
  [[ $TURN_ENABLED -eq 1 ]] && profiles+=("turn")
  [[ "$SEARCH_CHOICE" == "meili" ]] && profiles+=("search")
  local IFS=,
  PROFILES="${profiles[*]}"

  ENV_EXISTED=0
  if [[ -f "$ENV_FILE" ]]; then
    ENV_EXISTED=1
    ENV_BACKUP="$ENV_FILE.bak.$(date -u +%Y%m%dT%H%M%SZ)"
    cp -p "$ENV_FILE" "$ENV_BACKUP"
    chmod 600 "$ENV_BACKUP"
  else
    cp "$DEPLOY_DIR/.env.example" "$ENV_FILE"
  fi
  chmod 600 "$ENV_FILE"

  set_env FEDI3_DOMAIN "${DOMAIN:-}"
  set_env FEDI3_RELAY_BASE_DOMAIN ""
  set_env FEDI3_RELAY_PUBLIC_URL "$([[ "$MODE" != "p2p" ]] && echo "https://$DOMAIN" || echo "")"
  set_env FEDI3_RELAY_ADMIN_TOKEN "$ADMIN_TOKEN"
  set_env FEDI3_RELAY_TELEMETRY_TOKEN "$TELEMETRY_TOKEN"
  set_env FEDI3_RELAY_PG_PASSWORD "$PG_PASSWORD"
  set_env FEDI3_RELAY_DB_DRIVER postgres
  set_env FEDI3_RELAY_DB_URL ""
  set_env FEDI3_RELAY_SEARCH_BACKEND "$SEARCH_CHOICE"
  set_env FEDI3_RELAY_MEILI_MASTER_KEY "$MEILI_KEY"
  set_env FEDI3_RELAY_MEILI_API_KEY "$MEILI_KEY"
  set_env TURN_REALM "${DOMAIN:-fedi3.local}"
  set_env TURN_USER relay
  set_env TURN_PASS "$TURN_PASS"
  set_env TURN_EXTERNAL_IP "${PUBLIC_IP:-}"
  set_env FEDI3_P2P_INFRA_HOST ""
  set_env FEDI3_P2P_INFRA_PORT 4001
  if [[ "$MODE" != "relay" && -n "$PUBLIC_IP" ]]; then
    set_env FEDI3_P2P_EXTERNAL_ADDR "/ip4/$PUBLIC_IP/tcp/4001,/ip4/$PUBLIC_IP/udp/4001/quic-v1"
  else
    set_env FEDI3_P2P_EXTERNAL_ADDR ""
  fi
  if [[ -z "$P2P_BOOTSTRAP" && -n "$SEED_RELAYS" && "$MODE" != "relay" ]]; then
    P2P_BOOTSTRAP="$(discover_p2p_bootstraps "$SEED_RELAYS" || true)"
    if [[ -n "$P2P_BOOTSTRAP" ]]; then
      info "Discovered P2P bootstrap multiaddrs from the seed relays"
    fi
  fi
  set_env FEDI3_RELAY_SEED_RELAYS "$SEED_RELAYS"
  set_env FEDI3_P2P_BOOTSTRAP "$P2P_BOOTSTRAP"
  set_env FEDI3_EDGE_PROXY "$([[ $CADDY_ENABLED -eq 1 ]] && echo caddy || echo external)"
  set_env COMPOSE_PROFILES "$PROFILES"

  if [[ $DRY_RUN -eq 1 ]]; then
    return 0
  fi

  # A pre-profile install (or a previous Caddy-enabled install) may still have
  # Caddy running even when the new profile list no longer includes it.
  if [[ $CADDY_ENABLED -eq 0 ]]; then
    "${DOCKER_CMD[@]}" compose --profile caddy stop caddy >/dev/null 2>&1 || true
  fi

  if [[ -n "$OLD_PROFILES" && "$OLD_PROFILES" != "$PROFILES" ]]; then
    info "Installation mode changed ($OLD_PROFILES -> $PROFILES); stopping previous containers"
    COMPOSE_PROFILES="$OLD_PROFILES" "${DOCKER_CMD[@]}" compose down --remove-orphans || \
      warn "Could not stop the previous stack cleanly"
  elif [[ "$ENV_EXISTED" -eq 1 && -z "$OLD_PROFILES" ]]; then
    # Upgrade from a Compose file without profiles: the previous `up` started
    # every service. Stop them before applying the selected profile.
    info "Stopping containers from the previous installation"
    COMPOSE_PROFILES=relay,p2p,caddy,turn,search "${DOCKER_CMD[@]}" compose down --remove-orphans || \
      warn "Could not stop the previous stack cleanly"
  fi
}

# --------------------------------------------------------------------------- firewall
firewall_ports() {
  local ports=()
  [[ "$MODE" != "p2p" && $CADDY_ENABLED -eq 1 ]] && ports+=("80/tcp" "443/tcp")
  [[ "$MODE" != "relay" ]] && ports+=("4001/tcp" "4001/udp")
  [[ $TURN_ENABLED -eq 1 ]] && ports+=("3478/tcp" "3478/udp" "49152:65535/udp")
  printf '%s\n' "${ports[@]}"
}

print_firewall_ports() {
  local ports
  ports="$(firewall_ports | tr '\n' ' ')"
  cat <<EOF
Required ports for this mode: $ports
$(if [[ $CADDY_ENABLED -eq 0 && "$MODE" != "p2p" ]]; then echo "Ports 80/443 are owned by the external TLS proxy (YunoHost/nginx)."; fi)

If a host firewall is active, allow them, for example:
  ufw:       sudo ufw allow 4001/tcp && sudo ufw allow 4001/udp && ...
  firewalld: sudo firewall-cmd --permanent --add-port=4001/tcp && sudo firewall-cmd --reload
Also allow the same ports in the cloud provider's security group/firewall.
EOF
}

configure_firewall() {
  if [[ $SKIP_FIREWALL -eq 1 || $DRY_RUN -eq 1 ]]; then
    print_firewall_ports
    return 0
  fi

  local ports=()
  mapfile -t ports < <(firewall_ports)

  local ufw_active=0 firewalld_active=0
  if command -v ufw >/dev/null 2>&1 && as_root ufw status 2>/dev/null | head -n1 | grep -qi "status: active"; then
    ufw_active=1
  fi
  if command -v firewall-cmd >/dev/null 2>&1 && as_root firewall-cmd --state 2>/dev/null | grep -q "running"; then
    firewalld_active=1
  fi

  if [[ $ufw_active -eq 1 ]]; then
    local entry
    for entry in "${ports[@]}"; do
      as_root ufw allow "$entry" >/dev/null
    done
    info "ufw: allowed ${ports[*]}"
    return 0
  fi

  if [[ $firewalld_active -eq 1 ]]; then
    local entry
    for entry in "${ports[@]}"; do
      as_root firewall-cmd --permanent --add-port="${entry/:/-}" >/dev/null
    done
    as_root firewall-cmd --reload >/dev/null
    info "firewalld: allowed ${ports[*]}"
    return 0
  fi

  warn "No active ufw/firewalld detected"
  print_firewall_ports

  if [[ $ENABLE_FIREWALL -eq 1 ]]; then
    if command -v ufw >/dev/null 2>&1; then
      info "Enabling ufw (SSH port 22 is allowed)"
      as_root ufw allow 22/tcp >/dev/null
      local entry
      for entry in "${ports[@]}"; do
        as_root ufw allow "$entry" >/dev/null
      done
      as_root ufw --force enable >/dev/null
    elif command -v firewall-cmd >/dev/null 2>&1; then
      info "Enabling firewalld"
      as_root systemctl enable --now firewalld
      local entry
      for entry in "${ports[@]}"; do
        as_root firewall-cmd --permanent --add-port="${entry/:/-}" >/dev/null
      done
      as_root firewall-cmd --reload >/dev/null
    else
      warn "Neither ufw nor firewalld is installed; nothing to enable"
    fi
  fi
}

# --------------------------------------------------------------------------- stack
wait_for_relay() {
  local deadline=$((SECONDS + 240))
  while (( SECONDS < deadline )); do
    if curl -fsS -m 3 http://127.0.0.1:8787/healthz >/dev/null 2>&1; then
      return 0
    fi
    sleep 3
  done
  return 1
}

wait_for_https() {
  local domain="$1" deadline=$((SECONDS + 180)) curl_opts=(-fsS -m 5)
  case "$domain" in
    localhost|*.localhost|127.*) curl_opts+=(-k) ;;
  esac
  while (( SECONDS < deadline )); do
    if curl "${curl_opts[@]}" "https://$domain/healthz" >/dev/null 2>&1; then
      return 0
    fi
    sleep 5
  done
  return 1
}

wait_for_p2p() {
  local deadline=$((SECONDS + 120)) container_id status
  while (( SECONDS < deadline )); do
    container_id="$("${DOCKER_CMD[@]}" compose ps -q p2p_infra 2>/dev/null || true)"
    if [[ -n "$container_id" ]]; then
      status="$("${DOCKER_CMD[@]}" inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "$container_id" 2>/dev/null || true)"
      if [[ "$status" == "healthy" ]]; then
        return 0
      fi
    fi
    sleep 3
  done
  return 1
}

read_peer_id() {
  local peer_id=""
  for _ in {1..20}; do
    peer_id="$("${DOCKER_CMD[@]}" compose exec -T p2p_infra cat /data/fedi3_p2p_peer_id 2>/dev/null | tr -d '[:space:]' || true)"
    if [[ -n "$peer_id" ]]; then
      printf '%s' "$peer_id"
      return 0
    fi
    sleep 2
  done
  return 1
}

start_stack() {
  info "Validating the Compose configuration"
  "${DOCKER_CMD[@]}" compose config --quiet || die "Invalid Docker Compose configuration in $DEPLOY_DIR"

  if [[ $DRY_RUN -eq 1 ]]; then
    info "Dry run: not starting containers"
    return 0
  fi

  info "Building and starting containers (the first Rust build can take several minutes)"
  if ! "${DOCKER_CMD[@]}" compose up -d --build; then
    "${DOCKER_CMD[@]}" compose ps || true
    die "docker compose up failed; inspect logs with: cd $DEPLOY_DIR && docker compose logs --tail=200"
  fi
}

verify_stack() {
  local ok=1 peer_id="" multiaddrs=""

  if [[ "$MODE" != "p2p" ]]; then
    info "Waiting for the relay health check"
    if wait_for_relay; then
      info "Relay is healthy on 127.0.0.1:8787"
      if [[ $CADDY_ENABLED -eq 1 ]]; then
        info "Waiting for HTTPS/TLS on $DOMAIN (ACME can take a minute)"
        if wait_for_https "$DOMAIN"; then
          info "TLS is working on https://$DOMAIN"
        else
          warn "HTTPS not ready yet; check DNS and: docker compose logs --tail=200 caddy"
        fi
      else
        info "TLS is handled by the external proxy; verify https://$DOMAIN/healthz after configuring it"
      fi
    else
      warn "Relay did not become healthy within 240 s"
      ok=0
    fi
  fi

  if [[ "$MODE" != "relay" ]]; then
    info "Waiting for the P2P infrastructure node"
    if wait_for_p2p; then
      info "P2P infrastructure node is healthy"
    else
      warn "P2P node did not become healthy within 120 s"
      ok=0
    fi

    peer_id="$(read_peer_id || true)"
    if [[ -n "$peer_id" ]]; then
      set_env FEDI3_P2P_PEER_ID "$peer_id" "$SECRETS_FILE"
      if [[ -n "$PUBLIC_IP" ]]; then
        multiaddrs="/ip4/$PUBLIC_IP/tcp/4001/p2p/$peer_id,/ip4/$PUBLIC_IP/udp/4001/quic-v1/p2p/$peer_id"
        set_env FEDI3_P2P_MULTIADDRS "$multiaddrs" "$SECRETS_FILE"
      fi
      info "P2P peer id: $peer_id"
    else
      warn "Could not read the P2P peer id from the container"
    fi
  fi

  if [[ $TURN_ENABLED -eq 1 ]]; then
    local turn_id turn_status
    turn_id="$("${DOCKER_CMD[@]}" compose ps -q turn 2>/dev/null || true)"
    if [[ -n "$turn_id" ]]; then
      turn_status="$("${DOCKER_CMD[@]}" inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "$turn_id" 2>/dev/null || true)"
      if [[ "$turn_status" == "healthy" ]]; then
        info "TURN is healthy"
      else
        warn "TURN health status: ${turn_status:-unknown}"
      fi
    fi
  fi

  "${DOCKER_CMD[@]}" compose ps || true

  if [[ $ok -ne 1 ]]; then
    warn "One or more services are not healthy; see the logs above"
    return 1
  fi
  return 0
}

print_summary() {
  local mode_label relay_url="" turn_label
  case "$MODE" in
    full) mode_label="relay + P2P infrastructure" ;;
    relay) mode_label="relay only" ;;
    p2p) mode_label="P2P infrastructure only" ;;
  esac
  [[ "$MODE" != "p2p" ]] && relay_url="https://$DOMAIN"
  [[ $TURN_ENABLED -eq 1 ]] && turn_label="enabled" || turn_label="disabled"

  cat <<EOF

Fedi3 server installation complete.

  Mode:            $mode_label
  Deployment:      $DEPLOY_DIR
  Relay URL:       ${relay_url:-none (P2P only)}
  TLS proxy:       $([[ $CADDY_ENABLED -eq 1 ]] && echo "Caddy" || echo "external (no Caddy)")
  Public IPv4:     ${PUBLIC_IP:-unknown}
  TURN:            $turn_label
  P2P bootstrap:   ${P2P_BOOTSTRAP:-none}
  Credentials:     $SECRETS_FILE (mode 600)

Useful commands (run from $DEPLOY_DIR):
  docker compose ps
  docker compose logs -f relay
  docker compose logs -f p2p_infra
  docker compose down            # stop, volumes are preserved
  docker compose up -d --build   # update/restart

EOF

  if [[ "$MODE" != "p2p" ]]; then
    cat <<EOF
Client configuration (relay):
  - Onboarding: public base $relay_url, relay WebSocket wss://$DOMAIN, and a
    relay token of at least 16 characters (the app can generate one; keep it).
  - Then Settings -> Network -> Relay admin: paste the admin token from
    $SECRETS_FILE and register the same username with that relay token.
    The tunnel connects once the user is registered.
EOF
  fi

  if [[ "$MODE" != "relay" ]]; then
    local peer_id p2p_addrs
    peer_id="$(grep -E '^FEDI3_P2P_PEER_ID=' "$SECRETS_FILE" 2>/dev/null | cut -d= -f2- || true)"
    p2p_addrs="$(grep -E '^FEDI3_P2P_MULTIADDRS=' "$SECRETS_FILE" 2>/dev/null | cut -d= -f2- | tr ',' '\n' | sed 's/^/    /' || true)"
    cat <<EOF

Client configuration (P2P bootstrap):
  Settings -> Network -> Independent Fedi3 bootstrap nodes.
  Peer id: ${peer_id:-not available yet (check: docker compose logs p2p_infra)}
EOF
    if [[ -n "$p2p_addrs" ]]; then
      printf '%s\n' "$p2p_addrs"
    elif [[ -n "$peer_id" && -n "$PUBLIC_IP" ]]; then
      printf '    /ip4/%s/tcp/4001/p2p/%s\n' "$PUBLIC_IP" "$peer_id"
      printf '    /ip4/%s/udp/4001/quic-v1/p2p/%s\n' "$PUBLIC_IP" "$peer_id"
    elif [[ -n "$peer_id" ]]; then
      echo "    (no public IPv4 detected; add the multiaddrs manually)"
    else
      echo "    (multiaddrs appear after the first start)"
    fi
  fi

  cat <<EOF

Open these ports in the cloud firewall/security group:
$(firewall_ports | sed 's/^/  - /')

Verification:
EOF

  if [[ "$MODE" != "p2p" ]]; then
    if [[ $CADDY_ENABLED -eq 0 ]]; then
      cat <<EOF
  curl -fsS $relay_url/healthz    # through the external TLS proxy
  curl -fsS -H "Authorization: Bearer <ADMIN_TOKEN>" $relay_url/_fedi3/relay/stats
  # direct loopback check:
  curl -fsS http://127.0.0.1:8787/healthz
EOF
    else
      case "$DOMAIN" in
        localhost|*.localhost|127.*)
          cat <<EOF
  curl -k -fsS $relay_url/healthz   # local Caddy uses an internal CA
  curl -k -fsS -H "Authorization: Bearer <ADMIN_TOKEN>" $relay_url/_fedi3/relay/stats
EOF
          ;;
        *)
          cat <<EOF
  curl -fsS $relay_url/healthz
  scripts/relay_smoke_test.sh $relay_url <ADMIN_TOKEN>
EOF
          ;;
      esac
    fi
  fi
  if [[ "$MODE" != "relay" ]]; then
    cat <<EOF
  docker compose ps p2p_infra
  docker compose logs --tail=100 p2p_infra
EOF
  fi

  if [[ $DRY_RUN -eq 1 ]]; then
    printf '\nDry run: containers were not started. Configuration is ready.\n'
  fi
}

# --------------------------------------------------------------------------- main
info "Fedi3 server installer"
info "Detected: $OS_NAME ($FAMILY family)"

ensure_base_tools
resolve_deploy_dir

ENV_FILE="$DEPLOY_DIR/.env"
SECRETS_FILE="$DEPLOY_DIR/relay-secrets.txt"

if [[ -n "$PUBLIC_IP_OVERRIDE" ]]; then
  is_ipv4 "$PUBLIC_IP_OVERRIDE" || die "--public-ip is not a valid IPv4 address: $PUBLIC_IP_OVERRIDE"
  PUBLIC_IP="$PUBLIC_IP_OVERRIDE"
else
  PUBLIC_IP="$(detect_public_ip || true)"
fi

# Load existing values without exporting them (Compose reads .env itself).
# shellcheck source=lib/dotenv.sh
source "$DEPLOY_DIR/scripts/lib/dotenv.sh"
load_dotenv "$ENV_FILE"

profiles_contain() {
  local profile_list="$1" wanted="$2" item
  local -a items=()
  IFS=, read -r -a items <<< "$profile_list"
  for item in "${items[@]}"; do
    [[ "$item" == "$wanted" ]] && return 0
  done
  return 1
}

# Re-running the installer keeps the existing mode, domain and optional
# components unless the corresponding flag is given explicitly.
OLD_PROFILES="$(env_get COMPOSE_PROFILES || true)"
if [[ -z "$MODE" && -n "$OLD_PROFILES" ]]; then
  if profiles_contain "$OLD_PROFILES" relay && profiles_contain "$OLD_PROFILES" p2p; then
    MODE=full
  elif profiles_contain "$OLD_PROFILES" relay; then
    MODE=relay
  elif profiles_contain "$OLD_PROFILES" p2p; then
    MODE=p2p
  fi
fi
if [[ -z "$DOMAIN" && -n "${FEDI3_DOMAIN:-}" ]]; then
  DOMAIN="$(normalize_domain "$FEDI3_DOMAIN")"
fi
if [[ -z "$SEED_RELAYS" && -n "${FEDI3_RELAY_SEED_RELAYS:-}" ]]; then
  SEED_RELAYS="$FEDI3_RELAY_SEED_RELAYS"
fi
if [[ -z "$P2P_BOOTSTRAP" && -n "${FEDI3_P2P_BOOTSTRAP:-}" ]]; then
  P2P_BOOTSTRAP="$FEDI3_P2P_BOOTSTRAP"
fi
OLD_EDGE_PROXY="$(env_get FEDI3_EDGE_PROXY || true)"
if [[ $CADDY_SET -eq 0 && -n "$OLD_EDGE_PROXY" ]]; then
  if [[ "$OLD_EDGE_PROXY" == "external" ]]; then
    CADDY_ENABLED=0
  else
    CADDY_ENABLED=1
  fi
fi
if [[ -z "$PUBLIC_IP" && -n "${TURN_EXTERNAL_IP:-}" ]] && is_ipv4 "$TURN_EXTERNAL_IP"; then
  PUBLIC_IP="$TURN_EXTERNAL_IP"
fi
if [[ -z "$TURN_CHOICE" && -n "$OLD_PROFILES" ]]; then
  if profiles_contain "$OLD_PROFILES" turn; then
    TURN_CHOICE=yes
  else
    TURN_CHOICE=no
  fi
fi
if [[ $SEARCH_SET -eq 0 && -n "$OLD_PROFILES" ]]; then
  if profiles_contain "$OLD_PROFILES" search; then
    SEARCH_CHOICE=meili
  else
    SEARCH_CHOICE=db
  fi
  SEARCH_SET=1
fi

choose_mode
[[ "$MODE" == "p2p" ]] && CADDY_ENABLED=0
if [[ $CADDY_SET -eq 0 && $CADDY_ENABLED -eq 1 ]] && command -v yunohost >/dev/null 2>&1; then
  warn "YunoHost detected: nginx already owns ports 80/443. Use --no-caddy or scripts/install_yunohost.sh."
fi
choose_domain
choose_turn
choose_search
choose_seed_relays
confirm_plan

preflight
ensure_docker
prepare_secrets
write_env
write_secrets_file
configure_firewall

start_stack

if [[ $DRY_RUN -eq 0 ]]; then
  if ! verify_stack; then
    print_summary
    exit 1
  fi
fi

print_summary
