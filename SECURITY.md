# Security Policy

Fedi3 is pre-1.0 software. We take security reports seriously and prefer
coordinated disclosure over public bug reports.

## Supported versions

Security fixes are developed and released on the 0.11.x line. The current
release is 0.11.9 (build 2012).

| Version line | Status |
| :--- | :--- |
| 0.11.x | **Supported** — receives security fixes |
| 0.10.x and older | Best effort — fixes only when they are low risk and backportable |

Release tags follow `vX.Y.Z` and must match `app/pubspec.yaml`. Run the latest
0.11.x release; older builds may miss security fixes.

## Reporting a vulnerability

**Do not open a public issue for a security problem.**

Use GitHub Security Advisories: open the repository's *Security* tab and choose
**Report a vulnerability**
(<https://github.com/redhunt07/Fedi3/security/advisories/new>). This creates a
private advisory visible only to the maintainer and the reporter.

If you cannot use advisories, open a normal issue that says a security report
was sent privately, without disclosing details.

Please include, when possible:

- the affected component: relay, core, app, protocol crate, P2P infrastructure
  or deployment assets;
- the version, commit or release tag, and the platform (Linux, Windows, Android,
  iOS/macOS, server);
- a minimal reproduction: request, payload, configuration or sequence;
- the impact you believe it has (impersonation, signature bypass, privacy leak,
  data loss, denial of service, token exposure, SSRF);
- whether the issue is remotely exploitable without authentication;
- relevant sanitized logs or stack traces;
- a suggested fix, if you have one.

Remove credentials before posting: `Authorization` headers, `relay_token`,
`internal_token`, `FEDI3_RELAY_ADMIN_TOKEN`, database URLs and private keys.
Tokens must never appear in core or relay logs; if one does, that is itself a
bug worth reporting.

### What to expect

| Stage | Target |
| :--- | :--- |
| Acknowledgement | 3 business days |
| Initial assessment and severity | 7 days |
| Fix or mitigation for critical issues | 30 days |
| Public advisory | after a fixed release is available |

We follow coordinated disclosure: the advisory is published together with the
fixed release, and credit is given unless you prefer to stay anonymous. If a fix
takes longer, we will tell you why and keep you updated.

## Scope

In scope:

- **Relay** (`crates/fedi3_relay`): tunnel authentication, admin endpoints,
  HTTP signature verification of federated traffic, rate limiting, media
  handling, database access, spool, relay mesh.
- **Core** (`crates/fedi3_core`): ActivityPub inbox/outbox, HTTP signature
  verification, key handling, local API authentication, chat encryption, P2P
  transport, FFI boundary.
- **Protocol** (`crates/fedi3_protocol`): signature formats, key encoding,
  DID derivation, RSA-OAEP envelopes and shared crypto helpers.
- **P2P infrastructure** (`crates/fedi3_p2p_infra`): mailbox and circuit-relay
  services, peer identity and message routing.
- **App** (`app/`): token storage in the OS keyring, handling of federated
  HTML and media, URL opening, backup format, permission handling.
- **Deployment** (`deploy/`, `docker-compose.yml`, scripts): TLS termination,
  container hardening, secret handling, backup and restore paths.

Out of scope:

- vulnerabilities in upstream dependencies (report them upstream; we track them
  with `cargo audit`);
- denial of service that requires an unreasonable amount of resources;
- issues that only affect development builds or debug tooling;
- visual design and cosmetic UI defects;
- moderation decisions and relay policy, unless they cause a security problem.

## Security model (short)

- Each user instance runs a **local core** bound to `127.0.0.1` (or an
  explicitly configured interface), protected by a random `internal_token` sent
  as `X-Fedi3-Internal`.
- The core exposes the user's actor to the fediverse **through the relay
  tunnel**, authenticated by a per-user bearer token.
- Federated inbound requests must carry a valid **HTTP Signature**; the
  signature must be bound to the activity `actor`.
- Private keys live in the core data directory with restrictive permissions; the
  relay never sees them.
- Chat v1 derives per-message AES-256-GCM keys with HKDF-SHA256 from Kyber768
  shared secrets and authenticates envelopes with RSA-SHA256 signatures. There
  is no full ratcheting or forward-secrecy guarantee. RSA-OAEP wraps keys for
  compatibility HTTP envelopes and offline relay spool, not the chat KEM.
- The relay stores tokens as SHA-256 hashes; admin access requires a bearer token
  of at least 32 characters.
- **Identity anchor (optional)**: the actor `id`/`url`/key can live on a separate
  static host (`anchor_actor_url`), while inbox/outbox stay on the relay. The
  relay redirects `GET /users/<user>` to the canonical actor and its WebFinger
  points there. The anchor host serves only public documents; the private key
  never leaves the core.
- **Blind spool**: activities waiting for an offline core are stored encrypted
  (AES-256-GCM with a one-time key wrapped by RSA-OAEP-SHA256 under the user's
  public key). The relay forwards ciphertext envelopes to
  `POST /_fedi3/spool/envelope`; the core alone decrypts, rebuilds the signed
  request and verifies it. If the public key is not cached yet, the relay falls
  back to plaintext spool for that item.
- **Moderation enforcement**: relay-wide blocks (`/admin/blocks`) and per-user
  blocks pushed by the core with the relay token are applied on the inbox path;
  a blocked sender gets `202` and is neither forwarded nor spooled.
- Outbound federation fetches (actor documents, keys, objects, outbox pages,
  peer relays) go through a shared policy: only `http`/`https`, at most 3–5
  redirects, and a DNS check that rejects private, loopback, link-local,
  unique-local, CGNAT and documentation addresses. Bodies are streamed with a
  hard cap (256 KiB for actor/key metadata, 2 MiB for objects and pages). The
  escape hatches `FEDI3_ALLOW_PRIVATE_ACTOR_FETCH` (core) and
  `FEDI3_ALLOW_PRIVATE_REMOTE_FETCH` (relay) are off by default and exist only
  for loopback/LAN deployments.

## Hardening checklist for operators

- Serve the relay only through the TLS reverse proxy; do not publish port 8787
  directly.
- Set `FEDI3_RELAY_ADMIN_TOKEN` to a random value of at least 32 characters and
  keep it out of version control.
- Set `FEDI3_RELAY_ALLOW_SELF_REGISTER=false` unless you want open registration.
- Keep `FEDI3_RELAY_HSTS_MAX_AGE_SECS` above zero in production.
- Run `scripts/relay_smoke_test.sh` after every deployment.
- Keep `cargo audit` clean and rebuild images with `--locked`.
- Leave `FEDI3_ALLOW_PRIVATE_ACTOR_FETCH` / `FEDI3_ALLOW_PRIVATE_REMOTE_FETCH`
  off unless the instance legitimately talks to a LAN peer; they disable the
  SSRF address check.
- Back up the core data directory (private keys) separately from relay backups.

### Known residual risk (identity anchor and spool)

- The anchor is a static host: if it is unreachable, remote servers cannot fetch
  the actor document or the key, even though the relay and the core are healthy.
  Host it on reliable static infrastructure; the identity itself remains
  portable (update the two files and the inbox URL).
- The relay sees the plaintext body when it first receives an activity (it must
  route and check the signature); encryption protects the spool at rest and the
  flush path, not the ingress instant.
- The extended Date skew for spooled envelopes relies on the unsigned
  `X-Fedi3-Spooled-Skew-Secs` header, capped at seven days. A sender can always
  use it, but the signature is still verified against the signed headers.

### Outbound destination enforcement (SSRF)

Federation clients validate DNS answers inside the HTTP connector, including
redirect destinations, and connect using those exact addresses. Mixed
public/private answers fail closed. Literal private redirect targets are
rejected before connection. Automatic redirects remain bounded; explicit GET
redirects also drop signatures and credentials. Ambient HTTP proxies are
disabled for federation so they cannot bypass this resolver. Trusted
operator-configured storage/search clients are separate; Meilisearch redirects
are disabled. The explicit LAN escape hatches still disable address filtering.
This is destination filtering, not a sandbox for a compromised host.

## Dependency audit — 8 November 2026

The two remaining vulnerability findings from the 7 November audit were removed
from the release dependency graph, without ignoring either advisory:

- RSA operations now use AWS-LC through `fedi3_protocol::crypto`. The vulnerable
  RustCrypto `rsa` dependency is absent. Existing PKCS#8/SPKI PEM files, derived
  DIDs, PKCS#1 v1.5 SHA256 signatures and OAEP SHA256/MGF1-SHA256 envelopes
  retain their original formats. Existing identity files are validated, never
  rewritten or regenerated on a parse error.
- WebRTC uses 0.21.1 with its AWS-LC provider. The previous `webrtc-dtls` and
  `crypto-bigint` 0.5.5 graph is absent. The remaining `crypto-bigint` version is
  patched 0.7.10, used by the S3 signing stack.

`cargo audit` reports **zero vulnerabilities**, with an empty ignore list. Five
unmaintained-dependency notices remain (paste, pqcrypto-internals,
pqcrypto-kyber, pqcrypto-traits and rustls-pemfile); these do not fail the
current audit gate. The existing chat KEM is retained to preserve stored
messages and peer compatibility.

Compatibility is tested against fixed vectors produced by RSA 0.9.10, an old
encrypted v1 message, byte-identical profile public keys/DIDs and an isolated old
RSA/WebRTC peer in both SDP roles. The test oracle's disposable project is
outside the release graph and uses only public fixture keys and local test
sessions. CI runs these interoperability tests and requires a passing audit for
release tags. No security check or advisory has been suppressed.

See [the migration verification](docs/audit/crypto_compatibility_2026-11-08.md)
for evidence and platform limits. A clean dependency audit is not proof that no
implementation vulnerability exists.
