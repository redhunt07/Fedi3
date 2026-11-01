# ActivityPub Compatibility Report

Snapshot iniziale del lavoro di hardening ActivityPub.

## Pass
- Core e relay espongono `/.well-known/webfinger`, `/.well-known/nodeinfo`, `host-meta`, actor, inbox/outbox e collections.
- Core e relay supportano `application/activity+json` e `application/ld+json` nei path principali.
- `NodeInfo 2.1` aggiunto mantenendo `2.0`.
- WebFinger reso rigoroso in produzione e tollerante solo in ambienti locali/dev.
- Actor stubs del relay riallineati con `followers`, `following`, `sharedInbox`, `featured`, `featuredTags`.
- Signature parsing esteso per tollerare `hs2019`, `created`, `expires`.

## Known Deviations
- Il path legacy `Signature` resta il meccanismo principale; `HTTP Message Signatures / RFC 9421` non è ancora implementato end-to-end.
- Il core resta focalizzato su chiavi RSA per la verifica federata.
- Il gate interop con Mastodon containerizzato non è ancora aggiunto come job dedicato in CI.

## Next Verification
- `cargo test --workspace`
- smoke test ActivityPub/WebFinger/NodeInfo su core e relay
- interop reale con Mastodon per follow, create, reply, delete, update, undo
