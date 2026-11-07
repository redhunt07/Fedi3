# FEDI3 — Improvement and fix plan

**Reference:** [production_readiness_audit.md](production_readiness_audit.md) (12 P0, 66 HIGH, 37 MEDIUM, 20 LOW)
**Goal:** make the project releasable and verifiable, with a test suite covering every functional area.
**Date:** 2026-10-31

---

## 0. Execution principles

1. **No fix without a test.** Every security or protocol fix comes with at least one test that fails before and passes after. Where a test is impossible (platform config), an alternative automated check is needed (manifest parse, gradle/plist check).
2. **Priority: security → correctness → availability → performance → hygiene.** No optimization before the gaps are closed.
3. **No ActivityPub compatibility break.** Protocol changes must remain interoperable with Mastodon/Misskey; Fedi3 extensions remain optional.
4. **Small, isolated changes**, with disjoint write scopes between agents, to avoid conflicts.
5. **Fail-fast on configuration:** an invalid value must stop startup with a message, not be silently replaced.
6. **The default must be safe.** If an operator forgets a variable, the system must not become permissive.

---

## 1. Phases

### Phase A — Release unblocked (platform P0)

Goal: the app installs and works on Android, macOS, iOS; the release can be signed and published.

| ID | Task | File | Acceptance criterion |
|---|---|---|---|
| A1 | `INTERNET` + `POST_NOTIFICATIONS` permission in the main manifest; removal from the debug/profile manifests | `app/android/app/src/main/AndroidManifest.xml` | `aapt dump permissions` on the release APK shows INTERNET; the debug manifest does not duplicate it |
| A2 | `network_security_config.xml` with cleartext only for `127.0.0.1`/`localhost`; reference in the manifest; `allowBackup=false` | `app/android/app/src/main/res/xml/network_security_config.xml`, manifest | The file is valid XML, referenced, and `base-config` denies cleartext |
| A3 | macOS entitlements: `network.client`, `network.server`, `files.user-selected.read-write` in Debug and Release | `app/macos/Runner/*.entitlements` | `codesign -d --entitlements` on the bundle shows the three keys |
| A4 | iOS ATS: `NSAllowsLocalNetworking`; photo/camera usage descriptions; URL scheme | `app/ios/Runner/Info.plist` | `plutil -lint` OK; the required keys present |
| A5 | Final app id, `signingConfigs.release` from `key.properties`, fallback to debug only if the file is missing | `app/android/app/build.gradle.kts`, `app/linux/CMakeLists.txt` | Release build with external keystore; no residual `com.example` |
| A6 | Platform test: script that verifies manifest/plist/gradle | `scripts/verify_platform_config.sh` | Exits 0 on correct config, 1 with a clear message on regression |

### Phase B — Federation authentication

Goal: no actor can inject or impersonate activities.

| ID | Task | File | Acceptance criterion |
|---|---|---|---|
| B1 | Mandatory HTTP signature on the relay `/inbox`; 401 if missing/invalid | `crates/fedi3_relay/src/main.rs` | Test: POST without signature → 401; with valid signature → 202; altered signature → 401 |
| B2 | Binding `activity.actor` ↔ `summary.actor_url` in the core, on inbox and receipt | `crates/fedi3_core/src/ap.rs` | Test: activity signed by A but with `actor=B` → 401 |
| B3 | Authentication on `POST /users/<u>/outbox` (internal token or signature) | `crates/fedi3_core/src/ap.rs` | Test: POST without token → 401; with token → 202; the UI path keeps working |
| B4 | Mandatory `Digest` for POST and validated signed header set | `crates/fedi3_core/src/http_sig.rs`, `ap.rs` | Test: POST without Digest → 401; wrong Digest → 401; minimal header set missing → 401 |
| B5 | Constant-time token comparison + startup validation (admin ≥32 chars, relay ≥16) | `crates/fedi3_relay/src/main.rs`, `crates/fedi3_core/src/runtime.rs` | Unit test on the comparator; startup with a short token → explicit error |
| B6 | Object ownership on Create/Update/Delete | `crates/fedi3_core/src/ap.rs` | Test: Update of someone else's note ignored; Delete of someone else's note ignored |
| B7 | Accept bound to the correct Follow | `crates/fedi3_core/src/ap.rs` | Test: Accept from a different actor does not mark accepted |

