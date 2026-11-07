# ActivityPub Compatibility Report

Initial snapshot of the ActivityPub hardening work.

## Pass
- Core and relay expose `/.well-known/webfinger`, `/.well-known/nodeinfo`, `host-meta`, actor, inbox/outbox and collections.
- Core and relay support `application/activity+json` and `application/ld+json` on the main paths.
- `NodeInfo 2.1` added while keeping `2.0`.
- WebFinger made strict in production and tolerant only in local/dev environments.
- Relay actor stubs realigned with `followers`, `following`, `sharedInbox`, `featured`, `featuredTags`.
- Signature parsing extended to tolerate `hs2019`, `created`, `expires`.

## Known Deviations
- The legacy `Signature` path remains the main mechanism; `HTTP Message Signatures / RFC 9421` is not yet implemented end-to-end.
- The core remains focused on RSA keys for federated verification.
- The interop gate with containerized Mastodon has not yet been added as a dedicated CI job.

## Next Verification
- `cargo test --workspace`
- ActivityPub/WebFinger/NodeInfo smoke test on core and relay
- real interop with Mastodon for follow, create, reply, delete, update, undo
