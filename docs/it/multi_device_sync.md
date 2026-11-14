# Sincronizzazione chat multi-dispositivo, accesso QR e proprietà delle chiamate

Questo documento descrive il modello multi-dispositivo implementato da Fedi3:
un account (una identità attore) può essere usato da più client, la cronologia
delle chat viene replicata su ogni client collegato e una chiamata appartiene al
client che la accetta. È anche il contratto di implementazione per il core, il
relay e il client Flutter.

## Obiettivi

- Collegare un secondo client a un account esistente senza perdere l'identità
  dell'attore, i follower, i post o le chiavi di cifratura.
- Sincronizzare messaggi diretti e di gruppo, modifiche, eliminazioni, reazioni,
  ricevute e segnalazione delle chiamate su ogni client collegato dell'account.
- Consegnare gli envelope chat a un client che era offline quando sono stati
  inviati.
- Lasciare che ogni client collegato squilli per una chiamata in arrivo, ma far
  sì che esattamente un client possieda la chiamata: quando un client risponde,
  gli altri smettono di squillare e non possono entrare due volte nella stessa
  chiamata.
- Impedire al relay di leggere il contenuto delle chat o la chiave privata
  dell'account.

## Identità dell'account

Un account è identificato dal suo URL attore ActivityPub e dalla sua chiave di
firma RSA. Ogni client dello stesso account deve usare la stessa chiave privata
RSA, altrimenti i server remoti rifiutano le firme degli altri client. Anche la
chiave di identità chat (Kyber768) deve essere condivisa, perché gli envelope
chat sono cifrati una sola volta per l'account e consegnati a ogni client
dell'account.

Il pacchetto di identità trasferito a un nuovo client contiene:

- `identity_private_key_pem`: la chiave privata RSA dell'attore;
- `chat_identity_public_b64` e `chat_identity_secret_b64`: la coppia di chiavi
  di identità chat Kyber768;
- la configurazione dell'account (username, dominio, URL base pubblico, URL
  WebSocket del relay e token del relay);
- il profilo dell'account e le impostazioni di moderazione (nome visualizzato,
  descrizione, avatar e banner, campi del profilo, approvazione manuale dei
  follower, domini e attori bloccati, relay AP, modalità di consegna dei post),
  così il client collegato si presenta e si comporta come quello originale;