### Phase C — Availability and reliability

| ID | Task | File | Acceptance criterion |
|---|---|---|---|
| C1 | Tunnel reconnection loop with backoff, connect timeout and pong watchdog | `crates/fedi3_core/src/tunnel.rs`, `runtime.rs` | Test: relay that closes the connection → the client reconnects within N seconds; clean shutdown |
| C2 | `catch_unwind` on every FFI entry point + removal of `.lock().unwrap()` | `crates/fedi3_core/src/ffi.rs`, `runtime.rs` | Test: config that causes a panic → error code, process alive |
| C3 | Readiness handshake in `start()` (synchronous bind or oneshot) | `crates/fedi3_core/src/runtime.rs` | Test: port busy → `start` returns Err; free port → returns only after the bind |
| C4 | Release the lock before the await in `forward_to_user`; timeout on the send; spool on every failure | `crates/fedi3_relay/src/main.rs` | Test: slow client does not block a second client; failed delivery → spool row and 503 to the sender |
| C5 | Correct client IP on all handlers; guard on trusted proxies | `crates/fedi3_relay/src/main.rs`, `Caddyfile` | Unit test on `client_ip` with spoofed XFF; rate limit per real IP |
| C6 | Unauthenticated health/readiness without side effects; `HEALTHCHECK` | `crates/fedi3_relay/src/main.rs`, `deploy/relay/Dockerfile`, compose | Test: `/healthz` without token → 200; no audit row written |
| C7 | SQLite: `busy_timeout`, single writer, versioned schema | `crates/fedi3_core/src/social_db.rs`, `delivery_queue.rs`, relay | Concurrency test: 8 parallel writers, zero `SQLITE_BUSY` |
| C8 | Resilient delivery workers (error → backoff, not death) + job pruning | `crates/fedi3_core/src/delivery_queue.rs` | Test: simulated DB error → the worker continues; old jobs removed |
| C9 | Inbox: dedup only after success | `crates/fedi3_core/src/ap.rs` | Test: failed processing → retry reprocesses |
| C10 | `relay_bridge` without panic on remote input | `crates/fedi3_core/src/relay_bridge.rs` | Test: path without `/`, path with spaces → 400 response, no panic |
| C11 | Cap on P2P frame length | `crates/fedi3_core/src/p2p.rs` | Test: 0xFFFFFFFF header → error, no 4 GiB allocation |
| C12 | Core supervisor in the app (health watchdog + restart) | `app/lib/state/app_state.dart` | Widget test: dead core → `dead` state and restart attempted |

### Phase D — App: network, configuration, correct UX

| ID | Task | File | Acceptance criterion |
|---|---|---|---|
| D1 | HTTP client with timeout, typed errors, injectable repository | `app/lib/core/*`, `app/lib/services/*` | Test: server that does not respond → `timeout` error within 15 s, no pending Future |
| D2 | Atomic config/prefs (temp+rename), `.bak`, 600 permissions, corrupt/missing distinction | `app/lib/state/config_store.dart`, `prefs_store.dart` | Test: truncated file → recovery from `.bak`; no silent loss |
| D3 | `replaceConfig` that stops, saves, restarts and waits for readiness; used by all screens | `app/lib/state/app_state.dart`, settings screens | Test: save → core running; no empty timeline |
| D4 | Post text always converted to HTML with escaping | `app/lib/utils/mfm_codec.dart`, composer | Test: `<`, `&`, `"` → correct entities; no injected tag |
| D5 | Repaired widget test + coverage of the main screens with fake HTTP | `app/test/**` | `flutter test` green |
| D6 | Media kit initialized on Linux; software rendering opt-in | `app/lib/main.dart`, `scripts/build_appimage.sh` | Test: flag present in AppRun; startup without exceptions |

