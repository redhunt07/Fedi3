# Fedi3 Network Integrity

Fedi3 is a network, not a single product. Its value depends on remaining
interoperable, decentralized and predictable for every participant.

## What integrity requires

- **ActivityPub as the baseline.** Actor documents, inbox/outbox, WebFinger and
  NodeInfo must work with classic fediverse software. Federated ingress requires
  a valid HTTP signature bound to the declared actor and a valid `Digest`.
- **Optional P2P extensions.** P2P transport, mailbox routing and direct sync
  must be additive. A core without a relay, or a peer that does not implement an
  extension, must still federate through the standard path.
- **Replicable relays.** Public relays are replaceable and discoverable through
  the shared directory. No relay is a mandatory chokepoint, and the relay mesh
  uses plain HTTP peering rather than a proprietary runtime.
- **Stable identities.** Actor IDs, keys and handles should survive relay moves
  where possible, using documented migration fields instead of silent rewrites.
- **Honest versioning.** Protocol changes are announced, versioned and tested
  before they reach users.

## What weakens the network

- Private extensions required for basic federation.
- Clients that only work with one relay implementation.
- Relays that silently rewrite actor identities or drop signatures.
- Forks that diverge without documenting the divergence.
- Centralized directories that become gatekeepers instead of convenience.

## Responsibilities

- **Operators:** serve valid TLS, keep `GET /healthz` truthful, keep software
  updated, publish a contact for token requests and do not alter federated
  content silently.
- **Client developers:** verify signatures, respect blocks and privacy settings,
  handle remote HTML safely and degrade gracefully when a relay is unreachable.
- **Protocol changes:** document the behavior, add compatibility tests and
  announce the change in `CHANGELOG.md` and `ANNOUNCEMENTS.md`.

Fragmentation weakens decentralization. Network integrity is a technical and
ethical commitment, not a slogan.
