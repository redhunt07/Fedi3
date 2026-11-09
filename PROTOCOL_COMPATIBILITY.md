# Fedi3 Protocol Compatibility

Protocol compatibility is what keeps the network unified. Fedi3 implements
ActivityPub as its baseline and treats every extension as optional.

## Baseline

- ActivityPub actor documents, inbox/outbox, WebFinger and NodeInfo.
- Mandatory HTTP signature verification on federated requests, with a valid
  `Digest` and a signature bound to the declared `actor`.
- Standard media types and object representations that classic instances can
  parse.
- Relay-to-relay peering and note sync over HTTP; there is no separate libp2p
  relay mesh runtime.

## Extensions

- P2P transport, mailbox routing and direct sync are optional and documented.
- A peer that does not implement an extension must still be reachable through
  the standard relay path.
- Extensions must be negotiated or safely ignored, never assumed.
- Extension payloads must not break parsers that only understand ActivityPub.

## Versioning and change management

- Breaking protocol changes are versioned and documented.
- Deprecations are announced before removal, with a migration path.
- Capability negotiation is preferred over hard requirements.
- Protocol changes require tests against both Fedi3 peers and classic
  ActivityPub instances, and an entry in `CHANGELOG.md` and `ANNOUNCEMENTS.md`.

## Testing and evidence

- CI runs the workspace tests, the platform configuration checks and the
  compatibility vectors for signatures and crypto formats.
- Interoperability is verified against real Mastodon and Misskey instances in
  both directions where possible; see `docs/interop.md` and the
  [compatibility matrix](docs/audit/activitypub_compatibility_matrix.md).
- A compatibility claim must be backed by a test or a reproducible manual check,
  not by intent.

## Non-goals

- Forcing every instance to implement Fedi3 extensions.
- Replacing ActivityPub with a private protocol.
- Silently changing the meaning of existing fields.

See [NETWORK_INTEGRITY.md](NETWORK_INTEGRITY.md) for the network-level
commitments.
