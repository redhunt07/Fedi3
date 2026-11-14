# Multi-device chat sync, QR login and call ownership

This document describes the multi-device model implemented by Fedi3: one
account (one actor identity) can be used from several clients, chat history is
replicated to every linked client, and a call is owned by the client that
answers it. It is also the implementation contract for the core, the relay and
the Flutter client.

## Goals

- Link a second client to an existing account without losing the actor
  identity, followers, posts or encryption keys.
- Synchronize direct and group chat messages, edits, deletions, reactions,
  receipts and call signalling to every linked client of the account.
- Deliver chat envelopes to a client that was offline when they were sent.
- Let every linked client ring for an incoming call, but let exactly one
  client own the call: when one client answers, the other clients stop
  ringing and cannot join the same call twice.
- Keep the relay unable to read chat content or the account private key.

## Account identity

An account is identified by its ActivityPub actor URL and its RSA signing key.
Every client of the same account must use the same RSA private key, otherwise
remote servers reject signatures from the other clients. The chat identity key
(Kyber768) must also be shared, because chat envelopes are encrypted once for
the account and delivered to every client of the account.

The identity package transferred to a new client contains:

- `identity_private_key_pem`: the RSA private key of the actor;
- `chat_identity_public_b64` and `chat_identity_secret_b64`: the Kyber768 chat
  identity key pair;
- the account configuration (username, domain, public base URL, relay
  WebSocket URL and relay token);
- the account profile and moderation settings (display name, summary, avatar
  and banner, profile fields, manually-approved followers, blocked domains and
  actors, AP relays, post delivery mode), so the linked client looks and
  behaves like the original one;
- `chat_snapshot` (QR pairing only): the current chat state of the source
  client, so the new client starts with the same threads, messages, reactions,
  receipts, pins and call signals instead of an empty history.

Per-device values (P2P peer id, local bind address, internal token, local data
directory) are not transferred. Each client keeps its own values.

### Sealed identity envelope

Identity packages are always sealed before leaving the device. The envelope is
JSON:

```json
{
  "v": 2,
  "alg": "aes-256-gcm",
  "kdf": "hkdf-sha256",
  "salt_b64": "...",
  "nonce_b64": "...",
  "ciphertext_b64": "...",
  "mac_b64": "..."
}
```

The AES-256 key is derived with HKDF-SHA256 from an independent recovery key
for an account blob, or the random QR secret for pairing, plus the random salt
and `fedi3-identity-v1` info. The recovery key is SHA-256 of
`fedi3-identity-recovery/v2:` followed by the private PEM with whitespace removed,
encoded as unpadded base64url. It is available under Security settings and must
be kept separately from the relay token. Manual login needs both values, or a
QR/backup from an existing client. Failed identity recovery never generates a
replacement identity silently.

The QR secret stays on the two clients. The relay URL contains only the
unpadded base64url SHA-256 of `fedi3-pair-id/v2:` followed by that secret.
Knowing this locator, the bearer token, and the encrypted package does not
provide its decryption key. Version 1 uploads are rejected; old stored packages
are retained for migration but not served. An updated existing client rewrites
its legacy account blob using its existing local identity. This does not rotate
keys or undo any historical exposure of an old package.

## Relay endpoints

All endpoints below are served by the relay. `Authorization: Bearer <token>`
uses the account relay token; the relay stores only its SHA-256 hash, so the
token is never readable at rest.

### Account identity blob

- `PUT /_fedi3/users/:user/identity`: store or replace the sealed identity
  envelope of the account. Body: the envelope above. Response: `204`.
- `GET /_fedi3/users/:user/identity`: return the sealed envelope or `404`.

A client uploads the envelope only when the relay has none, so the first
client of an account establishes the canonical identity. A new client
downloads it during manual login and starts the core with the transferred
keys.

### QR pairing session

- `PUT /_fedi3/pair/:code?user=:user`: store a sealed pairing package for a
  short-lived code. Response: `204`. The package is the identity envelope with
  the account configuration inside the ciphertext. Sessions expire after ten
  minutes.
- `GET /_fedi3/pair/:code`: return the sealed package without authentication
  (the locator is a hash, not the decryption secret) or `404`/`410` when missing/expired.

The already logged-in client displays a QR code containing
`fedi3://pair?relay=<relay-base>&code=<code>`. Before showing it, that client
exports its identity package (identity keys, account configuration and a chat
snapshot), seals it with the QR secret and uploads it under the hashed locator to the relay. The device that
is not logged in yet scans the QR, fetches the sealed package, decrypts it and
starts its core with the transferred identity and chat snapshot.

### Chat delivery spool

- `POST /users/:user/_fedi3/chat/inbox` (the existing chat envelope endpoint):
  the relay appends the envelope to the recipient's spool before forwarding it
  to an online client. The response is `202` even when no client is connected.
- `GET /_fedi3/chat/spool?user=:user&after=:id&limit=:n`: return the envelopes
  stored for the account after the given spool id, oldest first, as
  `{"items":[{"id":1,"envelope":{...}}],"latest_id":1}`.

Envelopes are pruned after 90 days and capped per account so a spool cannot
grow without bound. The spool stores ciphertext only.

## Core endpoints

- `GET /_fedi3/identity/export` (internal token): return
  `{"public_key_pem","private_key_pem","chat_identity_public_b64","chat_identity_secret_b64"}`.
- `GET /_fedi3/chat/snapshot` (internal token): return the chat state used by
  the QR pairing package: `{"v":1,"threads":[],"members":[],"messages":[],
  "statuses":[],"reactions":[],"pins":[],"call_signals":[],"truncated":false}`.
  Messages are capped at the newest 10,000 entries (or about 8 MB of JSON);
  `truncated` reports when older messages were left out.
