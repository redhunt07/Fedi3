# Native push and verifiable forwards

Fedi3 supports opt-in native push notifications on Android and iOS, and a
transactional forward journal whose entries can be verified independently of the
relay operator. This page covers the mobile connectors, the relay/gateway
contract, the receipt formats and the verification tools. For the relay-side
retention and transparency policy, see
[network_resilience.md](network_resilience.md).

## Mobile connectors

Push is opt-in in **Settings → Security → Push notifications**. The relay must
advertise a configured gateway before the app requests system permission.
Provider services and their credentials remain operator-configured; no provider
account, subscription or public push service is silently created.

Android uses the official UnifiedPush native connector (through
`unifiedpush_android`). Choose an installed distributor. The connector handles
registration tokens, endpoint renewal, sender validation and Web Push decryption.
Fedi3 sends the endpoint and its P-256 encryption key/auth secret to its relay.
Only decrypted, bounded JSON with `event: "sync"` is accepted as a wakeup.
No sender, conversation or message text is placed in a push payload.

If Android starts a headless Flutter engine, Fedi3 displays a generic notification
and leaves the durable spool on the relay until the UI opens. It does not start
a competing native core in another engine. An existing UI engine can synchronize
its own core. Notifications permission is requested during explicit activation;
background callbacks only check permission and never attempt a permission prompt.

On iOS, APNs device registration/rotation, registration failure and background
notification callbacks are implemented in `AppDelegate.swift`. Native callbacks
queue until Dart is ready. At most eight callbacks can be pending; each completes
once, with a 25-second native fallback. Dart coalesces wakeups and uses a bounded
sync window, releasing a core it started in background. OS delivery/execution is
still discretionary. Missing/locked credentials defer sync without replacing a
profile. Push credentials use the device-only, after-first-unlock Keychain class;
enabling push re-saves existing iOS relay credentials into that class.

Endpoint renewal is retried when the UI resumes. Local disable blocks callbacks
immediately and requests relay revocation and native unregistration. Account and
relay changes invalidate the registration scope. Relay revocation can fail while
offline: the local connector remains disabled, and the server registration expires
after 30 days unless revoked/replaced earlier. One mobile installation per account
is currently retained on the relay.

## Gateway configuration and protocol

Set these in the relay's private `.env`, then recreate the relay service:

- `FEDI3_RELAY_PUSH_GATEWAY_URL`: public HTTPS endpoint implementing the contract below.
- `FEDI3_RELAY_PUSH_GATEWAY_TOKEN`: at least 32 random characters, shared with that gateway.
- `FEDI3_RELAY_PUSH_PROVIDERS`: only providers actually supported by the gateway,
  e.g. `unifiedpush,apns`.
- `FEDI3_RELAY_PUSH_VAPID_PUBLIC_KEY`: optional 65-byte uncompressed P-256 public
  key, base64url without padding, matching the gateway's private VAPID key.

`GET /_fedi3/relay/push-config` exposes only enabled/provider/public-VAPID settings.
`PUT /_fedi3/relay/push/{username}` requires the account's Bearer token and accepts:

```json
{"provider":"unifiedpush","token":"https://push.example/opaque-endpoint","webpush_keys":{"pub_key":"BASE64URL_P256","auth":"BASE64URL_AUTH"}}
```

For APNs use `{"provider":"apns","token":"HEX_DEVICE_TOKEN"}` without Web Push
keys. Registration requires an enabled account, validates provider/token/key
shape and rejects private HTTP push destinations. The endpoint is renewed for
30 days. `DELETE` revokes it; authenticated `GET` returns only `pending_items`.
Tokens and encryption metadata are never returned by public discovery APIs.

The gateway receives an authenticated HTTPS POST with:

```json
{"provider":"unifiedpush","token":"https://push.example/opaque-endpoint","event":"sync","ttl_secs":86400,"webpush_keys":{"pub_key":"BASE64URL_P256","auth":"BASE64URL_AUTH"}}
```

It must encrypt UnifiedPush messages according to RFC 8291, translating `pub_key`
to the Web Push subscription's `p256dh`, and use its configured VAPID private key
where required. For APNs it must use the appropriate team/key/topic and sandbox
or production endpoint, with payload
`{"aps":{"content-available":1},"fedi3":{"event":"sync"}}`,
`apns-push-type: background`, priority 5 and a coalescing identifier. The gateway
must authenticate the caller, restrict destinations, coalesce retries and respect
TTL; it learns the device destination, not chat contents or account credentials.
This repository defines the gateway contract; it does not contain or activate a
provider gateway deployment. Provider failures do not delete the spool.

The relay limits wake attempts to one/account/minute. A separate worker retries
pending offline accounts in cursor-based batches of 64, eight concurrent requests,
with four-second HTTP timeouts and no redirects. It does not hold up spool cleanup.
Without gateway configuration no outbound wakeups are attempted.

## Building mobile clients

On Linux with Flutter, a complete JDK 17, Android SDK 36 and NDK 28.2.13676358:

```sh
scripts/build_android.sh debug
```

