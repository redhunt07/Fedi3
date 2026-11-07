#!/usr/bin/env bash
# Called by Xcode before Runner links; requires macOS/Xcode and Rust.
set -euo pipefail
[[ "$(uname -s)" == Darwin ]] || { echo 'The iOS core requires macOS and the Xcode SDK.' >&2; exit 2; }
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
sdk_platform="${PLATFORM_NAME:-iphoneos}"
case "$sdk_platform" in iphoneos|iphonesimulator) ;; *) echo "Unsupported Apple platform: $sdk_platform" >&2; exit 2;; esac
export SDKROOT
SDKROOT="$(xcrun --sdk "$sdk_platform" --show-sdk-path)"
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-13.0}"
read -r -a task_arches <<< "${ARCHS:-arm64}"
task_archives=()
for task_arch in "${task_arches[@]}"; do
  case "$sdk_platform:$task_arch" in
    iphoneos:arm64) task_target=aarch64-apple-ios ;;
    iphonesimulator:arm64) task_target=aarch64-apple-ios-sim ;;
    iphonesimulator:x86_64) task_target=x86_64-apple-ios ;;
    *) echo "Unsupported iOS architecture: $task_arch" >&2; exit 2 ;;
  esac
  rustup target add "$task_target"
  (cd "$repo_root" && cargo rustc -p fedi3_core --lib --release --target "$task_target" --locked --crate-type staticlib)
  task_archives+=("$repo_root/target/$task_target/release/libfedi3_core.a")
done
task_output="${BUILT_PRODUCTS_DIR:-$repo_root/app/build/ios/core}"
mkdir -p "$task_output"
xcrun lipo -create "${task_archives[@]}" -output "$task_output/libfedi3_core.a"
