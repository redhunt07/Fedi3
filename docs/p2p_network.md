# Fedi3 network and public infrastructures

## Implemented discovery

The core uses four independent sources: contacts stored on the device,
configurable Fedi3 bootstraps, mDNS on the LAN and discovery on the public IPFS DHT.
Connected Fedi3 clients also exchange signed announcements through
`/fedi3/peers/1`. No VPN is required.

The file `p2p_known_peers.json`, in the profile data directory, contains up to
256 peers with addresses and the date of the last successful connection. It is written
atomically every minute and at shutdown, with 0600 permissions on Unix. Contacts
expire after seven days. Reconnection tries at most four peers every
30 seconds, with backoff up to 30 minutes and suspension of automatic attempts
when there are already at least 32 connections. This does not limit the connections
needed for internal Kademlia queries or user requests.

The exchange transmits at most 16 announcements per message and 64 KiB per frame.
Each announcement binds Peer ID, addresses and timestamp to the node's libp2p key;
it expires after 24 hours. A contact received from third parties is reshared only after a
connection to the announced node. LAN, loopback, DNS and reserved addresses are not
propagated via PEX; configured bootstraps and contacts learned
directly can instead use DNS and LAN addresses.

The announcements prove the Peer ID check, not ownership of an
ActivityPub account. They do not contain accounts, chats, credentials or private keys. Public
addresses and the Peer ID are visible to discovery participants.

## Public IPFS as support

The client contacts four bootstraps published by the IPFS documentation,
using DNSADDR to follow their current addresses. Verification of the
RSA identities used by the bootstraps is also enabled. The DHT protocols remain separate:

- `/fedi3/kad/1`: Fedi3 application records, Peer ID and DID;
- `/ipfs/kad/1.0.0`: client of the public DHT, without Fedi3 application records.

On the public DHT the client publishes and looks for **provider records** for the SHA-256
multihash of `fedi3/network/providers/v1`. This lets new Fedi3 clients
meet without a directory on our relay. It is a public identifier
of the network service, not the content of a conversation. The provider record
is a candidacy for connection: the protocol and node identity are
verified during the connection.

If a node encountered offers Circuit Relay v2, the client can try up to two
public reservations. They are opportunistic: capacity, duration and availability
depend on the operator. IPFS circuit relays mainly help
transition to direct connections, not to offer unlimited media bandwidth.

