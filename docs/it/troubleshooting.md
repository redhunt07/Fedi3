# Diagnostica

Questa pagina copre i guasti più comuni di relay, core, app e P2P. Per i passaggi
di deploy vedi [deploy_relay.md](deploy_relay.md) e
[deploy_server.md](deploy_server.md).

## Caddy: errore ACME

- Dominio non valido o DNS non propagato.
- Porte 80/443 chiuse.
- `FEDI3_DOMAIN` errato.
- Il servizio `relay` non è healthy: Caddy non parte finché `/healthz` non risponde. Controlla `docker compose ps` e `docker compose logs relay`.

## Relay non parte

- Verifica `.env`: il relay rifiuta di partire se `FEDI3_RELAY_ADMIN_TOKEN` è più corto di 32 caratteri o se `FEDI3_RELAY_DB_DRIVER` non è `sqlite`/`postgres`.
- Il compose si ferma se mancano `FEDI3_RELAY_PG_PASSWORD`, `TURN_REALM` o `TURN_PASS`. Caddy richiede `FEDI3_DOMAIN`; l'installer lo valida per le modalità relay.
- `docker compose logs relay`
- `docker compose logs postgres`
- Se il database è raggiungibile ma lo schema è più nuovo del binario, il relay esce con un errore esplicito: non fare downgrade senza migrazione.

## Richieste federate rifiutate (401)

La verifica delle firme HTTP è obbligatoria. Cause tipiche:

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

## P2P: chat, presenza e chiamate

Per bootstrap indipendenti, memoria dei peer, scambio di contatti firmati e
discovery IPFS vedere [Rete Fedi3 e infrastrutture pubbliche](p2p_network.md).

- `connected_peers` conta connessioni libp2p, inclusi i nodi infrastrutturali:
  non è il numero degli utenti online. La presenza dei contatti arriva dalla
  directory relay e dai messaggi di presenza P2P; le due fonti sono indipendenti.
- Il profilo ActivityPub deve pubblicare `endpoints.fedi3PeerId` e
  `endpoints.fedi3PeerAddrs`. Per un peer dietro NAT, gli indirizzi devono
  includere `<relay>/p2p/<relay-id>/p2p-circuit/p2p/<peer-id>`.
- Il server circuit relay deve annunciare un indirizzo raggiungibile tramite
  `FEDI3_P2P_EXTERNAL_ADDR`. Una prenotazione senza indirizzi viene rifiutata
  dal client anche se il server registra la richiesta come accettata.
- Il client riprova le prenotazioni chiuse dopo almeno 10 secondi, con controllo
  ogni 5 secondi. Cercare `relay reservation closed`, `ReservationReqAccepted`
  e `p2p listening` nei log.
- `queued` significa conservato nella mailbox, non ricevuto dal destinatario.
  La conferma `delivered` identifica il messaggio ricevuto; `seen` indica lettura.
- Il test di signaling verifica inviti/SDP/ICE trasportati e memorizzati; audio,
  video e attraversamento TURN richiedono anche una prova con dispositivi reali.

Test locali con profili temporanei, senza credenziali di produzione:

```bash
cargo test -p fedi3_core --lib connectivity_tests
cargo test -p fedi3_core --test p2p_chat --test semantics
cd app
flutter test test/unit/peer_presence_store_test.dart test/widget/incoming_calls_test.dart
```

Le correzioni richiedono di ricompilare sia il core Rust sia l'app Flutter:
una vecchia AppImage continua a contenere il codice precedente.

### Circuiti rifiutati o chiusi durante chat e chiamate

`Remote reported resource limit exceeded` indica un rifiuto del circuit relay,
non necessariamente un client offline. I limiti libp2p predefiniti (128 KiB,
120 secondi, riaperture limitate) non sono adatti ai frame Fedi3 e alle
riconnessioni prolungate. Il servizio infrastrutturale usa ora 64 circuiti
complessivi, 8 per peer, 16 MiB e 900 secondi per circuito. Gli operatori possono
modificarli con `FEDI3_P2P_RELAY_MAX_CIRCUITS`,
`FEDI3_P2P_RELAY_MAX_CIRCUITS_PER_PEER`,
`FEDI3_P2P_RELAY_MAX_CIRCUIT_BYTES` e `FEDI3_P2P_RELAY_MAX_CIRCUIT_SECS`.
I valori non validi impediscono l'avvio; restano attivi limiti per peer e IP.

La chiamata deve mostrare connessione effettiva, non soltanto l'accettazione
dell'invito. Nei log `Fedi3 call media` riporta i byte RTP inviati e ricevuti:
i contatori devono aumentare sui due dispositivi. Non viene registrato il
contenuto audio/video. Il chiamante genera l'offerta SDP dopo l'accettazione;
i candidati ICE arrivati prima dell'offerta vengono conservati e applicati
dopo la descrizione remota.

Se il signaling si conclude ma i `candidate_pairs` restano `in-progress` con
`responses_received: 0`, gli interlocutori non riescono a raggiungersi
direttamente: c’è una VPN, un NAT simmetrico o una rete che blocca UDP, quindi
serve TURN. Controlla `local_candidates` nella stessa riga di log: deve
comparire un candidato `relay`. Il client ricava gli endpoint coturn dall’host
del relay quando sono impostate le credenziali ICE, e il relay può pubblicare il
proprio pool (`FEDI3_RELAY_TURN_*`). Senza un candidato relay vengono scambiati
solo indirizzi STUN. Su desktop **Impostazioni > Audio** mostra microfono e
uscita realmente usati dalla chiamata; una registrazione di prova con
l’indicatore fermo a zero indica l’ingresso sbagliato.

## AppImage Linux 0.10.0 si chiude prima di aprire la finestra

`libmount.so.1: version MOUNT_2_40 not found` o
`libsystemd.so.0: version LIBSYSTEMD_251 not found` indica un conflitto del
pacchetto, non un profilo corrotto o permessi di esecuzione mancanti. La 0.10.0
includeva vecchie dipendenze usate dalla GLib di sistema. La correzione è pubblicata
nella [0.10.1](https://github.com/redhunt07/Fedi3/releases/tag/v0.10.1) ed è
inclusa nei pacchetti correnti (0.11.9). Installa la 0.10.1 o una versione
successiva, oppure ricompila con `scripts/build_appimage.sh` corretto. Conserva
dati e credenziali nel portachiavi. Il controllo SHA256 non rileva questo
conflitto ABI. Vedi [aggiornamenti del client](updates.md) per la procedura di
aggiornamento.
