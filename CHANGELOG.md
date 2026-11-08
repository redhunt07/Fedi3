# Changelog

All notable changes to FEDI3 are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.10.1] - 2026-11-08

### Linux AppImage startup

- Fixed Linux AppImage startup on newer desktops: bundled Ubuntu 22.04
  `libmount`/`libsystemd` and PCRE2 libraries could shadow the host GLib's required
  ABI. Host GLib dependencies now remain on the host, including files already
  present in the Flutter bundle.
- Use the host Fontconfig, font libraries and configuration together, avoiding
  invalid-font-configuration errors on newer distributions.
- Keep Xlib/XCB on the host with its Mesa drivers; old bundled Xlib could prevent
  EGL context creation on Ubuntu 24.04 even after the GLib conflict was fixed.
- Added an actual versioned-library regression reproducing `MOUNT_2_40` and
  required GUI startup tests of the packaged AppImage on Ubuntu 22.04 and 24.04.
  Publication now requires a visible first Flutter frame on both systems.
- Profile databases, keyring credentials and cryptographic formats are unchanged.

## [0.10.0] - 2026-11-08

### Client updates and compatible security fixes

Published for Linux x86_64 and Windows x64 with verified package checksums.

- Added stable-release checks, update notifications, verified SHA256 downloads,
  atomic Linux x86_64 AppImage replacement with a previous-image backup, and a
  guided Windows x64 ZIP update path. Existing clients need a manual first upgrade.
- Added Linux/Windows draft-release packaging, exact-package checksums and a tag
  version contract; the RustSec audit remains mandatory on release tags.
- Replaced RustCrypto RSA with AWS-LC and upgraded the Rust WebRTC transport to
  0.21.1 with AWS-LC. Existing PKCS#8/SPKI keys, DIDs, RSA signatures, OAEP/AES-GCM
  envelopes, chat keys, signaling JSON and v1 data-channel frames stay compatible.
- Added old/new RSA interoperability, complete legacy encrypted-message fixtures,
  unchanged-profile/DID checks and WebRTC 0.12 interoperability in both SDP roles.
  The old test oracle is isolated from the release dependency graph.
- Removed the two remaining dependency vulnerabilities: the release workspace
  audit reports zero findings, no ignored advisories and five maintenance notices.
- Fixed corrupt/truncated diagnostic log reads, serialized writes and bounded log
  rotation. Clarified that client diagnostics are local and exported manually;
  no automatic GitHub telemetry transport was present.
- Fixed relay directory fallback, authentication error guidance, submillisecond
  RTT reporting and release-script failures; synchronized the Palermo relay entry.
- Verification: 304 standard Rust tests, 184 Flutter tests, strict Clippy/analyzer,
  Linux core/relay/client release builds and Android ARM64 core compilation check.
  GitHub CI, legacy interoperability checks and native Linux/Windows release builds passed.
  Apple/mobile packages remain outside this release pipeline.
- Updated English/Italian update guides, security documentation and verification
  reports. See [compatibility verification](docs/audit/crypto_compatibility_2026-11-08.md).

### Hardening — 2026-11-07

Security:
- Relay inbox binds the declared sender to the verified actor; forged `actor` and `object.id` are rejected.
- Outbound federation fetches re-validate every redirect, strip authorization/signature headers on cross-origin hops and stream bodies with hard caps.
- P2P frames are length-capped (4 MiB) before allocation; mailbox writes use `INSERT OR IGNORE` so a peer cannot overwrite another sender's message.
- Tunnel tokens travel in `Authorization` and no longer appear in logged URLs; `X-Forwarded-For` is trusted only from the authenticated relay tunnel.
- Chat edit/delete and receipts are bound to the thread and the original author; device signatures require target, host, date and digest.
- Media quota reservations are atomic (SQLite `IMMEDIATE`, PostgreSQL row lock) and account deletion removes blobs before metadata.
- Sensitive client settings live in the OS keyring; exports use encrypted backup v2 (AES-256-GCM, PBKDF2-SHA256 600k, AAD) with a recovery screen when the keyring is unavailable.

