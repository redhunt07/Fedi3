# Resilienza della rete (7 novembre 2026)

## Directory e TLS

Il relay attivo è `https://fedi3.redhunt07.ovh` (WSS sullo stesso host).
`relay.fedi3.com` è ritirato. Non disattivare la validazione TLS per usarlo.
Il relay serve anche un mirror pacchettizzato su `/_fedi3/relay/directory`, usato
dall'app, dal core e dai relay accanto ai mirror del repository (GitHub raw e
jsDelivr). Sia l'anteprima al primo avvio sia la scoperta manuale nell'onboarding
usano la directory inclusa/cache quando il refresh fallisce. La directory fornisce candidati, non fiducia: il dispositivo ne
verifica la salute. Vedere [relay_directory.md](relay_directory.md) per il
formato del file e la classifica.

## Mobile: provisioning configurabile, non garanzia di tunnel in background

Android/iOS possono sospendere o terminare l'app e il core incorporato. Non
contare su un tunnel permanente. La riconnessione svuota lo spool durevole del
relay. `RelayCapabilities.registerPush` / `unregisterPush` e questi endpoint del
relay forniscono il contratto di provisioning usato dai connettori nativi delle
piattaforme:

- `PUT /_fedi3/relay/push/{username}` con il Bearer token del relay dell'account e
  `{"provider":"unifiedpush","token":"endpoint opaco o token del dispositivo"}`
  (oppure `apns`). Restituisce `expires_at_ms`; rinnovare prima di 30 giorni.
- `DELETE` sullo stesso percorso revoca la registrazione. Al momento è conservata
  una sola installazione per account; la registrazione sostituisce la precedente.
- Gli account disabilitati non possono registrarsi né ricevere wakeup. La
  cancellazione dell'account rimuove la registrazione. La revoca cancella il
  token del dispositivo memorizzato. La scadenza ferma i wakeup; un token scaduto
  resta nei metadati fino a sostituzione/revoca/cancellazione.

Servono sia `FEDI3_RELAY_PUSH_GATEWAY_URL` (HTTPS pubblico) sia
`FEDI3_RELAY_PUSH_GATEWAY_TOKEN` (almeno 32 caratteri casuali). Senza di essi la
registrazione restituisce 503 e non viene tentato alcun push. Il gateway riceve
JSON autenticato: `provider`, `token`, `event:"sync"`, `ttl_secs` e, per
UnifiedPush, `webpush_keys`. Deve validare
gli endpoint dei dispositivi, unire le notifiche, scartare i job scaduti e
implementare il protocollo del provider. Nessun contenuto, mittente, nome
dell'account, chiave privata o token relay dell'account entra in quel JSON. Il
gateway conosce la destinazione. I tentativi del relay sono limitati a uno per
account al minuto e a quattro secondi; i fallimenti lasciano intatto lo spool. I
redirect del gateway sono disabilitati.

I connettori nativi Android UnifiedPush e iOS APNs sono implementati. Vedere
[push_forward_receipts.md](push_forward_receipts.md) per il comportamento delle
piattaforme, la configurazione dei provider, i requisiti di build/firma e i limiti
di verifica. Nessun gateway push esterno è attivo per impostazione predefinita.

