# Decentralizzazione: identità, trasporto e fiducia

Questo documento descrive i meccanismi introdotti per rendere Fedi3 **sostituibile** nelle sue parti centrali: l'identità non appartiene più al relay, il relay non legge più lo spool, i relay possono essere più di uno e la moderazione è applicabile prima che il traffico raggiunga il core.

L'obiettivo realistico non è "zero server": un dispositivo spento ha bisogno di qualcuno che conservi i messaggi. L'obiettivo è **nessun server obbligatorio e nessun server insostituibile**.

## 1. Identità separata dal trasporto (anchor)

Con `anchor_actor_url` il core pubblica un'identità canonica su un dominio statico:

- l'actor `id`, `url` e la chiave (`#main-key`) vivono sull'anchor;
- `inbox`, `outbox`, `followers`, `following` restano sul relay (sono risorse dinamiche);
- il relay reindirizza `GET /users/<user>` all'anchor e il WebFinger del relay punta all'actor canonico;
- il core scrive i documenti statici in `<data_dir>/anchor/actor.json` e `<data_dir>/anchor/webfinger` e li espone su `GET /_fedi3/anchor`.

### Perché funziona con Mastodon

ActivityPub non richiede che `inbox` stia sullo stesso host dell'`id`. Un server remoto:

1. risolve `@alice@anchor.example` sul WebFinger statico dell'anchor;
2. scarica l'actor dall'anchor (documento statico, con la chiave pubblica);
3. consegna all'`inbox` indicato nell'actor, che è sul relay;
4. verifica la firma con la chiave dell'anchor.

Il relay resta un **provider di trasporto scelto**: cambiarlo significa aggiornare l'`inbox` nel documento statico, senza `Move` e senza cambiare l'identità.

### Pubblicazione dell'anchor

1. Avvia il core con `anchor_actor_url` impostato (es. `https://alice.example`).
2. Copia `<data_dir>/anchor/actor.json` su `https://alice.example/users/alice`.
3. Copia `<data_dir>/anchor/webfinger` su `https://alice.example/.well-known/webfinger`.
4. Assicurati che il TLS sia valido e che i file siano serviti con `Content-Type: application/activity+json` (actor) e `application/jrd+json` (webfinger).

Qualsiasi host statico va bene (GitHub Pages, Netlify, Cloudflare Pages, un bucket S3). Se la chiave ruota, rigenera i due file.

> Nota: gli `id` di oggetti e attività restano sul relay, così ogni oggetto è dereferenziabile anche se l'anchor è un host puramente statico.

## 2. Spool cieco (il relay non legge i messaggi in attesa)

Quando un core è offline il relay conserva le richieste in arrivo. Prima di questa modifica lo faceva in chiaro; ora:

1. il relay legge la chiave pubblica dell'utente dall'actor cache;
2. genera una chiave AES-256-GCM usa-e-getta, cifra il corpo e la chiave con RSA-OAEP-SHA256;
3. salva nel database solo il ciphertext e i metadati (header firmati originali, marca di cifratura, lunghezza);
4. alla riconnessione invia al core un **envelope** su `POST /_fedi3/spool/envelope` con il ciphertext;
5. il core decifra con la chiave privata (che non lascia mai il dispositivo), ricostruisce la richiesta originale con gli header firmati e la processa.

Conseguenze:

- il database del relay (e i suoi backup) non contengono il testo dei messaggi in attesa;
- il relay non deve più decifrare al flush: inoltra solo ciphertext;
- la finestra di validità della `Date` viene estesa al tempo trascorso nello spool (massimo 7 giorni), tramite l'header non firmato `X-Fedi3-Spooled-Skew-Secs`;
- se la chiave pubblica non è ancora in cache (primo contatto), il relay ripiega sullo spool in chiaro.

Limite dichiarato: il relay vede il corpo **al momento della ricezione** (deve instradarlo e verificare la firma). La cifratura protegge i dati a riposo e il percorso di flush, non l'istante di ingresso.

## 3. Mesh di relay

Ogni relay può essere collegato a relay pari:

