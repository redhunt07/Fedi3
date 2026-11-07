# Audit P1: fixes and deployment of 7 November 2026

Work on the P1 items A1–A20 of the audit received on 6 November, including those
already fixed together with C1–C10. The builds were deployed on Alice's local
Linux client and on the `relay` and `p2p_infra` services of our VPS. No deployment
was performed on Peppenamir's computer.

## Work

| Finding | Result |
| --- | --- |
| A1 | Relay SQL extracted into `db.rs`; asynchronous facade in `db_dispatch.rs`, operations on `spawn_blocking` and concurrency limited by the PostgreSQL pool. The global mutex was removed. Readiness uses a separate capacity and a timeout; test on a single-thread runtime with one blocked operation and a concurrent successful query. |
| A2 | SIGTERM/Ctrl-C trigger HTTP shutdown and tunnel closure; the P2P infrastructure drains the mailbox handlers with a five-second deadline. SIGTERM tests on the real binary: clean exit and the user still in the DB; a request already in the tunnel completes with 200 even after the signal. The reader drains up to five seconds, the writer is cancelled and awaited. |
| A3 | SVG rejected; magic bytes check for supported MIME types. Sandbox CSP and nosniff also at download. Unknown MIME types are not accepted as images/audio/video. |
| A4 | New tunnels with Bearer; relay tracing logs only the path. Compatibility with the query token of old clients kept. The Caddyfile in use does not enable access logs. Historical logs and tokens already possibly exposed were not deleted/rotated. |
| A5 | Before storing a relay's key, its stats endpoint is queried on the declared origin, verifying URL, signature and key. Existing pins remain binding. Test with a validly signed payload that tries to claim another relay's origin: rejected, no key stored. |
| A6 | Authorization headers, cookies, internal headers and hop-by-hop headers excluded from forwarding; headers named by Connection and Set-Cookie in responses are also removed. Host rebuilt for the HTTP recipient. Test with a real HTTP peer and sentinel credentials. |
| A7 | Discovery re-reads the peer ID file with a timeout even if it appears after relay startup. Compose dependency on the infrastructure added without changing identity or public addresses. Test of the file created after startup. |
| A8 | Mailbox handlers run in separate tasks, maximum 64; the swarm keeps processing ping, identify and circuits. Overload returns 503. |
| A9 | Edit/delete bound to the original author; receipts and reactions bound to the conversation. Previous fix included in the deployed builds. |
| A10 | P2P sync verifies actor, object ownership and moderation. Like/Announce between different instances keep the legitimate federated behaviour. |
| A11 | Device signatures require target, host, date and digest; those generating the device sync GET requests were also updated. Negative test for incomplete sets and positive test for a complete signature. |
| A12 | Empty or whitespace-only token generates a random secret. Both internal authorization paths reject empty tokens and use constant-time comparison. HTTP test without credentials. |
| A13 | Tunnel with 64 simultaneous handlers, 503 beyond the limit and cancellation on disconnect/shutdown. Test with 65 blocked requests and verification of handler release. |
| A14 | Independent SQLite versions per social and delivery module, with rejection of future versions; legacy marker not overwritten by the modules. Migration verified also on the installed DB. |
| A15 | Encrypted settings export v2: AES-256-GCM, PBKDF2-HMAC-SHA256 with 600,000 iterations, random salt/nonce. v1 import compatible. Wrong password and tampering rejected. Explicit UI: it does not include identity keys, messages or media; it is not a complete account migration. |
| A16 | Five sensitive fields in flutter_secure_storage; config and backup contain references to the keychain. Writes to the vault verified before the migration, no plaintext fallback; an unavailable keychain prevents startup while preserving the file. Real Linux migration verified. |
| A17 | Android permissions for microphone, camera and audio; runtime media requests delegated to the existing WebRTC plugin. Runtime notification request and tap on the local thread. Absence of the notification service does not block runApp. |
| A18 | Asynchronous call resources released even if the screen is closed during initialization; including the delayed microphone capture test. |
| A19 | A single global query of signals, with cursor by ID and membership/recipient check, replaces the scan of 100 chats. Polling from 3 to 15 seconds when inactive and wake-up via SSE; old expired/terminated invitations ignored. |
| A20 | Shared lifecycle observer: UI polling and SSE suspended in background, timers of an active call preserved. On exit the stop is awaited with a timeout. FFI wait on a separate isolate; starts serialized with the stop to avoid overlaps on the DB. |

