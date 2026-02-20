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

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app_dir="$repo_root/app"
bundle_dir="$app_dir/build/linux/x64/$profile/bundle"
appdir="$repo_root/dist/AppDir"
appimage_out="$repo_root/dist/Fedi3-${profile}-x86_64.AppImage"
icon_src="$app_dir/web/icons/Icon-512.png"
desktop_src="$repo_root/deploy/appimage/fedi3.desktop"

resolve_lib_path() {
  local name="$1"
  local path=""
  path="$(ldconfig -p 2>/dev/null | awk -v n="$name" '$1==n {print $4; exit}')"
  if [[ -z "$path" ]]; then
    for dir in /usr/lib /usr/lib64 /usr/lib/x86_64-linux-gnu /lib /lib64; do
      if [[ -f "$dir/$name" ]]; then
        path="$dir/$name"
        break
      fi
    done
  fi
  if [[ -n "$path" && -f "$path" ]]; then
    echo "$path"
    return 0
  fi
  return 1
}

if ! command -v flutter >/dev/null 2>&1; then
  echo "Missing command: flutter"
  exit 1
fi

if [[ ! -x "$repo_root/scripts/build_core.sh" ]]; then
  echo "Missing scripts/build_core.sh (run chmod +x scripts/build_core.sh)"
  exit 1
fi

"$repo_root/scripts/build_core.sh" "$profile"

(cd "$app_dir" && flutter build linux --"$profile")

if [[ ! -d "$bundle_dir" ]]; then
  echo "Missing bundle dir: $bundle_dir"
  exit 1
fi

rm -rf "$appdir"
mkdir -p "$appdir/usr"
cp -a "$bundle_dir/." "$appdir/usr/"

mkdir -p "$appdir/usr/lib"
if [[ -f "$repo_root/target/$profile/libfedi3_core.so" ]]; then
  cp -f "$repo_root/target/$profile/libfedi3_core.so" "$appdir/usr/lib/libfedi3_core.so"
elif [[ -f "$app_dir/libfedi3_core.so" ]]; then
  cp -f "$app_dir/libfedi3_core.so" "$appdir/usr/lib/libfedi3_core.so"
fi

# Bundle libmpv when available (Arch ships .so.2, Ubuntu ships .so.1).
mpv_dirs=(/usr/lib /usr/lib64 /usr/lib/x86_64-linux-gnu)
found_mpv=0
for dir in "${mpv_dirs[@]}"; do
  if [[ -f "$dir/libmpv.so.1" ]]; then
    cp -f "$dir/libmpv.so.1" "$appdir/usr/lib/libmpv.so.1"
    found_mpv=1
  fi
  if [[ -f "$dir/libmpv.so.2" ]]; then
    cp -f "$dir/libmpv.so.2" "$appdir/usr/lib/libmpv.so.2"
    ln -sf libmpv.so.2 "$appdir/usr/lib/libmpv.so.1"
    found_mpv=1
  fi
done
if [[ "$found_mpv" -eq 0 ]]; then
  echo "libmpv not found on build host. Install libmpv1 (Ubuntu/Debian) or mpv (Arch) and retry."
  exit 1
fi

# Bundle libmujs if available (required by some WebView builds on Debian/Ubuntu).
mujs_path="$(resolve_lib_path "libmujs.so.3")" || true
if [[ -z "$mujs_path" ]]; then
  mujs_path="$(resolve_lib_path "libmujs.so")" || true
fi
if [[ -n "$mujs_path" ]]; then
  cp -f "$mujs_path" "$appdir/usr/lib/$(basename "$mujs_path")"
  if [[ "$(basename "$mujs_path")" != "libmujs.so" ]]; then
    ln -sf "$(basename "$mujs_path")" "$appdir/usr/lib/libmujs.so"
  fi
else
  echo "Warning: libmujs not found on build host (WebView might fail on some distros)."
fi

if [[ -f "$icon_src" ]]; then
  cp -f "$icon_src" "$appdir/fedi3.png"
fi
if [[ -f "$desktop_src" ]]; then
  cp -f "$desktop_src" "$appdir/fedi3.desktop"
fi

