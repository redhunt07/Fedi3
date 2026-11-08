# Client 0.10.0 publication verification — 2026-11-08

Release: [v0.10.0](https://github.com/redhunt07/Fedi3/releases/tag/v0.10.0).
Source commit: `ad15842ce13d10740073ecf6b9e4803bcaf63a5a`.

- [Main CI](https://github.com/redhunt07/Fedi3/actions/runs/42456638207): passed.
- [Release CI](https://github.com/redhunt07/Fedi3/actions/runs/42456653252): passed,
  including Linux x86_64 AppImage and native Windows x64 ZIP builds.
- 304 standard Rust tests and 184 Flutter tests passed. Strict Clippy, Flutter
  analysis, formatting, PostgreSQL quota, backup and deployment contracts passed.
- The isolated legacy RSA 0.9 and WebRTC 0.12 oracle passed, including both SDP
  roles. Existing key encodings, DIDs, signatures and encrypted envelopes remain
  compatible. The release lockfile contains neither `rsa` nor `webrtc-dtls`.
- The mandatory RustSec audit found zero vulnerabilities. Five unmaintained
  dependency notices remain; no advisory is ignored.
- Stable assets: `Fedi3-linux-x86_64.AppImage`, `Fedi3-windows-x64.zip`,
  `checksums.txt`. The release workflow hashes those exact packages before upload.
- Client diagnostics remain local; there is no automatic telemetry Git push.

The forward-receipt integration test now explicitly sets its isolated relay's
public base URL and verifies its actor key against that core's identity. This
prevents default-port redirects from reaching an unrelated running local core.

Older clients need a manual first upgrade to receive the updater. Profile data,
keys and OS-keyring credentials must be preserved. Android/iOS/macOS package
publication and interactive Windows runtime tests are not part of these results.

Linux CI installs the GStreamer development packages and removes the conflict
with `libunwind-14-dev` preinstalled on the Ubuntu 22.04 runner.

## Published-package verification

Both packages and `checksums.txt` were downloaded from their public URLs.
Their SHA256 values match the checksum file and GitHub asset digests. The Windows
ZIP passes CRC validation and contains the executable, core DLL and Flutter DLL.
The Linux executable, core and Flutter library were extracted from the AppImage;
the published core loads and its FFI version function responds correctly.

| Asset | SHA256 |
| --- | --- |
| `Fedi3-linux-x86_64.AppImage` | `9fb3dea2bd191566f31ae01e254899908b0d1b7c9bdecfefffa473696240b334` |
| `Fedi3-windows-x64.zip` | `ea3127251cf208fe641f8f7aa82bee5898677c0ad350b95960f30b95f02838b7` |

## Follow-up: Linux GUI startup regression

A subsequent launch on a newer Arch desktop failed with `MOUNT_2_40` and then
`LIBSYSTEMD_251`: bundled older dependencies shadowed the host GLib stack. The
checks above validated the packages and core FFI, but did not test the complete
GUI startup. The corrected packaging policy and required two-distribution GUI
startup gate are prepared for 0.10.1. Existing profile data is unaffected.
