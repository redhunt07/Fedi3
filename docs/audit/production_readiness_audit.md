# FEDI3 — Production readiness audit

**Date:** 2026-10-31
**Scope:** entire repository — `crates/*` (28,358 lines of Rust), `app/lib` (27,244 lines of Dart), `deploy/`, `scripts/`, `.github/`, docs, platform configurations.
**Method:** static review of the code and configuration, conducted in parallel on four areas (relay, Rust core, Flutter app, ops/repo). In this environment **neither Rust nor Flutter/Dart is installed**, so nothing was compiled or executed: every conclusion derives from reading the source. Points marked `⚠️ to be verified` require a runtime test.

---

## 1. Verdict

The project has a serious and non-trivial architectural base: real ActivityPub, implemented HTTP Signatures, relay with Postgres/Redis/Meilisearch, libp2p/WebRTC P2P, E2E encryption, complete IT/EN i18n, storage GC, persistent delivery queue. It is not a prototype.

**It is not, however, ready for production.** The blockers are of three kinds:

1. **Release builds cannot work.** Android release does not have the `INTERNET` permission; traffic to the local core is blocked by the Android 9+ cleartext policy; macOS release has neither `network.client` nor `network.server`; iOS has no ATS exceptions; the app id is `com.example.fedi3` and the release is signed with the debug key.
2. **Federation has authentication gaps.** The relay `/inbox` endpoint **verifies no HTTP signature**; the core verifies the signature but **does not bind the activity's `actor` to the signer**; the `outbox` accepts unauthenticated POSTs even through the public tunnel. Anyone can inject or impersonate activities.
3. **Reliability is not guaranteed.** The tunnel to the relay never reconnects; a Rust panic crosses the FFI boundary and kills the app; there is not a single Rust test and no CI.

Below: blockers (P0), HIGH, MEDIUM, LOW, roadmap, and what is already done well.

---

## 2. Release blockers (P0)

### P0-01 — Android release: missing `INTERNET` permission
`app/android/app/src/main/AndroidManifest.xml` does not declare `<uses-permission android:name="android.permission.INTERNET"/>`. The permission exists only in `app/android/app/src/debug/AndroidManifest.xml:6` and `app/android/app/src/profile/AndroidManifest.xml:6`, which the manifest merger applies **only** to debug/profile builds.

**Effect:** a `--release` build has no network access. Every call to `127.0.0.1:8788` and to the relay fails with `SocketException`. The app appears permanently offline.

**Fix:** add `INTERNET` in `src/main/AndroidManifest.xml` (and `POST_NOTIFICATIONS`, see M-04), remove the duplicate from the debug/profile manifests, and add in CI a check on a release APK (`aapt dump permissions`).

### P0-02 — Android: cleartext to the local core is blocked
The app uses `http://$bind` (`app/lib/model/core_config.dart:83`, consumed by `app/lib/core/core_api.dart:24-48`). There is no `network_security_config.xml` (`find app/android -name "network_security_config*"` → empty) and the manifest does not set `android:usesCleartextTraffic`. With `targetSdk >= 28` Android blocks cleartext **even on loopback**.

**Effect:** `CLEARTEXT communication to 127.0.0.1 not permitted` on every `/_fedi3/*` endpoint. In addition `media_kit`/ExoPlayer on Android applies the same policy to federated `http://` media (while `dart:io` ignores it), creating inconsistent behaviour.

**Fix:** `app/android/app/src/main/res/xml/network_security_config.xml` with `cleartextTrafficPermitted="true"` limited to `127.0.0.1`/`localhost`, referenced by `android:networkSecurityConfig`.

### P0-03 — macOS release: entitlements without network and without file access
`app/macos/Runner/Release.entitlements` contains only `com.apple.security.app-sandbox`; `DebugProfile.entitlements:5-11` has `app-sandbox`, `allow-jit` and `network.server` but **not** `network.client`. Neither of them has `files.user-selected.read-write`.

**Effect:** with App Sandbox, the release cannot make outgoing connections (core + relay unreachable), nor do `TcpListener::bind` (`crates/fedi3_core/src/runtime.rs:695`); `file_selector` (backup export/import, avatar, media download) fails. **The macOS release is non-functional.**

**Fix:** add `com.apple.security.network.client`, `com.apple.security.network.server` and `com.apple.security.files.user-selected.read-write` to both files; verify with `codesign -d --entitlements`.

### P0-04 — iOS: no ATS exception
`app/ios/Runner/Info.plist` does not contain `NSAppTransportSecurity`. The app talks to the core on `http://127.0.0.1:8788`. ⚠️ to be verified on device, but on iOS `http://` traffic to loopback is not guaranteed to be exempt on all versions.

**Fix:** `NSAppTransportSecurity` → `NSAllowsLocalNetworking=true` (preferable to `NSAllowsArbitraryLoads`). Also add `NSPhotoLibraryUsageDescription`/`NSCameraUsageDescription` (both missing, see M-08).

### P0-05 — Placeholder application identity and signing with debug keys
`app/android/app/build.gradle.kts:24` `applicationId = "com.example.fedi3"`; `:34-38` the release uses `signingConfigs.getByName("debug")`. The same id appears in `app/linux/CMakeLists.txt:11` (`APPLICATION_ID "com.example.fedi3"`).

**Effect:** Google Play rejects/limits `com.example.*` (it must be unique and immutable) and a release signed with the debug key cannot be updated from a different keystore.

**Fix:** final id (e.g. `org.fedi3.client`), `key.properties` + release keystore, `signingConfig = signingConfigs.getByName("release")`, align the macOS bundle id and Windows metadata.

### P0-06 — The relay verifies no HTTP signature on `/inbox`
`crates/fedi3_relay/src/main.rs:2763-2857`: `shared_inbox` receives `headers: HeaderMap` but **never calls** a signature verification. `grep` for `Signature|verify` in the handler body → no results. `headers` is used only for `client_ip` and rate limiting.

**Effect:** anyone on the internet can `POST` an arbitrary activity with `to` towards any local user. The activity is (a) forwarded into the user's tunnel, (b) indexed in `relay_notes` and Meilisearch (`index_activity_bytes_for_search`, line 2806), (c) queued in the spool for 7 days if the user is offline. A single attacker can poison the shared search index and inflate the spool of any user.

**Fix:** call `verify_actor_signature_for_request(...)` at the beginning of the handler (the function already exists and is used by `webrtc_signal_send`), return 401 on failure, and make it mandatory (see also H-11).

### P0-07 — The core does not bind the activity's `actor` to the signer (impersonation)
`crates/fedi3_core/src/ap.rs:4793` resolves the key from `sig.key_id`, `ap.rs:4803` verifies the signature against `summary.actor_url`, then `ap.rs:4871` passes the activity to `process_inbox_activity`, which uses `activity.get("actor")` for **everything** (storage, follower, Accept targets: `ap.rs:5183`, `5205`, `5263`). **There is no comparison** `activity.actor == summary.actor_url` anywhere in the inbox/receipt path.

**Effect:** any server, with its own valid key, can send `{"type":"Create","actor":"https://good.example/users/alice","object":{...}}` and Fedi3 stores it as if it were Alice's. Impersonation in feeds, `Delete` of other people's notes, `Accept` sent to a URL chosen by the attacker; blocks are evaluated against the signer (`ap.rs:4815`) and not against the declared actor.

**Fix:** after key resolution, require that `activity.actor` (string or `object.id`) matches `summary.actor_url`, and that the actor part of `sig.key_id` matches; 401 otherwise. Apply the same check to `receipt_post`, P2P/gossip ingest and legacy sync.

### P0-08 — `POST /users/<u>/outbox` is not authenticated and is reachable from the relay and from P2P
`crates/fedi3_core/src/ap.rs:4071-4073`: `outbox_post` does not call `require_internal` nor verify a signature (unlike `media_upload` at `ap.rs:528`). The tunnel router (`runtime.rs:741-746`) and the P2P one (`runtime.rs:758-763`) use the same `handle_request`, so the endpoint is reachable from outside. The only guard is the actor equality (`ap.rs:4094-4106`), which is not a defence against anyone who knows the username.

**Effect:** anyone who reaches the relay can publish activities **as the local user**, with `id` chosen by them: fake posts in their own outbox, arbitrary `Delete`, `Undo` of follows.

**Fix:** require `X-Fedi3-Internal` (or a valid HTTP signature) for `POST` on the outbox; keep a separate UI-only path if remote peers cannot send the token.

### P0-09 — The tunnel to the relay never reconnects
`crates/fedi3_core/src/runtime.rs:750-752` starts the tunnel **only once**. `tunnel.rs:41-47` performs a single `connect_async` without timeout; `tunnel.rs:74` `let msg = msg?;` and `tunnel.rs:110` `ws_tx.send(...).await?` propagate any error out of the function; nobody restarts the task. The error is even discarded (`runtime.rs:820` `let _ = tunnel.await;`, executed only at shutdown).

**Effect:** a relay restart, a network change, a NAT timeout or a rotated token make the instance **unreachable from the fediverse forever**, while the UI shows "connected". For a federated app this is the most serious defect of all.

**Fix:** reconnection loop with exponential backoff + jitter until shutdown, explicit timeout on the connect, watchdog on pongs (today pings are sent every 5 s without ever checking the absence of pongs, `tunnel.rs:51`), and connection state exposed to the UI via `NetMetrics`.

### P0-10 — Rust panics across the FFI boundary and registry with `Mutex::lock().unwrap()`
`crates/fedi3_core/src/ffi.rs:20` and `:46` are `extern "C"` without `catch_unwind`; `runtime.rs:181` and `:195` use `REGISTRY.lock().unwrap()`.

