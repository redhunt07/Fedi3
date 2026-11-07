# Test between two clients — 4 November 2026

Devices: Alice on the local PC (EndeavourOS) and Peppenamir on Zorin OS 18.1,
with public relay `fedi3.redhunt07.ovh` and Mullvad active on the local PC.

## Verified results

- Alice → Peppenamir: message
  `urn:fedi3:chat:msg:e75bfe5f3b2cfc4fe630870d`, `sent=1`, `queued=0`,
  about 1700 ms. The same ID was found on the remote core, the text is visible
  in the client, `delivered` receipt on the sender.
- Peppenamir → Alice: message
  `urn:fedi3:chat:msg:984437f68277fbdc1d9bda51`, `sent=1`, `queued=0`,
  about 1731 ms. The same ID was found on the local core, `seen` receipt on the
  remote.
- `busy` presence propagated in both directions, then restored to `online`.
- Circuit relay rejections were detected due to limit exhaustion. The
  infrastructure service was updated; reservations were observed to resume with
  limits of 16 MiB and 900 seconds.

These tests confirm delivery and presence across the available paths; they do
not demonstrate a direct connection without relay.

## Calls: issue still open

The real test identified and had corrected the missing initial offer after
acceptance and the loss of early ICE candidates. Subsequent tests exchanged SDP
and ICE without establishing an audio stream: RTP counters at zero. Acceptance
and the SDP answer are no longer shown as a successful media connection.

Also corrected were the ICE configuration of the Linux bridge (one URL per
element) and the separation between TURN supported by the Flutter calls and the
one of the Rust backend. These are implemented fixes covered by local tests;
audio success between the two PCs still has to be verified.

The user reported that Peppenamir's PC was shut down. Device-to-device tests are
therefore suspended. The local client was restarted normally without the
temporary tracing library; the VPN was not modified.

## Validation and next test

Local suites: 227 Rust tests and 121 Flutter tests passed. The rebuilt AppImages
include core and Flutter code matching the compiled outputs. The checksums are
in the respective `.sha256` files in `dist/`.

When both PCs are available:

1. Repeat message and receipt in both directions on the updated builds.
2. Accept a call and check `Fedi3 call media` on both clients:
   connected state and growth of RTP bytes sent and received for the same ID.
3. If ICE fails, compare host, srflx and relay candidates and the responses to
   connectivity checks, keeping the VPN in its usual configuration.
4. Repeat after disconnecting and reconnecting a client.

Do not consider audio solved before step 2; simulated tests do not prove it.