cat > "$appdir/AppRun" <<'EOF'
#!/usr/bin/env bash
HERE="$(dirname "$(readlink -f "$0")")"
export APPDIR="$HERE"
export LD_LIBRARY_PATH="$HERE/usr/lib:${LD_LIBRARY_PATH:-}"
export XDG_DATA_DIRS="$HERE/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
args=()
if [[ "${FEDI3_SOFTWARE_RENDERING:-0}" == "1" ]]; then
  export WEBKIT_DISABLE_COMPOSITING_MODE="${WEBKIT_DISABLE_COMPOSITING_MODE:-1}"
  export LIBGL_ALWAYS_SOFTWARE="${LIBGL_ALWAYS_SOFTWARE:-1}"
  export LIBGL_DRI3_DISABLE="${LIBGL_DRI3_DISABLE:-1}"
  export MESA_LOADER_DRIVER_OVERRIDE="${MESA_LOADER_DRIVER_OVERRIDE:-llvmpipe}"
  args+=(--enable-software-rendering)
fi
cd "$HERE/usr"
exec "./fedi3" "${args[@]}" "$@"
EOF
chmod +x "$appdir/AppRun"

skip_lib() {
  case "$1" in
    libEGL.so.*|libGL.so.*|libGLX.so.*|libGLdispatch.so.*|libOpenGL.so.*|libgbm.so.*|libdrm.so.*|libwayland-client.so.*|libwayland-egl.so.*|libxshmfence.so.*)
      return 0
      ;;
  esac
  return 1
}

bundle_all_deps() {
  local changed=1
  while [[ "$changed" -eq 1 ]]; do
    changed=0
    while IFS= read -r bin; do
      local deps
      deps="$(ldd "$bin" 2>/dev/null | awk '/=>/ {print $1, $3}')" || true
      while read -r name path; do
        [[ -z "$name" ]] && continue
        case "$name" in
          ld-linux*|libc.so.*|libm.so.*|libpthread.so.*|librt.so.*|libdl.so.*|libgcc_s.so.*) continue ;;
        esac
        if skip_lib "$name"; then
          continue
        fi
        if [[ "$path" == "not" ]]; then
          if resolve_lib_path "$name" >/dev/null; then
            local src
            src="$(resolve_lib_path "$name")"
            cp -f "$src" "$appdir/usr/lib/$name"
            changed=1
          else
            echo "Warning: unable to resolve missing library: $name"
          fi
          continue
        fi
        if [[ -n "$path" && -f "$path" && "$path" != "$appdir"* ]]; then
          local base
          base="$(basename "$path")"
          if skip_lib "$base"; then
            continue
          fi
          if [[ ! -f "$appdir/usr/lib/$base" ]]; then
            cp -f "$path" "$appdir/usr/lib/$base"
            changed=1
          fi
        fi
      done <<< "$deps"
    done < <(find "$appdir/usr" -type f \( -name "fedi3" -o -name "*.so*" \))
  done
}

if [[ "${FEDI3_BUNDLE_DEPS:-0}" == "1" ]]; then
  bundle_all_deps
else
  echo "Skipping dependency bundling (set FEDI3_BUNDLE_DEPS=1 to enable)."
fi

rm -f "$appdir/usr/lib"/libEGL.so.* \
  "$appdir/usr/lib"/libGL.so.* \
  "$appdir/usr/lib"/libGLX.so.* \
  "$appdir/usr/lib"/libGLdispatch.so.* \
  "$appdir/usr/lib"/libOpenGL.so.* \
  "$appdir/usr/lib"/libgbm.so.* \
  "$appdir/usr/lib"/libdrm.so.* \
  "$appdir/usr/lib"/libwayland-client.so.* \
  "$appdir/usr/lib"/libwayland-egl.so.* \
  "$appdir/usr/lib"/libwayland-cursor.so.* \
  "$appdir/usr/lib"/libwayland-server.so.* \
  "$appdir/usr/lib"/libxshmfence.so.* || true

chmod -R a+rX "$appdir/usr/lib"
chmod a+rx "$appdir/usr/fedi3"

appimagetool_bin="${APPIMAGETOOL:-$repo_root/.cache/appimagetool-x86_64.AppImage}"
if [[ ! -x "$appimagetool_bin" ]] || head -n 1 "$appimagetool_bin" | grep -qi "not found"; then
  mkdir -p "$repo_root/.cache"
  curl -L -o "$appimagetool_bin" \
    https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage
  chmod +x "$appimagetool_bin"
fi

mkdir -p "$repo_root/dist"
APPIMAGE_EXTRACT_AND_RUN=1 "$appimagetool_bin" "$appdir" "$appimage_out"
echo "AppImage created: $appimage_out"
