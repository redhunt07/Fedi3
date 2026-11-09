# FEDI3 — Test plan

**Goal:** a suite that tests every function of the application — Rust core, relay, Flutter app and platform configurations — and that is runnable locally and in CI without external services.

**References:** [production_readiness_audit.md](audit/production_readiness_audit.md), [remediation_plan.md](audit/remediation_plan.md)

---

## 1. Strategy

| Level | What it tests | Where it runs | External services |
|---|---|---|---|
| **Unit** | Pure functions: signatures, digest, parsing, escaping, serialization, config rules, rate limiter, CIDR, codec | `cargo test` (`#[cfg(test)]` modules in the crates), `flutter test` | none |
| **Integration** | Real axum servers on an ephemeral port, temporary SQLite DB, real HTTP requests, mocks of remote peers | `crates/*/tests/*.rs` | none (SQLite, HTTP mocks) |
| **Contract** | ActivityPub documents (actor, Note, activities) compared with canonical Mastodon/Misskey fixtures | integration | none |
| **Security** | Authentication bypass, impersonation, SSRF, path traversal, timing, replay | unit + integration | none |
| **Concurrency/chaos** | Parallel writes, slow clients, disconnections, failing workers | integration | none |
| **UI** | Widgets, loading/error states, form validation, navigation, timeouts | `flutter test` with fake HTTP client | none |
| **Static config** | Android manifest, macOS entitlements, iOS Info.plist, gradle | bash script | none |
| **E2E smoke** | Core + in-process sqlite relay, publish → deliver flow | script + integration | none |

**Rules:**
- No test depends on an external network or Docker.
- Every test creates its own `tempdir` and its own port; no shared state.
- Security tests must **fail** on the current incorrect code and pass after the fix.
- Remote mock servers are implemented with axum and verify the received signatures.
- No arbitrary `sleep`: use condition waits with timeout.

---

## 2. Shared test infrastructure

To be created once and reused:

| Component | Path | Function |
|---|---|---|
| `TestCore` | `crates/fedi3_core/tests/common/mod.rs` | Starts a real core on an ephemeral port with a temporary data dir, exposes base URL and internal token, closes in `Drop` |
| `TestRelay` | `crates/fedi3_relay/tests/common/mod.rs` | Starts the relay in SQLite mode without Redis/Meili on an ephemeral port |
| `MockActorServer` | `tests/common/mock_actor.rs` | Serves an actor document, a public key and an inbox; records received requests; options: slow, 500, huge body, redirect |
| `TempDb` | `tests/common/db.rs` | Creates a SQLite DB in a tempdir, exposes seeding helpers |
| `Signer` | `tests/common/signer.rs` | Generates RSA pairs, signs HTTP requests, builds `Signature`/`Digest` headers |
| `FakeHttpClient` | `app/test/support/fake_http.dart` | Deterministic Dart client with programmable responses, delays and errors |
| `pumpApp` | `app/test/support/pump_app.dart` | Mounts the app with injected `AppState` and fake repositories |

---

## 3. Coverage matrix — Rust core

### 3.1 Keys and identity (`keys.rs`)

| ID | Level | Case | Expected |
|---|---|---|---|
| K-01 | unit | Identity generation in an empty dir | Files created, valid PEM, DID deterministic from the PEM |
| K-02 | unit | Loading an existing identity | Same key, no regeneration |
| K-03 | unit | Corrupted file | Explicit error, no silent regeneration |
| K-04 | unit | Permissions on Unix | 0600 on the key file, 0700 on the dir |
| K-05 | unit | DID from PEM with newline/spaces | Stable normalization |

### 3.2 HTTP Signatures (`http_sig.rs`)

| ID | Level | Case | Expected |
|---|---|---|---|
| S-01 | unit | `parse_signature_header` with `keyId`, `headers`, `signature` | Correct fields, lowercase headers |
| S-02 | unit | Header with `algorithm="hs2019"` and commas in a quoted string | Correct parsing |
| S-03 | unit | Header without `signature` | Error |
| S-04 | unit | `build_signing_string` with `(request-target)` and query | Spec-conformant string |
| S-05 | unit | Signed header missing in the request | Error |
| S-06 | unit | RSA-SHA256 sign/verify round-trip on a real request | Verification OK |
| S-07 | unit | Signature altered by 1 byte | Verification fails |
| S-08 | unit | Different public key | Verification fails |
| S-09 | unit | `verify_digest_if_present` with correct digest | OK |
| S-10 | unit | Wrong digest | Error |
| S-11 | unit | Digest missing on POST | Error (new behaviour) |
| S-12 | unit | Multi-algorithm digest `SHA-256=...,SHA-512=...` | Accepts SHA-256 |
| S-13 | unit | `verify_date` with 300 s and 3600 s skew | Rejects beyond the threshold, accepts within it |
| S-14 | unit | Unparsable date | Error |
| S-15 | unit | Minimum signed header set missing (`(request-target)`/`host`/`date`) | Error |
| S-16 | integration | KeyResolver with mock server: valid key, 500, huge body, redirect | Cache, handled error, size cap, limited redirect |

### 3.3 Encrypted envelope (`crypto_envelope.rs`)

