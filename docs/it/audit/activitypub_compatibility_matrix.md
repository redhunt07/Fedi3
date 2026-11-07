# ActivityPub Compatibility Matrix

Questo documento distingue tra requisiti `normative`, `interop` e `optional` per Fedi3.

## Normative
- `ActivityPub 2018`: actor discovery, inbox/outbox, followers/following, ordered collections.
- `Activity Streams 2.0`: object/activity serialization e content negotiation.
- `WebFinger / RFC 7033`: `acct:user@domain` e actor URL discovery.
- `NodeInfo`: `2.1` preferito, `2.0` mantenuto per backward compatibility.

## Interop
- `application/activity+json` e `application/ld+json; profile="https://www.w3.org/ns/activitystreams"`.
- Legacy HTTP Signature header con marker `hs2019` tollerato.
- `Digest: SHA-256=...` e date window compatibile con peer federati comuni.
- Actor documents con `followers`, `following`, `outbox`, `sharedInbox`, `featured`, `featuredTags`.
- WebFinger rigoroso in produzione; tolleranza solo per ambienti locali/dev.

## Optional
- Supporto futuro a `HTTP Message Signatures` come percorso evolutivo separato dal path legacy.
- Estensione della matrice interop oltre Mastodon verso software ulteriori.
