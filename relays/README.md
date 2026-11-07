# Fedi3 relay directory

`relays.json` is the shared, public list of Fedi3 relays. The Fedi3 client and
the Fedi3 core read this file to bootstrap relay discovery, and the app probes
every entry from the user's device to rank relays by real latency (in ms).

The list is deliberately a plain file in this repository: there is no central
server, no database and no account to create. Anybody who runs a Fedi3 relay can
add it with a pull request.

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

Fields:

| Field | Required | Notes |
| --- | --- | --- |
| `url` | yes | Public HTTPS base URL, no trailing slash. |
| `ws` | no | WebSocket base URL (`wss://`). Inferred from `url` when omitted. |
| `name` | yes | Short human-readable name of the relay. |
| `operator` | yes | Person or organization running the relay. |
| `contact` | yes | URL or handle where users can ask for a relay token. |
| `region` | no | Free-form region hint (`eu`, `us`, `global`, ...). |
| `added` | yes | ISO date (`YYYY-MM-DD`) when the entry was added. |

Rules enforced by `scripts/tests/test_relay_directory.py`:

- `url` must use `https://` (plain `http://` is allowed only for loopback).
- No trailing slashes, no duplicate URLs, hosts or WebSocket endpoints.
- `ws`, when present, must use `wss://` (or `ws://` for loopback) on the same host.
- `added` must be a valid ISO date and `updated_at` a valid ISO 8601 timestamp.
- Entries are kept in the order they were added; the client sorts them by
  measured latency at runtime.

## Adding your relay

1. Make sure the relay is publicly reachable over HTTPS with a valid
   certificate for its hostname, and that `GET /healthz` answers `200 ok`.
2. Add one object to the `relays` array in `relays/relays.json`, keeping the
   file valid JSON and the array sorted by `added` date.
3. Bump `updated_at` to the current UTC time.
4. Run the validator locally:

   ```bash
   python3 scripts/tests/test_relay_directory.py
   ```

5. Copy the updated file to `app/assets/relays.json` (the client bundles it as
   an offline fallback; CI checks that the two copies match).
6. Open a pull request titled `relay: add <hostname>` and include the relay
   URL, the operator and a contact where users can request a relay token.

The relay token itself is **not** stored in this list: each relay issues tokens
to its users. The directory only tells clients where relays exist and how fast
they answer.

## How clients use it

- The Flutter app reads the raw file from the repository (with a CDN mirror as
  fallback) and caches the last good copy for offline startup.
- Every entry is probed with `GET /healthz`; the measured round-trip time is
  shown next to the relay and the list is ranked from fastest to slowest.
- Unreachable relays stay visible but are sorted last, with the failure reason
  (TLS mismatch, timeout, HTTP error) instead of a latency value.
- The Rust core fetches the same file during relay sync so that relay discovery
  also works when the currently configured relay is temporarily unreachable.
- The relay service can seed its HTTP mesh from the same file when
  `FEDI3_RELAY_SEED_RELAYS` is empty.

See [docs/relay_directory.md](../docs/relay_directory.md) for the full
behaviour and configuration.
