# Audit residuals: second verification of 7 November 2026

This verification corrects and replaces the overly broad statement of the first
follow-up: the ordinary stop was asynchronous, but `markCoreDead` was not;
keychain and CI were not verified end-to-end, and several deploy, cleanup and
consistency findings remained open.

## Order followed and results

1. **CI:** the failed GitHub jobs of run 41549436948 were examined. Dart/Rust
   formatting, libp2p deprecations, borrows and warnings of the extracted modules
   were fixed. Annotations for numerous arguments remain limited to the two
   facades that keep the SQL contract, without suppressing global warnings. The
   tunnel timeout test now waits for the actual registration after the 101; the
   test HTTP client does not reuse closed connections. Compose/OpenAPI tests and
   a PostgreSQL job with an ephemeral database were added.
2. **Freeze:** `markCoreDead` starts the asynchronous stop, preserves the cause of
   the failure and records the cleanup future. Every new start awaits that future;
   dispose also does not block the UI thread. Test with delayed stop, UI timer
   still running and restart prevented until release.
3. **Keyring:** `credentialsUnavailable` state distinguished from corrupted JSON;
   recovery screen with Retry, without onboarding that could overwrite the
   profile. Test of the preserved file and of recovery after unlock.
   libsecret/Secret Service/jsoncpp prerequisites documented and added to the
   Linux build job.
4. **Deploy:** Compose passes the same PG_USER/PG_PASSWORD/PG_DB to PostgreSQL
   and relay; an empty DB_URL is composed in the relay with safe percent encoding,
   without a duplicated password in the template. The full override remains
   supported for external databases. README/AGENTS updated. Root/standalone TURN
   aligned, image pinned, external IP required, STUN healthcheck and memory limit;
   no full `.env` in the TURN container. Standalone P2P aligned with external
   addresses, circuit limits, peer-ID file and real TCP/ID healthcheck.
   The standalone bind mount remains unchanged so as not to regenerate the
   identities.
5. **Cleanup:** removed the uncompiled prototypes relay_mesh.rs/relay_notes.rs,
   nat.rs, the unused P2P entrypoint and `docs/linux error`. The operational HTTP
   peering between relays remains. OpenAPI updated for authentication, raw media,
   Range/ETag, deletions and global signals; the ambiguous media path removed.
   Automatic YAML, reference and path template validation.
6. **Consistency and security residuals:** described below with specific evidence.

## Consistency and security fixes

- **Blob and account:** deactivation before collection; deletion of blobs before
  metadata; failures keep account and references to repeat the DELETE. The final
  transaction also deletes blocks, moves, local indexes, notes/tags and signals
  traceable to the identity. An in-flight upload cannot restore media after
  deletion. Test with failing storage and retry, plus a full HTTP upload/deletion
  test of the files.
- **XFF:** the core uses the IP of the TCP connection. Forwarded headers are
  interpreted only with an internal provenance inserted by the authenticated
  relay tunnel, never from an HTTP header or a P2P peer. The last valid IP added
  by the proxy is used. Test of direct socket, P2P without provenance and relay.
- **Dead-letter:** classification via typed HTTP/P2P/WebRTC status and the reqwest
  error chain. Only 404/410 are permanent; 429 is retryable. Strings in the body
  or URL do not determine the fate of the job. Test 429/5xx containing the text
  "404/410" and false errors with numbers in the URL.
- **Legacy sync:** limit before parsing also for actor and collection, beyond the
  pages; policy verified before every redirect. Test of an oversized response for
  all three reads.
- **Quota:** atomic reservation with SQLite IMMEDIATE transaction or PostgreSQL
  user row lock; no fail-open on the DB. Reserved bytes count in the quota.
  Incomplete reservations persist and are cleaned after 15 minutes, deleting any
  blobs first; completed uploads remain intact. Parallel SQLite/PostgreSQL tests,
  eight concurrent HTTP uploads and recovery of an expired reservation.