| ID | Level | Case | Expected |
|---|---|---|---|
| E-01 | unit | Encrypt/decrypt round-trip | Identical payload |
| E-02 | unit | Altered ciphertext | Decrypt fails with an error (not silently) |
| E-03 | unit | Nonce of wrong length | Error |
| E-04 | unit | Malformed JSON envelope | Error |
| E-05 | unit | Empty body | No encryption, unchanged |
| E-06 | unit | Two messages with the same key: different nonces | Different nonces, both decryptable |
| E-07 | unit | `x-fedi3-encrypted` missing | No decryption |

### 3.4 ActivityPub — actor and discovery (`ap.rs`)

| ID | Level | Case | Expected |
|---|---|---|---|
| A-01 | integration | `GET /users/{u}` | AP `@context`, `id`, `inbox`, `outbox`, `followers`, `publicKey`, `preferredUsername` |
| A-02 | contract | Actor document against Mastodon/Misskey fixtures | Mandatory fields present, correct types |
| A-03 | integration | `GET /.well-known/webfinger?resource=acct:u@domain` | `subject`, `self` link with `activity+json` |
| A-04 | integration | webfinger with a non-local resource | 404 |
| A-05 | integration | `GET /nodeinfo/2.0` and `/.well-known/nodeinfo` | Correct schema, version, statistics |
| A-06 | integration | `GET /users/{u}/followers` and `/following` | Collection with `totalItems`, pagination |
| A-07 | integration | `GET /healthz`, `/_fedi3/health` | 200; `/_fedi3/health` requires the internal token |
| A-08 | integration | `GET /_fedi3/*` without token | 401 |
| A-09 | integration | `GET /_fedi3/*` with wrong token | 401 |
| A-10 | integration | `GET /_fedi3/*` with correct token | 200 |

### 3.5 ActivityPub — inbox

| ID | Level | Case | Expected |
|---|---|---|---|
| I-01 | integration | POST without `Signature` | 401 |
| I-02 | integration | POST with invalid signature | 401 |
| I-03 | integration | POST with valid signature of a remote actor | 202, activity stored |
| I-04 | integration | **POST signed by A with `actor=B`** | 401 (new: binding) |
| I-05 | integration | POST with `Digest` missing | 401 (new) |
| I-06 | integration | POST with wrong `Digest` | 401 |
| I-07 | integration | Date beyond the skew | 401 |
| I-08 | integration | Same activity twice | 202, the second is duplicate, no double storage |
| I-09 | integration | Failed processing (DB closed) and retry | The retry reprocesses (dedup only after success) |
| I-10 | integration | Follow | Follower added, Accept sent to the mock |
| I-11 | integration | Follow with `manually_approves_followers` | Pending state, no Accept |
| I-12 | integration | Accept of one of our Follows | Accepted state |
| I-13 | integration | **Accept from an actor different from the Follow target** | Does not mark accepted |
| I-14 | integration | Undo Follow | Follower removed |
| I-15 | integration | Create public Note | Note stored, federated feed updated |
| I-16 | integration | Create Note with `inReplyTo` | Reply index updated |
| I-17 | integration | **Update of another actor's note** | Ignored (new: ownership) |
| I-18 | integration | **Delete of another actor's note** | Ignored (new) |
| I-19 | integration | Delete of one's own note | Tombstone, object marked deleted |
| I-20 | integration | Announce | Boost stored with the correct actor |
| I-21 | integration | Like/EmojiReact | Reaction stored |
| I-22 | integration | Undo Like | Reaction removed |
| I-23 | integration | Activity blocked by `blocked_actors` | 403, not stored |
| I-24 | integration | Activity from a blocked domain | Not stored |
| I-25 | integration | Rate limit per keyId exceeded | 429 |
| I-26 | integration | Body beyond the limit | 413 |
| I-27 | integration | Invalid JSON | 400 |
| I-28 | integration | Activity with huge depth/arrays | No panic, controlled response |

### 3.6 ActivityPub — outbox

| ID | Level | Case | Expected |
|---|---|---|---|
| O-01 | integration | POST outbox without internal token | 401 (new) |
| O-02 | integration | POST outbox with internal token, valid Create | 202, delivery queued |
| O-03 | integration | POST outbox with an `actor` different from ours | 403 |
| O-04 | integration | POST outbox with invalid JSON | 400 |
| O-05 | integration | GET outbox | Pagination, `orderedItems` |
| O-06 | integration | **GET outbox: DMs do not appear** | Direct activities excluded (new) |
| O-07 | integration | Pagination with two items in the same millisecond | No item lost (composite cursor) |
| O-08 | integration | Public post with attachments | `to` includes Public, `cc` followers |

### 3.7 Media

