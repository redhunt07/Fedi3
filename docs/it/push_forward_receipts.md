# Push nativi e forward verificabili

Fedi3 supporta notifiche push native opt-in su Android e iOS e un registro
transazionale dei forward le cui voci possono essere verificate in modo
indipendente dall'operatore del relay. Questa pagina copre i connettori mobile,
il contratto relay/gateway, i formati delle ricevute e gli strumenti di verifica.
Per la policy di retention e trasparenza lato relay, vedere
[network_resilience.md](network_resilience.md).

## Connettori mobile

Il push è opt-in in **Impostazioni → Sicurezza → Notifiche push**. Il relay deve
annunciare un gateway configurato prima che l'app richieda il permesso di sistema.
I servizi dei provider e le loro credenziali restano configurati dall'operatore;
nessun account provider, abbonamento o servizio push pubblico viene creato
silenziosamente.

Android usa il connettore nativo ufficiale UnifiedPush (tramite
`unifiedpush_android`). Scegliere un distributore installato. Il connettore gestisce
token di registrazione, rinnovo dell'endpoint, validazione del mittente e
decifratura Web Push. Fedi3 invia al proprio relay l'endpoint e la sua chiave di
cifratura P-256/segreto auth. Solo JSON decifrato e limitato con
`event: "sync"` è accettato come wakeup. Nessun mittente, conversazione o testo
di messaggio è inserito nel payload push.

Se Android avvia un motore Flutter headless, Fedi3 mostra una notifica generica e
lascia lo spool durevole sul relay finché la UI non si apre. Non avvia un core
nativo concorrente in un altro motore. Un motore UI già esistente può
sincronizzare il proprio core. Il permesso per le notifiche viene richiesto
durante l'attivazione esplicita; i callback in background controllano solo il
permesso e non tentano mai di mostrare un prompt.

Su iOS, registrazione/rotazione del dispositivo APNs, fallimento della
registrazione e callback delle notifiche in background sono implementati in
`AppDelegate.swift`. I callback nativi restano in coda finché Dart è pronto. Al
massimo otto callback possono essere in attesa; ciascuno si completa una sola
volta, con un fallback nativo di 25 secondi. Dart unisce i wakeup e usa una
finestra di sync limitata, rilasciando un core avviato in background. La consegna
e l'esecuzione da parte del sistema operativo restano discrezionali. Credenziali
mancanti o bloccate rimandano il sync senza sostituire un profilo. Le credenziali
push usano la classe Keychain device-only, after-first-unlock; abilitare il push
risalva in quella classe le credenziali relay iOS esistenti.

Il rinnovo dell'endpoint viene ritentato quando la UI torna in primo piano. La
disattivazione locale blocca subito i callback e richiede revoca al relay e
deregistrazione nativa. Cambi di account e di relay invalidano lo scope della
registrazione. La revoca sul relay può fallire offline: il connettore locale resta
disabilitato e la registrazione sul server scade dopo 30 giorni, salvo
revoca/sostituzione precedente. Al momento è conservata una sola installazione
mobile per account sul relay.

## Configurazione e protocollo del gateway

Impostare questi valori nel `.env` privato del relay, poi ricreare il servizio:

- `FEDI3_RELAY_PUSH_GATEWAY_URL`: endpoint HTTPS pubblico che implementa il
  contratto seguente.
- `FEDI3_RELAY_PUSH_GATEWAY_TOKEN`: almeno 32 caratteri casuali, condivisi con
  quel gateway.
- `FEDI3_RELAY_PUSH_PROVIDERS`: solo i provider effettivamente supportati dal
  gateway, ad esempio `unifiedpush,apns`.
- `FEDI3_RELAY_PUSH_VAPID_PUBLIC_KEY`: chiave pubblica P-256 non compressa di 65
  byte, opzionale, base64url senza padding, corrispondente alla chiave privata
  VAPID del gateway.

`GET /_fedi3/relay/push-config` espone solo impostazioni enabled/provider/VAPID
pubblico. `PUT /_fedi3/relay/push/{username}` richiede il Bearer token
dell'account e accetta:

```json
{"provider":"unifiedpush","token":"https://push.example/opaque-endpoint","webpush_keys":{"pub_key":"BASE64URL_P256","auth":"BASE64URL_AUTH"}}
```

