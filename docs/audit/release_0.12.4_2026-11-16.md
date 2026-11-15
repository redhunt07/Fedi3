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

The Rust workspace gates, compatibility oracle, dependency audit, Python
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

At the release commit, final GitHub Actions package builds, Pages deployment
and production relay replacement are pending. The maintainer requested commit,
push, website/relay updates and preparation of the new app release. Final
outcomes are recorded by the deployment/CI checks and the completion report.

Widget audio tests simulate the native plugin; they do not prove decoding or
audio output for the reporting user's particular received file and device.
No new test involving another person's account was performed. A real-device
update/playback check remains necessary. There are no iOS/macOS packages.
