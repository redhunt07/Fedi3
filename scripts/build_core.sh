#!/usr/bin/env bash
set -euo pipefail

profile="${1:-release}"
case "$profile" in
  release|debug) ;;
  *)
    echo "Usage: $0 [release|debug]"
    exit 2
    ;;
esac

if command -v cargo >/dev/null 2>&1; then
  cargo_cmd="cargo"
elif [[ -x "$HOME/.cargo/bin/cargo" ]]; then
  cargo_cmd="$HOME/.cargo/bin/cargo"
else
  echo "Missing command: cargo (install rustup/cargo)"
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
core_dir="$repo_root/crates/fedi3_core"

(cd "$core_dir" && "$cargo_cmd" build -p fedi3_core --"$profile")

suffix="$profile"
so_path="$repo_root/target/$suffix/libfedi3_core.so"
if [[ ! -f "$so_path" ]]; then
  echo "Build completata, ma .so non trovata in: $so_path"
  exit 1
fi

app_dir="$repo_root/app"

copy_core_so() {
  local dest="$1"
  cp -f "$so_path" "$dest"
  echo "Copiata: $dest"
}

copy_core_so "$app_dir/libfedi3_core.so"

candidate_dirs=(
  "$app_dir/build/linux/x64/debug/bundle/lib"
  "$app_dir/build/linux/x64/release/bundle/lib"
  "$app_dir/build/linux/x64/profile/bundle/lib"
)

for dir in "${candidate_dirs[@]}"; do
  if [[ -d "$dir" ]]; then
    copy_core_so "$dir/libfedi3_core.so"
  fi
done