- `chat_snapshot` (solo per l'abbinamento QR): lo stato attuale delle chat del
  client di origine, così il nuovo client parte con gli stessi thread, messaggi,
  reazioni, ricevute, pin e segnali di chiamata invece di una cronologia vuota.

I valori specifici del dispositivo (peer id P2P, indirizzo di bind locale, token
interno, directory dati locale) non vengono trasferiti. Ogni client mantiene i
propri.

### Envelope di identità sigillato

I pacchetti di identità vengono sempre sigillati prima di lasciare il
dispositivo. L'envelope è JSON:

```json
{
  "v": 2,
  "alg": "aes-256-gcm",
  "kdf": "hkdf-sha256",
  "salt_b64": "...",
  "nonce_b64": "...",
  "ciphertext_b64": "...",
  "mac_b64": "..."
}
```

La chiave AES-256 deriva con HKDF-SHA256 da una chiave di recupero indipendente
per il blob account oppure dal segreto casuale del QR. Il token relay serve
soltanto per autenticarsi e non cifra più le identità. La chiave di recupero è
visibile nelle impostazioni Sicurezza e deve essere conservata separatamente.
Per un nuovo client servono token e chiave di recupero, oppure QR o backup.
Un recupero fallito non genera nuove identità sostitutive.

Il relay riceve soltanto l'identificatore SHA-256 del segreto QR, con dominio
`fedi3-pair-id/v2:`, codificato base64url senza padding. Il segreto originale
rimane nel QR. Le specifiche esatte della derivazione sono nel documento inglese.
Gli upload v1 sono rifiutati; i vecchi blob sono conservati per la migrazione ma
non vengono serviti. Il client esistente aggiornato riscrive il proprio blob v1
con le stesse chiavi locali. Ciò non annulla eventuali esposizioni storiche.

## Endpoint del relay

Tutti gli endpoint seguenti sono serviti dal relay. `Authorization: Bearer
<token>` usa il token del relay dell'account; il relay memorizza solo il suo hash
SHA-256, quindi il token non è mai leggibile a riposo.

### Blob di identità dell'account

- `PUT /_fedi3/users/:user/identity`: memorizza o sostituisce l'envelope di
  identità sigillato dell'account. Corpo: l'envelope qui sopra. Risposta: `204`.
- `GET /_fedi3/users/:user/identity`: restituisce l'envelope sigillato o `404`.

Un client carica l'envelope solo quando il relay non ne ha uno, quindi il primo
client di un account stabilisce l'identità canonica. Un nuovo client lo scarica
durante l'accesso manuale e avvia il core con le chiavi trasferite.

### Sessione di abbinamento QR

- `PUT /_fedi3/pair/:code?user=:user`: memorizza un pacchetto di abbinamento
  sigillato per un codice di breve durata. Risposta: `204`. Il pacchetto è
  l'envelope di identità con la configurazione dell'account e lo snapshot chat
  dentro il testo cifrato. Le sessioni scadono dopo dieci minuti.
- `GET /_fedi3/pair/:code`: restituisce il pacchetto sigillato senza
  autenticazione (il codice è un segreto di 128 bit) oppure `404`/`410` se
  manca/scaduto.

Il client già autenticato mostra un codice QR contenente
`fedi3://pair?relay=<relay-base>&code=<code>`. Prima di mostrarlo esporta il
proprio pacchetto di identità (chiavi di identità, configurazione dell'account e
snapshot chat), lo sigilla con il codice e lo carica sul relay. Il dispositivo
non ancora autenticato scansiona il QR, scarica il pacchetto sigillato, lo
decifra e avvia il proprio core con l'identità trasferita e lo snapshot chat.

### Spool di consegna chat

- `POST /users/:user/_fedi3/chat/inbox` (l'endpoint esistente per gli envelope
  chat): il relay aggiunge l'envelope allo spool del destinatario prima di
  inoltrarlo a un client online. La risposta è `202` anche quando nessun client
  è connesso.
- `GET /_fedi3/chat/spool?user=:user&after=:id&limit=:n`: restituisce gli
  envelope memorizzati per l'account dopo l'id di spool indicato, dal più vecchio
  al più recente, come
  `{"items":[{"id":1,"envelope":{...}}],"latest_id":1}`.

Gli envelope vengono eliminati dopo 90 giorni e limitati per account, così lo
spool non cresce senza limite. Lo spool memorizza solo testo cifrato.

## Endpoint del core

- `GET /_fedi3/identity/export` (token interno): restituisce
  `{"public_key_pem","private_key_pem","chat_identity_public_b64","chat_identity_secret_b64"}`.
- `GET /_fedi3/chat/snapshot` (token interno): restituisce lo stato delle chat
  usato dal pacchetto di abbinamento QR:
  `{"v":1,"threads":[],"members":[],"messages":[],"statuses":[],"reactions":[],
  "pins":[],"call_signals":[],"truncated":false}`. I messaggi sono limitati ai
  10.000 più recenti (o a circa 8 MB di JSON); `truncated` segnala quando i
  messaggi più vecchi sono stati esclusi.
- `POST /_fedi3/chat/snapshot` (token interno): importa uno snapshot in modo
  idempotente. Thread, membri, messaggi, stati, reazioni, pin e segnali di
  chiamata vengono uniti al database locale; i duplicati vengono ignorati.
- La configurazione di avvio del core accetta `identity_private_key_pem`,
  `chat_identity_public_b64` e `chat_identity_secret_b64`. Quando la chiave
  privata è fornita diventa l'identità locale; quando le chiavi chat sono
  fornite e il database locale non ne ha, diventano l'identità chat locale.
- Il bundle chat non pubblica prekey. Gli envelope sono cifrati con la chiave di
  identità chat dell'account, quindi ogni client collegato può decifrarli. Le
  prekey restano supportate per decifrare envelope prodotti da client
  precedenti.

## Replicazione delle chat

Ogni client interroga lo spool chat del relay con il token del relay dell'account
e applica localmente i nuovi envelope:

1. scarica gli envelope dopo il cursore memorizzato;
2. verifica e decifra ogni envelope;
3. memorizza il payload attraverso lo stesso percorso di ingresso usato dalla
   consegna live;
4. emette un evento UI `chat` così le schermate aperte si aggiornano;
5. avanza il cursore.

Il cursore è persistito per client. Un client appena collegato parte dal più
vecchio envelope ancora nello spool, il che gli dà la cronologia recente anche
quando gli altri client sono offline.

I payload chat originati da un client vengono consegnati anche allo spool
dell'account stesso (copia di self-sync), così i messaggi inviati da un client
compaiono su ogni altro client dell'account. Questo vale per messaggi, modifiche,
eliminazioni, reazioni, ricevute e segnalazione delle chiamate.

## Chiamate

Gli inviti alle chiamate vengono consegnati a ogni client collegato del
destinatario attraverso lo spool. Ogni client squilla. Quando un client
risponde, invia un segnale `accept`; il core lo copia nello spool dell'account,
così gli altri client del destinatario vedono l'accept e chiudono la finestra di
squillo. Un client già in chiamata non squilla di nuovo. Il chiamante mantiene
come peer della chiamata il primo client che ha accettato e ignora le accettazioni
successive.

Una chiamata è quindi posseduta da esattamente un client, mentre ogni client ha
la possibilità di rispondere.

## Limitazioni

- La cronologia più vecchia della ritenzione dello spool del relay è disponibile
  su un nuovo client tramite il pacchetto di abbinamento QR, che porta uno
  snapshot delle chat. Un accesso manuale riceve solo la cronologia dello spool.
- Gli account senza relay (modalità solo P2P) mantengono la consegna diretta
  esistente e non usano lo spool; la cronologia multi-dispositivo è best effort.
- Per un account creato prima del supporto multi-dispositivo, il primo client
  avviato pubblica la propria identità. Se un client nuovo si avvia prima che il
  client originale abbia pubblicato, usare l'abbinamento QR: trasferisce
  l'identità originale ed evita una chiave dell'account non corrispondente.

### Isolamento delle code e dei dispositivi (14 novembre 2026)

Il relay verifica schema e firma RSA del mittente prima di accettare un envelope
chat. I limiti di righe e byte sono atomici: una coda piena rifiuta nuovi elementi
senza eliminare quelli accettati. La cancellazione account comprende identità,
sessioni QR, coda chat e tutte le registrazioni push; i lock impediscono scritture
tardive dopo la cancellazione.

Ogni installazione push ha il proprio `installation_id`; la revoca usa
`X-Fedi3-Push-Installation` e riguarda solo quel dispositivo. Le vecchie richieste
senza ID modificano soltanto lo slot legacy. Il limite è 32 installazioni attive.
I client desktop mantengono il core quando nascosti; Android lo delega al servizio
background, preservando le chiamate. Un avvio fallito non avanza il cursore della
coda, che è separato per account e relay.

I post propri sono dati permanenti: il core conserva gli alias autore attuali e
precedenti in `local_meta` e li esclude dalla pulizia della cache per TTL, numero
e byte, anche nei vecchi record privi di `actor_id`. Questo non modifica lo stato
di pin mostrato nell’interfaccia.


## Identità e sincronizzazione dello stato sociale

Un account è identificato dall’URL completo dell’attore e dalla chiave RSA. Lo
stesso nome su relay diversi identifica account distinti. I peer ID identificano
i dispositivi: la directory li raggruppa sotto l’attore. Per aggiungere un secondo
dispositivo bisogna abbinarlo o importare l’identità esistente. Una nuova chiave
con lo stesso nome e token non può sostituire l’account già registrato.

La directory locale è vincolata ad attore e chiave pubblica. Le directory storiche
compatibili restano al loro posto; i nuovi account usano percorsi distinti per
attore. Una chiave importata diversa, una directory appartenente a un altro
attore o una chiave mancante su un account esistente interrompono l’avvio senza
sovrascrivere l’identità.

I dispositivi abbinati replicano campi del profilo, follower, seguiti, richieste,
post propri, outbox, reazioni e metadati degli allegati. I record AES-256-GCM sono
cifrati con materiale derivato dalla chiave privata condivisa e firmati RSA. Il
relay conserva record opachi tramite GET/PUT autenticati
`/_fedi3/account/{user}/state`; i peer dello stesso account li scambiano anche via
`/.fedi3/device/account` con richieste firmate. I file multimediali restano nel
backend configurato: questa sincronizzazione non copia file locali arbitrari e
non sostituisce il backup completo.

Le modifiche si uniscono per record e per singolo campo del profilo. Rimozioni
offline persistono come tombstone; un vecchio post modificato non annulla una
cancellazione. Le scritture locali concorrenti al download vengono conservate.
Le conferme riguardano soltanto la revisione caricata. I cursori sono distinti
per relay; un cambio di relay ripubblica tutto lo stato conservato. Gli errori
lasciano intatti i dati locali e sono visibili nelle impostazioni di rete.

Record e tombstone dell’account non scadono. Il relay rifiuta atomicamente nuovi
caricamenti oltre quota senza espellere dati sociali: 1.000.000 record e 1 GiB
per account per impostazione predefinita. Opzioni:
`FEDI3_RELAY_ACCOUNT_MAX_ROWS` (1.000–2.000.000),
`FEDI3_RELAY_ACCOUNT_MAX_BYTES` (1 MiB–16 GiB). L’eliminazione dell’account elimina
anche replica e chiave vincolata. La retention dello spool chat resta separata.

I vecchi client che firmano le ricevute dei forward possono provare la chiave
tramite una sfida fresca sul tunnel. Lasciare disattivata l’opzione di compatibilità
`FEDI3_RELAY_ALLOW_LEGACY_TUNNELS=true`, che aggira questa protezione.
