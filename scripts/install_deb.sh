#!/usr/bin/env bash
set -euo pipefail

REPO_URL="https://github.com/redhunt07/Fedi3.git"
REPO_DIR="/opt/fedi3/src"
INSTALL_DIR="/opt/fedi3/app"
FLUTTER_DIR="/opt/flutter"
ICON_PATH="/opt/fedi3/icon.png"
DESKTOP_FILE="/usr/share/applications/fedi3.desktop"

require_cmd() {
  command -v "$1" >/dev/null 2>&1
}

sudo_cmd() {
  if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
    "$@"
  else
    sudo "$@"
  fi
}

ensure_debian() {
  if [[ ! -f /etc/os-release ]]; then
    echo "Unsupported OS: missing /etc/os-release"
    exit 1
  fi
  . /etc/os-release
  if [[ "${ID:-}" != "debian" && "${ID:-}" != "ubuntu" && "${ID_LIKE:-}" != *"debian"* ]]; then
    echo "Unsupported OS: ${ID:-unknown}"
    exit 1
  fi
}

install_packages() {
  sudo_cmd apt-get update
  sudo_cmd apt-get install -y \
    git curl unzip xz-utils zip \
    clang cmake ninja-build pkg-config \
    libgtk-3-dev libblkid-dev liblzma-dev \
    ca-certificates
}

install_rust() {
  if require_cmd cargo; then
    return
  fi
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  if [[ -f "$HOME/.cargo/env" ]]; then
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
  fi
}

install_flutter() {
  if [[ -x "${FLUTTER_DIR}/bin/flutter" ]]; then
    return
  fi
  sudo_cmd mkdir -p "$FLUTTER_DIR"
  curl -L "https://storage.googleapis.com/flutter_infra_release/releases/stable/linux/flutter_linux_3.24.5-stable.tar.xz" -o /tmp/flutter.tar.xz
  sudo_cmd tar -xJf /tmp/flutter.tar.xz -C /opt
  rm -f /tmp/flutter.tar.xz
}

clone_or_update_repo() {
  sudo_cmd mkdir -p /opt/fedi3
  if [[ -d "$REPO_DIR/.git" ]]; then
    sudo_cmd git -C "$REPO_DIR" fetch --all --prune
    sudo_cmd git -C "$REPO_DIR" pull --ff-only
  else
    sudo_cmd git clone "$REPO_URL" "$REPO_DIR"
  fi
  sudo_cmd chown -R "$(id -u)":"$(id -g)" "$REPO_DIR"
}

build_core() {
  pushd "$REPO_DIR" >/dev/null
  if [[ -f ./scripts/build_core.sh ]]; then
    chmod +x ./scripts/build_core.sh
  fi
  bash ./scripts/build_core.sh release
  popd >/dev/null
}

build_flutter() {
  export PATH="${FLUTTER_DIR}/bin:$PATH"
  pushd "$REPO_DIR/app" >/dev/null
  flutter precache --linux
  flutter pub get
  flutter build linux --release
  popd >/dev/null
}

install_app() {
  sudo_cmd rm -rf "$INSTALL_DIR"
  sudo_cmd mkdir -p "$INSTALL_DIR"
  sudo_cmd cp -r "$REPO_DIR/app/build/linux/x64/release/bundle/." "$INSTALL_DIR"
  sudo_cmd mkdir -p "$(dirname "$ICON_PATH")"
  if [[ -f "$REPO_DIR/app/web/icons/Icon-512.png" ]]; then
    sudo_cmd cp "$REPO_DIR/app/web/icons/Icon-512.png" "$ICON_PATH"
  fi
  sudo_cmd tee "$DESKTOP_FILE" >/dev/null <<EOF
[Desktop Entry]
Type=Application
Name=Fedi3
Comment=Fedi3 client
Exec=${INSTALL_DIR}/fedi3
Icon=${ICON_PATH}
Terminal=false
Categories=Network;Internet;
StartupWMClass=Fedi3
EOF
  sudo_cmd chmod 644 "$DESKTOP_FILE"
}

main() {
  local mode="full"
  if [[ "${1:-}" == "--update-only" ]]; then
    mode="update"
  fi
  ensure_debian
  if [[ "$mode" == "full" ]]; then
    install_packages
    install_rust
    install_flutter
  fi
  clone_or_update_repo
  build_core
  build_flutter
  install_app
  echo "Done. Launch from applications menu or run: ${INSTALL_DIR}/fedi3"
}

main "$@"
