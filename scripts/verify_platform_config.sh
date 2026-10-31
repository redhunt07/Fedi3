#!/usr/bin/env bash
# Static verification of the platform configuration that makes or breaks a release.
# Fails with a clear message when a required key is missing or wrong.
set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="$ROOT_DIR/app"
FAIL=0

fail() {
  echo "FAIL: $*" >&2
  FAIL=$((FAIL + 1))
}

ok() {
  echo "ok:   $*"
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "FAIL: required command '$1' not found" >&2
    exit 2
  }
}

need_cmd python3

# ---------------------------------------------------------------- Android
MAIN_MANIFEST="$APP_DIR/android/app/src/main/AndroidManifest.xml"
if [[ ! -f "$MAIN_MANIFEST" ]]; then
  fail "missing $MAIN_MANIFEST"
else
  python3 - "$MAIN_MANIFEST" <<'PY'
import sys, xml.etree.ElementTree as ET

path = sys.argv[1]
NS = "{http://schemas.android.com/apk/res/android}"
problems = []
try:
    root = ET.parse(path).getroot()
except Exception as e:  # noqa: BLE001
    print(f"FAIL: cannot parse {path}: {e}", file=sys.stderr)
    sys.exit(1)

perms = {
    (p.get(NS + "name") or "").strip()
    for p in root.findall("uses-permission")
}
for required in ("android.permission.INTERNET", "android.permission.POST_NOTIFICATIONS"):
    if required not in perms:
        problems.append(f"missing <uses-permission {required}> in the main manifest")

app = root.find("application")
if app is None:
    problems.append("missing <application> element")
else:
    if (app.get(NS + "networkSecurityConfig") or "") != "@xml/network_security_config":
        problems.append("application has no android:networkSecurityConfig=\"@xml/network_security_config\"")
    if (app.get(NS + "allowBackup") or "").lower() != "false":
        problems.append("application should set android:allowBackup=\"false\" (tokens are backed up otherwise)")

if problems:
    for p in problems:
        print(f"FAIL: {p}", file=sys.stderr)
    sys.exit(1)
print("ok:   Android manifest has INTERNET, POST_NOTIFICATIONS, networkSecurityConfig and allowBackup=false")
PY
  [[ $? -eq 0 ]] || FAIL=$((FAIL + 1))
fi

# Debug/profile manifests must not be the only place where INTERNET lives.
for variant in debug profile; do
  f="$APP_DIR/android/app/src/$variant/AndroidManifest.xml"
  if [[ -f "$f" ]] && grep -q "android.permission.INTERNET" "$f"; then
    ok "INTERNET also declared in $variant manifest (harmless duplicate)"
  fi
done

# Network security config
NSC="$APP_DIR/android/app/src/main/res/xml/network_security_config.xml"
if [[ ! -f "$NSC" ]]; then
  fail "missing $NSC (cleartext to 127.0.0.1 is blocked on API 28+)"
else
  python3 - "$NSC" <<'PY'
import sys, xml.etree.ElementTree as ET

path = sys.argv[1]
problems = []
try:
    root = ET.parse(path).getroot()
except Exception as e:  # noqa: BLE001
    print(f"FAIL: cannot parse {path}: {e}", file=sys.stderr)
    sys.exit(1)

base = root.find("base-config")
if base is None:
    problems.append("missing <base-config>")
elif (base.get("cleartextTrafficPermitted") or "").lower() != "false":
    problems.append("<base-config> must set cleartextTrafficPermitted=\"false\"")

allowed = set()
for dc in root.findall("domain-config"):
    if (dc.get("cleartextTrafficPermitted") or "").lower() != "true":
        continue
    for d in dc.findall("domain"):
        name = (d.text or "").strip()
        if name:
            allowed.add(name)

for required in ("127.0.0.1", "localhost"):
    if required not in allowed:
        problems.append(f"domain-config must permit cleartext for {required}")

if problems:
    for p in problems:
        print(f"FAIL: {p}", file=sys.stderr)
    sys.exit(1)
print("ok:   network_security_config permits cleartext only for loopback")
PY
  [[ $? -eq 0 ]] || FAIL=$((FAIL + 1))