Reliability:
- Relay database access moved to `db.rs` with a bounded async `DbDispatch` (no global mutex); readiness uses a separate probe and timeout.
- Relay media is streamed with `Range`/`ETag`/`206`/`304`; graceful `SIGTERM` drains tunnels and in-flight requests.
- `markCoreDead` stops the core on a separate isolate, so the UI no longer freezes; startup waits for the cleanup future.
- Chat drafts survive send failures; stale search responses are discarded; translation and chat-status caches are bounded.
- Legacy sync caps actor/collection/page fetches before parsing; dead-letter classification uses typed HTTP/P2P status instead of string matching.
- CI adds ShellCheck, backup/deploy contract tests and an ephemeral PostgreSQL quota job.

Repository:
- Documentation is English-first, with Italian counterparts under `docs/it/`; README, CONTRIBUTING, SECURITY and templates were revised.
- Removed unused libp2p mesh prototypes (`relay_mesh.rs`, `relay_notes.rs`), `nat.rs`, the unused P2P entrypoint and the stale `docs/linux error` file.
- Runtime P2P identity/mailbox files and generated `AppDir` files are no longer tracked.

### Added
- Identity anchor (`anchor_actor_url`): the canonical actor id/key can live on a static domain while inbox/outbox stay on the relay; the relay redirects to the anchor and WebFinger resolves to it. Anchor documents are exported to `<data_dir>/anchor/` and `GET /_fedi3/anchor`.
- Blind spool: the relay encrypts pending activities with the user's public key (AES-256-GCM + RSA-OAEP) and forwards ciphertext envelopes to `POST /_fedi3/spool/envelope`, where the core decrypts and verifies the original signature.
- Relay mesh: peer relays exchange user directories (`GET /_fedi3/relay/users`) and forward requests for users hosted elsewhere, with an `X-Fedi3-Hops` loop budget.
- Relay moderation: admin blocklists (`/admin/blocks`, actor/domain with optional expiry) and per-user blocklists pushed by the core (`PUT /_fedi3/relay/user_blocks/<user>`), enforced on the inbox path with `202` for blocked senders.
- Core syncs its blocklist to the relay at startup and after every change.
- End-to-end tests for anchor identity, blind spool, relay mesh and moderation.
- Real interoperability verified against Mastodon 4.7.4 and Misskey 2026.10.1 (procedure and evidence in `docs/interop.md`).
- AP endpoint proxy on the identity host keeps the anchor/relay split while satisfying classic instances; photo, video and audio federation and timeline/thread consistency verified in both directions.
- End-to-end federation test: a real relay binary plus two real core runtimes (Follow, Accept, public note delivery, outbox).
- Outbound fetch policy with SSRF protection, bounded redirects and streamed body caps (`net_policy`).
- Pending follow requests for locked accounts, with internal endpoints and a Flutter screen to approve or reject them.
- Outbox visibility: direct messages never appear in the public outbox, followers-only notes are served only to followers.
- Chat membership enforcement on every inbound payload path.
- Chat message search inside a thread and message forwarding to another conversation, with widget tests.
- Shared pinned messages per chat (owner/admin only in groups), full-history search through the core, and group invite links with a join flow. Widget and core tests cover pinning and wildcard-safe search.
- Local, membership-scoped global chat search across every conversation, with debounced UI and indexes on chat ordering; searches message text only (JSON keys excluded) and never leaves the device.
- End-to-end WebRTC calls (1:1 and full-mesh groups) with audio/video, screen sharing and PC audio, signalled through encrypted chat envelopes; media never traverses the relay. Audio notes use the platform recorder; video notes use WebRTC recording with a file-picker fallback on Linux.
- Media upload limits: byte cap, magic-byte validation and bounded image decoding.
- Idempotent PostgreSQL `search_tsv` migration for existing databases.
- Test infrastructure for the core and the relay: in-process mock ActivityPub servers, request signers, ephemeral-port harnesses.
- CI pipeline: formatting, linting, build, tests, Flutter analyze/test, platform configuration checks.
- `scripts/verify_platform_config.sh` to catch release-breaking platform regressions.
- `SECURITY.md` with disclosure policy and the security model.
- Fixed hashtag cards/tags opening empty searches: tag counts now join stored objects (orphan tags excluded) and trending names are normalized, so clicking a tag always lists its posts.
- Right sidebar: live "Scopri" card with trending hashtags (new `/_fedi3/trending/hashtags` core endpoint), opening hashtag search on tap.
- Fixed hashtag search returning no results: the raw SQL `ESCAPE` literal used two backslashes, so SQLite rejected it and the handler silently returned an empty list.
- Chat: Telegram-style new chat screen with mutual-contact selection (interaction/A-Z sorting), group creation without a first message, direct navigation into the thread, group-only rename and member roles (owner/admin/member).
- Explore: the Discover feed now opens with trending hashtags and suggested accounts (new `/_fedi3/suggested/actors` endpoint, one-tap follow).
- Notifications: consecutive likes/boosts/reactions from the same actor are collapsed into an expandable group with a counter.
- Saved posts: bookmarking now stores a local AP snapshot, with a dedicated "Saved" screen (offline rendering, swipe-to-remove) reachable from Settings and the right sidebar.
- Notifications redesigned as compact actor rows with presence dots, type icons, excerpts and inline actions instead of full post cards.
- Composer media: real drag-and-drop upload zone, native file picker, multi-file selection and a Telegram-style rounded chat composer with gradient send button.
- Chat Telegram-style: gradient outgoing bubbles with tails, incoming avatars, member-aware delivery checks (sent, delivered, seen by all) and peer presence in the thread header.
- Presence network: Fedi3 peers exchange online/busy/afk/offline status through encrypted chat envelopes, with TTL expiry, relay online fallback and a status selector in Settings; avatars across the app show the matching coloured dot.
- Flutter client restyling: fixed aurora brand tile with drifting particles and rail ambient glow, mesh profile banner with overlapping avatar ring, card-based chat list, composer prompt card in the sidebar, search action in the timeline bar, compact post actions, branded empty states and actionable account/relay cards.
- Flutter client: animated Fedi3 logo in the navigation rail; clicking it opens an in-app announcements screen fed by `ANNOUNCEMENTS.md` in the Git repository, with a periodic check (every 6 hours), cached offline copy and a snackbar alert when new maintainer updates are published.
- Flutter client: removed the quick inline composer everywhere; the sidebar "Componi" card and the mobile FAB open the full rich composer directly. The active navigation item now has an animated rotating aurora border with orbiting particles and a click spark burst, plus a fade transition when switching sections. Motion respects the OS reduce-motion setting.
- Flutter client: WordPress-style composer with block styles (paragraph, headings, quote), font and size controls, alignment, undo/redo, inline formatting, lists, links, emoji/media insertion, live Markdown/MFM preview and document statistics. Keyboard shortcuts Ctrl+1..7 (sections), Ctrl+N (composer), Ctrl+F (search) and Esc; accessible rail labels with shortcut hints. Chat shows an E2E badge and image thumbnails for pending attachments.
- API: nuova specifica OpenAPI 3.1 (`docs/openapi.yaml`) con 66 path e 16 schemi per core, relay e admin, collegata nel README.
- Flutter client: cronologia delle ricerche persistente (max 10, dedupe) con chip richiamabili e cancellazione, più stati vuoti brandizzati nella ricerca.
- Flutter client: quick-connect login from instance URL and recovery code, profile pagination with a "load more" action, a media grid tab, and date-grouped notifications with branded empty states.
- Flutter client: new "Aurora" visual language (design tokens, dark/light palette, typography scale, animated brand mark, gradient CTAs, hover states), a custom navigation rail with selection indicator and live core status, pill timeline tabs, a restyled profile header with banner, a note-only profile feed, a grouped settings hub with icon tiles, and a guided onboarding surface with advanced options collapsed a recovery-code dialog after account creation, branded empty states, profile tabs (Post/Risposte/Media) and an edit-profile action for the signed-in user. The desktop default layout is now the tabbed single feed; the deck/columns mode remains available.
- Flutter client: followers/following list screens with per-actor follow/unfollow, tappable profile counters, a clickable Account card that opens the local profile, and a user-search fallback over actors already seen by the client and the local follow graph (the core index only stores activity objects).