Riferimenti: [protocollo UnifiedPush Android](https://unifiedpush.org/developers/spec/android/),
[push in background Apple](https://developer.apple.com/documentation/usernotifications/pushing-background-updates-to-your-app).

## Retention dello spool

`FEDI3_RELAY_SPOOL_TTL_SECS`: predefinito 604800 (7 giorni), intervallo accettato
60–2592000. La verifica dell'envelope nel core limita comunque la finestra estesa
della data della firma a sette giorni: usare al massimo sette giorni per una
consegna interoperabile. Gli elementi scaduti non vengono mai riprodotti, nemmeno
tra passaggi fisici di pulizia. La frequenza di pulizia è
`FEDI3_RELAY_WEBRTC_SIGNAL_CLEANUP_INTERVAL_SECS`, minimo 10 secondi.
Le quote per account predefinite sono 5000 righe e 67108864 byte di corpo quando
l'ambiente non le imposta; il file `.env.example` distribuito usa 500 righe e lo
stesso limite di byte. Zero disabilita la quota corrispondente. Inserimento ed
evizione del più vecchio sono atomici (transazione SQLite IMMEDIATE, lock della
riga utente in PostgreSQL). Gli elementi più grandi dell'intero limite di byte
sono rifiutati. Gli elementi scaduti vengono rimossi prima della valutazione
della quota.

Il superamento del limite elimina gli elementi più vecchi: è una coda best-effort
limitata, non un archivio garantito. Un successo HTTP del core elimina un
elemento; non è una ricevuta di lettura umana. La cifratura a riposo richiede che
la chiave pubblica dell'attore sia già in cache nel relay; altrimenti vale ancora
il ripiego dello spool in chiaro.

## Operatori TURN della comunità

Nessun servizio TURN pubblico non verificato o credenziale condivisa è incluso.
Il solo STUN non risolve ogni combinazione NAT. Più operatori consenzienti possono
fornire voci TURN tramite un file JSON privato, ad esempio `/data/turn-pool.json`:

```json
[
  {"urls":["turn:operator-one.example:3478?transport=udp","turn:operator-one.example:3478?transport=tcp"],"username":"issued-user-one","credential":"issued-secret-one"},
  {"urls":["turns:operator-two.example:5349"],"username":"issued-user-two","credential":"issued-secret-two"}
]
```

Impostare `FEDI3_RELAY_TURN_POOL_FILE` sul percorso nel container (limitare i
permessi del file). Al massimo otto operatori, quattro URL ciascuno, limite del
file 64 KiB. Le credenziali sono restituite solo agli account abilitati
autenticati con il proprio token relay, tramite
`GET /_fedi3/relay/turn/{username}` con `Cache-Control: no-store`.

Quando non è montato alcun file di pool, il relay deriva un pool a singolo
operatore da `FEDI3_RELAY_TURN_USERNAME`, `FEDI3_RELAY_TURN_CREDENTIAL`, un elenco
opzionale `FEDI3_RELAY_TURN_URLS` e `FEDI3_RELAY_TURN_PORT` (predefinito 3478),
usando l'host pubblico del relay. Compose li collega ai valori `TURN_USER`,
`TURN_PASS` e `TURN_PORT` usati dal profilo `turn`, così abilitare coturn lo
pubblica anche ai client. Una credenziale vuota disabilita il provisioning
automatico. Rotazione e quote di allocazione restano responsabilità
dell'operatore; usare credenziali con scope ristretto, mai un segreto
amministrativo TURN. Non c'è creazione automatica di account commerciali o
comunitari e nessuna garanzia implicita di disponibilità del servizio.

La schermata di chiamata Flutter carica il pool una volta per chiamata e aggiunge
ogni server con le proprie credenziali a ICE. Quando le credenziali ICE sono
configurate ma non è stato inserito alcun URL `turn:`, il client deriva anche gli
endpoint UDP e TCP `turn:<relay-host>:<port>`, così un'istanza coturn distribuita
accanto al relay viene usata senza URL digitati a mano. Il bridge nativo Linux
riceve un URL per voce. La configurazione ICE locale esplicita resta supportata e
un fallimento del provisioning lascia utilizzabili i percorsi STUN/diretti. Le
offerte e le risposte portano i candidati raccolti in una breve finestra, e una
coppia di candidati fallita attiva fino a due riavvii ICE automatici con
un'azione Retry manuale successiva. I data channel WebRTC Rust mantengono la loro
configurazione ICE separata.

Riferimento: [configurazione TURN WebRTC](https://webrtc.org/getting-started/turn-server).

## Replicazione pubblica

Il core pubblica già le attività pubbliche degli outbox su `/fedi3/global/1`
tramite gossipsub libp2p firmato. I ricevitori le instradano verso l'endpoint di
ingest del feed globale controllato dalle policy e le persistono in SQLite
locale. Gli outbox degli attori seguiti possono anche essere sincronizzati tramite
il protocollo di richiesta P2P diretto. I peer conosciuti e lo scambio di contatti
firmati consentono di riconnettersi senza un bootstrap relay.

La regressione `public_gossip_replicates_to_disk_without_a_relay` avvia due nodi
libp2p reali su TCP, disabilita discovery pubblica/mDNS, non usa relay e verifica
la persistenza durevole della replica. È una regressione di trasporto locale, non
un test tra due NAT fisici. Il gossip è best-effort, richiede peer connessi e non
è una replicazione storica completa né un protocollo di disponibilità. Le firme
del trasporto gossipsub identificano i peer; non autenticano in modo indipendente
l'attore ActivityPub dichiarato dentro ogni attività. I controlli esistenti di
origine e moderazione dell'ingest non vanno descritti come verifica end-to-end
della firma dell'attore.

## Trasparenza

`/transparency` è una pagina HTML pubblica di policy; `/_fedi3/relay/transparency`
restituisce uno snapshot di policy firmato Ed25519 con limiti delle code,
retention, numero di tunnel online e se il push è configurato. Non pubblica
elenchi di account, token dei dispositivi, contenuti o indirizzi IP. Decodificare
in base64 `payload_b64` e verificare quegli stessi byte contro `signature_b64` e
`public_key_b64`. Fissare la chiave pubblica del relay in modo indipendente: uno
snapshot autofirmato da solo non stabilisce fiducia.

Lo snapshot ancora un registro transazionale dei forward. I commitment pubblici e
le ricevute private e firmate dei forward sono implementati, comprese le conferme
del core firmate in modo indipendente. Vedere
[push_forward_receipts.md](push_forward_receipts.md). Le ricevute di lettura umana
e i testimoni indipendenti non sono implementati.
