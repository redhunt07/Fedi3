# ActivityPub Compatibility Matrix

This document distinguishes between `normative`, `interop` and `optional` requirements for Fedi3.

## Normative
- `ActivityPub 2018`: actor discovery, inbox/outbox, followers/following, ordered collections.
- `Activity Streams 2.0`: object/activity serialization and content negotiation.
- `WebFinger / RFC 7033`: `acct:user@domain` and actor URL discovery.
- `NodeInfo`: `2.1` preferred, `2.0` kept for backward compatibility.

## Interop
- `application/activity+json` and `application/ld+json; profile="https://www.w3.org/ns/activitystreams"`.
- Legacy HTTP Signature header with `hs2019` marker tolerated.
- `Digest: SHA-256=...` and date window compatible with common federated peers.
- Actor documents with `followers`, `following`, `outbox`, `sharedInbox`, `featured`, `featuredTags`.
- WebFinger strict in production; tolerance only for local/dev environments.

## Optional
- Future support for `HTTP Message Signatures` as an evolution path separate from the legacy path.
- Extension of the interop matrix beyond Mastodon towards further software.