fi

# ---------------------------------------------------------------- macOS
check_entitlements() {
  local file="$1"
  local label="$2"
  if [[ ! -f "$file" ]]; then
    fail "missing $file"
    return
  fi
  local missing=()
  for key in \
    "com.apple.security.network.client" \
    "com.apple.security.network.server" \
    "com.apple.security.files.user-selected.read-write"; do
    if ! python3 - "$file" "$key" <<'PY'
import plistlib, sys

path, key = sys.argv[1], sys.argv[2]
with open(path, "rb") as fh:
    data = plistlib.load(fh)
sys.exit(0 if data.get(key) is True else 1)
PY
    then
      missing+=("$key")
    fi
  done
  if [[ ${#missing[@]} -gt 0 ]]; then
    fail "$label entitlements missing: ${missing[*]}"
  else
    ok "$label entitlements grant network client/server and user-selected files"
  fi
}

check_entitlements "$APP_DIR/macos/Runner/DebugProfile.entitlements" "macOS debug"
check_entitlements "$APP_DIR/macos/Runner/Release.entitlements" "macOS release"

# ---------------------------------------------------------------- iOS
IOS_PLIST="$APP_DIR/ios/Runner/Info.plist"
if [[ ! -f "$IOS_PLIST" ]]; then
  fail "missing $IOS_PLIST"
else
  python3 - "$IOS_PLIST" <<'PY'
import plistlib, sys

path = sys.argv[1]
problems = []
try:
    with open(path, "rb") as fh:
        data = plistlib.load(fh)
except Exception as e:  # noqa: BLE001
    print(f"FAIL: cannot parse {path}: {e}", file=sys.stderr)
    sys.exit(1)

ats = data.get("NSAppTransportSecurity")
if not isinstance(ats, dict):
    problems.append("missing NSAppTransportSecurity (the local core is plain HTTP)")
elif ats.get("NSAllowsLocalNetworking") is not True:
    problems.append("NSAppTransportSecurity must set NSAllowsLocalNetworking=true")

for key in ("NSPhotoLibraryUsageDescription", "NSCameraUsageDescription"):
    value = data.get(key)
    if not isinstance(value, str) or not value.strip():
        problems.append(f"missing or empty {key}")

if problems:
    for p in problems:
        print(f"FAIL: {p}", file=sys.stderr)
    sys.exit(1)
print("ok:   iOS Info.plist has ATS local networking and media usage descriptions")
PY
  [[ $? -eq 0 ]] || FAIL=$((FAIL + 1))
fi

# ---------------------------------------------------------------- Gradle / app id
GRADLE="$APP_DIR/android/app/build.gradle.kts"
if [[ ! -f "$GRADLE" ]]; then
  fail "missing $GRADLE"
else
  if grep -q 'com\.example\.' "$GRADLE"; then
    fail "build.gradle.kts still uses a com.example.* applicationId/namespace"
  else
    ok "Android applicationId is not a placeholder"
  fi
  if grep -qE 'signingConfigs\.(create|getByName)\("release"\)' "$GRADLE"; then
    ok "release signing config declared"
  else
    fail "build.gradle.kts has no release signing config"
  fi
  if grep -q 'key.properties' "$GRADLE" \
    && grep -q 'signingConfigs.getByName("release")' "$GRADLE" \
    && grep -qE 'if *\(.*(hasReleaseKeystore|keystorePropertiesFile)' "$GRADLE"; then
    ok "release build uses the release signing config when key.properties is present"
  else
    fail "release build does not conditionally use signingConfigs.release from key.properties"
  fi
fi

LINUX_CMAKE="$APP_DIR/linux/CMakeLists.txt"
if [[ -f "$LINUX_CMAKE" ]] && grep -q 'com\.example\.' "$LINUX_CMAKE"; then
  fail "linux/CMakeLists.txt APPLICATION_ID still uses com.example.*"
else
  ok "Linux application id is not a placeholder"
fi

# ---------------------------------------------------------------- summary
echo
if [[ $FAIL -gt 0 ]]; then
  echo "$FAIL platform configuration check(s) failed." >&2
  exit 1
fi
echo "All platform configuration checks passed."
