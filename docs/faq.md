# FAQ

## Is the relay a complete social server?

No. The relay provides ActivityPub compatibility (actor, inbox/outbox, webfinger, nodeinfo) and routing to users' cores through the tunnel. It does not hold private keys and is not a Mastodon instance: it is a bridge.

## Can I use only P2P without a relay?

Yes. The core also starts without a relay: the configuration accepts an empty `relay_ws` and the tunnel is simply disabled. In this mode you have chat and P2P synchronization, but you are **not** reachable from traditional fediverse instances, which only speak ActivityPub over HTTPS. For federation with Mastodon/Misskey you need a relay (your own or public).

## Who sees my relay token?

Nobody except the relay. The client sends it as a bearer token on the tunnel; the relay keeps only its SHA-256 hash. The token is never written to the logs.

## Where are the private keys stored?

In the core data directory, with `0600` permissions on POSIX. They are never sent to the relay: signing happens locally. Make a separate backup of that directory if you want to be able to migrate the account.

## Is the chat encrypted?

Yes: AES-256-GCM with per-message keys, wrapped with RSA-OAEP for each recipient. Known limit: there is no forward secrecy. If the private key is compromised, already exchanged messages become readable.

## Telemetry?

It is optional and can be enabled by the user. The relay collects only aggregated data (number of users, peers, version) and local logs can be deleted.

## Where is media stored?

On the relay: local backend, S3 or WebDAV, configurable with `FEDI3_RELAY_MEDIA_BACKEND`. The core keeps a local cache with space limits and TTL.

## Why are some federated requests rejected with 401?

Since 0.1.1 the relay and the core mandatorily verify the HTTP signature on every federated POST, require a valid `Digest` and reject activities signed by an actor different from the one declared. If a remote instance does not sign correctly, it is rejected: this is the correct behavior, not a bug.

## Advanced P2P/ICE infrastructure?

Optional stacks are available in `deploy/p2p_infra` and `deploy/turn` (coturn for NAT traversal).

## Can I expose the relay port directly?

Better not. Put the relay behind a TLS reverse proxy and leave `FEDI3_RELAY_BIND=0.0.0.0:8787` reachable only from the internal Docker network. If you expose the port, enable TLS anyway and set `FEDI3_RELAY_TRUST_PROXY_HEADERS` accordingly.

## How do I make a backup?

See [ops_backup_restore.md](ops_backup_restore.md). The scripts read the configuration from `.env`, verify the archive and apply a retention. The app backup exports only configuration and preferences, not the private key.
