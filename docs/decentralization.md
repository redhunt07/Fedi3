# Decentralization: identity, transport and trust

This document describes the mechanisms introduced to make Fedi3 **replaceable** in its central parts: identity no longer belongs to the relay, the relay no longer reads the spool, relays can be more than one and moderation can be applied before traffic reaches the core.

The realistic goal is not "zero servers": a switched-off device needs someone to keep the messages. The goal is **no mandatory server and no irreplaceable server**.

## 1. Identity separated from transport (anchor)

With `anchor_actor_url` the core publishes a canonical identity on a static domain:

- the actor `id`, `url` and the key (`#main-key`) live on the anchor;
- `inbox`, `outbox`, `followers`, `following` stay on the relay (they are dynamic resources);
- the relay redirects `GET /users/<user>` to the anchor and the relay WebFinger points to the canonical actor;
- the core writes the static documents to `<data_dir>/anchor/actor.json` and `<data_dir>/anchor/webfinger` and exposes them on `GET /_fedi3/anchor`.

### Why it works with Mastodon

ActivityPub does not require `inbox` to be on the same host as the `id`. A remote server:

1. resolves `@alice@anchor.example` on the anchor's static WebFinger;
2. downloads the actor from the anchor (static document, with the public key);
3. delivers to the `inbox` indicated in the actor, which is on the relay;
4. verifies the signature with the anchor's key.

The relay remains a **chosen transport provider**: changing it means updating the `inbox` in the static document, without `Move` and without changing the identity.

### Publishing the anchor

1. Start the core with `anchor_actor_url` set (e.g. `https://alice.example`).
2. Copy `<data_dir>/anchor/actor.json` to `https://alice.example/users/alice`.
3. Copy `<data_dir>/anchor/webfinger` to `https://alice.example/.well-known/webfinger`.
4. Make sure TLS is valid and the files are served with `Content-Type: application/activity+json` (actor) and `application/jrd+json` (webfinger).

Any static host is fine (GitHub Pages, Netlify, Cloudflare Pages, an S3 bucket). If the key rotates, regenerate the two files.

> Note: object and activity `id`s remain on the relay, so every object is dereferenceable even if the anchor is a purely static host.

## 2. Blind spool (the relay does not read pending messages)

When a core is offline the relay keeps incoming requests. Before this change it did so in plaintext; now:

1. the relay reads the user's public key from the actor cache;
2. it generates a one-time AES-256-GCM key, encrypts the body and the key with RSA-OAEP-SHA256;
3. it stores only the ciphertext and metadata in the database (original signed headers, encryption mark, length);
4. on reconnection it sends the core an **envelope** on `POST /_fedi3/spool/envelope` with the ciphertext;
5. the core decrypts with the private key (which never leaves the device), rebuilds the original request with the signed headers and processes it.

Consequences:

- the relay database (and its backups) do not contain the text of pending messages;
- the relay no longer has to decrypt at flush: it forwards only ciphertext;
- the `Date` validity window is extended by the time spent in the spool (maximum 7 days), through the unsigned header `X-Fedi3-Spooled-Skew-Secs`;
- if the public key is not yet in cache (first contact), the relay falls back to the plaintext spool.

Declared limit: the relay sees the body **at the moment of receipt** (it must route it and verify the signature). Encryption protects data at rest and the flush path, not the entry instant.

## 3. Relay mesh

Every relay can be connected to peer relays:

- `POST /admin/peers {"url": "https://relay-b.example"}` adds a peer;
- every relay exposes its directory on `GET /_fedi3/relay/users`;
- a periodic task (`FEDI3_RELAY_PEER_SYNC_SECS`, default 60) synchronizes the peer directory into `remote_users`;
- a request for a non-local user is forwarded to the home relay, with a hop budget (`FEDI3_RELAY_PEER_MAX_HOPS`, default 3) and the `X-Fedi3-Hops` header to avoid loops.

This allows hosting users on different relays and having multiple interchangeable relays: the fall of one relay does not eliminate the network, and a user can migrate without changing identity (thanks to the anchor).

## 4. Moderation

Moderation is applied **on the relay, before the core**:

- relay-wide blocklist managed by the admin (`/admin/blocks`, types `actor` and `domain`, with optional expiry);
- per-user blocklist sent by the core with the relay token (`PUT /_fedi3/relay/user_blocks/<user>`, header `X-Fedi3-Relay-Token`);
- application on the shared `/inbox` and on `/users/<user>/inbox`: a blocked sender receives `202` and is neither forwarded nor put in spool;
- the core synchronizes its blocklist to the relay at startup and on every change (`/_fedi3/blocks`).

Why it matters: for the fediverse the relay is **a single instance** with many users. Blocking an entire relay is costly; with per-user blocklists and small, plural relays, a block hits a narrow group.

## 5. Trust model

| Actor | What it sees | What it does not see |
|---|---|---|
| Relay | routing metadata, body at the moment of receipt, hosted media, spool ciphertext | private key, spool plaintext at rest, E2E chat content |
| Anchor (static host) | only the public documents | everything else |
| Core | everything concerning the user | — |

The relay remains a trust point for: availability, media, relay-wide moderation and metadata. The mitigation is not to eliminate it, but to make it **replaceable and transparent**.

## 6. Residual risks and next steps

- **Mobile**: the core lives in the app process; on Android/iOS the system can
  suspend the app. Push provisioning through the relay and a configurable spool
  retention are implemented, but OS delivery remains discretionary.
- **Redirects and SSRF**: the network guard resolves and checks the exact
  addresses used by the connector, rejects private, loopback and link-local
  targets, and bounds redirects. See the outbound destination enforcement section
  in `SECURITY.md`.
- **Replication of public content**: public notes still live on the relay outbox.
  Signed gossipsub on `/fedi3/global/1` already replicates public activity between
  connected cores, but it is best-effort and not a complete historical
  replication; the relay remains the bootstrap and the durable store.
- **Community TURN**: symmetric NAT needs pools of TURN relays managed by the
  community, not a single centralized service. The relay can publish a private
  operator pool, but no operator is activated by default.
- **Accountability**: `/transparency` and the signed forward journal exist, with
  independently verifiable receipts. Independent witnesses and automatic
  cross-relay checkpoint comparison are still missing.

## Related documents

- [p2p_network.md](p2p_network.md) — discovery, DHT and direct transport.
- [network_resilience.md](network_resilience.md) — spool, TURN, push and transparency.
- [push_forward_receipts.md](push_forward_receipts.md) — receipt formats and verification.
- [interop.md](interop.md) — real Mastodon/Misskey tests of the anchored identity.
