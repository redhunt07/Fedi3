# Network resilience (7 November 2026)

## Directory and TLS

The active relay is `https://fedi3.redhunt07.ovh` (WSS on the same host).
`relay.fedi3.com` is retired. Do not suppress TLS validation to use it.
The relay also serves a packaged mirror at `/_fedi3/relay/directory`, used by
the app, core and relays alongside the repository mirrors. The raw GitHub and jsDelivr directory URLs can return 404 until the directory
is published on the remote default branch. Both first-run preview and manual
onboarding discovery use the bundled/cached directory when refresh fails.
The directory supplies candidates, not trust: the device probes their health.
See [relay_directory.md](relay_directory.md) for the file format and ranking.

## Mobile: configurable provisioning, not a background tunnel guarantee

Android/iOS can suspend or terminate the app and its embedded core. Do not rely
on a permanent tunnel. Reconnection flushes the relay's durable spool.
`RelayCapabilities.registerPush` / `unregisterPush` and these relay endpoints
provide the provisioning contract used by the native platform connectors:

- `PUT /_fedi3/relay/push/{username}` with the account's relay Bearer token and
  `{"provider":"unifiedpush","token":"opaque endpoint or device token"}`
  (or `apns`). Returns `expires_at_ms`; renew before 30 days.
- `DELETE` on the same path revokes the registration. Only one installation per
  account is currently retained; registration replaces the previous one.
- Disabled accounts cannot register or receive wakeups. Account deletion removes
  its registration. Unregistration clears the stored device token. Expiry stops
  wakeups; an expired token remains in metadata until replacement/revocation/deletion.

Both `FEDI3_RELAY_PUSH_GATEWAY_URL` (public HTTPS) and
`FEDI3_RELAY_PUSH_GATEWAY_TOKEN` (at least 32 random characters) are required.
Without them registration returns 503 and no pushes are attempted. The gateway
receives authenticated JSON: `provider`, `token`, `event:"sync"`, `ttl_secs`.
It must validate device endpoints, coalesce notifications, discard expired jobs,
and implement the provider protocol. No content, sender, account name, private
key or account relay token enters that JSON. The gateway learns the destination.
Relay attempts are limited to one/account/minute and bounded to four seconds;
failures leave the spool intact. Gateway redirects are disabled.

Native Android UnifiedPush and iOS APNs connectors are implemented. See
[push_forward_receipts.md](push_forward_receipts.md) for platform behavior,
provider configuration, build/signing requirements and verification limits.
No external push gateway is activated by default.

References: [UnifiedPush Android protocol](https://unifiedpush.org/developers/spec/android/),
[Apple background pushes](https://developer.apple.com/documentation/usernotifications/pushing-background-updates-to-your-app).

## Spool retention

`FEDI3_RELAY_SPOOL_TTL_SECS`: default 604800 (7 days), accepted range 60–2592000.
Core envelope verification still caps the extended signature date window at seven
days: use at most seven days for interoperable delivery. Expired items are never
replayed, even between physical cleanup passes. Cleanup frequency is
`FEDI3_RELAY_WEBRTC_SIGNAL_CLEANUP_INTERVAL_SECS`, minimum 10 seconds.
Per-account quotas default to 5000 rows and 67108864 body bytes when the
environment does not set them; the shipped `.env.example` uses 500 rows and the
same byte cap. Zero disables that corresponding quota. Insertion/oldest-item
eviction is atomic (SQLite IMMEDIATE transaction, PostgreSQL user-row lock).
Items larger than the complete byte allowance are rejected. Expired items are
removed before quota evaluation.

Overflow evicts oldest items: this is a bounded best-effort queue, not guaranteed
archival. A core HTTP success deletes an item; this is not a human read receipt.
Encryption at rest requires the actor public key already cached by the relay;
otherwise the existing plaintext spool fallback still applies.

## Community TURN operators

No unverified public TURN service or shared credentials are bundled. STUN alone
cannot solve every NAT combination. Multiple consenting operators can supply
TURN entries via a private JSON file, e.g. `/data/turn-pool.json`:

```json
[
  {"urls":["turn:operator-one.example:3478?transport=udp","turn:operator-one.example:3478?transport=tcp"],"username":"issued-user-one","credential":"issued-secret-one"},
  {"urls":["turns:operator-two.example:5349"],"username":"issued-user-two","credential":"issued-secret-two"}
]
```

Set `FEDI3_RELAY_TURN_POOL_FILE` to its container path (restrict file permissions).
At most eight operators, four URLs each, file limit 64 KiB. Credentials are
returned only to enabled accounts authenticated using their own relay token,
through `GET /_fedi3/relay/turn/{username}` with `Cache-Control: no-store`.

When no pool file is mounted, the relay derives a single-operator pool from
`FEDI3_RELAY_TURN_USERNAME`, `FEDI3_RELAY_TURN_CREDENTIAL`, an optional
`FEDI3_RELAY_TURN_URLS` list and `FEDI3_RELAY_TURN_PORT` (default 3478), using
the public relay host. Compose wires these from the `TURN_USER`, `TURN_PASS`
and `TURN_PORT` values used by the `turn` profile, so enabling coturn also
publishes it to clients. An empty credential disables automatic provisioning.
Rotation and allocation quotas remain the operator's responsibility; use scoped
credentials, never a TURN administrative secret. There is no automatic commercial
or community account creation and no implied service availability guarantee.

The Flutter call screen loads the pool once per call and adds each server with its
own credentials to ICE. When ICE credentials are configured but no `turn:` URL was
entered, the client also derives `turn:<relay-host>:<port>` UDP and TCP endpoints,
so a coturn instance deployed next to the relay is used without hand-typed URLs.
The native Linux bridge receives one URL per entry. Explicit local ICE
configuration remains supported, and provisioning failure leaves STUN/direct
paths usable. Offers and answers carry the candidates gathered within a short
window, and a failed candidate pair triggers up to two automatic ICE restarts
with a manual Retry action afterwards. Rust WebRTC data channels retain their
existing separate ICE configuration.

Reference: [WebRTC TURN configuration](https://webrtc.org/getting-started/turn-server).

## Public replication

The core already publishes public outbox activities on `/fedi3/global/1` using
signed libp2p gossipsub. Receivers route them to the policy-controlled global-feed
ingest endpoint and persist them in local SQLite. Followed actors' outboxes can
also be synchronized over the direct P2P request protocol. Known peers and signed
contact exchange support reconnecting without a relay bootstrap.

Regression `public_gossip_replicates_to_disk_without_a_relay` starts two real
libp2p nodes over TCP, disables public discovery/mDNS, uses no relay, and checks
durable replica storage. This is a local transport regression, not a test across
two physical NATs. Gossip is best effort, requires connected peers, and is not a
complete historical replication or availability protocol. Gossipsub transport
signatures identify peers; they do not independently authenticate the AP actor
claimed inside every activity. Existing ingest origin and moderation checks must
not be described as end-to-end actor signature verification.

## Transparency

`/transparency` is a public HTML policy page; `/_fedi3/relay/transparency` returns
an Ed25519-signed policy snapshot with queue limits, retention, online tunnel
count and whether push is configured. It publishes no account list, device
tokens, content or IP addresses. Base64-decode `payload_b64` and verify those exact
bytes against `signature_b64` and `public_key_b64`. Pin the relay's public key
independently: a self-signed snapshot alone does not establish trust.

The snapshot anchors a transactional forward journal. Public commitments and
private, signed forward receipts are implemented, including independently signed
core acknowledgments. See [push_forward_receipts.md](push_forward_receipts.md).
Human read receipts and independent witnesses are not implemented.
