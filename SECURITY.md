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

## Hardening checklist for operators

- Serve the relay only through the TLS reverse proxy; do not publish port 8787 directly.
- Set `FEDI3_RELAY_ADMIN_TOKEN` to a random value of at least 32 characters and keep it out of version control.
- Set `FEDI3_RELAY_ALLOW_SELF_REGISTER=false` unless you want open registration.
- Keep `FEDI3_RELAY_HSTS_MAX_AGE_SECS` above zero in production.
- Run `scripts/relay_smoke_test.sh` after every deployment.
- Keep `cargo audit` clean and rebuild images with `--locked`.
