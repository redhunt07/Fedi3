# Verification and distribution — 7 November 2026

Local client and public relay updated. Backups verified before replacement.

## Operational fixes

- Directory also published at `https://fedi3.redhunt07.ovh/_fedi3/relay/directory`.
  The Discover button uses the cache/bundled copy when all remote sources fail.
  The retired old domain is not restored and TLS remains verified.
- SSRF: shared core/relay resolver filters the addresses actually used by the
  connector, including after redirects to a hostname. The test verifies that
  localhost is not contacted. Ambient HTTP proxies disabled for federation;
  storage/search services configured by the operator use separate clients
  without redirects.
- Spool: expiry verified before delivery; atomic quota/eviction with SQLite
  transactions and PostgreSQL user-row lock. The shared inbox offline path uses
  the encrypted path when the user's key is available.
- Reconnections: an old authentication completed late does not replace a newer
  tunnel. Regression found in the suite and fixed.
- Public page `https://fedi3.redhunt07.ovh/transparency` and signed policy
  snapshot at `/_fedi3/relay/transparency`, without user lists or content.

## Preparations and limits

- Push: authenticated registration/revocation API and configurable gateway
  contract, disabled in production as requested. Native UnifiedPush/APNs
  connectors, their callbacks/background handlers and the provider gateway
  remain to be implemented and tested on devices. No guarantee of a permanent
  mobile tunnel.
- TURN: private file with multiple operators and distinct credentials,
  authenticated endpoint and ICE integration in Flutter calls. No new community
  operator or free service was activated. Rotation, quotas and availability
  require agreements with real operators; automatic ICE restart for calls
  already started is missing.
- Gossip: already present; transmission between two local libp2p nodes and
  persistence to SQLite without a relay verified. The test covers transport and
  receiver storage; it is not proof between two complete clients behind
  different physical NATs, nor of complete historical availability with an
  offline author. Transport signatures are not equivalent to independent
  verification of the ActivityPub actor.
- Transparency: the signed snapshot makes the declared policy verifiable. An
  append-only forward log, independent witnesses and receipts signed by
  recipients remain to be implemented; the page does not present them as
  existing.

## Verifications

- Rust workspace: 285 passed, 2 ignored (public IPFS bootstrap and PostgreSQL).
- Isolated PostgreSQL 16 on VPS: explicit test passed; media quota, 8 concurrent
  spool inserts, passwords with special characters and account deletion.
  Test container and tunnel removed. The production database was not used for
  these tests. The first stress with 20 requests over SSH hit the dispatcher
  wait limit; the final run uses 8 concurrent writes.
- Flutter: full suite 154 passed; 12 directory tests repeated after adding the
  mirror. Analyze fatal-infos and Clippy all-targets without errors.
- Compose/OpenAPI validated; candidate image tested before deployment:
  directory, signature, retention, authorization and distinct TURN credentials,
  push registration disabled. Signature independently rechecked.
- Production: health/readiness, directory and transparency HTTP 200 with valid
  TLS; signature verified; client reconnected; actor key equal to the local one.
  Relay/P2P/TURN healthy, no panic or search initialization error.

## Distribution and recovery

- AppImage: `dist/Fedi3-release-x86_64.AppImage`, running with the new core.
- Relay: image `fedi3-relay:network-resilience-20261107`.
- Rollback: `fedi3-relay:before-network-resilience-20261107`.
- VPS backup: `/home/rocky/fedi3-backups/network-resilience-20261107/`;
  verified PostgreSQL dump, private configuration and previous sources.
- Local encrypted backup: `~/.cache/fedi3-audit/alice-before-network-resilience-20261107.aesgcm`;
  key in the Secret Service, attributes application=fedi3,
  backup=fedi3-network-resilience-20261107. Format nonce 12 bytes + AES-GCM,
  AAD `fedi3-network-resilience-20261107`.
- Local identity, P2P infra identity and server `.env` preserved.
  P2P infra and TURN did not require a new image and were not recreated.

Configuration and protocols: [network_resilience.md](network_resilience.md).

## Artifact hashes

- Core: `1746c56dc3b8eae221aba302bd34b73ac998df2236a592918bff4cdf3545db53`
- AppImage: `d9a4fc697524fc57499fc71bbf62a563b2ce714303f3b508787c2263500ba5e4`
- Relay: `359bc442425de415d4c86d969758540c1fa4b3ab0be17e32f7528bada1539993`
