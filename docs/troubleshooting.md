# Troubleshooting

## Caddy: ACME error

- Invalid domain or DNS not propagated.
- Ports 80/443 closed.
- Wrong `FEDI3_DOMAIN`.
- The `relay` service is not healthy: Caddy does not start until `/healthz` responds. Check `docker compose ps` and `docker compose logs relay`.

## Relay does not start

- Verify `.env`: the relay refuses to start if `FEDI3_RELAY_ADMIN_TOKEN` is shorter than 32 characters or if `FEDI3_RELAY_DB_DRIVER` is not `sqlite`/`postgres`.
- The compose stops if `FEDI3_RELAY_PG_PASSWORD`, `TURN_REALM` or `TURN_PASS` are missing. Caddy needs `FEDI3_DOMAIN`; the installer validates it for relay modes.
- `docker compose logs relay`
- `docker compose logs postgres`
- If the database is reachable but the schema is newer than the binary, the relay exits with an explicit error: do not downgrade without a migration.

## Federated requests rejected (401)

Since 0.1.1 verification is mandatory. Typical causes:

- the remote instance does not sign the request, or signs only `(request-target)` and `host` without `date`/`digest`;
- the `Digest` does not match the body (proxy rewriting the body, undeclared compression);
- `Date` outside the allowed window (±300 s): check the server clock;
- the activity is signed by an actor different from the one declared in `actor`;
- the public key is unreachable or `keyId` points to a private IP (SSRF guard).

Diagnostics:

```bash
docker compose logs relay | grep -iE 'signature|digest'
```

## Tunnel: repeated disconnections

The core reconnects on its own with exponential backoff. If you see continuous reconnections:

- relay token revoked or wrong (the relay closes with 401);
- the relay does not respond within the connection timeout;
- no `pong` within 30 s: unstable network or a proxy that drops idle connections.

Check `docker compose logs relay | grep -E 'tunnel (connected|disconnected)'`.

## Offline user: activities in spool

If a user is not connected, activities are put in spool and delivered on reconnection. If the spool exceeds the per-user limit (`FEDI3_RELAY_SPOOL_MAX_ROWS_PER_USER`) the oldest ones are discarded. If the sender receives 503, it means that no recipient was delivered or queued: retry later.

## Dragonfly crash (memory)

Reduce threads or memory:

- `--proactor_threads=2`
- `--maxmemory=512mb`

## Meilisearch not healthy

- Check `MEILI_MASTER_KEY`.
- Check health: `http://meilisearch:7700/health`.
- With `FEDI3_RELAY_SEARCH_BACKEND=db` Meilisearch is not needed: the relay uses the database.

## Media not served

- Verify `FEDI3_RELAY_MEDIA_BACKEND` and, for S3/WebDAV, credentials and endpoint.
- `docker compose exec relay du -sh /data/media`
- A 415 on upload means MIME not in the allowlist; a 413 means quota exceeded.

## App: network error

- Verify relay and token.
- Check `GET /_fedi3/health` on the local core: if it does not respond, the core is dead and the watchdog restarts it.
- If the timeline stays empty after a configuration change, verify that the core restart succeeded: the configuration screen reports the error and restores the previous values.

## App: the core does not start

- Port already in use: change the bind or close the other instance.
- Invalid username: only `[A-Za-z0-9_]`, 1–64 characters.
- Data directory not writable: check permissions (the directory must be `0700` and the keys `0600`).
- Without a relay the core starts anyway: if it does not start, the problem is not the relay.

## App: corrupted configuration

The app writes the configuration atomically and keeps a `.bak`. If the main file is corrupted, the app recovers the backup; if that is corrupted too, it shows a warning instead of overwriting. In that case reset the configuration from the setup screen.

## Quick relay diagnostics

```bash
scripts/relay_diagnose.sh
```

It shows container status, healthcheck, tunnel logs and signature rejections. It does not require `curl` or `wget` in the relay image.

## P2P: chat, presence and calls

For independent bootstraps, peer memory, exchange of signed contacts and
IPFS discovery see [Fedi3 network and public infrastructures](p2p_network.md).

- `connected_peers` counts libp2p connections, including infrastructure nodes:
  it is not the number of online users. Contact presence comes from the
  relay directory and from P2P presence messages; the two sources are independent.
- The ActivityPub profile must publish `endpoints.fedi3PeerId` and
  `endpoints.fedi3PeerAddrs`. For a peer behind NAT, the addresses must
  include `<relay>/p2p/<relay-id>/p2p-circuit/p2p/<peer-id>`.
- The circuit relay server must announce a reachable address through
  `FEDI3_P2P_EXTERNAL_ADDR`. A reservation without addresses is rejected
  by the client even if the server records the request as accepted.
- The client retries closed reservations after at least 10 seconds, with a check
  every 5 seconds. Look for `relay reservation closed`, `ReservationReqAccepted`
  and `p2p listening` in the logs.
- `queued` means kept in the mailbox, not received by the recipient.
  The `delivered` confirmation identifies the received message; `seen` indicates read.
- The signaling test verifies invites/SDP/ICE carried and stored; audio,
  video and TURN traversal also require a test with real devices.

Local tests with temporary profiles, without production credentials:

```bash
cargo test -p fedi3_core --lib connectivity_tests
cargo test -p fedi3_core --test p2p_chat --test semantics
cd app
flutter test test/unit/peer_presence_store_test.dart test/widget/incoming_calls_test.dart
```

The fixes require rebuilding both the Rust core and the Flutter app:
an old AppImage still contains the previous code.

### Circuits rejected or closed during chat and calls

`Remote reported resource limit exceeded` indicates a rejection by the circuit relay,
not necessarily an offline client. The libp2p default limits (128 KiB,
120 seconds, limited reopenings) are not suitable for Fedi3 frames and prolonged
reconnections. The infrastructure service now uses 64 circuits overall, 8 per peer,
16 MiB and 900 seconds per circuit. Operators can
change them with `FEDI3_P2P_RELAY_MAX_CIRCUITS`,
`FEDI3_P2P_RELAY_MAX_CIRCUITS_PER_PEER`,
`FEDI3_P2P_RELAY_MAX_CIRCUIT_BYTES` and `FEDI3_P2P_RELAY_MAX_CIRCUIT_SECS`.
Invalid values prevent startup; per-peer and per-IP limits remain active.

The call must show an actual connection, not only acceptance
of the invite. In the logs `Fedi3 call media` reports the RTP bytes sent and received:
the counters must increase on both devices. The audio/video content is not recorded.
The caller generates the SDP offer after acceptance;
ICE candidates that arrived before the offer are kept and applied
after the remote description.

## Linux AppImage 0.10.0 closes before opening a window

`libmount.so.1: version MOUNT_2_40 not found` or
`libsystemd.so.0: version LIBSYSTEMD_251 not found` indicates a packaging conflict,
not profile corruption or a missing executable permission. Version 0.10.0 bundled
older copies of dependencies used by the host GLib. The fix is prepared for
0.10.1; install the replacement when published. Keep all existing data and keyring
credentials. Rebuilding with the corrected `scripts/build_appimage.sh` also
applies the fix. SHA256 integrity checks alone cannot detect this ABI mismatch.
