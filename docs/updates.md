# Client updates

Settings → Client updates checks the latest stable GitHub Release of
`redhunt07/Fedi3`. Release builds also check after startup and every six hours
while the app is in the foreground. A version is announced once per session.
Debug builds check only when the update screen is opened. Checks are read-only
HTTPS requests and do not send profile tokens, log files or telemetry.

The version comes from the installed application metadata. Only a strictly
newer `vX.Y.Z` stable release is offered. Drafts, prereleases, invalid metadata,
foreign download URLs and incompatible architectures are rejected. The first
version containing this updater is published as `0.10.0+1`, above the historical
`0.9.19` release. Older clients require a manual first upgrade.

## Installation

- **Linux x86_64 AppImage:** download, verify, then choose Install AppImage update.
  The AppImage named by `APPIMAGE` is replaced atomically on the same filesystem.
  Its previous contents are kept as `<original path>.previous`. Close and reopen
  Fedi3 to run the new version. If needed, close Fedi3 and restore that previous
  file to the original name. A failed download or verification leaves the
  installed application intact. The AppImage directory must be writable.
- **Windows x64:** download the verified ZIP and open its download folder. Close
  Fedi3, extract the entire package into the application directory (or a new
  directory), then reopen it. Replace the bundled core DLL together with the UI.
  No administrator access, local compilation or remote PowerShell execution is
  needed for a portable release.
- **Linux source/package installs, macOS and mobile:** use the release page and
  the installation source/package manager/store that provided the application.
  The client does not replace system-managed files or offer an x64 package to
  another architecture. This workflow does not yet produce mobile or macOS assets.

Profile configuration, keyring credentials, private keys and databases stay in
their existing data directories. Do not delete them during an update.

## Download verification

Packages are streamed to a private temporary directory, with size limits,
network timeouts and progress indication. The SHA256 must match the release
asset's `sha256:` digest, or its unique entry in `checksums.txt` for older
releases. Failed and partial downloads are removed. AppImage installation checks
the checksum again after staging the file. SHA256 verifies integrity against the
trusted GitHub release; this is not an independent code-signing system.

Expected package names:

| Platform | Asset |
| --- | --- |
| Linux x86_64 | `Fedi3-linux-x86_64.AppImage` |
| Windows x64 | `Fedi3-windows-x64.zip` |
| Checksums | `checksums.txt` |

Each checksum line has the format `<64 hex characters>  <exact asset name>`.

## Preparing a release

1. Set `app/pubspec.yaml` to `X.Y.Z+N`, increasing the version and build number.
2. Run the Rust/Flutter tests and deployment checks. Commit the reviewed source.
3. Push a tag `vX.Y.Z` matching the pubspec version. A mismatch fails packaging.
4. CI builds the core and Flutter client together for Linux and Windows, computes
   checksums for those exact packages and creates a **draft** GitHub Release.
5. Download and smoke-test both packages on their target systems, then publish
   the draft. The client sees the update only once it is published as stable.

The CI token is read-only by default. Only the draft-release job receives
`contents: write`. CI does not commit logs or push telemetry to the repository.

Local packaging:

```sh
NO_BUMP=1 bash scripts/release_full_linux.sh
```

```powershell
./scripts/release_full_windows.ps1 -NoBump
```

Build dependencies are required on the build machine, not on users' machines.
Both packaging paths include the native core and produce SHA256 checksums.

Release tags also require a passing RustSec audit. The RSA/WebRTC dependency
blockers were removed on 8 November 2026 with compatibility tests against the
previous implementation. See [SECURITY.md](../SECURITY.md) and the
[migration verification](audit/crypto_compatibility_2026-11-08.md).
The audit gate remains enforced; no advisory is ignored.

### Maintainer-requested publication

A push of `codex/release/vX.Y.Z` requests publication through CI. The branch must
point to the current `main` commit and the version must match pubspec. The same
Rust, Flutter, operations, compatibility, audit and platform gates run before
Linux/Windows packaging. The publishing job verifies checksums, uploads the
packages and publishes the release as latest stable. Existing published releases
are never overwritten, and an existing draft must target the same commit.
Ordinary tag builds keep the draft/manual-publication path described above.

Linux packages additionally require a real GUI startup under a virtual display on
Ubuntu 22.04 and 24.04 before publication. The window must become visible after
Flutter renders its first frame; successful compilation or loading only the core
library is insufficient. This gate covers the 0.10.0 host-library ABI regression.
