# Troubleshooting

## Caddy: ACME error

- Dominio non valido o DNS non propagato.
- Porte 80/443 chiuse.
- `FEDI3_DOMAIN` errato.

## Relay non parte

- Verifica `.env` e token admin.
- `docker compose logs relay`
- `docker compose logs postgres`

## Dragonfly crash (memoria)

Riduci thread o memory:

- `--proactor_threads=2`
- `--maxmemory=512mb`

## Meilisearch non healthy

- Controlla `MEILI_MASTER_KEY`.
- Controlla salute: `http://meilisearch:7700/health`.

## App: network error

- Verifica relay e token.
- Controlla `/_fedi3/health` sul core locale.