### Phase E — Observability, CI, hygiene

| ID | Task | File | Acceptance criterion |
|---|---|---|---|
| E1 | Full CI: fmt, clippy, locked build, Rust tests, analyze, Flutter tests, AppImage build | `.github/workflows/ci.yml` | The pipeline exists and is runnable locally with the same commands |
| E2 | `cargo audit`/`deny` + pinned dependencies in the Dockerfiles (`--locked`, images by digest) | `.github/workflows/`, `deploy/` | Reproducible build; no `latest` |
| E3 | Repo cleanup: binaries out of git, `.gitignore`, spurious files removed | `.gitignore`, `docs/` | `git ls-files` without AppImage/DLL/swp |
| E4 | `SECURITY.md`, `CHANGELOG.md`, full AGPL license, rewritten docs | root, `docs/` | The files exist and are consistent |
| E5 | Corrected operational scripts (wget, PG variables, tar) + backup retention | `scripts/` | `relay_diagnose.sh` works in the image; consistent backup/restore |
| E6 | Config: complete `.env.example`, fail-fast validation, safe defaults | `.env.example`, `main.rs`, `runtime.rs` | Every variable read is documented; an invalid value blocks startup |

---

## 2. Audit → plan traceability matrix

| Finding | Phase |
|---|---|
| P0-01, P0-02, P0-03, P0-04, P0-05 | A1–A5 |
| P0-06 | B1 |
| P0-07 | B2 |
| P0-08 | B3 |
| P0-09 | C1 |
| P0-10 | C2 |
| P0-11 | C3 |
| P0-12 | D3 |
| H-01 | B4 |
| H-02 | B6 |
| H-03, H-04 | Phase F (SSRF and size caps) |
| H-05 | Phase F |
| H-06 | C11 |
| H-07 | Phase F (ingest) |
| H-08 | Phase F (signed move notices) |
| H-09 | B5 + C3 |
| H-10 | C4 |
| H-11 | C4 |
| H-12, H-13 | C5 |
| H-14, H-15 | B5, Phase F |
| H-16 | Phase F |
| H-17, H-18 | Phase F |
| H-19 | Phase F (key permissions) |
| H-20 | Phase F (relay dedupe) |
| H-21, H-22 | Phase G (performance) |
| H-23 | C6 |
| H-24, H-25 | C7 |
| H-26 | C8 |
| H-27, H-28 | Phase G |
| H-29, H-30, H-31 | Phase F |
| H-32, H-33, H-34, H-35, H-36 | Phase G |
| H-37 | C9 |
| H-38, H-39 | Phase F |
| H-40, H-41 | C8 |
| H-42, H-43 | Phase F |
| H-44, H-45 | B7, Phase F |
| H-46, H-47, H-48 | Phase G |
| H-49 | C10 |
| H-50 | D1 |
| H-51 | C12 |
| H-52 | Phase G (UX/perf) |
| H-53, H-54 | Phase F (app) |
| H-55 | D6 |
| H-56, H-57 | Phase F (backup) |
| H-58 | Phase F (shutdown) |
| H-59, H-60 | D5 + E1 |
| H-61, H-62, H-63 | D4, Phase G |
| H-64, H-65, H-66 | Phase F (app) |
| M-* | Phase G / H |
| L-* | Phase H (hygiene) |

### Phase F — Protocol and data correctness (after phases A–C)

SSRF with a guard on private IPs, caps on inbound bodies, ingest with origin validation, signed move notices, relay dedupe, quotas and media DELETE, spool with a byte cap, pagination with a composite cursor, outbox visibility, key file permissions, complete shutdown, correct Accept, complete backup with rollback.

### Phase G — Performance, scalability, UX