- `POST /_fedi3/chat/snapshot` (internal token): import such a snapshot
  idempotently. Threads, members, messages, statuses, reactions, pins and call
  signals are merged into the local database; duplicates are ignored.
- Core start configuration accepts `identity_private_key_pem`,
  `chat_identity_public_b64` and `chat_identity_secret_b64`. When the private
  key is provided it becomes the local identity; when the chat keys are
  provided and the local database has none, they become the local chat
  identity.
- The chat bundle advertises no prekeys. Envelopes are encrypted to the
  account chat identity key, so every linked client can decrypt them. Prekeys
  remain supported for decrypting envelopes produced by older clients.

## Chat replication

Every client polls the relay chat spool with the account relay token and
applies new envelopes locally:

1. fetch envelopes after the stored cursor;
2. verify and decrypt each envelope;
3. store the payload through the same inbound path used by live delivery;
4. emit a `chat` UI event so open screens refresh;
5. advance the cursor.

The cursor is persisted per client. A newly linked client starts from the
oldest envelope still in the spool, which gives it recent history even when
the other clients are offline.

Chat payloads originated by a client are also delivered to the account's own
spool (self-sync copy), so messages sent from one client appear on every other
client of the account. This applies to messages, edits, deletions, reactions,
receipts and call signalling.

## Calls

Call invites are delivered to every linked client of the callee through the
spool. Every client rings. When one client answers, it sends an `accept`
signal; the core copies it to the account's own spool, so the other clients of
the callee see the accept and close their ringing dialog. A client that is
already in a call does not ring again. The caller keeps the first accepted
client as the call peer and ignores later accepts.

A call is therefore owned by exactly one client, while every client has a
chance to answer it.

## Limitations

- History older than the relay spool retention is available on a new client
  through the QR pairing package, which carries a chat snapshot. A manual
  login receives the spooled history only.
- Accounts without a relay (P2P-only mode) keep the existing direct delivery
  and do not use the spool; multi-device history is best effort.
- For an account created before multi-device support, the first client to start
  publishes its identity. If a brand-new client starts before the original
  client has published, use the QR pairing flow: it transfers the original
  identity and avoids a mismatched account key.

### Queue and installation isolation (2026-11-14)

Chat envelopes are schema-checked and their sender RSA signature is verified before
durable insertion. Row and byte capacity checks run in the same transaction as
insertion; a full queue rejects new requests without evicting accepted messages.
Account deletion removes identity packages, pairing sessions, chat spool and all
push registrations, with account locks preventing late writes from recreating them.

Push registrations carry `installation_id` (16–128 ASCII letters, digits, `_`, `-`).
DELETE uses the `X-Fedi3-Push-Installation` header to revoke that device only.
Legacy requests without an ID own only the legacy slot; other installations are
unchanged. Up to 32 active new installations are supported per account.

Desktop cores remain running when hidden. Android delegates to its background
service when no call holds the core. Failed background starts leave the wake
cursor unchanged so the next poll retries.

Own publications are durable data. The core records current and historical owned
actor aliases in `local_meta` and excludes them from object cache TTL, count and
byte eviction, including old records whose `actor_id` is absent. This is separate
from UI pinning and does not change the pinned state of a post.


## Account identity and social state

An account is its complete actor URL and RSA key. The same username on two relays
is two accounts. Peer IDs identify devices; the peer directory groups devices by
actor URL. Creating a second identity with an existing relay username does not
replace that account: tunnel connection and state uploads require the pinned RSA
key. A relay token alone is insufficient. Pair devices or import the existing
identity instead of creating another identity with the same username.

The core binds its data directory to the actor and public key. Existing compatible
username directories remain in place; new accounts use actor-qualified paths.
A mismatching imported key, a directory belonging to another actor, or a missing
key in an established account stops startup without replacing identity files.

Paired devices replicate profile fields, followers, following, follow requests,
owned posts and outbox entries, reactions and attachment metadata. AES-256-GCM
records are encrypted with key material derived from the shared account private
key and signed with RSA. The relay stores opaque records and exposes authenticated
GET/PUT `/_fedi3/account/{user}/state`; same-account peers also exchange records
through signed `/.fedi3/device/account` requests. Media bytes remain at their
configured storage backend; this protocol does not copy arbitrary local media
files or replace a full backup.

Records merge individually; profile fields merge independently. Offline removals
are retained as tombstones, and deleted posts cannot be resurrected by a stale
edit. A local write racing a download is captured before applying that download.
Acknowledgements clear only the exact uploaded revision. Relay cursors are scoped
to the relay URL; changing relays republishes all retained records. Failed sync
retains local state and exposes its last success/error in network settings.

Account records and tombstones have no time-based expiry. The relay rejects
writes atomically when the per-account limits are reached (default 1,000,000 rows
and 1 GiB), rather than evicting social data. Configure
`FEDI3_RELAY_ACCOUNT_MAX_ROWS` (1,000–2,000,000) and
`FEDI3_RELAY_ACCOUNT_MAX_BYTES` (1 MiB–16 GiB). Deleting an account removes its
replica and pinned key. Chat spool retention is a separate policy.

Old clients with signed forward receipts can prove the existing pinned identity
through a fresh tunnel challenge. `FEDI3_RELAY_ALLOW_LEGACY_TUNNELS=true` disables
this protection for legacy compatibility; leave it unset in normal deployments.
