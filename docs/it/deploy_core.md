# Deploy del core (runtime locale)

Il core è un server locale usato dall’app. Per impostazione predefinita ascolta su `127.0.0.1` e non va esposto pubblicamente.

## Avvio tramite app

L’app avvia il core automaticamente quando parte e lo riavvia dopo una modifica della configurazione.

## Avvio manuale (dev)

```bash
scripts/build_core.ps1
scripts/build_core.sh
```

Il binario viene caricato dalla app via FFI.

## Configurazione importante

- `internal_token` per proteggere gli endpoint UI/internal.
- `public_base_url` e `relay_ws` coerenti con il relay.
- `relay_token` quando si usa un tunnel verso il relay (almeno 16 caratteri).
- `data_dir` per sostituire la directory dati predefinita.

## Health/metrics

- `GET /healthz`
- `GET /readyz`
- `GET /_fedi3/health` (richiede `X-Fedi3-Internal`)
- `GET /_fedi3/net/metrics(.prom)` (richiede `X-Fedi3-Internal`)

### Autenticazione del core di sviluppo

Il processo autonomo `dev_client` usa normalmente `127.0.0.1:8788`, la stessa
porta del core gestito dall’app. Esegui un solo core su questo indirizzo.
Se la discovery relay restituisce `403 forbidden`, verifica che l’app contatti
il core corretto e che i token interni coincidano: è un errore di autenticazione.

Prima di avviare `dev_client`, imposta `FEDI3_INTERNAL_TOKEN` allo stesso valore
del token interno dell’app nelle impostazioni di sicurezza. Se la variabile è
assente o vuota, il client di sviluppo genera un nuovo token casuale a ogni
avvio. In alternativa, arresta il processo autonomo e avvia il core dall’app.
Conserva le credenziali del profilo esistente.

## Indirizzi P2P relayed

Con il P2P attivo e il relay che espone `/_fedi3/relay/p2p_infra`, il core
prenota un circuito su quel relay e pubblica l'indirizzo `/p2p-circuit`
risultante nel documento actor. Questo mantiene raggiungibili i peer dietro
NAT. Se il relay lascia vuoti `FEDI3_P2P_INFRA_PEER_ID` e
`FEDI3_P2P_INFRA_MULTIADDRS`, il core pubblica solo gli indirizzi `announce`
configurati.

## Documentazione correlata

- [app_guide.md](app_guide.md) — configurazione del client e primo avvio
- [deploy_server.md](deploy_server.md) — relay e infrastruttura P2P
- [troubleshooting.md](troubleshooting.md) — diagnosi di core e P2P