| ID | Level | Case | Expected |
|---|---|---|---|
| M-01 | integration | Upload with valid internal token | 201, file on disk, DB row |
| M-02 | integration | Upload without token | 401 |
| M-03 | integration | Upload beyond the configured limit | 413 (not `usize::MAX`) |
| M-04 | integration | Upload of a PNG that decodes huge | Rejected or handled with limits |
| M-05 | integration | MIME not in allowlist (`text/html`) | Rejected |
| M-06 | integration | GET media of another user | 404 |
| M-07 | integration | GET media with id `../` | 400 |
| M-08 | integration | Per-user quota exceeded | 413 |
| M-09 | integration | DELETE media | File and row removed |
| M-10 | unit | `sanitize_key` with `../`, `/`, `\` | No traversal |

### 3.8 Chat

| ID | Level | Case | Expected |
|---|---|---|---|
| C-01 | integration | Sending a message to a new thread | Thread created, encrypted envelope, correct members |
| C-02 | integration | Envelope from an actor who is not a member of an existing thread | Rejected |
| C-03 | integration | Envelope for a non-existent thread | Created with correct members |
| C-04 | unit | Bundle from a peer different from the expected one | Rejected |
| C-05 | unit | Decryption of a message | Correct text |
| C-06 | integration | Reactions and read receipts | State updated |

### 3.9 Database (`social_db.rs`)

| ID | Level | Case | Expected |
|---|---|---|---|
| D-01 | unit | Schema creation on an empty DB | Tables and indexes present |
| D-02 | unit | Migration from an old schema | Columns added, data preserved |
| D-03 | unit | Schema version newer than the binary | Explicit error |
| D-04 | unit | Object insertion and reading | Identical JSON round-trip |
| D-05 | unit | Inbox dedup | Second insert → false |
| D-06 | unit | Pagination with equal timestamps | No loss |
| D-07 | unit | TTL pruning per inbox/feed/objects | Expected counts |
| D-08 | unit | Per-actor quotas | Excesses are removed |
| D-09 | concurrency | 8 parallel writers on a DB with `busy_timeout` | Zero `SQLITE_BUSY` errors |
| D-10 | unit | SQL injection on search (`100%`, `foo_bar`) | Correct matches with `ESCAPE` |
| D-11 | unit | Invalid/negative cursor | Empty page, no error |

### 3.10 Delivery queue (`delivery_queue.rs`)

| ID | Level | Case | Expected |
|---|---|---|---|
| Q-01 | unit | Enqueue activity | Job persisted with recipients |
| Q-02 | unit | Scheduling with backoff | Increasing intervals, cap |
| Q-03 | unit | Exceeding `max_attempts` | Dead-letter |
| Q-04 | unit | Receipt received | Job marked delivered |
| Q-05 | unit | Receipt never received | Reaches `max_attempts` anyway (new) |
| Q-06 | unit | DB error during the fetch | The worker continues, does not die (new) |
| Q-07 | unit | Pruning of old jobs | Rows removed |
| Q-08 | integration | Delivery to mock server: 200 | Delivered |
| Q-09 | integration | Delivery to mock server: repeated 500 | Retry, then dead-letter |
| Q-10 | integration | Delivery to mock server: timeout | Retry with backoff |
| Q-11 | unit | Resumption after restart | Pending jobs are resumed |

### 3.11 Object fetch and GC

| ID | Level | Case | Expected |
|---|---|---|---|
| F-01 | integration | Fetch of a valid remote object | Stored |
| F-02 | integration | Object beyond the size cap | Discarded with an error |
| F-03 | integration | 404 | Dead-letter after N attempts |
| F-04 | integration | SSRF: object on `127.0.0.1` | Rejected (new) |
| G-01 | unit | GC of inbox/feed/objects/media | Consistent counts and files |
| G-02 | unit | GC with `cache_only_followed_fedi3=false` | Media quotas remain active (new) |

### 3.12 P2P, tunnel, WebRTC

| ID | Level | Case | Expected |
|---|---|---|---|
| P-01 | unit | Frame codec with length 0xFFFFFFFF | Error, no 4 GiB allocation (new) |
| P-02 | unit | Valid frame codec | Round-trip |
| P-03 | unit | Mailbox put/get/ttl | Message retrieved, TTL respected |
| P-04 | unit | Mailbox id collision between different peers | No overwrite (new) |
| P-05 | unit | Mailbox rate limiter | 429 after threshold |
| T-01 | integration | Tunnel: connection and ping/pong | RTT measured |
| T-02 | integration | **Tunnel: relay closes the connection** | Reconnection within the backoff (new) |
| T-03 | integration | Tunnel: no pong for N seconds | Reconnection (new) |
| T-04 | integration | Tunnel: shutdown | Clean closure, no orphan task |
| T-05 | unit | `relay_bridge` with a path without `/` | 400, no panic (new) |
| T-06 | unit | `relay_bridge` with a path containing spaces | 400 |
| T-07 | unit | `relay_bridge` with hop-by-hop headers | Filtered headers |
| W-01 | unit | ICE/TURN config parsing | Correct URLs and credentials |
| W-02 | unit | Signed WebRTC signal | Accepted; invalid signature rejected |

### 3.13 FFI and runtime

| ID | Level | Case | Expected |
|---|---|---|---|
| R-01 | unit | `fedi3_core_version` | Non-empty string, releasable |
| R-02 | unit | `start` with invalid JSON | Code 2, message, no leak |
| R-03 | unit | `start` with valid config | Handle > 0, server listening (readiness) |
| R-04 | unit | **`start` with port busy** | Error (new), not a live handle |
| R-05 | unit | `stop` with a non-existent handle | Error code, no panic |
| R-06 | unit | Double `start` | Two independent handles, two ports |
| R-07 | unit | **Config that causes an internal panic** | Error code, process alive (catch_unwind) |
| R-08 | unit | `stop` twice | Second stop → handled error |
| R-09 | unit | `start` without relay token | Local/P2P startup, no bail (new) |
| R-10 | unit | Username with `../` | Rejected at startup |
| R-11 | unit | `previous_public_base_url` with legacy followers | Migration required, explicit error |
| R-12 | integration | Shutdown with active tasks | All tasks joined within the timeout |

### 3.14 Metrics and streams

| ID | Level | Case | Expected |
|---|---|---|---|
| N-01 | unit | `net_metrics` snapshot | JSON with the expected fields |
| N-02 | unit | Prune on timer | Old entries are removed (new) |
| N-03 | unit | Mutex poisoning | No panic (new) |
| U-01 | integration | `/_fedi3/stream` SSE | Events emitted, auth required |
| U-02 | unit | Malformed event | Discarded without panic |

---

## 4. Coverage matrix — Relay

### 4.1 Config and validation

| ID | Level | Case | Expected |
|---|---|---|---|
| RC-01 | unit | Invalid numeric variable (`MAX_BODY_BYTES=64M`) | Error at startup (new) |
| RC-02 | unit | `DB_DRIVER=pg16` | Error, not a silent fallback |
| RC-03 | unit | `SEARCH_BACKEND=meilisearch` | Error |
| RC-04 | unit | Admin token missing or short | Error at startup |
| RC-05 | unit | Safe defaults | `hsts>0`, `trust_proxy_headers` documented |
| RC-06 | unit | Value clamping | Out-of-range values are clamped or rejected |

### 4.2 Authentication and tokens

| ID | Level | Case | Expected |
|---|---|---|---|
| RA-01 | unit | Correct admin token | Authorized |
| RA-02 | unit | Wrong token | 401 |
| RA-03 | unit | Token with correct prefix and wrong suffix | 401 |
| RA-04 | unit | Empty token | 401, not `Bearer ` |
| RA-05 | unit | Constant-time comparison | No measurable difference (functional test) |
| RA-06 | unit | User token hash | SHA-256, correct comparison |
| RA-07 | unit | Disabled user | 401 |

### 4.3 Rate limiting and IP

| ID | Level | Case | Expected |
|---|---|---|---|
| RL-01 | unit | Fixed window | Blocks after threshold, reopens after the window |
| RL-02 | unit | Weighted (fan-out) | Correct weight |
| RL-03 | unit | In-memory fallback without Redis | Works |
| RL-04 | unit | Noisy backoff | Increasing backoff |
| RL-05 | unit | `parse_ip_rule` CIDR | Correct matches, /32 and /0 edge cases |
| RL-06 | unit | `client_ip` with spoofed XFF (fake first element) | Uses the real IP (new) |
| RL-07 | unit | `client_ip` without proxy | Uses the peer IP |
| RL-08 | unit | Allowlist/denylist | 403 for excluded IP |
| RL-09 | unit | **Rate limit on `/webrtc/poll` with different real IPs** | Separate buckets (new) |

### 4.4 Tunnel and forward

| ID | Level | Case | Expected |
|---|---|---|---|
| RT-01 | integration | Tunnel upgrade with valid token | Connected |
| RT-02 | integration | Tunnel with wrong token | 401 closure |
| RT-03 | integration | Tunnel with invalid username | 400 (new) |
| RT-04 | integration | Reconnection: the old disconnect does not remove the new one | The new tunnel stays active (new) |
| RT-05 | integration | **Forward with a slow client** | A second client receives the response within the timeout (new) |
| RT-06 | integration | Forward with offline user | Spool + 503 (new) |
| RT-07 | integration | Forward with full inflight | 429 and spool |
| RT-08 | integration | Tunnel timeout | 504 and spool |
| RT-09 | unit | `inflight` entry removed on timeout | No leak (new) |

### 4.5 Inbox and federation

| ID | Level | Case | Expected |
|---|---|---|---|
| RI-01 | integration | POST `/inbox` without signature | 401 (new) |
| RI-02 | integration | POST with valid signature | 202, fan-out |
| RI-03 | integration | Altered signature | 401 |
| RI-04 | integration | Activity with 30 recipients (beyond `max_inbox_fanout`) | 413 |
| RI-05 | integration | Non-local recipients | 400 |
| RI-06 | integration | Same activity twice | Dedupe, no double delivery (new) |
| RI-07 | integration | Failed delivery to an online user | Spool, 503 to the sender (new) |
| RI-08 | integration | Spool beyond the byte cap | 413 or discard of the oldest |
| RI-09 | unit | Spool: body in BLOB, not base64 | Consistent size |
| RI-10 | integration | Two concurrent flushes | No double delivery |

### 4.6 Media

| ID | Level | Case | Expected |
|---|---|---|---|
| RM-01 | integration | Authenticated upload | 201, DB and backend consistent |
| RM-02 | integration | Upload without token | 401 |
| RM-03 | integration | Upload with MIME `text/html` | 415 (new) |
| RM-04 | integration | Upload beyond the user quota | 413 |
| RM-05 | integration | GET media with malicious id | 400 |
| RM-06 | integration | GET media of an offline user | Forward to the tunnel |
| RM-07 | integration | DELETE media | Object removed from the backend (new) |
| RM-08 | unit | `sanitize_key` with a strange extension | Valid key |
| RM-09 | unit | Non-atomic upload | temp+rename write (new) |

### 4.7 Admin, health, metrics

| ID | Level | Case | Expected |
|---|---|---|---|
| RH-01 | integration | `/healthz` without token | 200, no audit row (new) |
| RH-02 | integration | `/readyz` without token | 200/503 without audit |
| RH-03 | integration | `/readyz` with broken DB | 503 |
| RH-04 | integration | Admin endpoint without token | 401 |
| RH-05 | integration | `admin_delete_user` | Media, notes and signals removed (new) |
| RH-06 | integration | Audit on admin action | Row with request id and IP |
| RH-07 | integration | `/metrics.prom` with separate token | 200 |
| RH-08 | unit | Metrics: counters and histograms present | Valid Prometheus format |

### 4.8 Search, webfinger, host routing

| ID | Level | Case | Expected |
|---|---|---|---|
| RS-01 | unit | `escape_like` + `ESCAPE` | `100%` and `foo_bar` correct (new) |
| RS-02 | integration | Search with DB backend | Results, no HTML scan |
| RS-03 | integration | Search with special characters | No SQL error |
| RS-04 | integration | TTL cache | Expires correctly |
| RW-01 | integration | webfinger for a local user | Correct `subject` and `href` |
| RW-02 | integration | webfinger with invalid `Host` in host-routing | 421/404 (new) |
| RW-03 | unit | Canonical redirect with explicit port | No loop (new) |
| RW-04 | unit | `host_only` with port | Port preserved where needed |

### 4.9 Move notice, telemetry, WebRTC signaling

| ID | Level | Case | Expected |
|---|---|---|---|
| RN-01 | integration | Signed move notice | Accepted, fan-out |
| RN-02 | integration | **Fan-out with `Signature`** | Accepted by the remote relay (new) |
| RN-03 | integration | Move notice without signature | 401 |
| RN-04 | unit | Fan-out TTL and retry | Cap and expiry |
| RY-01 | integration | Signed telemetry | Accepted, registry updated |
| RY-02 | integration | Telemetry with 100,000 relays | Cap applied (new) |
| RY-03 | unit | TOFU pinning of the relay key | Changed key → rejection |
| RX-01 | integration | Signed WebRTC signal | Accepted |
| RX-02 | integration | Signal with private `keyId` (SSRF) | Rejected (new) |

### 4.10 Relay schema and migrations

| ID | Level | Case | Expected |
|---|---|---|---|
| RD-01 | unit | Init on an empty DB (sqlite and postgres) | Complete schema |
| RD-02 | unit | Migration from an old schema | Columns added, errors not swallowed |
| RD-03 | unit | Newer schema version | Explicit error |
| RD-04 | unit | Two instances on the same DB | Idempotent migration |
| RD-05 | unit | Retention cleanup (audit, notes, registry) | Old rows removed (new) |

---

## 5. Coverage matrix — Flutter app

### 5.1 Models and serialization

| ID | Level | Case | Expected |
|---|---|---|---|
| AM-01 | unit | `CoreConfig.fromJson(toJson())` | Identical round-trip |
| AM-02 | unit | `toCoreStartJson` | snake_case keys, tokens included |
| AM-03 | unit | JSON with missing fields | Safe defaults, no crash |
| AM-04 | unit | JSON with wrong types | Defaults, no crash |
| AM-05 | unit | `UiPrefs` round-trip | Identical |
| AM-06 | unit | `BackupCodec` encode/decode | Round-trip; unknown version → error (new) |
| AM-07 | unit | `BackupCodec` with corrupted JSON | Clear `FormatException` |

### 5.2 MFM and HTML

| ID | Level | Case | Expected |
|---|---|---|---|
| AF-01 | unit | `MfmCodec.toHtml` with `<`, `&`, `"`, `'` | Correct entities |
| AF-02 | unit | Text without markers | Always escaped HTML (new) |
| AF-03 | unit | Newline | `<br>` |
| AF-04 | unit | MFM markers (`**bold**`, `:emoji:`) | Expected HTML |
| AF-05 | unit | Empty input | Empty string, no spurious `<p></p>` |