This compiles and packages the Rust ARM64 core, not just the Flutter shell.
Minimum Android API is 24. `release` requires `app/android/key.properties` and the
operator's signing keystore. Generated native libraries/APKs are ignored by Git.
Mobile Rust HTTPS/WSS clients include Mozilla trust roots because mobile sandboxes
do not expose desktop OpenSSL CA directories. Certificate verification stays on.

For iOS, use macOS/Xcode. A Runner build phase invokes `scripts/build_ios_core.sh`,
producing a static Rust archive for device/simulator architectures and retaining
FFI process symbols through the xcconfig linker settings. This Linux release
cannot execute or validate the Apple SDK build. Configure the
Apple team, bundle identifier and provisioning profile with Push Notifications
and Background Modes → Remote notifications. `Runner.entitlements` uses
`FEDI3_APNS_ENVIRONMENT`: Debug defaults to `development`, Release/Profile to
`production`. Match the gateway's APNs environment/topic. Linux cannot compile,
sign or exercise the UIKit/APNs target; a provisioned device test remains required.

## Receipt meanings

The relay records non-GET forwards and successful spool insertions in an additive,
transactional journal. Each private receipt is Ed25519-signed by the existing relay
key and binds sequence, previous link, relay identity, timestamp, an opaque nonce,
HTTP status and a request digest. That digest binds method, path, query, nonce and
body bytes; it does not attest every incoming HTTP header.

The core signs an independent RSA-SHA256 acknowledgment after its HTTP handler
returns. It binds actor, the same request digest, status, response-body digest and
time. A fresh 256-bit nonce prevents reuse for a different attempt. The relay
verifies the acknowledgment against the cached actor key before assigning:

- `spooled`: queued on the relay; no core-delivery claim.
- `core_acknowledged`: verified core response with a 2xx status.
- `core_rejected`: verified core response with a non-2xx status.
- `unverified_forward_response`: no verified core acknowledgment, including older
  clients or transport failures. Status is recorded without upgrading the claim.

A successful core handler response is not a human read receipt, nor a proof that
another remote fediverse instance processed an outbound activity. The receipt for
a replayed encrypted spool envelope binds that envelope's transport bytes.

If journaling fails, the relay reports `X-Fedi3-Receipt-Status: unavailable` and
logs the failure; it does not fabricate a signature or roll back an HTTP operation
already processed by the core. Receipt issuance does not guarantee that a malicious
operator logged every event.

## Fetching and independent verification

Forwards return the signed envelope as base64 JSON in
`X-Fedi3-Forward-Receipt`. Enabled account owners can also fetch
`GET /_fedi3/relay/receipts/{username}?after=0&limit=10` using their relay Bearer token.
Public `GET /_fedi3/relay/forward-log?after=0&limit=100` returns commitments only,
plus an Ed25519-signed checkpoint for that exact head. `/transparency` links these
APIs; its signed policy snapshot also includes the journal head and retention.

A public entry has sequence, previous_hash, commitment and hash. Its commitment is
SHA256 of the private signed payload. Its link hash is SHA256 of the compact UTF-8
JSON array `["fedi3-forward-log-link/v1", sequence, previous_hash, commitment]`.
For a complete public page/log document, use
`python3 scripts/verify_forward_receipt.py public-log.json --public-log --relay-key PINNED_KEY`.
If pagination is necessary, collect entries through the checkpoint's signed head;
the verifier rejects incomplete, reordered or altered chains.
Anyone can verify the link sequence against the signed head without seeing private
account/actor data. Payloads include random nonces, preventing straightforward
content guessing from public commitments.

To verify a retained receipt offline (requires Python `cryptography`):

```sh
python3 scripts/verify_forward_receipt.py receipt.json \
  --relay-key PINNED_ED25519_PUBLIC_KEY_BASE64 \
  --core-key independently_verified_actor_public_key.pem
```

Pin the relay key and actor key independently; blindly trusting keys included by
the operator proves only self-consistency. The verifier refuses a claimed core
acknowledgment without the independently supplied core key. Retain signed
checkpoints/receipts externally to detect inconsistent histories. Independent
witnesses and automatic cross-relay checkpoint comparison are not implemented.

Journal insertions and key initialization are serialized transactionally (SQLite
IMMEDIATE / PostgreSQL row locks). Defaults: `FEDI3_RELAY_RECEIPT_TTL_SECS=2592000`
(30 days) and `FEDI3_RELAY_RECEIPT_MAX_ROWS=100000`. Limits are 60 seconds–365 days
and 1000–1000000 rows. Prefix pruning preserves a signed-head anchor and continues
sequence numbering. Periodic cleanup applies TTL even when no new events arrive.
Account deletion clears its private envelopes/usernames while retaining opaque
commitments until normal prefix pruning. Older relay binaries ignore the additive
journal tables; deployment rollback does not require dropping them.

References: [UnifiedPush Android](https://unifiedpush.org/developers/spec/android/),
[official Flutter native connector](https://pub.dev/packages/unifiedpush_android),
[Apple APNs registration](https://developer.apple.com/documentation/usernotifications/registering-your-app-with-apns).