DB connection pool, removal of the global lock, media streaming, concurrent fan-out, real metrics, polling with lifecycle, timeline N+1, accessibility, complete l10n.

### Phase H — Hygiene and maintenance

Clean repo, complete license, SECURITY/CHANGELOG, deterministic toolchain and lint, dependency updates (ML-KEM), operational scripts.

---

## 3. Execution order and parallelization

Phases A–D are independent in file scope and can proceed in parallel on three tracks:

- **Relay track**: A (Dockerfile/compose), B1, B5, C4, C5, C6, C7(relay), C8(relay).
- **Core track**: B2, B3, B4, B6, B7, C1, C2, C3, C7(core), C8(core), C9, C10, C11.
- **App track**: A1–A5, D1–D6, C12.

Phase E (CI, hygiene, docs) belongs to the Lead and does not touch the files of the three tracks, except `.github/`, `scripts/`, root.

**Anti-conflict rule:** each agent writes only in its own path prefixes. The Lead reviews the final diff and runs the full verification.

---

## 4. Final verification (completion gate)

The plan is complete only when, on the same machine:

1. `cargo fmt --check` clean.
2. `cargo clippy --workspace --all-targets -- -D warnings` clean.
3. `cargo build --workspace --locked` succeeds.
4. `cargo test --workspace --locked` green, with the suite described in [test_plan.md](../test_plan.md).
5. `flutter analyze` without errors; `flutter test` green.
6. `scripts/verify_platform_config.sh` exits 0.
7. The diff does not introduce regressions in the existing tests and does not break AP compatibility (verified by the contract tests on actor/activity documents).

If a point is not achievable (e.g. missing toolchain), it must be stated explicitly and replaced by an equivalent static verification.

---

## 5. Risks and mitigations

| Risk | Mitigation |
|---|---|
| The relay DB refactor breaks uncovered paths | Introduce the tests before the refactor; keep the `Db` API unchanged |
| The `actor`↔signature binding breaks legitimate peers with a non-canonical `keyId` | Accept both `actor` as a string and `object.id`; log the mismatch before rejecting; test with real Mastodon/Misskey documents |
| The mandatory signature on `/inbox` breaks existing relays | Reject only if the signature is missing or invalid; Fedi3 clients already sign; regression test on the legacy path |
| The reconnection loop generates reconnection storms | Exponential backoff with jitter, 5-minute cap, reset after a stable connection |
| Platform fixes are not verifiable without a build | Static verification script on manifest/plist/gradle, plus builds in CI |
| Two agents modify the same file | Declared disjoint scopes; the Lead is the only one touching `.github/`, `scripts/`, root |

---

## 6. Definition of done

- All P0 closed and covered by tests or static verification.
- At least the security HIGH items (B1–B7, C1–C6, C9–C11) closed and tested.
- Test suite runnable locally and in CI, with case counts and coverage per area.
- CI green on the three tracks.
- Updated documentation: audit, plan, test plan, `SECURITY.md`, `CHANGELOG.md`.
- No binary file in the repository; `.gitignore` correct.

---

## 7. Implementation status

Executed on 2026-10-31, in two iterations. Gates verified with real output on the final state of the code:

| Gate | Command | Outcome |
|---|---|---|
| Rust formatting | `cargo fmt --all -- --check` | ✅ no differences |
| Rust lint | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 0 warnings |
| Build | `cargo build --workspace --locked` | ✅ |
| Rust tests | `cargo test --workspace --locked` | ✅ 205 tests green, 0 failed (17 binaries) |
| Federation end-to-end | `cargo test -p fedi3_relay --test e2e_federation` | ✅ real relay + two real cores |
| App analysis | `flutter analyze --fatal-infos` | ✅ no issues |
| App formatting | `dart format --set-exit-if-changed lib test` | ✅ 0 files changed |
| App tests | `flutter test` | ✅ 82 tests green |
| Platform configuration | `scripts/verify_platform_config.sh` | ✅ 9/9 checks |