Per APNs usare `{"provider":"apns","token":"HEX_DEVICE_TOKEN"}` senza chiavi Web
Push. La registrazione richiede un account abilitato, valida la forma di
provider/token/chiavi e rifiuta destinazioni push HTTP private. L'endpoint viene
rinnovato per 30 giorni. `DELETE` lo revoca; una `GET` autenticata restituisce
solo `pending_items`. Token e metadati di cifratura non sono mai restituiti dalle
API pubbliche di discovery.

Il gateway riceve una POST HTTPS autenticata con:

```json
{"provider":"unifiedpush","token":"https://push.example/opaque-endpoint","event":"sync","ttl_secs":86400,"webpush_keys":{"pub_key":"BASE64URL_P256","auth":"BASE64URL_AUTH"}}
```

Deve cifrare i messaggi UnifiedPush secondo RFC 8291, traducendo `pub_key` nel
`p256dh` della sottoscrizione Web Push, e usare la chiave privata VAPID
configurata dove richiesto. Per APNs deve usare team/key/topic appropriati e
l'endpoint sandbox o production, con payload
`{"aps":{"content-available":1},"fedi3":{"event":"sync"}}`,
`apns-push-type: background`, priorità 5 e un identificatore di coalescing. Il
gateway deve autenticare il chiamante, limitare le destinazioni, unire i retry e
rispettare il TTL; conosce la destinazione del dispositivo, non i contenuti della
chat né le credenziali dell'account. Questo repository definisce il contratto del
gateway; non contiene né attiva un deployment di un gateway provider. I
fallimenti dei provider non eliminano lo spool.

Il relay limita i tentativi di wakeup a uno per account al minuto. Un worker
separato ritenta gli account offline in batch cursore di 64, otto richieste
concorrenti, timeout HTTP di quattro secondi e nessun redirect. Non blocca la
pulizia dello spool. Senza configurazione del gateway non viene tentato alcun
wakeup in uscita.

## Build dei client mobile

Su Linux con Flutter, un JDK 17 completo, Android SDK 36 e NDK 28.2.13676358:

```sh
scripts/build_android.sh debug
```

Questo compila e impacchetta il core Rust ARM64, non solo la shell Flutter.
L'API minima Android è 24. `release` richiede `app/android/key.properties` e il
keystore di firma dell'operatore. Le librerie native/APK generati sono ignorati da
Git. I client Rust HTTPS/WSS mobile includono le root di fiducia Mozilla perché
le sandbox mobile non espongono le directory CA OpenSSL del desktop. La verifica
dei certificati resta attiva.

Per iOS usare macOS/Xcode. Una fase di build Runner invoca
`scripts/build_ios_core.sh`, producendo un archivio Rust statico per le
architetture device/simulator e mantenendo i simboli di processo FFI tramite le
impostazioni del linker xcconfig. Questa release Linux non può eseguire né
validare la build dell'Apple SDK. Configurare team Apple, bundle identifier e
provisioning profile con Push Notifications e Background Modes → Remote
notifications. `Runner.entitlements` usa `FEDI3_APNS_ENVIRONMENT`: Debug
predefinito `development`, Release/Profile `production`. Allineare ambiente/topic
APNs del gateway. Linux non può compilare, firmare né provare il target
UIKit/APNs; resta necessario un test su dispositivo provisionato.

## Significato delle ricevute

Il relay registra i forward non-GET e gli inserimenti riusciti nello spool in un
registro additivo e transazionale. Ogni ricevuta privata è firmata Ed25519 con la
chiave esistente del relay e lega sequenza, link precedente, identità del relay,
timestamp, un nonce opaco, stato HTTP e digest della richiesta. Quel digest lega
metodo, percorso, query, nonce e byte del corpo; non attesta ogni header HTTP in
ingresso.

Il core firma una conferma RSA-SHA256 indipendente dopo che il suo handler HTTP
restituisce. Lega attore, lo stesso digest della richiesta, stato, digest del
corpo della risposta e tempo. Un nonce fresco di 256 bit impedisce il riuso per un
tentativo diverso. Il relay verifica la conferma contro la chiave dell'attore in
cache prima di assegnare:

