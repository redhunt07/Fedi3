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

ensure_arch() {
  if ! require_cmd pacman; then
    echo "Unsupported OS: pacman not found"
    exit 1
  fi
  if [[ -f /etc/os-release ]]; then
    . /etc/os-release
    if [[ "${ID:-}" != "arch" && "${ID_LIKE:-}" != *"arch"* ]]; then
      echo "Warning: OS is not Arch-based (${ID:-unknown}). Continuing anyway."
    fi
  fi
}

install_packages() {
  local packages=(
    base-devel git curl unzip xz zip python
    clang cmake ninja pkgconf
    gtk3
    gstreamer gst-plugins-base gst-plugins-good
    libsecret libnotify
    mpv
  )
  local webkit_pkg="webkit2gtk"
  if pacman -Si webkit2gtk-4.1 >/dev/null 2>&1; then
    webkit_pkg="webkit2gtk-4.1"
  fi
  packages+=("$webkit_pkg")

  local missing=()
  for pkg in "${packages[@]}"; do
    if ! pacman -Qi "$pkg" >/dev/null 2>&1; then
      missing+=("$pkg")
    fi
  done

  if [[ "${#missing[@]}" -eq 0 ]]; then
    echo "All Arch dependencies already installed."
    return
  fi

  sudo_cmd pacman -Sy --noconfirm --needed "${missing[@]}"
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
    if flutter_has_modern_dart; then
      return
    fi
    sudo_cmd rm -rf "$FLUTTER_DIR"
  fi
  sudo_cmd mkdir -p "$FLUTTER_DIR"
  local flutter_url
  flutter_url="$(latest_flutter_stable_url)"
  curl -L "$flutter_url" -o /tmp/flutter.tar.xz
  sudo_cmd tar -xJf /tmp/flutter.tar.xz -C /opt
  rm -f /tmp/flutter.tar.xz
}

latest_flutter_stable_url() {
  python - <<'PY'
import json
import urllib.request

data = json.load(urllib.request.urlopen(
    "https://storage.googleapis.com/flutter_infra_release/releases/releases_linux.json"
))
base = data["base_url"].rstrip("/")
stable_hash = data["current_release"]["stable"]
release = next(r for r in data["releases"] if r["hash"] == stable_hash)
print(f"{base}/{release['archive']}")
PY
}

flutter_has_modern_dart() {
  local dart_ver
  dart_ver="$("${FLUTTER_DIR}/bin/flutter" --version --machine | python - <<'PY'
import json, sys
data = json.load(sys.stdin)
print(data.get("dartSdkVersion","0.0.0"))
PY
)"
  python - <<PY
import re, sys
def parse(v):
    m = re.match(r"^(\d+)\.(\d+)\.(\d+)", v or "0.0.0")
    if not m:
        return (0, 0, 0)
    return tuple(int(x) for x in m.groups())
sys.exit(0 if parse("${dart_ver}") >= (3, 9, 0) else 1)
PY
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
  local cargo_cmd=""
  if require_cmd cargo; then
    cargo_cmd="cargo"
  elif [[ -x "$HOME/.cargo/bin/cargo" ]]; then
    cargo_cmd="$HOME/.cargo/bin/cargo"
  else
    echo "Missing command: cargo"
    exit 1
  fi

  local core_dir="${REPO_DIR}/crates/fedi3_core"
  local app_dir="${REPO_DIR}/app"
  local so_path="${REPO_DIR}/target/release/libfedi3_core.so"

  (cd "$core_dir" && "$cargo_cmd" build -p fedi3_core --release)
  if [[ ! -f "$so_path" ]]; then
    echo "Build completata, ma .so non trovata in: $so_path"
    exit 1
  fi

  cp -f "$so_path" "${app_dir}/libfedi3_core.so"
  for dir in \
    "${app_dir}/build/linux/x64/debug/bundle/lib" \
    "${app_dir}/build/linux/x64/release/bundle/lib" \
    "${app_dir}/build/linux/x64/profile/bundle/lib"; do
    if [[ -d "$dir" ]]; then
      cp -f "$so_path" "${dir}/libfedi3_core.so"
    fi
  done
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
  ensure_arch
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
