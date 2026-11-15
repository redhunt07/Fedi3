# Fedi3 0.12.4 release verification — 16 November 2026

Source version: **0.12.4+4020**. This report separates source checks from package
and deployment verification. Release notes are in [v0.12.4](../releases/v0.12.4.md).

## Scope

- Received voice-note play, retry, pause and replay; legacy attachment MIME
  fields; video-note sound and the selected fullscreen attachment.
- Shared timeline/activity reply editor with up to eight media attachments,
  media-only replies, removal and upload reuse after failed sends.
- Corrected public-site metadata, localization, relay configuration guidance,
  local key storage and relay migration descriptions. Attachment files in the
  media backend remain outside end-to-end encryption; this is now explicit.
- Version/build alignment across client, crates, public site, F-Droid metadata,
  announcements, Android changelogs and bilingual release notes.

## Source verification

**360 Rust tests passed** (4 ignored in the ordinary suite). The Rust workspace gates, compatibility oracle, dependency audit, Python
contracts, platform checks and ShellCheck passed. The full Flutter suite passed
with **278 tests**, including a narrow layout at 200% text, reply retry and
media removal. The Rust media-only reply regression additionally passed with a
real local core, uploaded PNG and outbox lookup. Formatting and analysis passed.
The public-site build resolves its internal links and emits 14 content pages.

The ordinary Rust suite skips external IPFS bootstrap, the isolated PostgreSQL
quota case and two legacy-oracle tests. The separate legacy gate executes the
oracle cases. Local PostgreSQL quota testing requires an isolated test URL;
the release CI supplies its own PostgreSQL service. Dependency audit retains
five allowed maintenance warnings (paste, pqcrypto-internals, pqcrypto-kyber,
pqcrypto-traits and rustls-pemfile).

## Deployment preparation

Database, media and deployment configuration were backed up before any relay
replacement. The database archive was restored into an isolated PostgreSQL 16
container with no network exposure. Account credential and sealed identity
checksums matched the originals. Media archive integrity was checked. A rollback
image is retained. The candidate relay is built separately from production.

## Publication and limitations

Pages was deployed by workflow `43772826777` and verified over HTTP. Relay
0.12.4 was tested with a temporary database and networking disabled, then deployed
in production: health/readiness and nodeinfo confirm the version. Volumes,
credentials, identities and configuration were preserved after replacement.

The first CI run exposed a race in the existing spool test: it slept for 100 ms
and read the cursor before the atomic write completed. It now waits for cursor
persistence with a five-second bound; service behaviour is unchanged. The release
pipeline is rerun against the commit with the corrected test.

Release [v0.12.4](https://github.com/redhunt07/Fedi3/releases/tag/v0.12.4) was
published as the latest stable release from commit `5fb980e`. All 14 jobs in the
[release CI](https://github.com/redhunt07/Fedi3/actions/runs/43773343370) passed,
including package builds and AppImage startup on Ubuntu 22.04 and 24.04.

All five published packages were downloaded. Their SHA256 values match both
`checksums.txt` and the GitHub asset digests. The Windows archive passed its CRC
check and contains the client executable and core DLL. The AppImage header is valid.
Both Android APK signatures verify and match the 0.12.3 certificate; version codes
are 6020 (arm64-v8a) and 8020 (x86_64), with versionName 0.12.4 and their native cores.

The updated public sites passed an HTTP/anchor check of 46 URLs without errors.
The updated FAQ and relay live view were inspected in the browser without console
errors. The feed uses the actual release publication timestamp. Temporary restore
and candidate containers were removed; private backups and the rollback image remain.

| Package | SHA256 |
| --- | --- |
| `Fedi3-linux-x86_64.AppImage` | `e5b2136c6ff8c204f78fcfce6df9b475c6028f9dcaab1dcde8f48ef067d577d7` |
| `Fedi3-windows-x64.zip` | `7a70d5ea4b456fc3c2cece97ab4b057904e8c81130055ee09c6dc943b6e77320` |
| `app-arm64-v8a-release.apk` | `9218d0425a9b3f530d19417c1ad836a91b6a5322f34b7991cc11d60364e02780` |
| `app-x86_64-release.apk` | `030f618ba0b88f29e081f92b3f025e4a989f5352211971bad7b69840abec884e` |
| `fedi3_0.12.4-1_amd64.deb` | `dc10c34965709738fc47529880b49566dad3df3d0461bb66cf71ee0f3e743a68` |

Widget audio tests simulate the native plugin; they do not prove decoding or
audio output for the reporting user's particular received file and device.
No new test involving another person's account was performed. A real-device
update/playback check remains necessary. There are no iOS/macOS packages.
