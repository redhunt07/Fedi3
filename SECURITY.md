# Security Policy

## Supported versions

FEDI3 is pre-1.0. Only the latest release receives security fixes.

| Version | Supported |
|---|---|
| 0.1.x | ✅ |
| < 0.1 | ❌ |

## Reporting a vulnerability

**Please do not open a public issue for a security problem.**

Use GitHub Security Advisories ("Report a vulnerability" in the *Security* tab of the repository), or email the maintainer listed in `GOVERNANCE.md`.

Include, when possible:

- affected component (relay, core, app) and version/commit;
- a minimal reproduction (request, config, payload);
- the impact you believe it has (impersonation, data loss, DoS, privacy leak);
- whether the issue is remotely exploitable without authentication.

### What to expect

| Stage | Target |
|---|---|
| Acknowledgement | 72 hours |
| Initial assessment and severity | 7 days |
| Fix or mitigation for critical issues | 30 days |
| Public advisory | after a fixed release is available |

We follow coordinated disclosure: the advisory is published together with the fixed release, and credit is given unless you prefer to stay anonymous.

## Scope

In scope:

- **Relay** (`crates/fedi3_relay`): authentication of tunnels and admin endpoints, HTTP signature verification of federated traffic, rate limiting, media handling, spool, database access.
- **Core** (`crates/fedi3_core`): ActivityPub inbox/outbox, HTTP signature verification, key handling, local API authentication, chat encryption, P2P transport, FFI boundary.
- **App** (`app/`): local token storage, handling of federated HTML/media, URL opening, backup format.
- **Deployment** (`deploy/`, `docker-compose.yml`): TLS termination, container hardening, secret handling.

Out of scope:

- vulnerabilities in upstream dependencies (report them upstream; we track them with `cargo audit`);
- denial of service that requires an unreasonable amount of resources;
- issues that only affect development builds;
- the visual design.

## Security model (short)

- Each user instance runs a **local core** bound to `127.0.0.1` (or an explicitly configured interface), protected by a random `internal_token` sent as `X-Fedi3-Internal`.
- The core exposes the user's actor to the fediverse **through the relay tunnel**, authenticated by a per-user bearer token.
- Federated inbound requests must carry a valid **HTTP Signature**; the signature must be bound to the activity `actor`.
- Private keys live in the core data directory with restrictive permissions; the relay never sees them.
- Chat uses per-message AES-256-GCM keys wrapped with RSA-OAEP; this provides confidentiality and integrity but **not forward secrecy** (documented limitation).
- The relay stores tokens as SHA-256 hashes; admin access requires a bearer token of at least 32 characters.
- **Identity anchor (optional)**: the actor `id`/`url`/key can live on a separate static host (`anchor_actor_url`), while inbox/outbox stay on the relay. The relay redirects `GET /users/<user>` to the canonical actor and its WebFinger points there. The anchor host serves only public documents; the private key never leaves the core.
- **Blind spool**: activities waiting for an offline core are stored encrypted (AES-256-GCM with a one-time key wrapped by RSA-OAEP-SHA256 under the user's public key). The relay forwards ciphertext envelopes to `POST /_fedi3/spool/envelope`; the core alone decrypts, rebuilds the signed request and verifies it. If the public key is not cached yet, the relay falls back to plaintext spool for that item.
- **Moderation enforcement**: relay-wide blocks (`/admin/blocks`) and per-user blocks pushed by the core with the relay token are applied on the inbox path; a blocked sender gets `202` and is neither forwarded nor spooled.
- Outbound federation fetches (actor documents, keys, objects, outbox pages, peer relays) go through a shared policy: only `http`/`https`, at most 3–5 redirects, and a DNS check that rejects private, loopback, link-local, unique-local, CGNAT and documentation addresses. Bodies are streamed with a hard cap (256 KiB for actor/key metadata, 2 MiB for objects and pages). The escape hatches `FEDI3_ALLOW_PRIVATE_ACTOR_FETCH` (core) and `FEDI3_ALLOW_PRIVATE_REMOTE_FETCH` (relay) are off by default and exist only for loopback/LAN deployments.

## Hardening checklist for operators

- Serve the relay only through the TLS reverse proxy; do not publish port 8787 directly.
- Set `FEDI3_RELAY_ADMIN_TOKEN` to a random value of at least 32 characters and keep it out of version control.
- Set `FEDI3_RELAY_ALLOW_SELF_REGISTER=false` unless you want open registration.
- Keep `FEDI3_RELAY_HSTS_MAX_AGE_SECS` above zero in production.
- Run `scripts/relay_smoke_test.sh` after every deployment.
- Keep `cargo audit` clean and rebuild images with `--locked`.
- Leave `FEDI3_ALLOW_PRIVATE_ACTOR_FETCH` / `FEDI3_ALLOW_PRIVATE_REMOTE_FETCH` off unless the instance legitimately talks to a LAN peer; they disable the SSRF address check.

### Known residual risk (identity anchor and spool)

- The anchor is a static host: if it is unreachable, remote servers cannot fetch the actor document or the key, even though the relay and the core are healthy. Host it on reliable static infrastructure; the identity itself remains portable (update the two files and the inbox URL).
- The relay sees the plaintext body when it first receives an activity (it must route and check the signature); encryption protects the spool at rest and the flush path, not the ingress instant.
- The extended Date skew for spooled envelopes relies on the unsigned `X-Fedi3-Spooled-Skew-Secs` header, capped at seven days. A sender can always use it, but the signature is still verified against the signed headers.

### Known residual risk (SSRF)

The address check runs before the request. A redirect to a *hostname* that resolves to a private address is not re-evaluated (reqwest resolves it internally); redirects to literal private IPs are rejected by the redirect policy, and the number of hops is capped. This is an accepted trade-off, comparable to other ActivityPub implementations.
