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

## Relay API check senza jq

Se la VPS non ha `jq`, usa questi comandi:

- Header + stato actor:
  - `curl -sS -D - -o /tmp/actor.json https://relay.example.com/users/<user> | sed -n '1,20p'`
- Header + stato outbox page:
  - `curl -sS -D - -o /tmp/outbox.json 'https://relay.example.com/users/<user>/outbox?page=true&limit=5' | sed -n '1,20p'`
- Pretty print JSON:
  - `python3 -m json.tool /tmp/actor.json | sed -n '1,60p'`
  - `python3 -m json.tool /tmp/outbox.json | sed -n '1,80p'`
- Conteggio rapido item outbox:
  - `python3 - <<'PY'\nimport json\np='/tmp/outbox.json'\nwith open(p,'r',encoding='utf-8') as f:\n d=json.load(f)\nprint(len(d.get('orderedItems',[])))\nPY`

## Reconcile AP aggregates (admin)

Quando follower/following/outbox risultano disallineati dopo clean DB o crash:

- Avvio manuale async:
  - `curl -sS -X POST -H "Authorization: Bearer <ADMIN_TOKEN>" https://relay.example.com/_fedi3/relay/reconcile`
- Avvio manuale sync (attende completamento):
  - `curl -sS -X POST -H "Authorization: Bearer <ADMIN_TOKEN>" "https://relay.example.com/_fedi3/relay/reconcile?sync=true"`
- Stato job:
  - `curl -sS -H "Authorization: Bearer <ADMIN_TOKEN>" https://relay.example.com/_fedi3/relay/reconcile | python3 -m json.tool`
- Reconcile completo actor/collections:
  - `curl -sS -X POST -H "Authorization: Bearer <ADMIN_TOKEN>" "https://relay.example.com/_fedi3/relay/reconcile?sync=true&full=true"`

Compat policy (strict-by-default + allowlist):

- Lista policy:
  - `curl -sS -H "Authorization: Bearer <ADMIN_TOKEN>" https://relay.example.com/_fedi3/relay/compat/policy | python3 -m json.tool`
- Upsert policy host-only:
  - `curl -sS -X POST -H "Authorization: Bearer <ADMIN_TOKEN>" -H "Content-Type: application/json" -d '{"host":"www.foxyhole.io","policy":"compat_relaxed_digest"}' https://relay.example.com/_fedi3/relay/compat/policy`
- Upsert policy host+family:
  - `curl -sS -X POST -H "Authorization: Bearer <ADMIN_TOKEN>" -H "Content-Type: application/json" -d '{"host":"www.foxyhole.io","family":"sharkey","policy":"compat_relaxed_headers"}' https://relay.example.com/_fedi3/relay/compat/policy`
- Delete policy:
  - `curl -sS -X POST -H "Authorization: Bearer <ADMIN_TOKEN>" -H "Content-Type: application/json" -d '{"host":"www.foxyhole.io","family":"sharkey","delete":true}' https://relay.example.com/_fedi3/relay/compat/policy`
- Diagnostica consistenza AP:
  - `curl -sS -H "Authorization: Bearer <ADMIN_TOKEN>" https://relay.example.com/_fedi3/relay/diagnostics/ap-consistency | python3 -m json.tool`

Metriche utili (`/_fedi3/relay/metrics.prom`):

- `fedi3_relay_ap_inbox_accept_total`
- `fedi3_relay_ap_inbox_reject_invalid_sig_total`
- `fedi3_relay_ap_actor_resolve_404_total`
- `fedi3_relay_ap_public_get_fallback_total`
- `fedi3_relay_ap_public_get_fallback_total_by_reason_route{reason,route}`
- `fedi3_relay_ap_signature_policy_applied_total`
- `fedi3_relay_ap_inbox_compat_accept_total`
- `fedi3_relay_ap_spool_deadletter_total`
- `fedi3_relay_ap_consistency_mismatch_total`
