#!/usr/bin/env bash
# Build the Android APK for one ABI exactly as the F-Droid recipe needs it:
# compile the Rust core for the NDK target, drop it into jniLibs, then let
# Flutter produce the split APK for that ABI.
set -euo pipefail

abi="${1:-arm64-v8a}"
case "$abi" in
  arm64-v8a) triple="aarch64-linux-android"; platform="android-arm64" ;;
  x86_64) triple="x86_64-linux-android"; platform="android-x64" ;;
  *) echo "Usage: $0 [arm64-v8a|x86_64]" >&2; exit 2 ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v flutter >/dev/null 2>&1; then
  export PATH="$HOME/flutter/bin:$PATH"
fi
command -v flutter >/dev/null 2>&1 || { echo "Missing command: flutter" >&2; exit 2; }
command -v cargo >/dev/null 2>&1 || { echo "Missing command: cargo" >&2; exit 2; }

if [[ -z "${JAVA_HOME:-}" ]]; then
  for candidate in /usr/lib/jvm/java-21-openjdk /usr/lib/jvm/java-21-openjdk-amd64; do
    if [[ -d "$candidate" ]]; then
      export JAVA_HOME="$candidate"
      break
    fi
  done
fi

sdk_root="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-$HOME/Android/Sdk}}"
ndk_root="${FEDI3_ANDROID_NDK:-$sdk_root/ndk/28.2.13676358}"
ndk_bin="$ndk_root/toolchains/llvm/prebuilt/linux-x86_64/bin"
cc="$ndk_bin/${triple}24-clang"
[[ -x "$cc" ]] || { echo "Missing NDK clang: $cc (set FEDI3_ANDROID_NDK)" >&2; exit 2; }

rustup target add "$triple" >/dev/null
upper="$(printf '%s' "$triple" | tr 'a-z-' 'A-Z_')"
(cd "$repo_root" && PATH="$ndk_bin:$PATH" \
  env "CARGO_TARGET_${upper}_LINKER=$cc" "CC_${upper}=$cc" "AR_${upper}=$ndk_bin/llvm-ar" \
  cargo build --release --target "$triple" -p fedi3_core --lib --locked)

mkdir -p "$repo_root/app/android/app/src/main/jniLibs/$abi"
cp "$repo_root/target/$triple/release/libfedi3_core.so" \
  "$repo_root/app/android/app/src/main/jniLibs/$abi/libfedi3_core.so"

(cd "$repo_root/app" && flutter build apk --release --target-platform "$platform" --split-per-abi)
