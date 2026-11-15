#!/usr/bin/env bash
# Run source release gates without changing versions, committing or publishing.
set -euo pipefail

if [[ $# -ne 0 ]]; then
  echo "Usage: $0" >&2
  echo "Optional: FEDI3_VERIFY_LOG_DIR, FEDI3_TEST_POSTGRES_URL (isolated test DB)." >&2
  exit 2
fi
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
for tool in cargo flutter dart python3 docker shellcheck; do
  command -v "$tool" >/dev/null || { echo "Missing command: $tool" >&2; exit 2; }
done
python3 -c 'import yaml' || { echo "Install Python PyYAML first." >&2; exit 2; }
log_dir="${FEDI3_VERIFY_LOG_DIR:-$(mktemp -d -t fedi3-release-checks-XXXXXXXX)}"
mkdir -p "$log_dir"
log_dir="$(cd "$log_dir" && pwd)"
echo "Logs: $log_dir"

run_check() {
  local name="$1"
  shift
  echo "Checking $name"
  if "$@" >"$log_dir/$name.log" 2>&1; then
    echo "PASS $name"
  else
    echo "FAIL $name (see $log_dir/$name.log)" >&2
    tail -n 60 "$log_dir/$name.log" >&2
    return 1
  fi
}

run_check git-diff git diff --check
run_check rust-format cargo fmt --all -- --check
run_check rust-clippy cargo clippy --workspace --all-targets --locked -- -D warnings
run_check rust-build cargo build --workspace --locked
run_check rust-tests cargo test --workspace --locked -- --test-threads=4
run_check legacy-compatibility python3 scripts/verify_legacy_webrtc.py
run_check rust-audit cargo audit
for test in scripts/tests/test_*.py; do
  run_check "$(basename "$test" .py)" python3 "$test"
done
run_check platform-config bash scripts/verify_platform_config.sh
run_check shellcheck shellcheck -x -P scripts \
  scripts/lib/dotenv.sh scripts/backup_relay_pg.sh scripts/restore_relay_pg.sh \
  scripts/backup_relay_media.sh scripts/restore_relay_media.sh \
  scripts/relay_diagnose.sh scripts/install_server.sh scripts/install_yunohost.sh \
  scripts/verify_release.sh
(
  cd app
  run_check dart-format dart format --output=none --set-exit-if-changed lib test
  run_check flutter-analysis flutter analyze --fatal-infos
  run_check flutter-tests flutter test --reporter expanded
)
if [[ -n "${FEDI3_TEST_POSTGRES_URL:-}" ]]; then
  run_check postgres-quota cargo test -p fedi3_relay \
    postgres_media_quota_reservation_is_atomic --locked -- --ignored
else
  echo "NOT RUN: PostgreSQL quota (set FEDI3_TEST_POSTGRES_URL to an isolated test DB)."
fi
echo "Source gates passed. Review skipped tests, audit warnings and target-platform release checks."
echo "Release procedure: docs/it/release_checklist.md (Italian), docs/release_checklist.md (English)."