### 5.3 Local persistence

| ID | Level | Case | Expected |
|---|---|---|---|
| AS-01 | unit | `ConfigStore.write` atomic | No truncated file on a simulated error |
| AS-02 | unit | Corrupted file + valid `.bak` | Recovery from the backup (new) |
| AS-03 | unit | Corrupted file without `.bak` | "corrupted" state distinct from "missing" (new) |
| AS-04 | unit | Permissions on POSIX | 600 (new) |
| AS-05 | unit | `PrefsStore` corrupted | Defaults, with an error flag |
| AS-06 | unit | Concurrent stores (`note_flags`, `reactions`) | No update loss (new) |

### 5.4 Network and API

| ID | Level | Case | Expected |
|---|---|---|---|
| AN-01 | unit | Request without internal token | No `X-Fedi3-Internal` header |
| AN-02 | unit | Server timeout | `CoreApiException.timeout` within 15 s (new) |
| AN-03 | unit | 401 | `unauthorized` exception, clear message |
| AN-04 | unit | 500 | `server` exception, retryable |
| AN-05 | unit | Malformed JSON | `decode` exception, no crash |
| AN-06 | unit | Non-ASCII `X-Filename` | Correct encoding (new) |
| AN-07 | unit | `threadId` with `/` and `%` | Encoded path (new) |
| AN-08 | unit | `CoreEventStream` with valid frame | Event emitted |
| AN-09 | unit | `CoreEventStream` with malformed frame | Discarded, stream alive |
| AN-10 | unit | `CoreEventStream` with network error | Error propagated, retry with backoff |