Phases completed: **A** (platform blockers), **B** (federation and authentication), **C** (availability), **D** (app), **E** (CI, hygiene, docs), **F**/**G** (residual gaps).

### Second iteration — gaps closed

| Gap | Fix | Verification |
|---|---|---|
| `H-17` | Chat: the incoming payload requires an existing thread and the sender's membership; the first DM creates the thread only if the id matches the local sender/recipient pair; group system messages create the thread only if both participants are announced. | 4 tests in `tests/semantics.rs` |
| `H-39` | Outbox: `visibility` column derived from the audience (public/unlisted/followers/direct), `list_outbox_visible` with requester and follower status, DMs never appear in the public outbox; legacy NULL rows remain public. | 6 tests |
| `H-44` | Blocked accounts: `follow_requests` table, Follow → pending without Accept, internal endpoints `GET /_fedi3/follow_requests` and `POST .../<actor>/approve|reject`, dedicated Flutter UI reachable from the settings. | 4 Rust tests + 6 widget tests |
| `H-05` | Media: precheck on `Content-Length`, streamed reading with cap, magic bytes validation, decoding with `ImageReader::with_limits` (8192×8192). | 4 tests |
| `H-03`/`H-04` | New `net_policy` module: only http(s), DNS resolution and rejection of private/loopback/link-local/unique-local/CGNAT/documentation addresses, limited redirects, body reading with cap (256 KiB actors, 2 MiB objects/pages). Applied to `KeyResolver`, `Delivery`, `object_fetch`, `legacy_sync`, runtime client. Escape hatch `FEDI3_ALLOW_PRIVATE_ACTOR_FETCH`. | 6 integration tests + unit tests on IP ranges |
| `H-46` | `KeyResolver`: real LRU (10k) instead of arbitrary eviction, `keyId`/`actor.id` already validated. | unit test on eviction |
| `H-42` | Shutdown: all workers registered and joined with a 10 s deadline, `PRAGMA wal_checkpoint(TRUNCATE)` before exit. | verified by the runtime/FFI tests |
| `H-19` | `0600` permissions on the DB and `-wal`/`-shm`/`-journal`. | unix-only test |
| Postgres `search_tsv` | Idempotent migration at startup (column, backfill, GIN index, trigger) and updated schema. | assertions on the SQL (no live Postgres) |
| Relay SSRF (`H-35`) | Shared policy for fetches triggered by the relay (telemetry, `next`, sync): public IPs, redirects ≤3, 2 MiB/256 KiB cap, escape hatch `FEDI3_ALLOW_PRIVATE_REMOTE_FETCH`. | unit + 2 integration tests |

### Real bugs discovered by the end-to-end tests

The `e2e_federation.rs` test (real relay, two real cores, signed Follow, Accept, public note delivered) surfaced two defects not visible in the mock tests:

1. **Tunnel deadlock**: the tunnel loop handled one request at a time, so two peers resolving each other's key deadlocked until the relay timeout. Now every request is dispatched in a concurrent task with a single writer for the socket.
2. **Shared inbox unusable behind the relay**: the actor document announced `{base}/inbox` (the relay's shared inbox), which has no follow graph and therefore cannot expand `followers` collections: followers-only deliveries were lost. The actor document now announces the personal inbox; the relay routes by path to the correct tunnel.

### Known residual

- The relay Dockerfile was not built in this session (Docker unavailable); the tag `rust:1.99-bookworm` is verified on the registry and aligned with the toolchain of the tests.
- The PostgreSQL migration is covered by assertions on the SQL, not by a live database.
- The SSRF check happens before the request; a redirect to a hostname that resolves to a private IP is not re-evaluated (redirects to literal private IPs are rejected). Documented in `SECURITY.md`.
- The SQLite DB may recreate `-wal` with the umask for a short interval after a checkpoint, until the next connection reapplies `0600`.
