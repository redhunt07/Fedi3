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