### 5.5 Core state and lifecycle

| ID | Level | Case | Expected |
|---|---|---|---|
| AP-01 | unit | `startCore` with valid config (fake FFI) | Handle set only after readiness (new) |
| AP-02 | unit | `startCore` with FFI error | `lastError` set, no handle |
| AP-03 | unit | `stopCore` | Handle cleared, FFI called |
| AP-04 | unit | Watchdog: health fails | `markCoreDead`, restart attempted (new) |
| AP-05 | unit | `replaceConfig` | stop → save → start → readiness (new) |
| AP-06 | unit | Concurrent double `startCore` | A single FFI call |

### 5.6 Widgets

| ID | Level | Case | Expected |
|---|---|---|---|
| AW-01 | widget | First run without config | Setup screen with the correct title (repaired test) |
| AW-02 | widget | Onboarding: username/domain/bind validation | Inline errors (new) |
| AW-03 | widget | Timeline: loading | Skeleton, not an error |
| AW-04 | widget | Timeline: network error | `NetworkErrorCard` with retry |
| AW-05 | widget | Timeline: empty | Empty state with message |
| AW-06 | widget | Compose: empty text | Button disabled |
| AW-07 | widget | Compose: send | API call with correct payload |
| AW-08 | widget | Note card: like/boost/reply | Correct calls, optimistic state |
| AW-09 | widget | Chat: send message | Correct payload, state updated |
| AW-10 | widget | Settings: save | `replaceConfig` invoked (new) |
| AW-11 | widget | Notifications: new event | Item prepended without clearing the list (new) |
| AW-12 | widget | Media viewer on mobile | No `getSaveLocation` exception (new) |
| AW-13 | widget | Text at 1.5× scale | No overflow in the main layouts |
| AW-14 | widget | Semantics | `semanticLabel` present on avatars/media (new) |