**Effect:** since Rust 1.81 an unwind that crosses `extern "C"` **aborts the process**. A panic in `start`/`stop`, or a mutex poisoned by a previous panic, kills the whole Flutter app. It is the most likely total crash in production.

**Fix:** `std::panic::catch_unwind(AssertUnwindSafe(...))` on every FFI entry point, with error code and message; replace `.unwrap()` with `lock().unwrap_or_else(|e| e.into_inner())`.

### P0-11 — `start()` returns success before the core has started
`runtime.rs:175-190` spawns the thread and returns `Ok(handle)` immediately; identity, DB and `TcpListener::bind` happen inside `run_core` (`runtime.rs:695`), and failures end up only in an `error!` (`runtime.rs:177`). On the Dart side `app/lib/state/app_state.dart:236` treats the return as "core active".

**Effect:** port already busy, invalid bind, corrupted DB or the `bail!` on the relay token (see H-09) produce a live handle with no server. The UI starts, requests fail with `connection refused`, and the user sees an empty timeline with repeated errors.

**Fix:** validate the config, open the DB and perform the `bind` **before** returning (or create the listener on the calling thread and pass it to the runtime), with a readiness oneshot awaited by `start()` and a timeout; return `Err` with a message.

### P0-12 — `edit_config` stops the core and does not restart it
`app/lib/ui/screens/edit_config_screen.dart:412-416`: `await stopCore(); await saveConfig(cfg); Navigator.pop();` — **no** `startCore()`. Comparison: `security_settings_screen.dart:146-148` does `stop → save → start` correctly.

**Effect:** after changing username/relay/bind the user returns to the timeline with `isRunning == false`; every column is empty (`_loadMore` exits immediately if the core is not running) and `AutoStartCore` does not restart because `_started` is already `true` (`auto_start_core.dart:46-52`). The app looks broken until the process is restarted.

**Fix:** centralize in `AppState.replaceConfig(cfg)` that does stop → persist → start → readiness wait, used by all settings screens and by backup import.

---

## 3. HIGH issues

### Security — federation and relay

**H-01 — Digest not mandatory and signed header set not validated.** `crates/fedi3_core/src/http_sig.rs:213-215`: if `Digest` is missing, `verify_digest_if_present` returns `Ok(())`. `http_sig.rs:159-162` uses `"date"` as the default for `headers`. The inbox (`ap.rs:4749`) does not require `digest` to be among the signed headers. A sender that signs only `date` leaves body and path unauthenticated: a MITM can rewrite the body and the `Date` while the RSA signature keeps verifying. **Fix:** for POSTs require a valid `Digest` and that `sig.headers` contains at least `(request-target)`, `host`, `date`, `digest`.

**H-02 — The inbox does not verify object ownership.** `ap.rs:5191-5205`: `Create|Update` does `upsert_object_with_actor(...)` without checking `object.attributedTo == actor`; `ap.rs:5212-5220`: `Delete` does `mark_object_deleted` without checking that the signer owns the object. Any federated actor can overwrite another actor's cached note, or delete arbitrary objects (including the local user's notes) with a signed `Delete`. **Fix:** require the `attributedTo`/actor match for Create/Update and equality with the stored actor for Delete.

**H-03 — SSRF on attacker-controlled URLs, without a guard on private IPs.** `http_sig.rs:38` `reqwest::Client::new()` and `http_sig.rs:60-71` fetch `actor_url` derived from `keyId`; `delivery.rs:36`/`79-105` fetch the actor and trust `endpoints.sharedInbox`; `ap.rs:6030-6054` fetch `object_url`; `legacy_sync.rs:155-189` follow remote `next` links. Nowhere is there a `.redirect(...)` or a check on loopback/private/link-local. An actor can point to `http://127.0.0.1:8788/...` or `http://169.254.169.254/...` and the core fetches it (reqwest follows up to 10 redirects). **Fix:** shared client with a redirect policy limited to http(s) and rejection of non-public IPs; apply it to KeyResolver, Delivery, object fetch, media and legacy sync.

**H-04 — Unbounded inbound response bodies (OOM).** `delivery.rs:92`, `http_sig.rs:73`, `ap.rs:6054`, `legacy_sync.rs:180`: `resp.text()`/`resp.bytes()` without cap. A server returning a 2 GB document gets buffered entirely in RAM. **Fix:** `Content-Length` + streaming with cap (256 KiB for actor/keys, 2 MiB for objects).

**H-05 — Media upload that bypasses the body limit and can decode a bomb.** `ap.rs:531` `to_bytes(body, usize::MAX)` cancels the router's `DefaultBodyLimit` (`runtime.rs:691`, 20 MB); `ap.rs:700-701` calls `image::load_from_memory` before any dimensional limit. A small compressed PNG can decode into hundreds of MB. **Fix:** use the configured limit, validate magic bytes and use `image::io::Reader::with_limits`.

**H-06 — The P2P codec allocates the length declared by the peer (up to 4 GiB).** `crates/fedi3_core/src/p2p.rs:312-313`: `let len = u32::from_be_bytes(...) as usize; let mut buf = vec![0u8; len];`. Any libp2p peer can send a `0xFFFFFFFF` header and force 4 GiB of zeroed allocation. **Fix:** reject `len > MAX_P2P_FRAME` (2–4 MiB).

**H-07 — Gossip/global ingest: first writer wins.** `ap.rs:1074-1099` accepts a public `Create`/`Announce` from any peer (the internal token protects the transport, not the author); `social_db.rs:795-810` uses `INSERT OR IGNORE`, so the first copy of an `id` wins and subsequent legitimate copies are discarded. A malicious peer can inject a note attributed to anyone and "occupy" the id of a real note with false content. **Fix:** validate the origin of actor/id before storing and prefer the version from the actor's domain in case of conflict.

**H-08 — `move_notice` fan-out does not sign requests, so it always fails.** `main.rs:8549-8562` sends `POST {relay}/_fedi3/relay/move_notice` without a `Signature` header; the receiver (`main.rs:7486-7505`) accepts only if `verify_move_notice_signature` is true (which returns `false` when `Signature` is missing, `main.rs:8770-8773`) or with an admin token. Every delivery gets 401, records `sent_ok=0` and is retried every 60 s with backoff for 7 days (`move_notice_ttl_secs`). **Effect:** migration between relays is a dead feature that generates traffic and useless DB rows. **Fix:** sign with the relay's Ed25519 key (already used for telemetry) and verify against the key in the registry.

**H-09 — The mandatory relay token also kills the local API.** `runtime.rs:715-716` `if relay_token.len() < 16 { anyhow::bail!(...) }` happens **after** the bind and the spawn of the server (`runtime.rs:695-706`) but **inside** `run_core`: the `bail` exits the thread, the tokio runtime is destroyed and the axum server aborted. A user who only wants P2P or local-only is left without UI. Note: the default value `"devtoken"` is 8 characters long, so the defaults fail too. **Fix:** make the relay optional (skip check and tunnel if `relay_ws` is empty) and report the error via the readiness handshake.

**H-10 — `forward_to_user` holds the global tunnel lock across a `send` without timeout and 15 s of waiting.** `main.rs:2606-2637`: `let tunnels = state.tunnels.read().await;` stays alive until the end of the function, `tunnel.tx.send(msg).await` has no timeout and blocks when the 64-slot channel is full (client not reading), then there is `timeout(15s, resp_rx)`. **Effect:** a single slow client blocks every new tunnel connection and every other forward. It is head-of-line blocking that brings the service to its knees. **Fix:** clone the `Sender` and **release the guard before** awaiting; put a timeout on the send and treat expiry as 503/spool.

**H-11 — A failed delivery to an online user is discarded and the relay still answers 202.** `main.rs:2810-2827`: if `is_online`, it calls `forward_to_user` and on `429/503/504/502` does `continue` **without** queueing in the spool. Then `main.rs:2852-2855` answers `202 ACCEPTED` if at least one recipient succeeded. **Effect:** permanent message loss right under load (inflight limit reached, or tunnel reconnecting). **Fix:** on any non-2xx response fall back to the spool; answer 202 only if every recipient was delivered **or** queued, otherwise 503.

**H-12 — Rate limit keyed on the proxy IP, not the client IP.** `main.rs:7534-7537` (and `7601`, `7362`, `7411`, `7450`, `6982`, `7266`) use `peer_ip(&peer)`, which behind Caddy is **always the same** for all internet clients, while `shared_inbox`/`webfinger` correctly use `client_ip(...)`. With `FEDI3_RELAY_RL_FORWARD_PER_MIN=240` a handful of instances exhausts the shared bucket and **everyone** gets 429, including `/webrtc/poll` which every client calls every few seconds. **Fix:** `client_ip(&state.cfg, &peer, &headers)` in all handlers and separate buckets per route.

