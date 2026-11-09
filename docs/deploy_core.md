# Deploy core (local runtime)

The core is a local server used by the app; it must not be exposed publicly.

## Starting through the app

The app starts the core automatically at login.

## Manual start (dev)

```
scripts/build_core.ps1
scripts/build_core.sh
```

The binary is loaded by the app via FFI.

## Important config

- `internal_token` to protect UI/internal endpoints.
- `public_base_url` and `relay_ws` consistent with the relay.

## Health/metrics

- `GET /healthz`
- `GET /readyz`
- `GET /_fedi3/health` (requires `X-Fedi3-Internal`)
- `GET /_fedi3/net/metrics(.prom)` (requires `X-Fedi3-Internal`)

### Development core authentication

The standalone `dev_client` uses `127.0.0.1:8788` by default, the same port as
an app-managed core. Run only one core on that address. If relay discovery
returns `403 forbidden`, check that the app is contacting the intended core
and that its internal token matches; this is an authentication failure.

For a standalone development core, set `FEDI3_INTERNAL_TOKEN` to the same value
as the app's internal token in Security settings before starting `dev_client`.
When this variable is absent or blank, the development core generates a new
random token at each startup. Alternatively, stop the standalone process and
start the app-managed core. Keep existing profile credentials intact.

## Relayed P2P addresses

When P2P is enabled and the relay exposes `/_fedi3/relay/p2p_infra`, the core
reserves a circuit on that relay and advertises the resulting `/p2p-circuit`
address in the actor document. This keeps peers reachable behind NAT. If the
relay leaves `FEDI3_P2P_INFRA_PEER_ID` and `FEDI3_P2P_INFRA_MULTIADDRS` empty,
the core advertises only its configured `announce` addresses.
