# Troubleshooting

## Caddy: ACME error

- Dominio non valido o DNS non propagato.
- Porte 80/443 chiuse.
- `FEDI3_DOMAIN` errato.
- Il servizio `relay` non è healthy: Caddy non parte finché `/healthz` non risponde. Controlla `docker compose ps` e `docker compose logs relay`.

## Relay non parte

- Verifica `.env`: il relay rifiuta di partire se `FEDI3_RELAY_ADMIN_TOKEN` è più corto di 32 caratteri o se `FEDI3_RELAY_DB_DRIVER` non è `sqlite`/`postgres`.
- Il compose si ferma se mancano `FEDI3_RELAY_PG_PASSWORD`, `FEDI3_DOMAIN`, `TURN_REALM` o `TURN_PASS`.
- `docker compose logs relay`
- `docker compose logs postgres`
- Se il database è raggiungibile ma lo schema è più nuovo del binario, il relay esce con un errore esplicito: non fare downgrade senza migrazione.

## Richieste federate rifiutate (401)

Dal 0.1.1 la verifica è obbligatoria. Cause tipiche:

- l'istanza remota non firma la richiesta, oppure firma solo `(request-target)` e `host` senza `date`/`digest`;
- il `Digest` non corrisponde al body (proxy che riscrive il body, compressione non dichiarata);
- `Date` fuori dalla finestra consentita (±300 s): controlla l'orologio del server;
- l'attività è firmata da un attore diverso da quello dichiarato in `actor`;
- la chiave pubblica non è raggiungibile o `keyId` punta a un IP privato (SSRF guard).

Diagnosi:

```bash
docker compose logs relay | grep -iE 'signature|digest'
```

## Tunnel: disconnessioni ripetute

Il core riconnette da solo con backoff esponenziale. Se vedi riconnessioni continue:

- token del relay revocato o errato (il relay chiude con 401);
- il relay non risponde entro il timeout di connessione;
- nessun `pong` entro 30 s: rete instabile o proxy che interrompe le connessioni idle.

Controlla `docker compose logs relay | grep -E 'tunnel (connected|disconnected)'`.

## Utente offline: attività in spool

Se un utente non è connesso, le attività vengono messe in spool e consegnate alla riconnessione. Se lo spool supera il limite per utente (`FEDI3_RELAY_SPOOL_MAX_ROWS_PER_USER`) le più vecchie vengono scartate. Se il mittente riceve 503, significa che nessun destinatario è stato consegnato né accodato: riprova più tardi.

## Dragonfly crash (memoria)

Riduci thread o memory:

- `--proactor_threads=2`
- `--maxmemory=512mb`

## Meilisearch non healthy

- Controlla `MEILI_MASTER_KEY`.
- Controlla salute: `http://meilisearch:7700/health`.
- Con `FEDI3_RELAY_SEARCH_BACKEND=db` Meilisearch non è necessario: il relay usa il database.

## Media non serviti

- Verifica `FEDI3_RELAY_MEDIA_BACKEND` e, per S3/WebDAV, credenziali e endpoint.
- `docker compose exec relay du -sh /data/media`
- Un 415 su upload significa MIME non in allowlist; un 413 quota superata.

## App: network error

- Verifica relay e token.
- Controlla `GET /_fedi3/health` sul core locale: se non risponde, il core è morto e il watchdog lo riavvia.
- Se la timeline resta vuota dopo un cambio di configurazione, verifica che il riavvio del core sia andato a buon fine: la schermata di configurazione riporta l'errore e ripristina i valori precedenti.

## App: il core non parte

- Porta già in uso: cambia il bind o chiudi l'altra istanza.
- Username non valido: solo `[A-Za-z0-9_]`, 1–64 caratteri.
- Directory dati non scrivibile: controlla i permessi (la directory deve essere `0700` e le chiavi `0600`).
- Senza relay il core parte comunque: se non parte, il problema non è il relay.

## App: configurazione corrotta

L'app scrive la configurazione in modo atomico e conserva un `.bak`. Se il file principale è corrotto, l'app recupera il backup; se anche quello è corrotto, mostra un avviso invece di sovrascrivere. In quel caso reimposta la configurazione dalla schermata di setup.

## Diagnosi rapida del relay

```bash
scripts/relay_diagnose.sh
```

Mostra stato dei container, healthcheck, log di tunnel e rifiuti di firma. Non richiede `curl` o `wget` nell'immagine del relay.
