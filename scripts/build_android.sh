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
rustup target add aarch64-linux-android
(cd "$repo_root" && \
  PATH="$ndk_bin:$PATH" \
  CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ndk_bin/aarch64-linux-android24-clang" \
  CC_aarch64_linux_android="$ndk_bin/aarch64-linux-android24-clang" \
  AR_aarch64_linux_android="$ndk_bin/llvm-ar" \
  cargo build --release --target aarch64-linux-android -p fedi3_core --lib --locked)
mkdir -p "$repo_root/app/android/app/src/main/jniLibs/arm64-v8a"
cp "$repo_root/target/aarch64-linux-android/release/libfedi3_core.so" "$repo_root/app/android/app/src/main/jniLibs/arm64-v8a/libfedi3_core.so"
(cd "$repo_root/app" && flutter build apk --"$profile" --target-platform android-arm64)