- `POST /admin/peers {"url": "https://relay-b.example"}` aggiunge un peer;
- ogni relay espone la propria directory su `GET /_fedi3/relay/users`;
- un task periodico (`FEDI3_RELAY_PEER_SYNC_SECS`, default 60) sincronizza la directory dei peer in `remote_users`;
- una richiesta per un utente non locale viene inoltrata al relay di casa, con un budget di hop (`FEDI3_RELAY_PEER_MAX_HOPS`, default 3) e l'header `X-Fedi3-Hops` per evitare loop.

Questo permette di ospitare utenti su relay diversi e di avere più relay intercambiabili: la caduta di un relay non elimina la rete, e un utente può migrare senza cambiare identità (grazie all'anchor).

## 4. Moderazione

La moderazione è applicata **sul relay, prima del core**:

- blocklist relay-wide gestite dall'admin (`/admin/blocks`, tipi `actor` e `domain`, con scadenza opzionale);
- blocklist per-utente inviate dal core con il token del relay (`PUT /_fedi3/relay/user_blocks/<user>`, header `X-Fedi3-Relay-Token`);
- applicazione su `/inbox` condiviso e su `/users/<user>/inbox`: un mittente bloccato riceve `202` e non viene né inoltrato né messo in spool;
- il core sincronizza la propria blocklist al relay all'avvio e a ogni modifica (`/_fedi3/blocks`).

Perché è importante: per il fediverse il relay è **una sola istanza** con molti utenti. Bloccare un relay intero è costoso; con blocklist per-utente e relay piccoli e plurali, un blocco colpisce un gruppo ristretto.

## 5. Modello di fiducia

| Attore | Cosa vede | Cosa non vede |
|---|---|---|
| Relay | metadati di routing, corpo al momento della ricezione, media ospitati, ciphertext dello spool | chiave privata, plaintext dello spool a riposo, contenuto delle chat E2E |
| Anchor (host statico) | solo i documenti pubblici | tutto il resto |
| Core | tutto ciò che riguarda l'utente | — |

Il relay resta un punto di fiducia per: disponibilità, media, moderazione relay-wide e metadati. La mitigazione non è eliminarlo, ma renderlo **sostituibile e trasparente**.

## 6. Rischi residui e prossimi passi

- **Mobile**: il core vive nel processo dell'app; su Android/iOS il sistema può
  sospendere l'app. Il provisioning push tramite il relay e una retention dello
  spool configurabile sono implementati, ma la consegna da parte del sistema
  operativo resta discrezionale.
- **Redirect e SSRF**: la guardia di rete risolve e controlla gli indirizzi esatti
  usati dal connettore, rifiuta destinazioni private, loopback e link-local e
  limita i redirect. Vedere la sezione sull'enforcement delle destinazioni in
  uscita in `SECURITY.md`.
- **Replicazione dei contenuti pubblici**: le note pubbliche vivono ancora
  sull'outbox del relay. Il gossipsub firmato su `/fedi3/global/1` replica già
  l'attività pubblica tra core connessi, ma è best-effort e non una replicazione
  storica completa; il relay resta il bootstrap e lo store durevole.
- **TURN comunitario**: per l'NAT simmetrico servono pool di relay TURN gestiti
  dalla comunità, non un unico servizio centralizzato. Il relay può pubblicare un
  pool privato di operatori, ma nessun operatore è attivo per impostazione
  predefinita.
- **Accountability**: `/transparency` e il registro firmato dei forward esistono,
  con ricevute verificabili in modo indipendente. Mancano ancora testimoni
  indipendenti e il confronto automatico tra checkpoint di relay differenti.

## Documenti correlati

- [p2p_network.md](p2p_network.md) — discovery, DHT e trasporto diretto.
- [network_resilience.md](network_resilience.md) — spool, TURN, push e trasparenza.
- [push_forward_receipts.md](push_forward_receipts.md) — formati delle ricevute e verifica.
- [interop.md](interop.md) — test reali con Mastodon/Misskey dell'identità ancorata.
