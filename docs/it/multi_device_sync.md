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
  "v": 1,
  "alg": "aes-256-gcm",
  "kdf": "hkdf-sha256",
  "salt_b64": "...",
  "nonce_b64": "...",
  "ciphertext_b64": "...",
  "mac_b64": "..."
}
```

La chiave AES-256 è derivata con HKDF-SHA256 dal segreto condiviso (il token del
relay per il blob dell'account, il codice di abbinamento per una sessione QR),
dal salt casuale e dalla stringa info `fedi3-identity-v1`. Il relay memorizza
solo l'envelope e non può decifrarlo.

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