The [IPFS utilities](https://docs.ipfs.tech/concepts/public-utilities/) are
explicitly offered best effort, with no guarantee for the critical production
path. They complement the Fedi3 network: they do not replace independent bootstraps
and contact memory. The network grows when new Fedi3 clients announce themselves,
connect and exchange contacts; reachable nodes can participate in the
Fedi3 DHT. A normal client is not turned into a TURN server or a
circuit relay for third parties.

The updated `fedi3_p2p_infra` service supports the new protocol and the previous
DHT name to allow progressive upgrading. It must be updated together
with the new clients to maintain the Fedi3 network record service.

## Settings

In **Settings → Network** you can enable/disable IPFS discovery and
peer memory/exchange, and enter independent Fedi3 bootstraps. These options
require P2P active. Creating a new profile now proposes P2P active;
existing profiles keep their own choice.

Advanced core configuration:

```json
{
  "p2p": {
    "enable": true,
    "bootstrap": [],
    "public_discovery_enable": true,
    "peer_exchange_enable": true,
    "mdns_enable": true
  }
}
```

`bootstrap` accepts up to 32 multiaddresses ending in `/p2p/<peer-id>`.
`public_bootstrap`, optional, replaces the default IPFS seeds (maximum 8);
an empty list removes those seeds. To disable all participation
in the public DHT use `public_discovery_enable: false`. Disabling the exchange
also stops automatic saving and reconnection; it does not delete the existing file.
Explicit chat requests can still open connections.

`/_fedi3/net/metrics`, protected by the internal token, exposes under `p2p`:
`known_peers`, `peer_exchange_contacts` and `public_discovery_ready`. The latter
indicates a successful public bootstrap query, not a guarantee of connectivity
or future availability.

## Reducing dependence on the HTTP relay

Chat keeps in the DB up to 256 actor documents obtained from their HTTP
origin, verifying their ID. For one hour it reuses that account–peer link without
calling the relay; for seven days it can reuse it if the origin does not respond or
returns 5xx. 4xx rejections and documents with wrong identity are not
masked with the expired fallback. A PEX announcement cannot replace this
link with its own identity claim.

This lets already known contacts continue direct chat with the
HTTP relay unavailable. **It does not yet complete independence from the relay for
all functions**: the first contact with an account still requires profile
resolution, and the Rust core's WebRTC fallback uses the relay's HTTP signaling.
A successful network discovery does not automatically imply
resolution of any username.

## Audio/video and external bridges

Calls use WebRTC. Fedi3 signaling exchanges SDP and ICE; the media stream
has its own connection. The new automatic ICE values use the public STUN servers
Cloudflare and Google, without deriving STUN/TURN from the ActivityPub relay domain.
The circuit relay list is no longer automatically compiled starting from
that domain. Values explicitly saved in existing profiles remain
usable; they are not proof that traffic passes outside our server.

STUN helps find a direct path, but does not relay media. When that
path is impossible a TURN is needed, or the call must fail.

| Solution | Role and conditions verified on November 4, 2026 |
| --- | --- |
| [Cloudflare STUN](https://developers.cloudflare.com/realtime/turn/) | Public endpoint `stun:stun.cloudflare.com:3478`; helps direct connection, not a media bridge. |
| [Metered Open Relay](https://www.metered.ca/tools/openrelay/) | TURN usable with WebRTC, with 20 GB free per month stated. Requires an account and credentials/API; no shared credential is embedded in Fedi3. It is a hosted service, not a community guarantee of availability. |
| [meet.jit.si](https://jitsi.org/user-faq/) | Free hosted conference based on open software. It is not a generic TURN/SFU to connect to existing Fedi3 calls. Using it means joining a Jitsi conference and whoever creates the room must authenticate. |

The [Metered terms](https://www.metered.ca/terms) do not assign an SLA to free
plans. [meet.jit.si](https://jitsi.org/meet-jit-si-terms-of-service/) also
does not guarantee availability; see also its
[authentication rule](https://jitsi.org/blog/authentication-on-meet-jit-si/).
No external account was activated and no public conference was started.

To use Open Relay, get the ICE values from your account and enter them in the
ICE URL, TURN user and TURN credential fields of the Fedi3 configuration. Enter
the TURN credential returned by the service, not the API administrative key.
The current model accepts one user/credential pair shared by the configured TURN URLs;
providers with different credentials require a richer ICE model.
The external bridge still has to be verified with a real call. Previous tests
between Alice and Peppenamir have not yet demonstrated a working RTP flow.

## Reproducible checks

```bash
cargo test -p fedi3_core --lib p2p::discovery
cargo test -p fedi3_core --lib peers_exchange_signed_contacts_and_reconnect_after_restart_without_seeds
cargo test -p fedi3_core --test p2p_chat
# Explicit Internet test: uses the public IPFS network, does not run in the ordinary suite.
cargo test -p fedi3_core --lib public_ipfs_bootstrap_smoke -- --ignored --nocapture
```

The Internet test completed handshakes with the public bootstraps and a
DHT bootstrap query. The local regressions verify signatures, expirations,
address filtering, persistent contacts, reconnection without seeds and chat
with the HTTP profile unavailable. They do not replace an audio/video test between
different networks.


## HTTP relay and removed prototypes

The network between relays uses peering, directory and HTTP synchronization of the
`fedi3_relay` service; the `fedi3_p2p_infra` node remains a separate libp2p service for
clients. The old uncompiled prototypes `relay_mesh.rs`/`relay_notes.rs` and the
inactive UPnP module have been removed. There are no operational
`FEDI3_RELAY_MESH_*` or `upnp_*` core options.

The standalone node compose still uses the `deploy/p2p_infra/data` bind mount
to preserve existing installations. Create the directory with the UID/GID
configured in `P2P_UID`/`P2P_GID` before starting. Do not change the volume or
delete the keypair to fix a permissions error.