- **Preferences:** queue not poisoned by errors for temporary files in PrefsStore;
  UI mutations queued on a snapshot updated in AppState.
  Read receipts merge per thread and advance monotonically.
  Tests of 25 simultaneous writes, filesystem error followed by retry and 20
  conversations updated together with a UI setting.

## Evidence and deployment

The code of commit `90378fff16ab49c4e34026c8f054e9f64e78b5bd` passed
on [GitHub Actions, run 175](https://github.com/redhunt07/Fedi3/actions/runs/42355489694):
eight green jobs (Rust fmt/Clippy/test, Flutter analyze/test, platforms,
scripts/deploy and PostgreSQL). The AppImage job, reserved for tags, is excluded
from the PR; the AppImage was built and verified locally.

[PR #93](https://github.com/redhunt07/Fedi3/pull/93) opened as a draft, without
merge into main. The operational documentation and this report were completed
after the code verification and the deployment.

- Workspace suite: **269 Rust tests passed**, zero failed; two optional tests
  ignored (public IPFS and isolated PostgreSQL). The latter was run separately
  with success, for **270 distinct Rust regressions**.
- Flutter: **139 tests passed**; analyze with fatal infos and formatting clean.
- Clippy with `-D warnings`, ShellCheck, platform configurations,
  simulated backup/restore and Compose/OpenAPI contracts passed.
- Docker candidate on isolated PostgreSQL: URL composed from components with a
  special password, eight concurrent uploads (one admitted and seven 413 with a
  32-byte quota) and account/blob deletion verified.
- Installed on the Linux client and on the relay, p2p_infra and TURN containers of
  the VPS: all healthy, zero automatic restarts, health/readiness 200. Thirty-six
  concurrent stats requests on the production PostgreSQL DB: all 200.
- Public tunnel connected; Alice actor responds with the correct local key.
  RSA/libp2p keys and infrastructure identity preserved; no panic in the updated
  client log. Config and `.bak` contain keyring references.
- Production now uses PG_USER/PG_PASSWORD/PG_DB with an empty DB_URL, after
  transferring the same working credentials from the previous override.
  Public addresses and P2P limits preserved. TURN no longer contains DB secrets
  or admin tokens in the environment.

 Local logs are in `/home/morpheus/Documenti/` with prefix `fedi3-residual-`.
 The additional PostgreSQL test uses an isolated PostgreSQL 16 container, removed
 together with the SSH forward after the test; it does not use the production
 database.

No audio/video calls with Peppenamir were performed. The verifications on the
transports, services and queue do not replace an audio/video test between those
two PCs. Removing the old files does not rewrite Git history.


## Restore and artifacts

VPS: `/home/rocky/fedi3-backups/audit-residual-20261107/` (0700), PostgreSQL dump
verified with pg_restore, previous Compose/dotenv and sources, manifest and
candidate results. Previous images kept with tag
`before-residual-20261107`; automatic rollback planned if the verifications fail.

Client: consistent SQLite/profile/preferences backup encrypted in
`/home/morpheus/.cache/fedi3-audit/alice-before-residual-20261107.aesgcm`, key in
the keyring with attributes `application=fedi3`, `backup=fedi3-residual-20261107`.
Previous AppDir in `client-before-residual.AppDir` in the same protected
directory. No profile reset or deletion of historical dead-letters.

| Installed artifact | SHA256 |
| --- | --- |
| fedi3_relay | `cfe69f7cb217b3e2c275ba9ae5e5318f463463b63d7d06a943f8ca631606dc2f` |
| fedi3_p2p_infra | `8c574a609db5f9700df6965cb619adfe8dce8649ef9f22e8941718f6da0c3d63` |
| Core Linux | `cf0e789177e381ce1d565371ff60809677e0c24a0d94e6195184afd9e58044e7` |
| AppImage Linux | `6e8e2921ad62066653d80713961a8d500737bc36d9d27d0c09041d8ff4709249` |
