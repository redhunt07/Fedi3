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
  "v": 1,
  "alg": "aes-256-gcm",
  "kdf": "hkdf-sha256",
  "salt_b64": "...",
  "nonce_b64": "...",
  "ciphertext_b64": "...",
  "mac_b64": "..."
}
```

The AES-256 key is derived with HKDF-SHA256 from the shared secret (the relay
token for the account blob, the pairing code for a QR pairing session), the
random salt and the info string `fedi3-identity-v1`. The relay only stores the
envelope and cannot decrypt it.

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
  (the code is a 128-bit secret) or `404`/`410` when missing/expired.

The desktop client displays a QR code containing
`fedi3://pair?relay=<relay-base>&code=<code>`. The already logged-in mobile
client scans it, seals its identity package with the code and uploads it. The
desktop polls the relay, decrypts the package and starts its core with the
transferred identity.

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
