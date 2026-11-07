# Client, repository and update audit — 7 November 2026

> Follow-up, 8 November 2026: the two security blockers recorded in this historical snapshot have been resolved. See [the compatibility migration verification](crypto_compatibility_2026-11-08.md).

Scope: local working tree on `feat/relay-directory`, including its pre-existing
uncommitted core, relay and mobile changes; read-only inspection of public
`redhunt07/Fedi3` releases and CI. Existing work was preserved. No commits,
pushes, tags, public releases or production deployments were performed.

## Result

The functional checks pass, and an update system is implemented. **This is not a
clean security release:** two RustSec vulnerability findings and seven
unmaintained-dependency notices remain. Release-tag CI now blocks on the audit;
a passing functional test suite cannot override that gate.

## Problems corrected

- Published CI run [42417030565](https://github.com/redhunt07/Fedi3/actions/runs/42417030565)
  failed the directory validator and tunnel RTT test. The local directory now
  includes Palermo with its normalized URL, and its bundled copy matches.
  RTT measurements below one millisecond are rounded to one instead of being
  discarded as the zero/not-measured sentinel. A unit regression covers this.
- Three strict Clippy failures were corrected. libp2p was upgraded from 0.53 to
  0.57 with native async codecs, current event types and a boxed Identify event.
  Infra still supports the private and legacy DHT protocols through separate
  stores; private records are not copied into the public DHT.
- Rust dependencies were refreshed within the declared constraints. S3 now uses
  the current HTTPS client without legacy Rustls 0.21/Hyper 0.14 features.
  RustSec vulnerability findings fell from **39 to 2**. No advisory was ignored.
- Local telemetry now tolerates truncated/corrupt JSON lines, serializes writes,
  rotates at 2 MiB, bounds legacy oversized logs and deletes both copies on clear.
  Disk failures cannot interrupt the action being diagnosed.
- Release scripts now build the actual AppImage, hash only the current package,
  support the correct Flutter location, work without executable script bits, handle
  Windows debug builds correctly, fail on native build errors, and keep Windows
  `-NoBump` version strings valid. Linux CI installs the WebKit 4.1 development
  dependency required by the desktop webview plugin.

## Telemetry and Git

No automatic telemetry-to-GitHub transport or Git push was found in the client,
tracked scripts or workflow. Client diagnostics are local, disabled by default,
and explicitly exportable by the user. The UI and FAQ now state this accurately.
A regression checks that recording diagnostics issues no shared HTTP requests.
Relay-to-relay signed statistics/discovery via HTTP are separate features.

CI has `contents: read` by default. Only the draft-release publishing job has
`contents: write`, and it uploads application packages/checksums, not diagnostic
logs. Existing diagnostics or Git history were not erased.

## Updates implemented

Settings → Client updates provides manual checking, notes, download progress and
SHA256 verification. Release builds check after startup and every six foreground
hours. Drafts, prereleases, downgrades, foreign asset URLs, malformed metadata,
oversized responses and incompatible architectures are rejected. No account
credentials or diagnostic payloads are sent with the checks.

Linux x86_64 AppImages support an atomic replacement with a `.previous` copy and
manual restart. Windows x64 receives a verified ZIP and guided installation of
the complete bundle. Source/package installs, macOS and mobile use their existing
installation source or release page. Mobile/macOS installation is not automated.

Version `0.10.0+1` is prepared so the historical stable `0.9.19` is not offered as
an upgrade to current source builds. CI validates `vX.Y.Z` against pubspec, builds
Linux and Windows packages together with the native core, computes checksums and
creates a draft release only after required checks pass. Publication is manual.
See [the update guide](../updates.md).

## Security blockers

| Finding | Dependency path | Required follow-up |
| --- | --- | --- |
| [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html) | `rsa` 0.9.10, used for compatibility envelopes and signatures | Replace affected private-key operations/provider with interoperability and timing review; upstream lists no patched version. |
| [RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html) | WebRTC DTLS → P-256/P-384 → `crypto-bigint` 0.5.5 | Migrate to a DTLS/provider stack using patched arithmetic, rather than force an incompatible dependency version. WebRTC P2P is disabled by default. |

The remaining maintenance notices concern bincode, paste, pin-utils,
pqcrypto-internals, pqcrypto-kyber, pqcrypto-traits and rustls-pemfile. In
particular, changing the chat KEM requires protocol compatibility work. These
findings do not establish that a practical exploit against this app was
successfully reproduced. See [SECURITY.md](../../SECURITY.md).

## Verification

| Check | Result |
| --- | --- |
| `flutter test --no-pub` | 184 passed |
| `flutter analyze --no-pub` | No issues |
| `cargo test --workspace --locked --offline -- --test-threads=4` | 297 passed; two explicitly opt-in tests run separately below |
| P2P library tests with `--include-ignored` | 12 passed, including public IPFS bootstrap; also rerun after the final Identify event change |
| PostgreSQL atomic quota test with `--ignored` | Passed against a freshly initialized temporary PostgreSQL instance after dependency updates |
| S3 current-client signed upload regression | Passed, also included in the workspace suite |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | Passed |
| Rust/Dart formatting and `git diff --check` | Passed |
| `flutter build linux --release --no-pub` | Passed |
| Deploy/OpenAPI, relay directory, backup/restore, forward receipts and release-contract Python checks | Passed |
| Platform configuration verifier, release-script ShellCheck and Linux preflight | Passed |
| RustSec audit using cargo-audit 0.22.2 | **Failed: 2 vulnerabilities, 7 unmaintained notices; no ignores** |

The two opt-in Rust tests (public P2P bootstrap and PostgreSQL quota) both passed,
for **299 distinct passing Rust tests** across the workspace and explicit runs.
The AppImage replacement tests use temporary files and verify integrity,
permissions, backup creation, cleanup and preservation of the current app on a
tampered update. They do not replace a user's installed application.


## Limits

Windows, macOS, Android and iOS native builds were not executed in this Linux
session. Platform configuration checks do not substitute for device, store,
notification or call testing. The release workflow was checked locally but not
run or published on GitHub. Production federation and live accounts were not
modified. No finite test suite proves the absence of every possible bug.
