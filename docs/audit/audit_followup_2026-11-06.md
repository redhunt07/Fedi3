# Audit verification and fixes — 6 November 2026

The received audit was used as a list of findings to verify in the code, not as
proof that every conclusion or proposed solution was correct. This work covers
the ten critical findings C1–C10 and some related issues.
The changes are in the sources: they were not deployed to the servers or to the
clients running during this work.

## Critical findings fixed

| Finding | Fix and evidence |
| --- | --- |
| C1 — infrastructure frames | Rejection of zero lengths or lengths above 4 MiB before allocation, both for request and response; limit also on write. The value remains compatible with the core limit. Tests with `u32::MAX`, limits and round trip. |
| C2 — backup and dotenv | CSP quoted in the template; five scripts use a literal parser without `source` of the `.env` file or `eval`. Values do not override already defined variables nor are they exported to Compose, which keeps its own interpolation. Retention validated before arithmetic. Backup/restore tested with simulated Docker, the real template and values with metacharacters. ShellCheck and regression added in CI. Diagnostics also redact DB URL and keys. |
| C3 — remote objects | Synchronous fetch and worker use GET with validation before every redirect and reading with a 2 MiB limit. Data received via P2P is also limited before parsing; the returned ID must belong to the requested origin. |
| C4 — remote media in the core | HTTP fetch under the same policy, reading limited to 32 MiB, checks also on P2P results. A rejected private URL is not re-proposed to the client via redirect. Test of the public endpoint with a counter of requests to the private target: zero contacts and 502 response without Location. |
| C5 — relay inbox signature | The declared sender, including in object form with `id`, must match the actor verified by the signature. Test with valid signature and forged actor: 401 and no spool. |
| C6 — core recovery | `markCoreDead` tries to stop the handle before abandoning it. Watchdog installed also after the first failure; after the maximum number of attempts it applies a five-minute cooldown. Manual retry reopens the budget. The UI latch does not create a start loop and recognizes new configurations. Onboarding stays open if the core does not start. Test of the first failed start and of manual recovery. |
| C7 — chat draft | Text and attachments remain available during sending and on error. After confirmation only the sent contents are removed, preserving any new text. Test 503 → draft intact → successful retry. |
| C8 — links | Notes without CW, bio and profile fields use the URL policy. Test of the tap on a link in a note and of the actual call to the simulated launcher. |
| C9 — relay media | Upload keeps `Bytes` without duplicating the entire buffer in `Vec`; local, WebDAV and S3 backends return a stream. Single range, suffix, 206/416, ETag, 304 and If-Range implemented; media metadata isolated with sandbox CSP and nosniff. HTTP tests of the local backend and a WebDAV test with a response that stays open while the caller receives the first chunk. |
| C10 — dedup and sync | Claim with `INSERT OR IGNORE`; concurrency tested with 16 threads. P2P sync saves claim, inbox and feed in the same transaction and keeps the cursor on errors. A DB error simulated via trigger must also roll back the claim and allow the retry. |

The solution suggested in the audit of binding Like/Announce objects to the
origin of the actor that reacts or re-shares **was not applied**: it would break
federation between different instances. The check of the fetched document and the
policy on its URLs remain distinct from the identity of the Announce author.

## Further related work

- **A3, mitigation at download:** CSP `default-src 'none'; sandbox` and `nosniff`
  on media responses. Magic bytes validation and the SVG upload policy still have
  to be completed.
- **A4, new tunnels:** the client sends the token in `Authorization: Bearer` and
  the logged URL no longer contains the token. Server compatibility with old
  clients using a query string remains; historical logs and their possible secret
  rotation were not modified.
- **A9:** received edit/delete require the original sender; receipts and
  reactions must refer to messages of the declared conversation.
  Regression with two threads and multiple members, including a legitimate edit.
- **A10:** sync verifies the expected actor, local moderation and ownership of
  editable objects. Like and Announce towards other instances remain valid.
  Tests of forged attribution and federated re-share.
- **A18:** capture, renderer and peer connection resources returned after the
  screen is closed are released. Test with delayed microphone opening: track and
  stream released, no invitation sent.

## Verifications performed

- `cargo test --workspace --locked --offline -- --test-threads=4`: **245 passed**,
  zero failed, one public IPFS test intentionally ignored.
- Subsequently, on the version with the media redirect check:
  `cargo test -p fedi3_core --test media_fetch_policy --locked --offline`:
  **1 passed**. The total number of distinct Rust regressions executed is **246**.
- `flutter test --no-pub`: **130 passed**.
- `flutter analyze --fatal-infos --no-pub`: no findings.
- `cargo fmt --all -- --check`, `git diff --check` and
  `scripts/verify_platform_config.sh`: passed.
- ShellCheck 0.11.0 on the five operational scripts and on the parser: passed.
- `python3 scripts/tests/test_backup_scripts.py`: passed. It is a test of the
  commands and archives with simulated Docker, not a restore on a real PostgreSQL.

A first run of the relay tests with unlimited concurrency had a transient HTTP
closure in `inbox_requires_digest_header`. With the four threads used by the CI,
the group and the workspace suite passed.

## 7 November update

The remaining P1 items were addressed in the following phase, with new builds and
installation on the local client and on the VPS. Details of the work, the
verifications and the backups are in [audit_p1_deployment_2026-11-07.md](audit_p1_deployment_2026-11-07.md).
The medium findings and the audio/video verifications between two PCs on different
networks remain.


## Verification of the residuals

The initial statement about the P1 items was too broad: `markCoreDead`, the
keyring screen, deploy/configuration, cleanup and consistency still had residuals.
The subsequent verification and fixes are documented in
[audit_remaining_fixes_2026-11-07.md](audit_remaining_fixes_2026-11-07.md).
