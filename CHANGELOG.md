# Changelog

All notable changes to FEDI3 are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
- Media upload limits: byte cap, magic-byte validation and bounded image decoding.
- Idempotent PostgreSQL `search_tsv` migration for existing databases.
- Test infrastructure for the core and the relay: in-process mock ActivityPub servers, request signers, ephemeral-port harnesses.
- CI pipeline: formatting, linting, build, tests, Flutter analyze/test, platform configuration checks.
- `scripts/verify_platform_config.sh` to catch release-breaking platform regressions.
- `SECURITY.md` with disclosure policy and the security model.
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