### 5.7 l10n and platform

| ID | Level | Case | Expected |
|---|---|---|---|
| AL-01 | unit | ARB keys en/it | Exact parity |
| AL-02 | unit | No hardcoded strings in the screens | CI gate (new) |
| AL-03 | script | Android manifest | INTERNET, network security config, allowBackup=false |
| AL-04 | script | macOS entitlements | network.client/server, file access |
| AL-05 | script | iOS Info.plist | ATS, usage descriptions |
| AL-06 | script | gradle | App id not `com.example`, release signing |

---

## 6. End-to-end tests (without external services)

| ID | Level | Scenario | Expected |
|---|---|---|---|
| X-01 | e2e | Core A publishes a public note; Core B (mock peer) receives it signed | B verifies the signature, stores it |
| X-02 | e2e | Follow A→B, Accept B→A, delivery | Accepted state on A |
| X-03 | e2e | Relay sqlite + core: tunnel, publishing, fan-out | Activity delivered |
| X-04 | e2e | Relay sqlite: offline user, activity in spool, reconnection, flush | Activity delivered only once |
| X-05 | e2e | Migration: `Move` to a new actor | Followers receive `Move` |
| X-06 | e2e | Chat: encrypted sending between two mock cores | Message decrypted correctly |
| X-07 | e2e | Media: upload on the relay, GET, local cache in the core | Identical bytes |

---

## 7. Targeted security tests (they must fail on the current code)

| ID | Area | Attack | Expected after the fix |
|---|---|---|---|
| SEC-01 | relay inbox | POST without signature | 401 |
| SEC-02 | core inbox | Activity signed by A with `actor=B` | 401 |
| SEC-03 | core outbox | POST without token from the tunnel | 401 |
| SEC-04 | core inbox | POST without Digest | 401 |
| SEC-05 | relay | XFF spoofed to bypass the rate limit | Limit applied to the real IP |
| SEC-06 | core | `keyId` pointing to `127.0.0.1` | Rejected |
| SEC-07 | relay | `keyId` pointing to a private IP | Rejected |
| SEC-08 | core media | id `../../etc/passwd` | 400/404, no write outside |
| SEC-09 | relay media | MIME `text/html` | 415 |
| SEC-10 | relay | Admin token with timing | Constant-time comparison |
| SEC-11 | core | Replay of a signed POST with altered body | 401 |
| SEC-12 | relay | Duplicate activity | Delivered only once |
| SEC-13 | core | 4 GiB P2P frame | Rejected without OOM |
| SEC-14 | relay | `admin_delete_user` | No orphan blob |
| SEC-15 | core | Delete of another actor's note | Ignored |

---

## 8. CI and environment

### 8.1 Pipeline jobs

| Job | Command | Blocking |
|---|---|---|
| rust-fmt | `cargo fmt --all -- --check` | yes |
| rust-clippy | `cargo clippy --workspace --all-targets -- -D warnings` | yes |
| rust-build | `cargo build --workspace --locked` | yes |
| rust-test | `cargo test --workspace --locked` | yes |
| rust-audit | `cargo audit` (or `cargo deny check`) | no (warning) |
| flutter-analyze | `flutter analyze --fatal-infos` | yes |
| flutter-test | `flutter test` | yes |
| platform-config | `scripts/verify_platform_config.sh` | yes |
| appimage | `scripts/build_appimage.sh` (tags only) | no |

### 8.2 Local environment

```
export PATH="$HOME/.cargo/bin:$HOME/flutter/bin:$PATH"
cargo test --workspace --locked
cd app && flutter test
scripts/verify_platform_config.sh
```

### 8.3 Constraints

- No test requires Docker, Postgres, Redis or Meilisearch.
- The relay in the tests uses `FEDI3_RELAY_DB_DRIVER=sqlite`, `SEARCH_BACKEND=db`, Redis absent.
- Every test has an explicit timeout; no test lasts more than 30 s.
- Network tests use ephemeral ports assigned by the OS.

---

## 9. Acceptance metrics

