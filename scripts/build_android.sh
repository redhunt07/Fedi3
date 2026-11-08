#!/usr/bin/env bash
# Build the native core and Flutter client together; avoid APKs missing the core.
set -euo pipefail
profile="${1:-debug}"
case "$profile" in debug|release) ;; *) echo 'Usage: scripts/build_android.sh [debug|release]' >&2; exit 2;; esac
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
sdk_root="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-$HOME/Android/Sdk}}"
ndk_root="${FEDI3_ANDROID_NDK:-$sdk_root/ndk/28.2.13676358}"
ndk_bin="$ndk_root/toolchains/llvm/prebuilt/linux-x86_64/bin"
[[ -x "$ndk_bin/aarch64-linux-android24-clang" ]] || { echo 'Install Android NDK 28.2.13676358 or set FEDI3_ANDROID_NDK' >&2; exit 2; }
if [[ "$profile" == release && ! -f "$repo_root/app/android/key.properties" ]]; then
  echo 'Configure app/android/key.properties for a signed release; use debug for local validation.' >&2
  exit 2
fi

# Debug builds also ship x86_64 so the app can run on Android emulators; release
# APKs stay arm64-only to keep the download small.
targets=("aarch64-linux-android")
abis=("arm64-v8a")
platforms=("android-arm64")
if [[ "$profile" == debug ]]; then
  targets+=("x86_64-linux-android")
  abis+=("x86_64")
  platforms+=("android-x64")
fi

build_core() {
  local triple="$1"
  local abi="$2"
  local cc="$ndk_bin/${triple}24-clang"
  local upper
  upper="$(printf '%s' "$triple" | tr 'a-z-' 'A-Z_')"
  rustup target add "$triple"
  (cd "$repo_root" && \
    PATH="$ndk_bin:$PATH" \
    env "CARGO_TARGET_${upper}_LINKER=$cc" "CC_${upper}=$cc" "AR_${upper}=$ndk_bin/llvm-ar" \
    cargo build --release --target "$triple" -p fedi3_core --lib --locked)
  mkdir -p "$repo_root/app/android/app/src/main/jniLibs/$abi"
  cp "$repo_root/target/$triple/release/libfedi3_core.so" \
    "$repo_root/app/android/app/src/main/jniLibs/$abi/libfedi3_core.so"
}

for i in "${!targets[@]}"; do
  build_core "${targets[$i]}" "${abis[$i]}"
done

platform_list="$(IFS=,; echo "${platforms[*]}")"
(cd "$repo_root/app" && flutter build apk --"$profile" --target-platform "$platform_list")
