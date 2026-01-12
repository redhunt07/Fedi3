# FEDI3

FEDI3 è un social federato con componente P2P.  
Privacy-first, interoperabile e resiliente senza sacrificare la compatibilità.

## Cosa include

- App Flutter (UI/UX + impostazioni)
- Core Rust locale (AP + P2P + cache)
- Relay pubblico (compatibilità legacy + routing)

## Quick start (relay)

```
cp .env.example .env
docker compose up -d --build
```

## Docs

- `docs/getting_started.md`
- `docs/deploy_relay.md`
- `docs/deploy_core.md`
- `docs/app_guide.md`
- `docs/ops_backup_restore.md`
- `docs/troubleshooting.md`
- `docs/faq.md`

## Licenza

AGPLv3. Modifiche network-facing devono rimanere open.
