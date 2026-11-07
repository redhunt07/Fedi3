# Compatible RSA and WebRTC remediation — 8 November 2026

The release dependency graph no longer contains the two cryptographic
vulnerabilities identified on 7 November. This work preserves existing profiles,
keys, identities and message formats; it does not migrate or overwrite user data.

## RSA provider replacement

RSA key generation, import, signing, verification, encryption and decryption now
use AWS-LC through `fedi3_protocol::crypto`. Core and relay no longer depend on
RustCrypto `rsa`, including their test helpers. This removes
[RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html) by replacing
the affected implementation, not by claiming the unpatched crate is fixed.

The formats remain PKCS#8 private PEM, SPKI public PEM, PKCS#1 v1.5 SHA256
signatures and RSA-OAEP with SHA256, MGF1-SHA256 and the empty label. The hybrid
AES-256-GCM envelope, key filenames, normalization used for DIDs, JSON field
names and ActivityPub signature headers are unchanged. Existing key files are
read/validated without rewriting; corrupt identities fail without regeneration.
The existing chat KEM and its stored keys are unchanged.

Evidence:

- Public test-only fixtures produced by RSA 0.9.10 verify the original signature,
  decrypt the original OAEP ciphertext and yield byte-identical public PEM and DID.
- A complete old encrypted v1 message decrypts into its original body; tampered
  ciphertext is rejected without treating it as plaintext.
- A disposable RSA 0.9.10 oracle decrypts new envelopes and verifies new signatures.
- Repeated profile loading preserves the private file byte-for-byte. A corrupt
  key cannot trigger replacement with a new identity.

## WebRTC transport replacement

The optional Rust data-channel transport now uses WebRTC 0.21.1 with its AWS-LC
provider. The old WebRTC 0.12/DTLS implementation and `crypto-bigint` 0.5.5 are
absent from the release graph. The S3 signing stack retains patched
`crypto-bigint` 0.7.10. This removes
[RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html) without
forcing an incompatible transitive version or disabling WebRTC.

An internal adapter preserves the existing manager's SDP/trickle ICE interface,
`fedi3` ordered reliable channel, v1 binary framing and relay signaling payloads.
The new driver handles DTLS/SCTP and bounded send buffering. Reader tasks and
callback cycles are released on shutdown. A canceled channel-open notification
is treated as a failed connection, not as a successful open.

Evidence:

- Two current peers complete DTLS/SCTP and exchange a 32,000-byte binary payload.
- Closing peers releases the channel reader rather than retaining it indefinitely.
- Serialized candidate/SDP fields match legacy JSON.
- An isolated WebRTC 0.12 peer interoperates with the new transport as both SDP
  offerer and answerer and echoes unchanged Fedi3 v1 binary frames.

`python3 scripts/verify_legacy_webrtc.py` builds the old RSA/WebRTC oracle in a
disposable project with its own lockfile, outside the workspace/release graph.
Only intentionally public fixture keys and local test peers are used. CI runs
both compatibility tests explicitly; production never falls back to the old code.

## Audit and release checks

`cargo audit` reports **0 vulnerabilities**, an empty ignore list and five
non-blocking unmaintained notices: paste, pqcrypto-internals, pqcrypto-kyber,
pqcrypto-traits and rustls-pemfile. The existing KEM is retained for compatibility;
a future KEM change requires a versioned protocol, not silent replacement.

The audit remains mandatory for release tags. A release contract regression also
rejects reintroduction of `rsa`, old `webrtc-dtls` or vulnerable `crypto-bigint`.
No advisory or security failure has been ignored. Current tag `v0.10.0` matches
the prepared app version; no tag or release was published during verification.

| Verification | Result |
| --- | --- |
| Workspace Rust tests, locked, four test threads | 303 passed, zero failures; four opt-in tests run explicitly below |
| Legacy RSA 0.9.10 oracle | New OAEP envelopes decrypted and new signatures verified |
| Legacy WebRTC 0.12 oracle | Both SDP roles passed; v1 binary frames unchanged |
| PostgreSQL atomic quota, isolated temporary database | Passed |
| Public IPFS/P2P bootstrap | Passed |
| All distinct Rust tests including the four explicit tests | **307 passed** |
| Flutter tests | **184 passed** |
| Strict Clippy, all targets | Passed |
| Flutter analyzer | No issues |
| Linux release core library | Built successfully |
| Linux release relay binary | Built successfully |
| Flutter Linux release | Built successfully |
| Android ARM64 core, NDK 28.2, `cargo check` | Passed |
| Deploy/OpenAPI, relay directory, forward receipt, backup and release-contract checks | Passed |
| Platform configuration verifier, Rust formatting, diff whitespace checks | Passed |
| Release tag contract with `FEDI3_RELEASE_TAG=v0.10.0` | Passed |
| RustSec audit | **Exit 0; zero vulnerabilities; no ignored advisories** |

The RSA/WebRTC security blockers are cleared and the local release checks pass.
The remote tag workflow and native Windows/Apple runners have not been executed
in this session; their platform validation remains part of publication.


## Limits

The native release builds were verified on Linux. Android was cross-checked with
the installed NDK for compilation; this is not an on-device call test. Windows,
macOS and iOS native builds and the remote tag workflow still need their normal
platform runners. User profiles, production services and deployed clients were
not modified. The historical 7 November report remains available with a link to
this remediation.