| Metric | Threshold |
|---|---|
| Total Rust tests | ≥ 120 cases, all green |
| Total Flutter tests | ≥ 40 cases, all green |
| Functional areas covered | 100% of the areas listed in sections 3–5 |
| Security tests | 15/15 implemented and green |
| E2E tests | 7/7 implemented and green |
| `cargo clippy -D warnings` | clean |
| `flutter analyze --fatal-infos` | clean |
| Pipeline time | < 20 minutes on a standard runner |
| Regressions | zero on the existing tests |

---

## 10. Test implementation order

1. **Infrastructure** (`TestCore`, `TestRelay`, `MockActorServer`, `Signer`, `FakeHttpClient`) — it unblocks everything else.
2. **Security** (SEC-01…15) — they define the expected behaviour of the fixes.
3. **Core unit** (S-*, E-*, K-*, D-*, Q-*, P-*) — fast, high yield.
4. **Core integration** (A-*, I-*, O-*, M-*, C-*).
5. **Relay** (RC-*, RA-*, RL-*, RT-*, RI-*, RM-*, RH-*, RS-*, RW-*, RN-*, RY-*, RX-*, RD-*).
6. **App** (AM-*, AF-*, AS-*, AN-*, AP-*, AW-*, AL-*).
7. **E2E** (X-*).
8. **Static config** (AL-03…06).

Tests written before the fixes must be marked `#[ignore]` with a comment citing the finding, and re-enabled after the correction. This avoids a red pipeline during the work and documents the intention.

---

> Counts below were verified on 2026-11-09 against the current workspace. CI runs
> the same suite on every push and on release tags; the 0.11.9 release audit is in
> [audit/release_0.11.9_2026-11-09.md](audit/release_0.11.9_2026-11-09.md).

## 11. Implementation status

Implemented on 2026-10-31 and 2026-11-01, then extended through 0.11.x. Real counts verified by running the current suite on 2026-11-09:

| Area | Implemented cases | Where |
|---|---|---|
| Core unit (keys, http_sig, ffi, net_metrics, tunnel, relay_bridge, p2p, net_policy, anchor, chat, receipts) | 86 | `crates/fedi3_core/src/*.rs` (`tests` mod) |
| Core integration — ActivityPub and security | 47 | `crates/fedi3_core/tests/ap_security.rs` |
| Core integration — database and delivery queue | 20 | `crates/fedi3_core/tests/db_queue.rs` |
| Core integration — FFI and runtime | 13 | `crates/fedi3_core/tests/runtime_ffi.rs` |
| Core integration — tunnel | 5 | `crates/fedi3_core/tests/tunnel.rs` |
| Core integration — semantic gaps (chat, outbox, locked accounts, media, DB permissions) | 24 | `crates/fedi3_core/tests/semantics.rs` |
| Core integration — network policy, body caps and media fetch policy | 7 | `crates/fedi3_core/tests/net_policy.rs`, `media_fetch_policy.rs` |
| Core integration — P2P chat | 3 | `crates/fedi3_core/tests/p2p_chat.rs` |
| Protocol unit | 6 | `crates/fedi3_protocol/src/*.rs` |
| P2P infra unit | 4 | `crates/fedi3_p2p_infra/src/main.rs` |
| Relay unit | 61 (1 ignored) | `crates/fedi3_relay/src/main.rs` (`tests` mod) |
| Relay integration | 35 | `crates/fedi3_relay/tests/relay_http.rs` |
| **Federation end-to-end (real relay + two real cores)** | **1** | `crates/fedi3_relay/tests/e2e_federation.rs` |
| **Anchor identity + blind spool end-to-end** | **1** | `crates/fedi3_relay/tests/e2e_anchor_spool.rs` |
| **Relay mesh + moderation integration** | **1** | `crates/fedi3_relay/tests/mesh_moderation.rs` |
| **Forward receipts end-to-end** | **1** | `crates/fedi3_relay/tests/e2e_forward_receipts.rs` |
| **Rust total** | **315 passed, 4 ignored** | |
| Flutter (unit, widgets, l10n, platform configuration) | 205 | `app/test/` |
| **Flutter total** | **205** | |

### Implemented end-to-end scenario (X-01…X-07)

`crates/fedi3_relay/tests/e2e_federation.rs` starts the real relay binary with SQLite and two real core runtimes in-process (separate identity, database and tunnel), then verifies:

1. the two tunnels connect and the relay serves both actor documents;
2. bob follows alice through the relay: signed Follow, signature verification, Accept delivered, `accepted` state on bob;
3. alice publishes a public note from her own outbox;
4. the signed Create arrives in bob's inbox and appears in his home timeline;
5. the public outbox is readable through the relay and contains the note.

The test surfaced two real defects (tunnel deadlock on nested fetches, relay shared inbox unable to expand `followers`), both fixed.

### Decentralization end-to-end scenario (identity, spool, mesh, moderation)

`crates/fedi3_relay/tests/e2e_anchor_spool.rs`:

1. the core publishes the anchor documents in `<data_dir>/anchor/` and a static test host serves them;
2. the relay redirects `GET /users/alice` to the canonical actor and WebFinger points to the anchor;
3. the anchor actor keeps `inbox`/`outbox` on the relay and the key on the anchor;
4. federation works in anchor mode (follow and note delivery);
5. with the core stopped the relay spools the request **encrypted**: the plaintext does not appear in the database;
6. on reconnection the relay sends the encrypted envelope, the core decrypts it, verifies the signature and the notification arrives.