- `spooled`: accodato sul relay; nessuna dichiarazione di consegna al core.
- `core_acknowledged`: risposta del core verificata con stato 2xx.
- `core_rejected`: risposta del core verificata con stato non-2xx.
- `unverified_forward_response`: nessuna conferma del core verificata, inclusi
  client più vecchi o fallimenti di trasporto. Lo stato è registrato senza
  promuovere la dichiarazione.

Una risposta riuscita dell'handler del core non è una ricevuta di lettura umana,
né prova che un'altra istanza fediverse remota abbia elaborato un'attività in
uscita. La ricevuta di un envelope cifrato dello spool riprodotto lega i byte di
trasporto di quell'envelope.

Se la registrazione nel journal fallisce, il relay riporta
`X-Fedi3-Receipt-Status: unavailable` e registra il fallimento nel log; non
fabbrica una firma né annulla un'operazione HTTP già elaborata dal core.
L'emissione della ricevuta non garantisce che un operatore malevolo abbia
registrato ogni evento.

## Recupero e verifica indipendente

I forward restituiscono l'envelope firmato come JSON base64 in
`X-Fedi3-Forward-Receipt`. I proprietari di account abilitati possono anche
recuperare `GET /_fedi3/relay/receipts/{username}?after=0&limit=10` usando il
Bearer token del proprio relay. La `GET` pubblica
`/_fedi3/relay/forward-log?after=0&limit=100` restituisce solo i commitment, più
un checkpoint firmato Ed25519 per quella testa esatta. `/transparency` collega
queste API; il suo snapshot di policy firmato include anche la testa del journal
e la retention.

Una voce pubblica ha sequence, previous_hash, commitment e hash. Il commitment è
lo SHA256 del payload privato firmato. Il suo link hash è lo SHA256 dell'array
JSON UTF-8 compatto
`["fedi3-forward-log-link/v1", sequence, previous_hash, commitment]`.
Per un documento pubblico completo di pagina/log usare
`python3 scripts/verify_forward_receipt.py public-log.json --public-log --relay-key PINNED_KEY`.
Se serve la paginazione, raccogliere le voci fino alla testa firmata del
checkpoint; il verificatore rifiuta catene incomplete, riordinate o alterate.
Chiunque può verificare la sequenza dei link contro la testa firmata senza vedere
i dati privati di account/attore. I payload includono nonce casuali, impedendo
una semplice ipotesi del contenuto dai commitment pubblici.

Per verificare offline una ricevuta conservata (richiede Python `cryptography`):

```sh
python3 scripts/verify_forward_receipt.py receipt.json \
  --relay-key PINNED_ED25519_PUBLIC_KEY_BASE64 \
  --core-key independently_verified_actor_public_key.pem
```

Fissare la chiave del relay e quella dell'attore in modo indipendente; fidarsi
ciecamente delle chiavi incluse dall'operatore prova solo l'autoconsistenza. Il
verificatore rifiuta una conferma del core dichiarata senza la chiave del core
fornita indipendentemente. Conservare checkpoint/ricevute firmati esternamente
per rilevare storie incoerenti. Testimoni indipendenti e confronto automatico tra
checkpoint di relay differenti non sono implementati.

Inserimenti nel journal e inizializzazione delle chiavi sono serializzati
transazionalmente (SQLite IMMEDIATE / lock di riga PostgreSQL). Valori
predefiniti: `FEDI3_RELAY_RECEIPT_TTL_SECS=2592000` (30 giorni) e
`FEDI3_RELAY_RECEIPT_MAX_ROWS=100000`. I limiti sono 60 secondi–365 giorni e
1000–1000000 righe. La potatura del prefisso conserva un'ancora firmata della
testa e continua la numerazione delle sequenze. La pulizia periodica applica il
TTL anche quando non arrivano nuovi eventi. La cancellazione dell'account elimina
i suoi envelope privati/username ma conserva i commitment opachi fino alla
normale potatura del prefisso. I binari relay più vecchi ignorano le tabelle
additive del journal; il rollback del deployment non richiede di eliminarle.

Riferimenti: [UnifiedPush Android](https://unifiedpush.org/developers/spec/android/),
[connettore nativo Flutter ufficiale](https://pub.dev/packages/unifiedpush_android),
[registrazione Apple APNs](https://developer.apple.com/documentation/usernotifications/registering-your-app-with-apns).