### Changed
- The relay now verifies the HTTP signature on `/inbox` and requires a `Digest` bound to the signed headers.
- Inbound activities must be signed by the actor they claim to come from.
- `POST /users/{u}/outbox` requires the internal token or a valid signature.
- The tunnel reconnects with exponential backoff instead of dying on the first network error.
- `fedi3_core_start` returns success only after the local server is listening.
- The relay token is optional: local-only and P2P-only instances start without a relay.
- Outbound HTTP clients use `rustls-tls-native-roots`, so the system CA store is honoured (needed for enterprise/private CAs; public CAs keep working). The tunnel WebSocket client uses the same native-root trust store.
- Health and readiness endpoints are unauthenticated and side-effect free.
- Client IP is derived from the proxy-appended `X-Forwarded-For` entry; rate limits are per route.
- SQLite uses `busy_timeout`; delivery workers survive transient database errors.
- The app uses bounded HTTP timeouts, typed API errors, atomic configuration writes, and supervises the core with a health watchdog.
- Post text is always converted to escaped HTML before publishing.

### Fixed
- A duplicated `Content-Type` header in `Delivery::deliver_json` made the signed value differ from the value received by Mastodon, which rejected the HTTP signature (401). The header is now sent once, from the signed header set.
- Two real bugs found by the end-to-end test: the tunnel deadlocked on nested federation fetches (now dispatched concurrently), and the actor document advertised the relay shared inbox, which cannot expand `followers` collections, silently losing followers-only deliveries.
- The core now joins every background worker on shutdown and checkpoints the SQLite WAL.
- The key resolver cache evicts least-recently-used entries instead of arbitrary ones.
- Android release builds had no `INTERNET` permission.
- Android cleartext traffic to the loopback core was blocked on API 28+.
- macOS release entitlements granted no network or file access.
- iOS lacked the ATS exception for the local core.
- Editing the configuration stopped the core and never restarted it.
- Search was rejected with `503 search coverage incomplete` on standalone relays with no peer relays and on fresh relays with an empty user index, because a missing relay-sync timestamp was treated as stale. Coverage is now vacuously complete in those cases, with unit tests for the decision logic.
- The widget test asserted a string the app never rendered.
- `relay_diagnose.sh` used `wget`, which is not present in the relay image.
- Backup scripts ignored the configured PostgreSQL user/database and nested media archives incorrectly.
- PostgreSQL media quota queries read `SUM(size)` (numeric) as `i64`, causing a panic and a 502 on every media upload. The query now casts to `bigint`; the same fix covers spool bytes and tag counts, and move-notice `BOOLEAN` status is converted before reading.
- Added the production VPS deployment notes for `fedi3.redhunt07.ovh` in `docs/deploy_vps.md`.
- Chat list: direct chats can be deleted and groups can be left; group owners can delete the group for everyone. Leaving removes the local thread and notifies remaining members, promoting a new owner when needed.
- Chat unread indicators were shown for threads with no messages and could survive opening a thread. Empty threads are no longer marked unread, and opening a thread records the latest message timestamp before navigating.
- After an identity/relay move, old chat memberships and local sender ids are migrated to the current actor URL, so existing threads remain operable.
- Chat, presence and sidebar surfaces now share the Aurora panel language: compact grouped sidebar actions, one shared presence selector, a styled presence menu and shorter copy.

### Security
- SSRF guard on all peer-controlled fetches (core and relay), with body caps against memory exhaustion.
- SQLite database, WAL and SHM files are created with `0600` permissions.
- Panics no longer cross the FFI boundary.
- Actor impersonation through unbound HTTP signatures is rejected.
- Admin and user tokens are compared in constant time.
- Private key files are created with `0600` permissions on Unix.
- P2P frames are length-capped before allocation.
- Media identifiers received from the relay are sanitized before touching the filesystem.

## [0.1.0] - 2026-01-13

### Added
- Initial public snapshot: Flutter client, Rust core (ActivityPub, P2P, chat, media), public relay, Docker Compose deployment, operational documentation.
