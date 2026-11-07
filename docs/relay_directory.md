# Relay directory

Fedi3 relay discovery starts from a shared, public list of relays:
[`relays/relays.json`](../relays/relays.json). The file lives in this repository
and is maintained through pull requests. The relay serves a packaged mirror at `/_fedi3/relay/directory` alongside the
GitHub/jsDelivr sources. The app also bundles the file for offline fallback.
No directory account is required.

The client reads the list, probes every relay from the user's device and shows
the results ranked by measured latency in milliseconds. The Rust core reads the
same list so relay discovery keeps working when the currently configured relay
is unreachable. Relays can also seed their HTTP mesh from the list.

## Why the directory exists

Before this mechanism the client knew exactly one bootstrap relay. When that
relay was down or its TLS certificate did not match the configured hostname,
discovery failed completely and the app showed a raw handshake error
(for example `TLSV1_ALERT_UNRECOGNIZED_NAME`). The directory removes that single
point of failure: the list is served by the repository CDN, and each relay is
measured independently.

## File format

```json
{
  "schema": "fedi3-relay-directory/v1",
  "updated_at": "2026-11-07T12:00:00Z",
  "relays": [
    {
      "url": "https://relay.example.com",
      "ws": "wss://relay.example.com",
      "name": "Example relay",
      "operator": "Alice",
      "contact": "https://example.com/@alice",
      "region": "eu",
      "added": "2026-11-07"
    }
  ]
}
```

| Field | Required | Notes |
| --- | --- | --- |
| `url` | yes | Public HTTPS base URL, no trailing slash. |
| `ws` | no | WebSocket base URL (`wss://`); inferred from `url` when omitted. |
| `name` | yes | Short human-readable relay name. |
| `operator` | yes | Person or organization running the relay. |
| `contact` | yes | URL or handle where users request a relay token. |
| `region` | no | Free-form region hint. |
| `added` | yes | ISO date when the entry was added. |

The relay token is not stored in the directory: each relay issues tokens to its
own users. The directory only advertises where relays exist.

## Adding a relay

See [`relays/README.md`](../relays/README.md) for the full contribution guide.
In short:

1. Make the relay publicly reachable over HTTPS with a valid certificate and a
   `GET /healthz` that answers `200 ok`.
2. Add one object to `relays/relays.json` and bump `updated_at`.
3. Run `python3 scripts/tests/test_relay_directory.py`.
4. Open a pull request titled `relay: add <hostname>`.

CI validates the file on every push and pull request. The validator enforces
HTTPS, unique hosts, matching WebSocket hosts, valid dates and entry ordering.

## Client behaviour

The Flutter client:

- downloads the directory from
  `https://raw.githubusercontent.com/redhunt07/Fedi3/main/relays/relays.json`
  and falls back to the jsDelivr mirror;
- caches the last good copy under the application support directory, so the
  first-run screen can show relays even when GitHub is unreachable;
- falls back to `app/assets/relays.json`, a copy of the directory bundled with
  the build, when neither the network nor the cache is available;
- probes every entry with `GET /healthz` and a 4 s timeout, up to six relays in
  parallel;
- ranks reachable relays from the lowest to the highest latency and keeps
  unreachable relays visible at the end with the failure reason (TLS hostname
  mismatch, invalid certificate, timeout, HTTP error, network error);
- shows the rank, the relay host and the latency in milliseconds on the
  first-run screen, in onboarding and in the networking settings;
- lets users open the onboarding screen with a relay selected from the list;
  the relay token still has to be entered manually.

Latency is measured live, from the user's device, every time the list is
refreshed. It is never written into the directory file.

## Core behaviour

The core fetches the same directory during relay sync (`relay_sync.rs`) and
stores the entries in its relay registry with the `directory` source. The
refresh happens on the regular relay-sync interval (300 s) and is best-effort.

Environment override:

```
FEDI3_RELAY_DIRECTORY_URLS=https://mirror.example.com/relays.json
```

A comma-separated list replaces the built-in sources. An explicitly empty value
disables directory discovery. When the variable is unset, the built-in
repository sources are used.

## Relay behaviour

When `FEDI3_RELAY_SEED_RELAYS` is empty, the relay fetches the directory at
startup and uses the listed relays as its initial HTTP mesh seeds, excluding
itself. Manual seed relays always take precedence:

```
FEDI3_RELAY_SEED_RELAYS=https://relay-a.example.com,https://relay-b.example.com
```

The same `FEDI3_RELAY_DIRECTORY_URLS` override applies to the relay.

## Privacy and safety

- The client performs plain HTTPS `GET` requests to the directory and to each
  relay's public `/healthz` endpoint. No tokens, usernames or post content are
  sent.
- A directory entry is only an advertisement. The client still validates the
  relay with a probe and reports failures instead of hiding them.
- A bad entry cannot break the whole list: the parser skips malformed items and
  the validator rejects the pull request in CI.
