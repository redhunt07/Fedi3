# Deploy core (runtime locale)

Il core è un server locale usato dall’app, non va esposto pubblicamente.

## Avvio tramite app

L’app avvia il core automaticamente al login.

## Avvio manuale (dev)

```
scripts/build_core.ps1
scripts/build_core.sh
```

Il binario viene caricato dalla app via FFI.

## Config importanti

- `internal_token` per proteggere endpoints UI/internal.
- `public_base_url` e `relay_ws` coerenti con il relay.

## Health/metrics

- `GET /healthz`
- `GET /readyz`
- `GET /_fedi3/health` (richiede `X-Fedi3-Internal`)
- `GET /_fedi3/net/metrics(.prom)` (richiede `X-Fedi3-Internal`)