**H-13 — `X-Forwarded-For` spoofable on two levels.** `main.rs:6404-6427` takes the **first** XFF value; Caddy appends the real IP to an existing XFF, so the first element is controlled by the client. Moreover `docker-compose.yml` publishes `0.0.0.0:8787:8787` besides Caddy and sets `FEDI3_RELAY_TRUST_PROXY_HEADERS=true`: whoever hits 8787 directly can send any `X-Real-IP`/`X-Forwarded-For` (Caddy does not set `X-Real-IP` by default and lets the client's one through). **Effect:** rate limit, IP allow/deny lists and the audit `ip` become attacker-controlled; moreover port 8787 serves plain HTTP without TLS. **Fix:** use the last XFF element or configure Caddy with `trusted_proxies` + overwrite; do not publish 8787 on the host.

**H-14 — Token comparison not constant-time and no startup validation.** `main.rs:3262-3267` `token == expected`; `main.rs:4218-4245` compares hashes with `==`; `main.rs:1217` `std::env::var("FEDI3_RELAY_ADMIN_TOKEN").ok()` without trim or length check. With `FEDI3_RELAY_ADMIN_TOKEN` empty (`docker-compose.yml:24` passes it as an empty string if missing in `.env`) `expected == ""` and only an `Authorization: Bearer ` with a trailing space passes, while `bearer_token()` treats empty as `None`: two conflicting notions of "empty", and all admin/health/metrics endpoints answer 401 with no error at startup. **Fix:** `subtle::ConstantTimeEq`, `bail!` at startup if the token is missing or < 32 characters.

**H-15 — `verify_actor_signature_for_request` accepts signatures covering almost nothing and only RSA.** `main.rs:8609` default `headers="date"`; `main.rs:8722-8734` the Digest is optional; `main.rs:8763` verifies only RSA PKCS#1 v1.5. Ed25519 (used by many modern implementations and by the project itself for telemetry) cannot be verified, so WebRTC signalling silently fails with 401 for those actors. Header parsing is split on `,` and `keyId=` is case-sensitive. **Fix:** require `(request-target)`, `host`, `date` (+ `digest` if there is a body), support Ed25519 (`ed25519-dalek` is already a dependency), parse quoted-string and case-insensitive keys.

**H-16 — The tunnel token travels in the query string and ends up in the logs.** `main.rs:571-573` `struct TunnelQuery { token: Option<String> }`; `tunnel.rs:37-39` builds `{relay_ws}/tunnel/{user}?token={token}` and logs it with `info!(%url, ...)`. Caddy records the full URI in access logs. The token is the user's long-lived credential (also used for media, search, move, WebRTC). **Fix:** accept `Authorization: Bearer` on the upgrade as the primary mechanism, keep `?token=` as fallback, or use short-lived one-time tickets; do not log the query.

**H-17 — The chat inbox accepts envelopes from any actor in any thread.** `ap.rs:2618-2636` decrypts and calls `chat::store_incoming_payload`; `chat.rs:260-279` does `create_chat_thread(thread_id, "dm", None)` and `upsert_chat_member(...)` for `op == "message"` without checking that the sender is a member of an existing thread. DM thread ids are a hash of the members' URLs, not a secret: anyone who knows the id can inject messages and create phantom threads. **Fix:** require that the thread exists and that `env.sender_actor` is a member.

**H-18 — Chat bundles are not bound to the peer that serves them.** `ap.rs:3544-3563` returns any `ChatBundle` received from `peer_id`; `chat.rs:167-181` only checks that the bundle signature validates against `bundle.actor`'s key, without checking that `bundle.actor` is the expected actor, that `bundle.peer_id` is the peer that served it, or that `bundle.did` matches. A peer can return another actor's valid bundle and the message becomes undecryptable. **Fix:** validate `peer_id`, `actor` and `did` after the fetch.

**H-19 — Key files and the DB have no restricted permissions.** `crates/fedi3_core/src/keys.rs:40-48` writes `identity_private_key.pem` with `fs::write` without `set_permissions`; no `chmod`/`PermissionsExt` exists in the crate (verified with grep). `p2p.rs:1136-1145` writes `p2p_keypair.pb` the same way. SQLite creates `fedi3.db`/`-wal` with the process umask (typically 0644). On a multi-user desktop the RSA private key, the P2P keypair and the Kyber secrets are readable by other local users. **Fix:** 0600 on key files and on the DB/WAL/SHM, 0700 on the data directory, atomic write (temp + rename).

**H-20 — No deduplication of activities in the relay.** `main.rs:2763-2857` reads only `to`/`cc`/`bcc`/`audience`; the activity `id` is never recorded or checked. Combined with H-11 (202 even on partial failure) and with sender retries, every retry re-delivers the same `Create`/`Announce` to the user, generating duplicate posts and notifications. **Fix:** dedupe table `(activity_id, recipient)` with short TTL and `ON CONFLICT DO NOTHING`.

### Relay reliability and scalability

**H-21 — A single global `Mutex<Db>` serializes every operation and SQLite opens one connection per query.** `main.rs:223` `db: Arc<Mutex<Db>>`; `main.rs:3299-3303` `open_sqlite_conn` does `Connection::open` + 5 PRAGMA (including `journal_mode=WAL`, which takes a lock) **for every query**; for Postgres `block_on_result` uses `block_in_place(|| handle.block_on(...))` (`main.rs:547-558`) parking a worker **while holding the global mutex**. Effective DB concurrency is 1, regardless of `FEDI3_RELAY_PG_POOL_MAX_SIZE=16`; `/healthz` takes the same mutex and can block behind a slow query. **Fix:** connection pool (r2d2/deadpool), removal of the outer `Mutex`, `spawn_blocking` for the sync path, or an async-native `Db` with `deadpool-postgres`.

**H-22 — Blocking IO (`std::fs`) inside async handlers, with multiple copies of the body.** `media_store.rs:58-63` and `:71-74` use `std::fs::create_dir_all/write/read` inside `async fn`; `main.rs:1967` does `body.to_vec()`, `media_store.rs:179` does another `to_vec()` for S3. A 64 MB upload blocks a tokio worker for the whole duration of the I/O and occupies up to 3 copies in RAM; with `rt-multi-thread` (worker = CPU core) a few large uploads starve the runtime: tunnels stop responding and the 15 s timeouts fire. **Fix:** `spawn_blocking`/`tokio::fs`, `ByteStream::from(bytes)` for S3 without copy, streaming on download (`ReaderStream` + `Body::from_stream`).

**H-23 — `/healthz` and `/readyz` require the admin token and write an audit row per probe.** `main.rs:2067-2079` calls `admin_guard` (rate limit + token) and then `insert_admin_audit`. A probe every 10 s writes ~8,640 rows/day; without a token the probe gets 401 and the orchestrator restarts the container in a loop; if probe and Prometheus share the 120/min bucket, a 429 makes the system believe the relay is down. Moreover **no `HEALTHCHECK` exists** in the Dockerfile or in the `relay` service of the compose, and `caddy` depends on `relay` without `condition`. **Fix:** `/healthz` unauthenticated, without DB and without side effects; `/readyz` with a read-only ping and no audit; add `HEALTHCHECK`.

**H-24 — Missing migrations and `ALTER TABLE` with swallowed errors.** `main.rs:3533-3541` runs `let _ = conn.execute("ALTER TABLE ...")` — a failure for a reason other than "column already exists" goes unnoticed. There is no `schema_version` table in either backend. `sql/postgres_schema.sql` and the inline DDLs are **two sources of truth** (the file is included with `include_str!` at `main.rs:3568`), and `docs/ops_backup_restore.md` says to apply it by hand even though the relay already does. **Fix:** version table + numbered migrations in a transaction, errors logged and fail-fast, a single source of truth.

**H-25 — The core schema has no version and no `busy_timeout`.** `social_db.rs:106-112` and `delivery_queue.rs:638` open the same `fedi3.db` with WAL but without `busy_timeout`; two concurrent writers (HTTP handler + queue worker) immediately produce `SQLITE_BUSY`. **Fix:** `busy_timeout=5000`, `synchronous=NORMAL`, single writer, and schema version.

**H-26 — Delivery workers die permanently on a transient DB error.** `delivery_queue.rs:164` `let jobs = self.fetch_due_jobs(40).await?;` inside the loop; on `Err` the task returns and logs only a `warn!` (`delivery_queue.rs:143`). `object_fetch.rs:77-81`/`129`/`151` use `.await??`. With H-25 a single `SQLITE_BUSY` is enough to stop **all** outbound federation until the app is restarted. **Fix:** handle the error inside the loop with backoff and continue; a supervisor that restarts the worker if it ends unexpectedly.

**H-27 — `admin_delete_user` leaves orphan blobs, notes and signals.** `main.rs:4552-4562` deletes rows from `inbox_spool`, `user_cache`, `user_collection_cache`, `media_items`, `peer_directory` but **not** the actual objects in S3/local/WebDAV (`MediaBackend` has no `delete`), nor `relay_notes`, `user_moves`, `relay_outbox_index`, `webrtc_signals`; and there is no transaction. **Effect:** storage paid forever and notes of the deleted user still in the public search index (GDPR/privacy problem). **Fix:** `delete` in the backend trait, blob deletion, transaction, and removal of the actor's notes from the index.

**H-28 — Media without quota, without DELETE, without rate limit, fully buffered.** `main.rs:2017-2059`: `media_get` has no rate limiter (unlike `media_upload`), does no conditional GET, loads the whole object into `Vec<u8>`; there is no `DELETE` route. An authenticated user can upload 64 MB forever and nobody ever deletes them. **Fix:** per-user quota (`SUM(size)`), DELETE route, separate rate limit per IP, streaming/presigned URL for S3.

**H-29 — The relay accepts in the tunnel usernames that all other handlers then reject.** `main.rs:1683-1714` `handle_tunnel` does not call `is_valid_username` (unlike `register`, `media_upload`, `forward_to_user`). With `allow_self_register=true`, `GET /tunnel/Alice?token=…` creates the user `Alice`; then `forward_to_user("Alice")` answers 400, `media_upload` 400, `register` rejects. The user exists (consumes spool, Meili documents, telemetry) but can never serve a request. **Fix:** validate the username before the upgrade.

**H-30 — Reconnection race: the old tunnel's disconnect removes the new one.** `main.rs:1722` inserts `TunnelHandle { tx }`; `main.rs:1818` does `remove(&user)` without a generation/connection id. If the user reconnects, the cleanup of the old connection deletes the new entry, which becomes invisible to `forward_to_user` → 503 → discarded activities (H-11). **Fix:** store a connection id and remove only if it matches (`Arc::ptr_eq`).

**H-31 — `inflight` entries are not removed on timeout.** `main.rs:1769-1782` inserts `id → oneshot::Sender`; `main.rs:2632-2634` on timeout returns without removing it. It is removed only when the client sends the corresponding response or closes. A client that stops responding to certain ids accumulates entries for the whole life of the tunnel. **Fix:** `Drop` guard that removes the id, or periodic pruning.

**H-32 — The spool has no byte cap, stores base64 in the DB and concurrent flushes can duplicate.** `main.rs:4611-4631` limits only the number of rows (`spool_max_rows_per_user`, default 5,000): 5,000 × 64 MB ≈ 320 GB per user, and `body_len` is recorded but never used. The body is base64 in a TEXT column (+33% and decode cost). `flush_spool_for_user` (`main.rs:3044-3113`) lists, delivers, then deletes: two concurrent flushes (one per reconnection) can deliver the same rows twice. **Fix:** byte cap (`SUM(body_len)`), BLOB/BYTEA, per-user mutex or `FOR UPDATE SKIP LOCKED`.

**H-33 — WebSocket not hardened.** `main.rs:1790-1806` reads only `Message::Text`; no ping/pong keepalive, no idle timeout, no explicit `max_message_size`. A half-open connection stays "alive" forever, forwards wait 15 s and then fail, the user is reported online while dead. Moreover the base64 body of a response can exceed axum's default inbound limit. **Fix:** keepalive + closure after N missing pongs, explicit limits consistent with `MAX_BODY_BYTES`, cap on `resp.body_b64`.

**H-34 — No application timeout, no concurrency limit, no CORS handling.** `main.rs:1150-1174` has only `DefaultBodyLimit`, `TraceLayer` and `enforce_ip_policy`. Handlers can wait 15 s (or indefinitely on `tx.send`, H-10) and sockets accumulate without limit; a browser client from a different origin cannot call the relay APIs (no `Access-Control-Allow-Origin`, `OPTIONS` → 405). **Fix:** `TimeoutLayer` (30 s), `ConcurrencyLimitLayer` sized on the DB pool, `CorsLayer` with allowlist.

**H-35 — The relay accepts arbitrary URLs from telemetry and from outbox `next` links.** `main.rs:2981-2983` and `:7299-7302` check only the `http://`/`https://` prefix; then `sync_relays_once` (`main.rs:8168-8171`) GETs `{relay_url}/_fedi3/relay/sync/notes`. Anyone can register via telemetry (self-signed signature) and announce `http://127.0.0.1:5432`: the relay will query it forever every 120 s with a 30 s timeout. **Fix:** the same private-IP guard as H-03, and a cap on the number of announced relays (H-36).

**H-36 — Telemetry accepts unlimited lists of relays/users/peers and writes them under the global lock.** `main.rs:7286-7339`: a signed POST can announce 100,000 relays (body limit 64 MB), each becoming a permanent row in `relay_registry` and a future HTTP target, all while holding the DB mutex. The periodic fan-out iterates up to 500 relays **sequentially** with a 30 s timeout: a dead relay blocks the cycle for 30-90 s, and 500 dead relays take it to hours (alphabetical starvation). **Fix:** per-payload cap (500/1000/1000), writes in a transaction, fan-out with `buffer_unordered(8)` and 5 s timeout.

### Core — reliability

**H-37 — The inbox marks the activity as seen before processing it: failures lose the activity forever.** `ap.rs:4859` `mark_inbox_seen(&dedup_id)` then `ap.rs:4871` `process_inbox_activity(...)`; if it fails it returns 502 but the dedup row remains, so the sender's retry receives `202 duplicate` (`ap.rs:4861-4864`) and the activity is never reprocessed. Same pattern in `p2p.rs:880-885` and `device_sync.rs:246-248`. **Fix:** transactional dedup and processing (insert "seen" only after success), or delete the row and answer 5xx so the sender retries.

**H-38 — Pagination that loses items with the same timestamp.** `social_db.rs:1978-1981`/`1999-2005` use `WHERE created_at_ms < ?1`; bulk inserts share the millisecond (`social_db.rs:630-648`). Page 2 permanently skips all rows left with the same ms: followers, outbox and feed silently lose items. **Fix:** composite cursor `(created_at_ms, id)`.

**H-39 — The public outbox also exposes DMs.** `ap.rs:4042` `list_outbox` and `social_db.rs:1974-2009` select **all** rows of `outbox_items` without a visibility filter; `outbox_get` (`ap.rs:4013-4070`) returns them verbatim. `/users/<u>/outbox` is public: anyone can page through the user's direct messages. **Fix:** filter by visibility (Public always; Followers only to a signed follower; direct omitted or served only via inbox).

**H-40 — Jobs awaiting a receipt never reach `max_attempts`.** `delivery_queue.rs:340`/`350`/`372` call `mark_awaiting_receipt(...)`; `delivery_queue.rs:270` selects `status IN (0, 3)`; only the HTTP error branch (`delivery_queue.rs:405-408`) applies `attempt_no >= max_attempts`. If a peer never sends the receipt, the job stays at status 3 and is resent forever (30 s → 6 h), growing the DB and generating duplicate traffic. Moreover there is no `DELETE FROM delivery_jobs` (verified): jobs are **never** cleaned up. **Fix:** apply `max_attempts` also to status 3, dead-letter, and `prune_delivery_jobs` in the GC.

**H-41 — An actor resolution failure does not reschedule the job: hot loop every 2 s.** `delivery_queue.rs:194-203` and `219-221` do `else { ... continue; }` without reschedule, while the tick is 2 s (`delivery_queue.rs:158`). An unreachable actor stays "due" forever and is re-resolved every 2 s, hammering the remote and starving the other jobs (40 per tick). **Fix:** reschedule with `next_backoff` and dead-letter after N attempts.

**H-42 — Incomplete shutdown.** `runtime.rs:809-822` waits only for `tunnel` and `server`; the delivery queue, object fetch, storage GC, relay sync, legacy sync, device sync, P2P sync and WebRTC workers are spawned and never joined. `stop()` (`runtime.rs:193-209`) removes the handle, sends the watch and joins on a detached thread, returning immediately. **Effect:** in-flight writes interrupted, DB not checkpointed at exit, and an immediate restart can race on the old listener (failed bind, silent because of P0-11). **Fix:** `JoinSet` with await and timeout, blocking `stop()` or with a completion signal, explicit DB checkpoint.

**H-43 — Username and domain are not validated; the username ends up in a path.** `runtime.rs:227` `default_data_dir()?.join("app").join(&cfg.username)`. `username = "../other"` or `"a/b"` escapes the data directory or makes every route 404; a space breaks the URLs. **Fix:** validate `^[A-Za-z0-9_]{1,64}$` and a hostname-shaped domain at startup.

**H-44 — `manually_approves_followers` is advertised but ignored.** `runtime.rs:377` passes it, `ap.rs:3810` publishes it in the actor document, but the Follow branch of `process_inbox_activity` (`ap.rs:5263-5280`) always does `add_follower` and sends `Accept`. A "locked" account accepts everyone. **Fix:** `pending` state with approval/rejection from the UI.

**H-45 — Accept can accept a follow from the wrong actor.** `ap.rs:5310-5313`: if the object is a string that does not resolve, **any** `Accept` from `accept_actor` marks that actor as accepted; the embedded branch (`ap.rs:5328-5331`) does `following(obj_object)` accepted without checking `accept_actor == obj_object`. **Fix:** require `accept_actor == follow.object` and that the embedded Follow has `actor` = us.

**H-46 — `KeyResolver`: unlimited cache, loose keyId binding, fetch amplification on GET.** `http_sig.rs:24` `HashMap` without cap or global eviction; entries expire only if the same actor is accessed again (`http_sig.rs:49-58`, `104-127`). The returned `publicKey.id` does not have to match the requested `keyId`, and `actor.id` is not verified against the fetched URL. An attacker can use always-new keyIds (their server can generate infinite paths): every request creates a permanent entry and a fetch. **Fix:** bounded LRU (10k), validate `pk.id == key_id` and `actor.id == actor_url` (or same origin), rate limit on GETs.

**H-47 — Inbox limiter that can exceed its own 50k limit.** `ap.rs:5082`/`5098-5100`: `if guard.len() > 50_000 { retain(|_, v| now - v.window_start_ms < 10*60_000) }`. Under a flood of unique keys within the 10-minute window the `retain` removes nothing and the map keeps growing. **Fix:** hard cap with LRU eviction.

**H-48 — Network metrics with unlimited maps and pruning only when the UI queries.** `net_metrics.rs:98-102` (with `.lock().unwrap()`); `prune_seen` (`net_metrics.rs:178-199`) is called only by `snapshot_json`. A peer flood grows three HashMaps without limit when the UI is closed. **Fix:** prune on a timer, cap/LRU, poisoning handling.

**H-49 — `relay_bridge` panics on a malformed path.** `relay_bridge.rs:18` builds `http://localhost{path}{query}` and `:31` does `request.body(...).unwrap()`. A path without a leading `/` or with spaces makes the URI invalid and the `.unwrap()` panics. In P2P this happens in the swarm loop (`p2p.rs:819`), so the panic takes down the whole P2P task; in the tunnel it takes down the tunnel (which does not restart, P0-09). A malicious peer can kill P2P with a single request. **Fix:** normalize the path, build the URI with `?` and answer 400.

### Flutter app

**H-50 — No HTTP timeout anywhere in the Dart network layer.** `app/lib/core/core_api.dart:88`, `:123`, `:135`, `:292`; `actor_repository.dart:225`; `relay_admin_api.dart:37,51,61`; `core_event_stream.dart:61`. `package:http` has no default timeout. If the core stalls or a remote host does not respond, the Future never completes: infinite spinners, disabled buttons, no cancellation. Every screen reimplements retry with string comparisons (`timelines_screen.dart:563`, `notifications_screen.dart:175`, `chat_screen.dart:170`). **Fix:** an injected `http.Client` with `connectionTimeout` + `.timeout(15s)` on every request, a typed `CoreApiException` with `statusCode`/`retryable`, cancellation tokens.

**H-51 — The core is considered "active" before the bind and `markCoreDead` is never called.** `app/lib/state/app_state.dart:38-47` defines `markCoreDead` but `grep` finds no other uses; `app_state.dart:236` sets the handle immediately. In Rust `run_core` only logs the error (`runtime.rs:177`). **Effect:** if the core dies, `isRunning` stays `true` forever, the UI keeps polling a dead port, no "core stopped" banner, no restart. **Fix:** a watchdog that queries `/_fedi3/health` every 5 s, calls `markCoreDead` and attempts a restart with backoff; in Rust, status callbacks on the thread.

**H-52 — Polling storm without lifecycle awareness.** `nerd_status_bar.dart:51` 2 s; `timelines_screen.dart:410` 8 s × 4 timelines; `right_sidebar.dart:47` 10 s; `relays_screen.dart:42` 10 s; `peer_presence_store.dart:29` 10 s; `shell.dart:47-54` builds all six screens in an `IndexedStack` at startup; 7 `CoreEventStream(` sites open SSE; `grep "WidgetsBindingObserver|didChangeAppLifecycleState"` → **no results**. **Effect:** ~30-60 requests/minute and 6 SSE sockets even in background on mobile: battery, radio, and constant work on the core. **Fix:** a `WidgetsBindingObserver` in `AppRoot` that suspends everything that is not `resumed`, a single shared SSE stream (broadcast), polling 5-10 s only when the widget is visible, lazy tabs.

**H-53 — Notification refresh clears the list and the cursor on every event.** `notifications_screen.dart:116-124`: `setState(() { _items.clear(); _cursor = null; ... }); await _loadMore();` called by the SSE debounce (`:92-95`) and by the app state listener (`:67-71`). If a notification arrives while the user is reading page 3, the list goes back to page 1 and the scroll jumps; if a load is in flight, `_loadMore` exits immediately (`if (_loading) return;`, `:139-141`) and the refresh becomes a no-op leaving stale data. **Fix:** prepend new items without clearing the cursor; a generation counter to discard old responses.

**H-54 — Race in timeline refresh/load and `setState` after dispose.** `timelines_screen.dart:466-472` clears `_items`/`_cursor` and calls `_loadMore`, which starts with `if (_loading) return;` (`:474`) and ends in `finally { setState(...) }` (`:512-514`) without a `mounted` guard. A pull-to-refresh during a load empties the list and returns; if the Shell is unmounted (logout/config change) the `finally` calls `setState` on a disposed State → exceptions and log spam. **Fix:** generation counter, `if (!mounted) return;` before every `setState`, refresh disabled during the load.

**H-55 — `media_kit` is never initialized on Linux: videos are broken in the AppImage.** `app/lib/main.dart:22` `if (!Platform.isLinux || Platform.environment['FEDI3_ENABLE_MEDIA_KIT_LINUX'] == '1') { MediaKit.ensureInitialized(); }`; the AppRun generated by `scripts/build_appimage.sh` and `AppDir/AppRun` export `LD_LIBRARY_PATH`, `GDK_BACKEND`, `LIBGL_*`… but **not** `FEDI3_ENABLE_MEDIA_KIT_LINUX` (verified with grep). `fedi_media_player.dart:37` creates `Player()` in `initState`. **Effect:** touching a video/audio throws an exception in the widget, even though `libmpv` is correctly bundled by the script. Moreover `--enable-software-rendering` is active by default (`FEDI3_SOFTWARE_RENDERING:-1`) and degrades playback on machines with a GPU. **Fix:** remove the gate or set the variable in AppRun; make software rendering opt-in.

**H-56 — The "backup" contains neither the private key nor the data, and is in plaintext.** `app/lib/services/backup_codec.dart:21-28` exports only `coreConfig` + `uiPrefs`. Missing: the RSA key (in the core data dir, `keys.rs`), `fedi3.db`, media and chat history. The file is plaintext JSON and contains `relayToken`/`internalToken`; `decode` (`:30-44`) ignores the `v` field. Sharing the backup = sharing the secrets, and restoring it on another device does not restore the identity. **Fix:** include key and DB (or a logical export), encrypt with a password (Argon2 + AES-GCM), validate `v`.

**H-57 — Backup import without rollback.** `app/lib/ui/screens/backup_screen.dart:169-175`: `stopCore(); saveConfig(bundle.config); savePrefs(bundle.prefs); startCore();` in a single `try`; the `catch` only sets `_status`. If `savePrefs` fails (disk full, path_provider) the config is already overwritten and the core is stopped: the user has another account and a dead app, with an "import failed" message. **Fix:** snapshot of config/prefs, validation of `v`, application with restore of the snapshot and restart on error.

**H-58 — No core shutdown when the app closes.** `main.dart`/`app_root.dart` have no lifecycle hook; closing the window terminates the process with SQLite and the delivery queue writing, without checkpoint. **Fix:** `AppLifecycleListener`/`onWindowClose` that awaits `stopCore()` with a timeout.

**H-59 — The existing widget test asserts a text the app never shows, and there is no CI.** `app/test/widget_test.dart:23-24` `expect(find.text('Fedi3 setup'), findsOneWidget)`; `'Fedi3 setup'` is `onboardingTitle` (`app_en.arb:573`), while `FirstRunScreen` renders `firstRunTitle` = "Welcome" (`app_en.arb:203`, used in `first_run_screen.dart:115`). **The only test in the project is red or never runs** (there is no CI: `.github/` contains only `copilot-instructions.md`). Moreover the test pumps a frame while `FirstRunScreen.initState` makes real HTTP calls (`first_run_screen.dart:47-75`), so it is flaky. **Fix:** assert "Welcome", inject a fake `http.Client`, `pumpAndSettle`, and add the pipeline.

**H-60 — Zero Rust tests.** `grep -rn "#\[test\]\|#\[tokio::test\]" crates/` → **0** over 28,358 lines. For a project with HTTP signatures, cryptography, asynchronous delivery and DB schema, it is risk number one: no refactor is verifiable. **Minimum fix:** tests on `parse_signature_header`/`build_signing_string` (round-trip with a real signature), `verify_digest_if_present` (missing/present/wrong), `verify_date`, `activity_dedup_id`, `sanitize_key`, `parse_ip_rule`, `user_from_host`, `CoreConfig`↔JSON round-trip, `BackupCodec`.

**H-61 — Federated content that generates direct requests to arbitrary hosts.** `actor_repository.dart:222-229`, `object_repository.dart:21-31`, `link_preview_repository.dart:68-93` (256 KB cap but no timeout), `status_avatar.dart:58` and `inline_media_tile.dart:77` (`Image.network`). Every rendered post can make the client connect to arbitrary third-party hosts (including `http://192.168.1.1/...`), revealing IP/User-Agent and allowing tracking pixels via unique URLs. A host that does not respond appends the Future in `_inflight` forever (`actor_repository.dart:198-209`), and every subsequent hover waits on the same never-completed Future. **Fix:** proxy remote media through the core or at least require https, timeout and cap; respect `blockedDomains`; "load remote images" toggle.

**H-62 — Post text is sent as raw HTML when there are no MFM markers.** `compose_screen.dart:384` and `inline_composer.dart:399`: `final content = MfmCodec.hasMarkers(text) ? MfmCodec.toHtml(text) : text;`. `MfmCodec.toHtml` correctly performs escaping (`mfm_codec.dart:20-27`), but the branch without markers bypasses it. **Effect:** writing `1 < 2 & 3` publishes `content: "1 < 2 & 3"` as `text/html`; Mastodon/Misskey interpret it wrongly or drop it, and a literal `<b>` becomes bold. It is a correctness bug visible to other servers. **Fix:** always produce HTML (`plainToHtml` with escaping only), with tests on `<`, `&`, quotes and newlines.

**H-63 — N+1 for every note in the timeline.** `note_card.dart:139-155`: for every card a post-frame callback calls `_loadFlags(note.id)`, `_loadMyReactions(api, note.id)` and `EmojiStore.addAll(note.emojis)`; `_loadActors()` (`:120-122`) fetches author and booster. `emoji_store.dart:35-55` `addAll` does a `read()` (full JSON parse) + `writeAsString` of the entire map for every new emoji. A timeline of 50 items generates ~50 `/_fedi3/reactions/me` requests, 50+ actor fetches and up to 50 file read/writes. **Fix:** batch endpoint for reactions, `EmojiStore` in memory with serialized and debounced flush, profile cache.

**H-64 — Settings refresh does not rebuild the core and the screens save stale snapshots.** `security_settings_screen.dart:31`, `privacy_settings_screen.dart:30`, `moderation_settings_screen.dart:30` capture `_cfg = widget.appState.config!` in `initState`, then rebuild a `CoreConfig(...)` with 25 fields copied by hand. If the config changes elsewhere (backup import, another screen) while the screen is open, saving overwrites it with the old snapshot; adding a field requires touching 5 constructors and a forgotten field is silently reset. **Fix:** `CoreConfig.copyWith` and read `widget.appState.config` at save time.

**H-65 — The full composer has no file picker, no limits and drafts lose attachments.** `compose_screen.dart:33` `_filePath` is a `TextEditingController`, `:217-220` a `TextField` "Media file path" that requires an absolute path; `:341-352` `_addMediaFromPath` reads `File(path)`. `ComposeDraft` (`draft_store.dart:11-53`) has no media field. The screen is reachable only from `right_sidebar.dart:333` (desktop). **Effect:** on desktop you have to type a path; on mobile the full composer does not exist; no client-side size cap; attachments are lost on tab change. **Fix:** `openFile`/`desktop_drop`, limits consistent with the inline composer (`inline_composer.dart:327-331`), media in the draft.

**H-66 — Headers and paths interpolated without encoding.** `core_api.dart:657-661` sets `'X-Filename': filename` with the raw name from `XFile.name`; `:264` and `:248` interpolate `threadId` without `Uri.encodeComponent`. `dart:io` rejects headers with non-ASCII bytes (`HttpException: Invalid header field value`): uploading `città.jpg` fails with an opaque error. **Fix:** percent-encoding or filename in the JSON body, ASCII fallback sanitization, `Uri.encodeComponent` for the ids.

---

## 4. MEDIUM issues

**Relay**
- **M-01** `LIKE` on SQLite is broken: `escape_like` (`main.rs:8243-8245`) inserts backslashes but no query adds `ESCAPE '\'`; searching `100%` or `foo_bar` finds nothing. Moreover it also searches `content_html` (`main.rs:5152`), so `p` or `div` match almost everything, and `LIKE '%…%'` is a full table scan under the global mutex. **Fix:** `ESCAPE`, a plain-text column, FTS5.
- **M-02** Canonical redirect in an infinite loop with an explicit port: `canonical_origin` (`main.rs:8442-8448`) includes the port, `host_only` (`main.rs:8390-8398`) removes it → `FEDI3_RELAY_PUBLIC_URL=http://localhost:8787` generates a 308 to itself. Actor URLs lose the port. **Fix:** comparison with a normalized authority.
- **M-03** In host-routing the actor URLs derive from the client's `Host` header (`main.rs:8431-8439`): with `Host: evil.example` the webfinger returns `acct:alice@evil.example` and an `href` on that domain. **Fix:** validate `Host` against `*.{base_domain}` and derive the origin from `public_url`.
- **M-04** Android 13+ never asks for the notification permission: `POST_NOTIFICATIONS` is missing from the manifest and `notification_service.dart:14-27` does not call `requestNotificationsPermission()`; notifications exist only with the app in foreground (no foreground service/push). **Fix:** permission + explicit request; document the background limitation.
- **M-05** Tables `admin_audit`, `relay_notes`, `peer_registry`, `relay_registry`, `relay_user_directory` are never cleaned (`main.rs:1045-1067` cleans only spool, move notices, webrtc signals, peer directory). `relay_notes` keeps the full `note_json` forever: the search index is a monotonically growing copy of every note ever seen. **Fix:** configurable retention in the cleanup loop.
- **M-06** `FEDI3_RELAY_HSTS_MAX_AGE_SECS` defaults to 0 (`main.rs:1272-1275`) and is not in `.env.example`; a deployment without compose serves plain HTTP without enforcement. **Fix:** default 31,536,000 and documentation.
- **M-07** Config: parsing errors silently replaced with defaults (`main.rs:1268-1271` and ~40 occurrences). `FEDI3_RELAY_DB_DRIVER=pg16` becomes SQLite, `SEARCH_BACKEND=meilisearch` falls back to DB (`main.rs:920`), `MAX_BODY_BYTES=64M` becomes 64 MiB. **Fix:** a parser that fails with a message on security-relevant variables.
- **M-08** iOS: `NSCameraUsageDescription`/`NSPhotoLibraryUsageDescription` are missing and no URL scheme/deep link is registered (complete `Info.plist`, 44 lines). **Fix:** usage descriptions + `CFBundleURLTypes`.
- **M-09** `.env.example` does not document `TRUST_PROXY_HEADERS`, `HSTS_MAX_AGE_SECS`, `CSP`, `BASE_DOMAIN`, `IP_ALLOWLIST/DENYLIST`, `TUNNEL_TIMEOUT_SECS`, `SPOOL_*`, `MAX_INBOX_FANOUT`, `MAX_INFLIGHT_PER_USER`, `MEDIA_DIR`, `MEDIA_WEBDAV_*`, `TURN_*`, `P2P_UID/GID`, `P2P_*`. Whoever copies the file runs with unsafe defaults (`trust_proxy_headers=false` breaks the client IP behind a proxy; `hsts=0`). In the local `.env` the **same token** is used for admin and Meilisearch.
- **M-10** Coturn: `--lt-cred-mech` with static `--user=${TURN_USER}:${TURN_PASS}` shared by all clients, `TURN_PASS:-CHANGE_ME` as default, no `--external-ip` (behind NAT it announces private IPs), no TLS, `latest` image. **Fix:** temporary credentials (TURN REST), `external-ip`, TLS 5349, pinned image.
- **M-11** Compose without resource limits, without log rotation, without `HEALTHCHECK` for the relay, `caddy` depending on `relay` without a condition; `POSTGRES_PASSWORD:-fedi3` and `MEILI_MASTER_KEY:-` (empty = Meilisearch without auth on the compose network). **Fix:** `mem_limit`, `logging.options.max-size`, images pinned by digest, mandatory values.
- **M-12** `Db::init` for Postgres does not retry if the schema fails: `main.rs:3565-3570` `conn.batch_execute(include_str!(...))?` returns immediately and `main` panics with `.expect("db init")` (`main.rs:968`). **Fix:** treat the schema like the connection (retry with backoff).
- **M-13** Thin observability: 5 gauges without HELP or histograms, gated by the admin token, which recompute the full telemetry on every scrape (8+ DB queries + Ed25519 signature); `TraceLayer` logs at DEBUG while the default filter is `info`, so **no line per request** and no logging of 5xx. **Fix:** real metrics (counter/histogram per route/status, tunnel latency, spool depth), `on_response` log at INFO, separate metrics endpoint.
- **M-14** Media upload not atomic and orphaned: `std::fs::write` truncates and writes without fsync/rename (`media_store.rs:63`); if the DB upsert fails (`main.rs:2001-2005`) the file remains forever without a reference. **Fix:** temp + fsync + rename, best-effort delete on DB failure.
- **M-15** The extension from `X-Filename` is neither validated nor limited (`main.rs:1972-1980`): `#`, `?`, spaces break the WebDAV keys, which are not percent-encoded (`media_store.rs:112`). **Fix:** allowlist `^[a-z0-9]{1,8}$` and `urlencoding` in the WebDAV URLs.
- **M-16** The media MIME type is trusted from the client (`main.rs:1979-1988`, returned verbatim at `:2043`) and there is no per-user quota. A user can upload `text/html` and serve it from the relay domain. **Fix:** allowlist + magic bytes sniffing + quota.
- **M-17** The relay Dockerfile is not reproducible: `FROM rust:1-bookworm` (floating tag), `Cargo.lock` not copied, build without `--locked`, no `strip`, no `HEALTHCHECK` (`deploy/relay/Dockerfile:3-22`). **Fix:** `rust:1.86-bookworm`, `COPY Cargo.lock`, `cargo build --locked --release`, `[profile.release] strip`, healthcheck.

**Core**
- **M-18** The relay-controlled `id` of media can escape the directory: `ap.rs:166-168` `dir.join(&response.id)` and `media_backend.rs:83` `media_dir(data_dir).join(stored_name)` without validation. A relay returning `id: "../../etc/passwd"` writes/reads outside. **Fix:** allowlist `[A-Za-z0-9._-]`, reject `..`/`/`/`\`.
- **M-19** Media pruning for non-followed actors is accidentally nested inside `if cfg.cache_only_followed_fedi3` (`storage_gc.rs:367-401`), so setting that flag to `false` — a valid configuration — disables the `other_max_media_items_per_actor`/`other_max_media_bytes_per_actor` quota and leaves only the global directory cap (`storage_gc.rs:404-405`): a single non-followed actor can fill the disk. **Fix:** detach media pruning from that flag or use a dedicated flag.
- **M-20** E2E chat without forward secrecy: `chat.rs:451-456` derives the AES key from a Kyber secret with HKDF, but every message encapsulates to a static key or a prekey from the bundle; no ratchet, and the secrets of the used prekeys remain in the DB (`social_db.rs:2919-2927` sets only `used_at_ms`). If `identity_private_key.pem` or the DB leaks, **all** past messages are decrypted. `crypto_envelope.rs:38-45` also uses a static RSA key. **Fix:** document the exact guarantee (no FS), add a ratchet (or hybrid X25519+ML-KEM with ephemeral keys) and delete used secrets; consider SQLCipher.
- **M-21** `pqcrypto-kyber 0.7` is archived and Kyber has been replaced by ML-KEM (FIPS 203); no hybrid with X25519. **Fix:** migrate to `ml-kem` with hybrid.
- **M-22** `net_metrics`/`inbox_limiter` (H-47/H-48) and the KeyResolver cache (H-46): three unbounded maps with opportunistic pruning.
- **M-23** No actor key rotation: `ap.rs:3766` uses a single `#main-key`; if `identity_private_key.pem` disappears, `keys.rs:41-48` generates a new one and **changes the DID**, without publishing the previous key. **Fix:** list of keys (current + previous) in the actor document and key history.
- **M-24** `tokio` declared without `sync`/`time`/`net` (`Cargo.toml:28`): relies on transitive feature unification, fragile. No `rust-version`, no `zeroize` on key material.

**App**
- **M-25** Config/prefs written non-atomically and parse errors silently `null` (`config_store.dart:32-35`, `:20-29`; `prefs_store.dart:19-29`). A crash halfway through the write makes the app believe there is no configuration: the user goes back to first run, with the identity still in the core data dir. No `chmod 600`: `relayToken`, `internalToken`, `translationAuthKey` (DeepL) and `relayAdminToken` are in plaintext and readable by other local users. **Fix:** temp+rename, `.bak`, distinguish "missing" from "corrupted", 600 permissions, `flutter_secure_storage` for secrets.
- **M-26** The relay token can travel over `ws://`: `app_state.dart:116-127` `normalizeRelayWs` upgrades to `wss://` only if `publicBaseUrl` is https; with `ws://` the token (and the admin bearer in `relay_admin_api.dart:16-25`) is readable on the wire. `bind=0.0.0.0` exposes the `/_fedi3/*` endpoints on the LAN in cleartext, and `http://0.0.0.0` as a client target is not portable. **Fix:** warn/reject `ws://`, default `127.0.0.1`, connect to `127.0.0.1` when the bind is `0.0.0.0`.
- **M-27** Fragile discoverability of the FFI library: `fedi3_core.dart:64-85` adds the bare name (`DynamicLibrary.open('fedi3_core.dll')` uses the OS search path) and walks up 6-8 parent directories; no version handshake with `fedi3_core_version()`. A stale DLL from another installation can be loaded silently. **Fix:** only documented locations, version comparison, removal of the 28 MB DLL from the repo.
- **M-28** The composer/UI shows about 45 hardcoded strings, many only in Italian in an English-default app: `onboarding_screen.dart:148,152,162,163,172` (`'P2P + Relay (fallback dopo 5s)'`, `'Solo P2P (mai relay)'`, …), `edit_config_screen.dart:140,144,154,155,164`, `notifications_screen.dart:279-304`, `timelines_screen.dart:882-906`, `media_viewer_screen.dart:190,193`, `note_card.dart:499,502,645,1256`. The ARB files are complete and in parity (489 keys for en and it). **Fix:** move everything into the ARBs and add a CI gate.
- **M-29** Preference stores with uncoordinated read-modify-write: `note_flags_store.dart:55-67`, `reaction_store.dart:32-44`, `direct_recipient_store.dart:32-44`, `emoji_recent_store.dart:32-44` read, mutate and rewrite the whole JSON without serialization. Two quick actions overwrite each other and a crash leaves invalid JSON that is silently read as `{}` (bookmarks/mutes/pins disappear). **Fix:** in-memory stores with serialized and atomic writes.
- **M-30** Relay screen: `relays_screen.dart:141` `Text(_stats.toString())` prints the raw Dart map; `:168-176` searches on every keystroke without debounce (and the `_loadingPeers` guard discards keystrokes, so results can be from a previous prefix); `:300-310` pings every relay in parallel on every refresh. **Fix:** typed fields with l10n, 300 ms debounce with generation token, latency cache.
- **M-31** Media download broken on mobile: `media_viewer_screen.dart:176-183` calls `getSaveLocation` **outside** the `try`; on Android/iOS it throws `UnimplementedError` → unhandled exception, the button seems to do nothing. The suggested name is `parts.last`, so `.../image.jpg?x=1` produces `image.jpg?x=1`. **Fix:** per-platform branch (`Share`/`XFile.saveTo` on mobile), name from decoded `Uri.pathSegments.last`.
- **M-32** No accessibility: `grep -rn "Semantics(" lib` → 0; `Image.network` in `status_avatar.dart:58`, `inline_media_tile.dart:77`, `note_card.dart:1664`, `media_viewer_screen.dart:143` without `semanticLabel`; online status indicated only by colour (`status_avatar.dart:88-99`); the only shortcut is Ctrl+Enter in the composer. **Fix:** `Semantics`/`semanticLabel`, shortcuts for tabs/search/refresh, tests with `SemanticsTester`, check at 1.5× text scale.
- **M-33** Config not validated and relay calls unauthenticated against `publicBaseUrl`: `core_api.dart:923-941` `Uri.parse(config.publicBaseUrl.trim())` throws `FormatException` on an empty value or one without a scheme; `fetchRelayList`/`fetchRelayStats`/`fetchRelayPeers` send no token while `fetchRelayCoverage`/`fetchRelayMe` do. **Fix:** validate URL and bind in onboarding/edit_config with inline errors, normalize the scheme, map 401/403 to clear messages.
- **M-34** Federated links inert or unvalidated: `note_card.dart:1851-1859` passes `onTapUrl` that returns `false` for every non-actor URL (`:353-372`), and `_QuotedPreview` (`:1826`) does not pass `onTapUrl` at all; `open_url.dart:10-17` does not filter schemes (`javascript:`, `file:`). **Fix:** a single handler that allowlists `http/https/mailto` and calls `openUrlExternal`, applied to quoted previews too.
- **M-35** Local telemetry without rotation and without remote crash reporting: `telemetry_service.dart:60-88` appends a JSON line per event with stack trace, without a size limit; `loadRecent` (`:97-108`) parses the whole file and can throw on a corrupted line. **Fix:** 2 MB × 3 rotation, per-line parsing in try/catch, clearly visible manual export, opt-in Sentry/GlitchTip.
- **M-36** Onboarding with placeholder defaults (`onboarding_screen.dart:56-64`: `alice@example.invalid`, `ws://127.0.0.1:8787`) and tokens in cleartext in the fields (`:131-136`); `first_run_screen.dart:46-48` has a single hardcoded relay bootstrap and calls `http.get` without timeout in `initState` (no offline path). **Fix:** a real relay by default or explicit "local-only", `obscureText` on tokens, 3-5 s timeout, offline path.
- **M-37** `pubspec.lock` requires `dart >=3.10.0` and `flutter >=3.38.0` while `pubspec.yaml:7` declares `sdk: ">=3.3.0 <4.0.0"`: the committed lock is unusable with older SDKs (`pub get --enforce-lockfile` fails). `.dart_tool/package_config.json` contains Windows paths (`file:///C:/Users/m0rph/...`) in a Linux checkout. **Fix:** align the constraints, regenerate `.dart_tool`, decide whether to commit the generated l10n.

---

## 5. LOW / hygiene issues

- **L-01 Repo: 200 MB of versioned binaries.** `git ls-files` includes `Fedi3-release-x86_64.AppImage` (183 MB), `app/fedi3_core.dll` (28 MB, and it shows as modified), `docs/.linux error.kate-swp` (a Kate swap file) and `docs/linux error` (with a random token `7t2qKx8mE5nQp3LrV9xwAh1B` at the bottom). `.gitignore` does not exclude `dist/`, `*.AppImage`, `*.dll`, `*.so`, `**/*.swp`. The repository has **a single commit** and `.git` weighs 202 MB: no history, no bisect, no provenance. **Fix:** ignore the artifacts, `git rm --cached`, publish the binaries as release assets.
- **L-02 `LICENSE` does not contain the AGPL text.** It is a 20-line notice: the AGPL requires that a copy of the license accompany the program, and GitHub/compliance tools will not recognize it. **Fix:** official AGPL-3.0 text + `NOTICE`.
- **L-03 No `SECURITY.md`, `CHANGELOG`, `CODE_OF_CONDUCT`, `CODEOWNERS`, issue/PR templates.** `GOVERNANCE.md` promises public advisories but there is no disclosure channel, for a project that manages keys.
- **L-04 `docs/app_guide.md` is an accumulation.** It contains three pasted documents: the real guide, a "linux deploy app" block with **contradictory** instructions (install Flutter with `pacman` and then clone Flutter again from GitHub) and a chat transcript ending with "If you want, paste the output of: … and I'll tell you the exact missing point". `docs/faq.md` says "Can I use only P2P without a relay? It is possible" but H-09 contradicts the promise. **Fix:** rewrite `app_guide.md`, move the Linux deploy into `docs/build_linux.md`.
- **L-05 `analysis_options.yaml` is 45 bytes:** only `include: package:flutter_lints/flutter.yaml`, no extra rules, no `errors:` promoting warnings, no `dart format` in CI. **Fix:** `strict-casts`, `strict-inference`, `strict-raw-types`, warnings promoted to errors.
- **L-06 Two free functions for the same allocation type:** `fedi3_core_string_free` (`lib.rs:41`) and `fedi3_core_free_cstring` (`ffi.rs:57`). `set_err` (`ffi.rs:9-17`) writes to `out_err` only on error: a caller that reuses the buffer without initializing it to `null` reads a stale pointer. **Fix:** unify, document ownership in a C header, always initialize.
- **L-07 `openUrlExternal` accepts any scheme** (`open_url.dart:10-17`); `link_preview_repository.dart:39` creates an `http.Client` never closed and without timeout; `resolveLocalMediaUrl` (`media_url.dart`) rewrites the URL to the local core without fallback if the core is stopped; `notification_service.dart:33` uses a fixed id `1001`, so a chat notification overwrites the previous one.
- **L-08 `DevCoreScreen` shows the core start JSON, tokens included** (`dev_core_screen.dart:57`), and leaks its `HttpClient` on the error path (`:108-121`, `close()` only on success). **Fix:** redact the tokens, `try/finally`, reuse `CoreApi`.
- **L-09 Product version and metadata:** `version: 0.1.0+1`, no `assets:` section, default Flutter icon, `web/index.html` with "A new Flutter project.", no deep link, no update mechanism, `DevCoreScreen` visible to normal users.
- **L-10 Ephemeral unread counters:** `app_state.dart:27-28` are in memory only and reset in `markCoreDead`; `notifications_screen.dart:45` calls `clearUnreadNotifications()` in `initState`, before the list is loaded. **Fix:** derive the counters from the API and persist the timestamps.
- **L-11 Optimistic follow never reverted and polling every 6 s:** `profile_screen.dart:151-163` does not restore the state in `catch`; `:186-188` polls every 6 s indefinitely if the follow stays pending. **Fix:** revert in `catch`, cap the polling.
- **L-12 Thread view with sequential requests (up to 200 nodes) before the first paint** (`note_detail_screen.dart:55-105`), without cancellation. **Fix:** render the focal note immediately and stream parents/replies.
- **L-13 `escape_like` without `ESCAPE`** (`social_db.rs:3569-3571` vs queries at `:1325/1463/1515`): searching `100%` or `foo_bar` finds nothing. **Fix:** `ESCAPE '\'` or remove the escaping.
- **L-14 Incomplete handling of `algorithm` in signatures** (`http_sig.rs:145-180` ignores it; `:252-262` always verifies RSA PKCS#1 v1.5; `:219-230` `split_once('=')` rejects multiple digests). Peers with `hs2019`/PSS or Ed25519 receive 401. **Fix:** support rsa-sha256, hs2019 and ed25519, digest as a list.
- **L-15 `sanitize_key` does not remove `..`** (`media_store.rs:268-271`): today it is safe because keys are generated by the server, but it is an implicit dependency. **Fix:** explicitly reject `..`.
- **L-16 In the compose the `relay` service publishes `8787` on the host** besides Caddy (two ingress paths, one without TLS); `postgres`/`meilisearch` are reachable from every container on the compose network with default passwords.
- **L-17 The build scripts do not use `--locked`** (`scripts/build_core.sh`, `deploy/*/Dockerfile`) and there is no macOS equivalent of `build_core.ps1`.
- **L-18 `scripts/build_appimage.sh:180-186`** downloads `appimagetool` from the `continuous` tag (mutable) without checksum verification; the `libmpv` bundling copies the host version (mismatch risk).
- **L-19 `docs/ops_backup_restore.md`** says to apply `postgres_schema.sql` by hand, but the relay already does it with `include_str!` (`main.rs:3568`): misleading documentation and risk of drift between the two sources of truth.
- **L-20 No `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`:** non-deterministic formatting and lint, no `cargo audit`/`deny` for CVEs.

---

## 6. Broken scripts and operations

- **`scripts/relay_diagnose.sh:50-52`** runs `docker compose exec -T relay sh -lc 'wget -qO- ...'`, but the image (`deploy/relay/Dockerfile:16-19`) installs only `ca-certificates` and `gosu`: **`wget` does not exist**. The command prints `wget: not found` and the error is swallowed by `|| true`: the diagnostics pretend to have checked health.
- **`scripts/backup_relay_pg.sh:13`** and **`restore_relay_pg.sh:17`** hardcode `-U fedi3 -d fedi3_relay`, ignoring `FEDI3_RELAY_PG_USER`/`FEDI3_RELAY_PG_DB`.
- **`scripts/backup_relay_media.sh:12`** archives with an absolute path (`tar -czf - /data/media`); **`restore_relay_media.sh:17`** extracts with `-C /data/media` → you get `/data/media/data/media/...` (wrong nesting). **Fix:** `tar -czf - -C /data media` and `tar -xzf - -C /data`.
- **No retention, verification or scheduling of backups:** only manual scripts, no cron/systemd timer, no restore test.
- **`relay_smoke_test.sh`** is correct but is never run automatically.

---

## 7. Recommended roadmap

### Week 1 — unblock the release and close the authentication gaps
1. Android manifest (`INTERNET`, `POST_NOTIFICATIONS`, network security config), macOS entitlements, iOS ATS (P0-01…04).
2. Final app id + release keystore (P0-05).
3. Mandatory signature on the relay `/inbox` (P0-06) and `actor`↔signature binding in the core (P0-07), with dedicated tests.
4. Authentication on the `outbox` (P0-08).
5. Tunnel reconnection loop with backoff and watchdog (P0-09).
6. `catch_unwind` on all FFI entry points (P0-10) and readiness handshake in `start()` (P0-11).
7. `replaceConfig` that always restarts the core (P0-12).
8. Minimum CI: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo build --locked`, `cargo test --locked`, `flutter analyze`, `flutter test`, AppImage build.

### Weeks 2-3 — sustain the load and do not lose data
9. Relay: `spawn_blocking`/pool for the DB, removal of the global lock, unauthenticated healthcheck without side effects (H-21, H-23).
10. `forward_to_user`: release the lock before the await, timeout on the send, spool on every failure (H-10, H-11).
11. Correct client IP in all handlers + Caddy `trusted_proxies` + do not publish 8787 (H-12, H-13).
12. Constant-time token, startup validation, mandatory `Digest`, Ed25519 in signatures (H-14, H-15, H-01).
13. SQLite: `busy_timeout`, versioned schema, GC of `delivery_jobs`, inbox dedup (H-25, H-24, H-40, H-37).
14. `CoreApi` with timeout, typed errors and injectable repository; first real widget tests (H-50, H-59).
15. Atomic config and 600 permissions; core watchdog with restart; complete backup (M-25, H-51, H-56).

### Weeks 4-6 — observability, hardening, maintenance
16. JSON logs, real metrics, separate metrics endpoint, Prometheus/Grafana stack, alerts (M-13).
17. Pinned images and `--locked` builds, coturn with temporary credentials, resource limits and log rotation (M-17, M-10, M-11).
18. Repo cleanup, `SECURITY.md`, `CHANGELOG`, full AGPL license, rewritten docs (L-01…L-05).
19. Dependency updates (ML-KEM, libp2p, webrtc, rusqlite) with `cargo audit`/`deny` in CI (M-21, M-24, L-20).
20. Media quotas, MIME allowlist, streaming, accessibility, complete l10n (H-28, M-16, H-22, M-32, M-28).

---

## 8. What is already done well

The picture is not negative, and it must be stated precisely:

- **No SQL injection.** Every value goes through `params!`/`$n`; `format!` is used only for `IN (?,?,…)` placeholder lists and Meili filters, with correct escaping of `\` and `"`.
- **Solid tokens where they exist:** 24 bytes from `OsRng`, stored as SHA-256, never logged; usernames with a strict allowlist (`^[a-z0-9_-]{1,64}$`) that blocks injection in paths and URLs.
- **Real HTTP Signatures, not decorative:** RSA-SHA256, Digest, time skew, explicit signed headers, Ed25519 with TOFU pinning for telemetry.
- **Layered anti-abuse defences in the core:** rate limits per IP/keyId/actor, daily quotas per keyId/actor/host, strikes with temporary blocks, audit events. Much more than many AP instances.
- **Storage GC** with TTL and quotas per feed, objects and media, differentiated for followed Fedi3 actors, with batching.
- **Persistent delivery queue** with exponential backoff and jitter, `max_attempts`, dead-letter for object fetch.
- **E2E encryption** with AES-256-GCM, random nonce per message, HKDF-derived keys, signed bundles; correct use of the primitive (only forward secrecy is missing).
- **Relay HTTP security:** complete security headers, configurable HSTS, optional CSP, request/correlation id, IP allow/deny with CIDR, audit trail with user agent.
- **Effective media path traversal prevention:** server-generated ids, only the extension taken from the client, rejection of `..`/`/`/`\`.
- **Defensive configuration:** clamping on almost all numeric variables, limit validation.
- **Complete i18n in parity:** 489 identical keys in en/it, 719 uses of `context.l10n`, only ~45 hardcoded strings.
- **Privacy consistent with the declared ethics:** opt-in telemetry, local only, no tracking SDK, no dependency on `shared_preferences`.
- **Polished UI:** light/dark themes, density, text scale, desktop breakpoints, loading/empty/error states with retry, correct `dispose` of timers and subscriptions in most screens.
- **Basic operational documentation** present (backup, troubleshooting, deploy) and build scripts for Linux/Windows.

---

## 9. Limits of this verification

- **No compilation.** Rust and Flutter are not installed: type errors, lint, tests and feature-unification were not verified.
- **No runtime test** of real federation (Mastodon/Misskey), WebRTC/TURN, P2P, nor performance measurements: the conclusions on `spawn_blocking`, global lock and N+1 are reasoning on the code, not benchmarks.
- **iOS ATS** and local network privacy: a device is needed.
- **`ap.rs` (6,502 lines) and the relay `main.rs` (8,836 lines)** were read in full on the critical paths (signature, inbox, outbox, media, health, config, tunnel) and scanned for patterns elsewhere.
- To be verified at build time: presence of `wget`/`redis-cli` in the images used by the compose healthchecks; axum default limits for WebSocket messages; behaviour of `flutter_widget_from_html_core` when `onTapUrl` returns `false`.

---

## 10. Summary

| Severity | Count |
|---|---|
| P0 (blockers) | 12 |
| HIGH | 66 |
| MEDIUM | 37 |
| LOW | 20 |

**The three interventions with the best cost/benefit ratio:**

1. **Fixing the manifests and entitlements** (a few lines, they unblock the release on Android, macOS and iOS).
2. **Tunnel reconnection loop** (a few lines, they eliminate the defect most visible to federated users and the total silence after a network blip).
3. **Mandatory signature on `/inbox` + `actor`↔signature binding in the core** (two checks, they close the two most serious authentication gaps).

Right after: CI with `cargo test` and `flutter test` (a day of work, it enables everything else), and the fix of the relay global lock (the difference between "holds up" and "collapses under load").