During the installed verification, a Rustls panic of the WSS WebSocket was also
fixed: cryptographic provider explicitly chosen when the dependencies enable both
ring and AWS-LC. Tests of the TLS builder and connection to the real relay
succeeded after the fix.

## Verifications

The full workspace suite passed **259 Rust tests**, zero failed and one public
IPFS test intentionally ignored. After the last refinement of the shutdown, all
**77 relay tests** were repeated, including a new test on draining the in-flight
request: all passed. The total number of distinct Rust regressions executed is
**260**. Commands with `--locked --offline` and `--test-threads=4`.

- Flutter: **134 tests passed**, analyze with `--fatal-infos` without findings.
- Additional native test: FFI handle stopped from a separate isolate, server
  closed and caller timer kept active during the wait.
- dotenv parser and backup/restore: automated test with simulated Docker passed;
  it is not equivalent to a restore test of the production dump.
- Platform configuration, Rust formatting, whitespace check and
  ShellCheck 0.11.0 (`-x -P scripts`) verified.
- Upload to the installed public relay: HTML declared as PNG and SVG both
  rejected with 415, without creating media.
- On the VPS real PostgreSQL: 36 stats requests with 12 workers, all 200;
  `/healthz` and `/readyz` 200, zero container restarts after deployment.
- On the installed client: DB healthy, WSS tunnel connected, P2P peers connected;
  public Alice actor with the same local RSA key. Public IPFS peers do not
  represent as many Fedi3 users.
- Profile RSA and libp2p keys and infrastructure peer ID compared with the
  previous copies: unchanged. No secrets in the config JSON and `.bak` files.

## Backup and restore

VPS: `/home/rocky/fedi3-backups/audit-p1-20261107/`, directory 0700.
PostgreSQL custom dump verified with `pg_restore --list`: 402,869 bytes,
SHA256 `0933b0f1bafeab3debcc92c54496456799a450d7fab999c7c6d222942dd882ed`.
Previous images kept as
`fedi3-relay:before-audit-p1-20261107` and
`fedi3-p2p_infra:before-audit-p1-20261107`.
Previous Compose configuration and sources were also kept. The source archives
and the manifest of the installed version remain next to the dump.

Client: consistent copy of SQLite and profile files saved before the migration in
`/home/morpheus/.cache/fedi3-audit/alice-before-p1-20261107.aesgcm`.
AES-GCM archive, with random key in the system keychain, attributes
`application=fedi3`, `backup=fedi3-audit-p1-20261107`. It is not encrypted with the
passphrase of the new UI backups: it is a local operational copy for rollback.
Do not disclose its key. Restoring requires stopping the client first.

## Limits of the verifications

This is not a production readiness certification and it does not close the medium
findings. No new audio/video call between our PC and Peppenamir's was tested in
this deployment. Android hardware and real S3/WebDAV cloud backends were not
verified in the field. The mailbox and tunnel tests do not reproduce all possible
NAT/CGNAT conditions.

The UI backup is deliberately an encrypted settings backup: to transfer the same
identity a protected copy of the core data folder is also needed. Old v1 backups
remain readable but are in plaintext.
The local queue also contains historical delivery dead-letters: they were neither
deleted nor manually re-sent during this maintenance.


## Deployed artifacts

Rust builds with GNU target and GLIBC 2.34 baseline, compatible with the VPS
Debian Bookworm runtime; Linux AppImage with updated core. Server images built
from the previous runtime images, replacing the verified binaries.
Database, volumes and `.env` configuration preserved; Compose updated for startup
order and infrastructure stop time. Sources and operational scripts synchronized
on the VPS for subsequent rebuilds.

| Artifact | SHA256 |
| --- | --- |
| Relay | `6c6c14afe9ac88bf88f6cbe526ca17595079756267d950f101751ed3b93a9395` |
| P2P infrastructure | `fa35f2c46bc7a6092894723c087085534c0f2812acb20dd371c77eebd2b34dab` |
| Core Linux | `0c73696c9f7651c1cade0638e40201ff60723fbf0dcdd3514f9a77717b7fb16b` |
| AppImage Linux | `b22361f4edd3b1f2d304c1b926f403849c361d126729270f66d9c8ae677a214a` |

Local verification logs are in `/home/morpheus/Documenti/`, prefix
`fedi3-p1-`: `validated-workspace.log`, `relay-complete.log`,
`flutter-final-verified.log`, `analyze-final.log`, `native-stop.log`,
`installed-checks.json`. The initial failed tests were kept separately: the
report above uses only the final verified outcomes.
