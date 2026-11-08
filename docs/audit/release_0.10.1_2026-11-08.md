# Client 0.10.1 publication verification — 2026-11-08

Release: [v0.10.1](https://github.com/redhunt07/Fedi3/releases/tag/v0.10.1).
Source commit: `dfb98c277eccc1dcd7091700edc51b3354a157df`.

## Regression and correction

The original 0.10.0 AppImage passed checksum and core-FFI checks but failed before
opening the GUI on a newer Arch desktop: bundled old `libmount` shadowed the host
GLib requirement `MOUNT_2_40`. Once removed, old `libsystemd` similarly lacked
`LIBSYSTEMD_251`. Old Fontconfig also rejected the host's font configuration.

A single packaging policy now excludes the host desktop's base libraries from
both dependency copying and pre-existing Flutter bundle files, including GLib's
mount/blkid/systemd/udev/PCRE2/FFI dependencies and the host font stack. The local
repacked image opened its window on the affected desktop; the maintainer confirmed
it was visible. Profiles, databases, keyring credentials and encryption formats
were not migrated or reset.

## Release gates

- [Main CI](https://github.com/redhunt07/Fedi3/actions/runs/42461970385): passed.
- [Release CI](https://github.com/redhunt07/Fedi3/actions/runs/42461972792): passed,
  including native Linux/Windows builds and mandatory GUI startup of the exact
  packaged AppImage on Ubuntu 22.04 and 24.04 under Xvfb and a fresh D-Bus session.
- The GUI startup gate requires a mapped Fedi3 window, which the Linux runner
  shows only after Flutter's first frame, and verifies the process stays alive.
- The ABI regression test compiles two incompatible, versioned `libmount.so.1`
  fixtures and reproduces `MOUNT_2_40` failure before applying the host policy.
- 304 standard Rust tests and 184 Flutter tests passed, alongside legacy RSA and
  WebRTC interoperability, PostgreSQL, strict Clippy/analyzer and deployment checks.
- Dependency audit: zero vulnerabilities, five maintenance notices, no ignored
  advisories. Cryptographic implementation and profile/message formats are unchanged.

Linux 0.10.0 users whose GUI cannot start need a manual download of 0.10.1. The
updater becomes usable once the fixed client starts. macOS/mobile packaging and
interactive Windows runtime testing remain outside this verification.

The Ubuntu 24.04 startup gate also exposed old bundled Xlib preventing Mesa
from creating an EGL context. Xlib and XCB now follow the host graphics drivers.
The smoke runners explicitly install the OpenGL ES and Wayland desktop runtime.

## Public-download verification

Both packages and `checksums.txt` were downloaded from public URLs. Their SHA256
values match the checksums and GitHub asset digests. The Windows ZIP passes CRC
and contains the executable and core/Flutter DLLs. The full extracted AppImage
client passes `ldd -r` on the affected Arch desktop, with no missing libraries or
incompatible symbols; the disallowed host-library copies are absent.

| Asset | SHA256 |
| --- | --- |
| `Fedi3-linux-x86_64.AppImage` | `70db90896c430ad73bb01361079b200bcb0dad5d72a040c86de0e51e4c686ae1` |
| `Fedi3-windows-x64.zip` | `becc29514f7188a700ecd2fdfee98ff4bf424938f2e43c3ad8c02c637f142555` |