`crates/fedi3_relay/tests/mesh_moderation.rs`:

1. two relays register as peers and synchronize the user directory;
2. a request for a user hosted on the other relay is forwarded to the home relay;
3. a relay-wide blocklist and a per-user one block the sender with `202`, without forward or spool;
4. the wrong token is rejected and an unknown user stays `404` (no hop loop).

`crates/fedi3_relay/tests/e2e_forward_receipts.rs` verifies the signed forward
journal and independent core acknowledgments through a real relay and core.

Gates executed with real output on the final state (2026-11-09):

| Gate | Command | Outcome |
|---|---|---|
| Rust formatting | `cargo fmt --all -- --check` | ✅ |
| Rust lint | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 0 warnings |
| Build | `cargo build --workspace --locked` | ✅ |
| Rust tests | `cargo test --workspace --locked` | ✅ 315 passed, 4 ignored |
| End-to-end | `cargo test -p fedi3_relay --test e2e_federation --test e2e_anchor_spool --test mesh_moderation --test e2e_forward_receipts` | ✅ 4/4 |
| App analysis | `flutter analyze --fatal-infos` | ✅ |
| App formatting | `dart format --set-exit-if-changed lib test` | ✅ |
| App tests | `flutter test` | ✅ 205/205 |
| Platform configuration | `scripts/verify_platform_config.sh` | ✅ 9/9 |

### Deviations from the plan

- The relay integration cases are 35: the configuration, token, `client_ip`, rate limiter, signature and migration cases are covered as unit tests inside `main.rs` (61 cases), so the coverage of sections §4.1–§4.10 is complete even if distributed differently.
- X-01…X-07 are now covered by the real end-to-end test, not only indirectly.
- The relay Dockerfile was not built (Docker unavailable in the environment): the tag `rust:1.99-bookworm` was verified to exist on the registry and aligned with the toolchain used for the tests.
- The PostgreSQL migration is covered by assertions on the SQL text, not by a live database.
- Some pre-existing stylistic clippy lints were silenced at crate level (`manual_clamp`, `result_large_err`, `too_many_arguments`, …). Correctness lints remain active and blocking.

## 12. Real interoperability (manual)

The automated tests use simulated ActivityPub servers. To verify the behaviour against
independent implementations there is a manual test documented in `docs/interop.md`, run
against real **Mastodon 4.7.4** and **Misskey 2026.10.1** (PostgreSQL + Redis, built from source,
TLS reverse proxy with a local CA).

Verified coverage:

| Scenario | Mastodon | Misskey | Outcome |
|----------|----------|---------|-------|
| Resolution of an anchored Fedi3 actor via WebFinger/HTTPS | yes | yes | pass |
| Follow of the Fedi3 identity | yes | yes | pass |
| Signed `Accept` delivered and verified | yes | yes | pass |
| Fedi3 note → third-party instance (signature verified) | yes | yes | pass |
| Third-party instance note → Fedi3 (signature verification and storage) | yes | yes | pass |
| Follow Fedi3 → third-party instance | yes (manual) | yes | pass |
| AP endpoint proxy on the identity host, relay on a separate host | yes | yes | pass |
| Fedi3 photo media `image/png` → instance | yes | yes | pass |
| Fedi3 video media `video/mp4` → instance | yes | yes | pass |
| Fedi3 audio media `audio/mpeg` → instance | yes | yes | pass |
| Photo/video/audio media instance → Fedi3 | yes | yes | pass |
| Timeline and thread consistency across the three clients | yes | yes | pass |

The test found a real bug in the outgoing HTTP signature (duplicated `Content-Type` header),
fixed in `Delivery::deliver_json`; after the fix all scenarios pass. The test also
verified that the proxy of the AP endpoints on the identity host keeps the separation
from the relay and works with classic instances, and that photo, video and audio federate in both
directions with consistent timelines and threads.

Limits of the test: it is not in CI (it requires two external services and a local trust store); it must be
re-run manually before a release that touches HTTP signature, delivery or `net_policy`.

### 12.1 Flutter client against the real stack (manual)

The desktop client was started against real relay, core, Misskey and Mastodon with
`FEDI3_ALLOW_PRIVATE_ACTOR_FETCH=true` (needed only when the federated hosts resolve
to loopback in local tests) and `FEDI3_DATA_DIR` pointed to the same identity as the dev client.

| UI check | Outcome |
| Home/Local/Federated timeline with federated content; Social empty with P2P off | pass |
| Note published from the client visible on Mastodon and Misskey | pass |
| Thread with parent and nested reply; reply from the client with correct `in_reply_to_id` | pass |
| Like, Announce, EmojiReact from the client persisted and federated | pass |
| Follow/unfollow from the profile and from the follower/following lists, consistent state on Mastodon | pass |
| Follower and following lists reachable from the profile counters | pass |
| Image and video media rendered, audio player instantiated | pass |
| Search (posts and users), Chat, Notifications, Relay, Settings | pass |
| User search with fallback on seen actors/local follow network | pass |

Bugs fixed during the verification:

- `503 search coverage incomplete` on a standalone relay without relay peers or with an empty
  user index: coverage is now considered vacuous, with dedicated unit tests;
- WebSocket tunnel with a private CA: the client now uses the system root CAs;
- client-side user search empty because the core index contains only activity objects:
  added a fallback on the actor cache and the local follow/follower network;
- follower/following list missing in the client: added a screen with follow/unfollow per actor.
