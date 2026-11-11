# Annunci Fedi3

Aggiornamenti ufficiali dai maintainer. Questa pagina è letta direttamente dal
repository Git e il client la controlla periodicamente. La controparte inglese è
[ANNOUNCEMENTS.md](ANNOUNCEMENTS.md).

## 2026-11-12 — Rilasciata Fedi3 0.11.11

La build 0.11.11 (4015) è disponibile sulla [pagina delle release](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.11) con l'AppImage Linux x86_64, lo ZIP Windows x64, gli APK Android arm64-v8a e x86_64 e i checksum SHA256. Raccoglie il lavoro pubblicato da 0.11.10: sincronizzazione chat multi-dispositivo con abbinamento QR, note vocali e video riproducibili, il servizio Android solo-push con minor consumo in background e sospensione P2P, e il push gateway self-hosted opzionale. La direzione del login QR è invertita: il client già loggato mostra il codice e il nuovo dispositivo lo scansiona. [Changelog](CHANGELOG.md) · [Screenshot](README.md#screenshots).

## 2026-11-11 — Servizio integrato solo-push: notifiche senza core in idle

Il foreground service Android può ora funzionare in modalità solo-push, senza app di terze parti e senza core in idle. Mentre l'app è in background il servizio interroga solo lo spool chat del relay via HTTPS autenticato ogni 15 secondi e salva l'ultimo id visto; quando arriva qualcosa di nuovo avvia il core locale on-demand, svuota lo spool, mostra le notifiche di chat, chiamate e timeline, e ferma di nuovo il core dopo 30 secondi di quiete. In idle non ci sono swarm P2P né tunnel relay.

Misurato su un Xiaomi 2201123G (Android 15): con lo spool stabilizzato il poller restituisce `items=0` e la CPU in idle del servizio è circa 0,1 % di un core, senza wakeup del core nei log. Un primo poll su un backlog ampio drena al massimo 50 elementi storici una volta sola e poi avanza il cursore. Le chiamate mantengono il core attivo per tutta la durata; gli account solo-P2P (senza tunnel relay) tengono il core attivo perché non c'è uno spool da interrogare. La modalità è opt-in in **Impostazioni → Sicurezza → Notifiche in background**, e il percorso UnifiedPush/gateway resta disponibile per chi preferisce un distributore.

## 2026-11-11 — Gateway push self-hosted con ntfy, per ogni relay

Fedi3 può ora risvegliare un telefono tramite un gateway push self-hosted invece di tenere vivo un servizio in background. Il nuovo `fedi3_push_gateway` (in `crates/fedi3_push_gateway`, distribuito con il profilo Compose `push` e un container ntfy) accetta i wakeup autenticati del relay e li consegna ai distributori UnifiedPush. Sono supportati due trasporti: distributori Web Push (NextPush) con `webpush_keys` e cifratura VAPID, e distributori basati su topic come ntfy, dove l'app registra un URL del topic senza chiavi e il gateway pubblica il wakeup in chiaro `{"event":"sync"}`. Il relay accetta registrazioni senza chiavi e il client mobile accetta wakeup in chiaro.

Ogni relay esegue la propria istanza di gateway e ntfy. Ogni URL è specifico del relay, quindi la configurazione di un relay non influenza gli altri. I relay con sottodomini DNS usano host dedicati; quelli senza li montano sotto un percorso del dominio del relay (`/_fedi3/push`) e ntfy sotto `/ntfy`, configurati nel `.env` di quel relay. `deploy/push_gateway/Caddyfile.example` mostra entrambi i layout.

Con un distributore installato e il push attivo, il client può fermare il proprio servizio in primo piano e lasciare che il distributore mantenga la connessione; le notifiche arrivano senza il core in esecuzione. La modalità FGS resta disponibile.

## 2026-11-11 — Sospensione P2P in background: consumo inattivo vicino alle app mainstream

Un client in background non tiene più viva l'intera mesh P2P. Quando la UI non è visibile e c'è un tunnel relay configurato, il core chiude le connessioni DHT e gossip e mantiene solo i peer relay che servono la mailbox store-and-forward e le nostre prenotazioni; i circuiti relay in ingresso restano aperti, così i peer che consegnano messaggi non vengono disconnessi a metà richiesta e costretti a riprovare. Tornando in foreground il core riavvia entrambi i DHT e ricollega subito i peer Fedi3 conosciuti, quindi il P2P riprende con la stessa raggiungibilità di prima. Gli account solo-P2P (senza tunnel relay) mantengono lo swarm attivo, perché il relay non copre la consegna.

Sono state allungate anche le cadenze residue in background: device sync a 60 s, mailbox a 120 s, keep-alive SSE a 30 s e ping del tunnel a 30 s (60 s in doze), con il watchdog del pong adeguato.

Su uno Xiaomi 2201123G (Android 15) la CPU del core in background è ora l'1,2-1,4 % di un core, rispetto al 5,6 % del giro precedente e a circa il 24 % prima dell'inizio dei lavori; l'app tiene 11 socket TCP (tunnel relay, HTTPS in pool e loopback della UI) invece di 138, il continuo viavai di circuiti relay che teneva sveglio il runtime è sparito e, una volta assestata la sincronizzazione, non si misura traffico radio in background. Timeline, chat, notifiche e ripresa del P2P in foreground sono state provate sul dispositivo.

## 2026-11-10 — Meno consumo di batteria e meno calore su Android

Il client non tiene più occupata la CPU del telefono mentre resta in background. Il core ora espone un profilo di alimentazione a livello di processo: l'app lo porta in foreground, background o doze seguendo il ciclo di vita dell'activity, e ogni poller di rete allunga la propria cadenza invece di girare a quella del foreground. I controlli dello spool chat passano da 3 s a 10 s (30 s in doze), la sincronizzazione dispositivi da 5 s a 30 s, la segnalazione WebRTC dall'intervallo configurato a 10 s, la mailbox P2P da 15 s a 60 s, la sincronizzazione relay e legacy a 15/30 minuti, e la coda di consegna, il fetch degli oggetti e la GC dello storage rallentano allo stesso modo. La latenza in foreground è invariata.

Due consumi di background sono stati eliminati del tutto: mDNS è ora disattivato per impostazione predefinita su Android e iOS (il socket multicast aperto teneva un wakelock WiFi multicast per ore; relay e DHT forniscono già la scoperta) e lo swarm libp2p è limitato a 48 connessioni stabilite, al massimo 2 per peer, con dial pendenti limitati. Prima del limite il DHT pubblico continuava a contattare ogni peer conosciuto e lo swarm superava le 130 connessioni inattive su un telefono, ed era questo a tenere calda la CPU. Gli indirizzi IPv6 vengono filtrati quando il dispositivo non ha una rotta IPv6, l'heartbeat di gossipsub è di 5 s invece di 1 s e le schede inattive non fanno più girare le animazioni.

Su uno Xiaomi 2201123G (Android 15) la CPU del core in background è scesa da circa il 24 % di un core (con picchi oltre il 60 %) al 5-10 %, il numero di connessioni resta tra 24 e 44 invece di salire oltre 130 e non viene più tenuto alcun wakelock multicast. Timeline, chat, notifiche e segnalazione WebRTC sono state provate sul dispositivo dopo la modifica.

## 2026-11-09 — Sincronizzazione multi-dispositivo, accesso QR e note vocali riproducibili

Le note vocali e le video note ora si caricano e si riproducono su entrambi i lati: il client legge il campo camelCase `mediaType` scritto dai core più recenti, ripiega sul nome del file o sull'estensione dell'URL quando il tipo è vuoto o generico e mostra un controllo di riproduzione sulla bolla del destinatario invece di un semplice allegato. Le video note mantengono il player inline con l'overlay di riproduzione; le note audio mantengono la bolla vocale dedicata. Il registratore ora abilita anche la traccia audio per le video note, così il clip registrato contiene il suono.

Un account può ora essere usato da più client. Il primo client pubblica sul relay un pacchetto di identità sigillato (chiave di firma RSA, chiave di identità chat Kyber e configurazione dell'account); il relay memorizza solo l'envelope sigillato e non può leggere le chiavi. Un nuovo client accede manualmente e scarica il pacchetto, oppure mostra un codice QR (`fedi3://pair?relay=…&code=…`) che il telefono già autenticato scansiona: il telefono sigilla la propria identità e uno snapshot delle chat con il codice monouso e li carica, e il desktop parte con lo stesso account, thread, messaggi, reazioni, ricevute, pin e segnali di chiamata. I valori specifici del dispositivo (peer id P2P, indirizzo di bind, token interno, directory dati) restano locali.

L'attività delle chat viene replicata su ogni client collegato attraverso uno spool del relay: messaggi, modifiche, eliminazioni, reazioni, ricevute e segnalazione delle chiamate sono memorizzati come testo cifrato per un massimo di 90 giorni e scaricati da ogni client dell'account. Un client che era offline si aggiorna quando si riconnette; un client con accesso manuale ricostruisce la lista chat dallo spool, mentre un client collegato via QR parte dallo snapshot di abbinamento. Le chiamate squillano su ogni client collegato, ma la prima accettazione possiede la chiamata: l'accept viene replicato sugli altri client dell'account, che smettono di squillare, e un client già in chiamata non squilla di nuovo.

Gli account solo P2P mantengono la consegna diretta esistente e non usano lo spool. La cronologia più vecchia della ritenzione dello spool è disponibile su un nuovo client tramite lo snapshot dell'abbinamento QR. La nuova voce **Collega un nuovo dispositivo** nelle Impostazioni avvia lo scanner su mobile o mostra il QR su desktop, e **Sincronizza attività tra dispositivi** in Rete controlla la replica dell'attività.

Il pacchetto di abbinamento trasporta anche il profilo dell'account e le impostazioni di moderazione, così un client collegato mostra lo stesso nome visualizzato, descrizione, avatar, banner, campi del profilo, liste bloccate, relay AP e modalità di consegna. Gli snapshot delle chat vengono importati in un'unica transazione e le cronologie grandi ricevono un timeout più lungo; dopo il riavvio del core il client scarta le socket keep-alive stantie e riavvia il core se sta ancora girando con la configurazione precedente, così la prima richiesta dopo l'abbinamento non fallisce più.

## 2026-11-09 — Prenotazioni relay stabili e P2P mobile raggiungibile

Il relay non rifiuta più a ripetizione le prenotazioni dei client: il budget per
peer sale a 8 e il rate limiter delle prenotazioni si ricarica nel tempo invece
di un token ogni due minuti. Il core chiede una sola prenotazione per relay
(quella risposta copre già TCP e QUIC), riprova con backoff esponenziale da 5
secondi fino a 2 minuti e riscopre periodicamente l'indirizzo P2P del relay,
così un telefono avviato prima che la rete fosse pronta diventa raggiungibile
senza riavviare l'app. Su Android il resolver DNS di sistema andava in panico
perché il contesto NDK non è inizializzato: il core ora intercetta il panico e
continua con indirizzi IP, quindi il P2P mobile si avvia davvero. Build 0.11.10
(2013).

## 2026-11-09 — Rilasciata Fedi3 0.11.9

La build 0.11.9 (2012) è disponibile nella [pagina delle release](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.9) con l'AppImage Linux x86_64, lo ZIP Windows x64, gli APK Android arm64-v8a e x86_64 e i checksum SHA256. Raccoglie le correzioni pubblicate dalla 0.11.1: pulizia del composer, task Android unico, indirizzi P2P relayed, normalizzazione dell'outbox migrato, banner del profilo, logo neon, particelle di navigazione continue e avvio Linux pulito. Lo ZIP Windows è compilato dalla CI sullo stesso tag. [Changelog](CHANGELOG.md) · [Screenshot](README.it.md#screenshot).

## 2026-11-09 — Avvio Linux senza avvisi superflui

Il client Linux non stampa più il critical ATK innocuo sull'ID del plug
mancante. L'embedder di Flutter chiama `atk_socket_embed` prima che ATK possa
fornire un plug ID; il runner ora filtra solo quel messaggio, quindi gli altri
diagnostici di accessibilità restano visibili. Build 0.11.9 (2012).

## 2026-11-09 — Effetti particellari continui nella barra di navigazione

Il campo di particelle della sezione selezionata e il marchio ora si animano in
modo continuo invece di fermarsi dopo qualche giro. Flutter sospende i ticker
quando l'app non è visibile, quindi il consumo in background non cambia. Build
0.11.8 (2011).

## 2026-11-09 — Pulizia composer, layout presenza e task Android unico

Il composer non ripete più i pulsanti di scelta file e MFM cheatsheet; l'azione
di allegato resta nell'area media e nella toolbar. Il selettore di presenza
tiene tutti e quattro gli stati su una sola riga sui telefoni stretti. Android
ora usa un task unico (`singleTask`) e la notifica keep-alive lo riutilizza,
quindi nei processi recenti non compare più una seconda sessione Fedi3 offline.
Build 0.11.7 (2010).

## 2026-11-09 — Correzione avvio del servizio keep-alive Android

Il servizio foreground keep-alive ora entra in foreground da `onCreate` e viene
avviato mentre l'app è ancora in primo piano (`onPause`), così la finestra di
cinque secondi di Android per `startForeground` non scade durante un lento
teardown del motore Flutter. Build 0.11.6 (2009).

## 2026-11-09 — Indirizzi P2P relayed e paternità degli outbox migrati

I core ora prenotano un circuito sull'infrastruttura P2P del relay e pubblicano
l'indirizzo relayed nel documento actor, così i peer dietro NAT possono
chiamarsi direttamente attraverso il relay. Le attività degli outbox migrati
vengono riscritte sull'actor corrente nel core e nel proxy del relay, così le
istanze classiche vedono l'autore giusto. Il follow Alice → RedHunt07 è
accettato negli account di test attuali, quindi arrivano anche i post riservati
ai follower.

## 2026-11-09 — Timeline: post federati e coerenza dei profili

I post pubblici degli account con richiesta di follow ancora in attesa vengono
ora raccolti nella timeline federata invece di essere ignorati fino
all'accettazione. Le pagine profilo tengono solo i post dell'account e uniscono
i post pubblici già nella cache locale, così un account migrato non mostra più
i post di un'altra identità. L'outbox di un account migrato riscrive la
paternità delle vecchie attività sull'actor corrente. La build Android è 0.11.5
(2008).
[Documentazione](docs/it/app_guide.md).

## 2026-11-09 — I banner dei profili dietro l'avatar

I banner dei profili ora riempiono l'intestazione del profilo dietro l'avatar
invece di una scheda separata sotto. Il gradiente predefinito viene usato solo
se l'account non ha un'immagine di banner. La build Android è 0.11.5 (2008).
[Documentazione](docs/it/app_guide.md).

## 2026-11-09 — Fedi3 0.11.4: nuovo logo e icone delle app

Il marchio di Fedi3 è ora la F neon retrò su un globo orbitale. Sostituisce il
marchio segnaposto nella schermata di benvenuto e nella shell, ed è l'icona di
avvio per l'AppImage desktop, Android, iOS, macOS e Windows. La build Android è
0.11.4 (2007). Android ha anche un'icona adattiva e un'icona monocromatica per
le notifiche.
[Documentazione](docs/it/app_guide.md).

## 2026-11-09 — Chiamate: la condivisione schermo mantiene il microfono e il formato della webcam

Condividere lo schermo non silenzia più chi parla: la chiamata invia microfono
e audio di sistema del PC come tracce separate, così l’interlocutore sente
entrambi. Su Linux l’audio di sistema viene preso dall’uscita predefinita;
se non è scelta un’uscita della chiamata, l’app la sposta su un altro
dispositivo per la durata della condivisione, così l’interlocutore non risente
la propria voce attraverso il loopback.
Le immagini della webcam ora rispettano il formato nativo invece di essere
tagliate alla forma del riquadro, e le chiamate uno-a-uno occupano lo spazio
disponibile. Su Android e iOS le videochiamate hanno un pulsante per passare
dalla fotocamera frontale a quella posteriore; la build Android è 0.11.3
(2006). I suoni di notifica e chiamata ora usano i toni di messaggio, menzione e
squillo di Discord. **Impostazioni > Audio** su desktop mostra lo stesso
suggerimento di routing.
[Documentazione](docs/it/app_guide.md).

## 2026-11-08 — Chiamate: TURN automatico e impostazioni audio desktop

Le chiamate squillavano, venivano accettate e si chiudevano correttamente, ma
audio e video non passavano mai. La causa era l’ICE: le credenziali TURN
configurate venivano ignorate se non si digitava a mano un URL `turn:`, quindi
le chiamate con solo STUN fallivano quando un interlocutore era dietro una VPN
o un NAT dell’operatore. Ora il client ricava gli endpoint coturn dall’host del
relay, offerte e risposte includono i candidati raccolti in una breve finestra
e un percorso media fallito avvia fino a due riavvii ICE automatici con
l’azione «Riprova». I relay possono pubblicare il pool coturn agli account
autenticati (`FEDI3_RELAY_TURN_*`) e i segnali di chiamata in ritardo non
vengono più scartati dopo un minuto. Su desktop **Impostazioni > Audio**
permette di scegliere e provare microfono e uscita usati dalle chiamate, senza
cambiare l’audio predefinito di sistema. Su Linux riproduzione e routing usano lo stack GStreamer e PulseAudio/PipeWire dell’host.
[Documentazione](docs/it/app_guide.md).

## 2026-11-08 — Release di correzione Android 0.11.1

La [0.11.1](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.1) corregge i
guasti di chiamate e notifiche Android segnalati dopo la 0.11.0. Il resource
shrinker della release rimuoveva i suoni delle notifiche, quindi Android
rifiutava ogni avviso con `invalid_sound`: chiamate e messaggi non producevano
alcuna notifica con l'app chiusa. Inoltre il motore keep-alive registrava i
plugin due volte e perdeva la configurazione nel Keystore, smettendo di
interrogare il core. Entrambi i problemi sono risolti, con un fallback di
configurazione privata dell'app per il motore headless. Le chiamate in arrivo
suonano in primo piano e in background, i segnali invite/accept vengono
ritentati, i retry della chat partono ogni cinque secondi e il composer della
chat e i controlli di chiamata restano sopra la barra di navigazione Android.
Le chiamate Android non espongono più la condivisione schermo, quindi le
chiamate audio/video non avviano il servizio MediaProjection; le chiamate
desktop la mantengono. Le finestre di chiamata in arrivo senza risposta scadono
dopo un minuto e si chiudono quando il chiamante riaggancia.
[Verifiche e limiti](docs/it/audit/release_0.11.1_2026-11-08.md).

## 2026-11-08 — Release 0.11.0 Linux e Android

La [0.11.0](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.0) unisce il client
Android aggiornato alle correzioni AppImage della 0.10.1. Include notifiche in
background, chiamate, chat con allegati e recupero dei post dei peer seguiti.
Sono disponibili AppImage Linux x86_64 e APK Android ARM64/x86_64 con SHA256.
Gli APK sono firmati con la chiave di debug locale e destinati all'installazione
manuale. [Verifiche e limiti](docs/it/audit/release_0.11.0_2026-11-08.md).

## 2026-11-08 — Media in chat, notifiche e aggiornamento del relay su Android

Il client Android è stato consolidato end-to-end su due device reali contro il
relay pubblico.

- La chat accetta foto, file, note vocali e videomessaggi, anche senza testo.
  Le registrazioni vuote vengono scartate invece di caricare un body vuoto e gli
  upload media non incappano più nel limite pensato per le attività federate.
- I videomessaggi producono file reali e mostrano un poster; le righe dei membri
  di gruppo mantengono i nomi leggibili.
- Le notifiche non si duplicano più e attività, chat e chiamate mantengono i tre
  suoni distinti anche con l'app chiusa.
- I peer Fedi3 seguiti vengono interrogati e riportati nel timeline home, così i
  post delle persone che segui compaiono senza aggiornamento manuale.
- Il relay pubblico è stato ri-deployato dal main corrente ed espone
  `/_fedi3/relay/push-config`. UnifiedPush resta opzionale finché un operatore non
  configura un gateway; nel frattempo il servizio in foreground copre le notifiche.
- Validazione: 51 test Rust e 189 test Flutter superati; la CPU in idle resta sotto
  l'1% di un core e la memoria stabile intorno ai 200 MB.


## 2026-11-08 — Pubblicata la 0.10.1: avvio AppImage Linux

[La versione 0.10.1 è pubblicata](https://github.com/redhunt07/Fedi3/releases/tag/v0.10.1)
per Linux x86_64 e Windows x64. Corregge il mancato avvio dell’AppImage 0.10.0 sui
desktop recenti: vecchie librerie del pacchetto nascondevano le dipendenze GLib di
sistema (`MOUNT_2_40` / `LIBSYSTEMD_251`). Fontconfig usa insieme librerie dei font
e configurazione del sistema.

Prima della pubblicazione la GUI del pacchetto supera ora i controlli obbligatori
di primo frame su Ubuntu 22.04 e 24.04, oltre a test e audit esistenti. Gli utenti
Linux con la 0.10.0 che non si avvia devono scaricare manualmente la 0.10.1: il
vecchio client non può aprire l’updater. Profili, chiavi, credenziali e messaggi
restano nelle cartelle esistenti; non servono reset o migrazioni.

[Verifica di pubblicazione](docs/it/audit/release_0.10.1_2026-11-08.md).

## 2026-11-08 — Pubblicata la 0.10.0: aggiornamenti client e sicurezza compatibile

[La versione `0.10.0` è pubblicata](https://github.com/redhunt07/Fedi3/releases/tag/v0.10.0) per Linux x86_64 e Windows x64, con checksum SHA256. I client
precedenti richiedono un primo aggiornamento manuale per ricevere il nuovo updater.

- Il client controlla le release stabili dalle impostazioni, segnala nuove
  versioni e scarica pacchetti verificati SHA256. Le AppImage Linux x86_64
  supportano sostituzione atomica e backup della versione precedente; Windows x64
  usa uno ZIP verificato con installazione guidata. Le altre piattaforme mantengono
  la propria fonte di installazione.
- Le operazioni RSA usano AWS-LC e il trasporto Rust WebRTC usa lo stack attuale.
  Chiavi dei profili, chiavi pubbliche, DID, firme e messaggi cifrati restano
  compatibili; l’interoperabilità tra vecchie e nuove implementazioni è verificata.
- Verifiche: 304 test Rust standard e 184 Flutter superati, insieme alle prove
  di interoperabilità RSA e WebRTC tra vecchie e nuove implementazioni. CI GitHub
  e build release native Linux/Windows superate. L’audit riporta zero vulnerabilità
  e nessun advisory ignorato; restano cinque avvisi di manutenzione.
- La diagnostica del client resta sul dispositivo, con log limitati ed
  esportazione manuale. Nessun invio automatico di telemetria o push Git.
- Corretti fallback della directory relay, messaggi di autenticazione del core
  e rilevamento RTT sotto il millisecondo.

Dettagli: [aggiornamenti](docs/it/updates.md), [verifica di compatibilità](docs/it/audit/crypto_compatibility_2026-11-08.md),
[politica di sicurezza](SECURITY.md) e [changelog](CHANGELOG.md).

## 2026-11-07 — Installazione del relay su YunoHost

`scripts/install_yunohost.sh` installa un nodo Fedi3 completo su YunoHost senza
conflitti con nginx sulle porte 80/443. Esegue l'installer con un proxy TLS
esterno, scrive una location nginx di YunoHost con supporto WebSocket e bypass
SSO, apre 4001/tcp e 4001/udp nel firewall di YunoHost e verifica l'endpoint
HTTPS pubblico. Un nodo completo contribuisce alla mesh HTTP dei relay ed
esegue l'infrastruttura libp2p di mailbox/circuit relay. Vedi
`docs/it/deploy_yunohost.md`.

## 2026-11-07 — Installer del server con un comando

Un nuovo installer configura un server Fedi3 su derivate Debian/Ubuntu e Arch.
Offre tre modalità: relay + infrastruttura P2P, solo relay oppure solo P2P, con
TURN e Meilisearch opzionali. Lo script installa Docker, controlla DNS e porte,
scrive `.env` e `relay-secrets.txt` con segreti casuali, seleziona il profilo
Compose, avvia lo stack e verifica gli health check. Vedi
`docs/it/deploy_server.md`.

## 2026-11-07 — Directory relay e classifica per latenza

La scoperta dei relay non dipende più da un singolo relay di bootstrap:

- `relays/relays.json` è una lista pubblica su Git che chiunque può estendere con
  una pull request; la CI valida ogni voce.
- Il client legge la lista, misura ogni relay dal dispositivo e li ordina per
  latenza in millisecondi, con errori TLS, timeout e rete espliciti.
- Il core Rust legge la stessa lista durante il sync relay, e il servizio relay
  può inizializzare la mesh HTTP da essa quando non ci sono seed manuali.
- Le copie in cache della directory mantengono utilizzabile la schermata
  iniziale quando GitHub non è raggiungibile.

## 2026-11-07 — Aggiornamento di sicurezza, affidabilità e repository

Questo aggiornamento rafforza federazione, relay e client senza cambiare il
protocollo:

- I fetch federati validano ogni redirect e limitano il corpo delle risposte; le
  firme dell'inbox del relay sono legate all'attore dichiarato.
- Relay: concorrenza DB limitata, media in streaming con Range/ETag, shutdown
  ordinato, quota media atomica e cancellazione account che rimuove i blob.
- Core: limite di 4 MiB sui frame P2P, deduplicazione inbox atomica, firme
  device più rigorose, classificazione dead-letter tipizzata e legacy sync
  limitata.
- Client: nessun blocco della UI quando il core fallisce, schermata di recupero
  del portachiavi, backup impostazioni cifrato, polling consapevole del
  foreground, tap sulle notifiche e bozze chat preservate.
- Repository: documentazione inglese con controparte italiana, controlli CI (Rust,
  Flutter, script e PostgreSQL), dati runtime ripuliti.

Nessun cambiamento che rompa il protocollo: l'interoperabilità con le istanze
ActivityPub classiche è preservata.

## 2026-11-02 — Chiamate E2E, note audio e video

Chiamate audio/video 1:1 e di gruppo in mesh P2P, con condivisione schermo e
audio del PC. La segnalazione passa negli envelope cifrati della chat, mentre i
media viaggiano diretti tra i peer in WebRTC. Aggiunte anche note audio e video
messaggi registrabili dal composer.

## 2026-11-02 — Ricerca globale nelle chat

La ricerca ora copre tutte le conversazioni dell'utente, resta locale al core
(nessuna query inviata alla rete), è limitata ai thread di cui si è membri e
cerca solo il testo dei messaggi. Il client apre direttamente il thread con il
filtro attivo.

## 2026-11-02 — Chat: fissati, ricerca completa e inviti

I gruppi possono fissare un messaggio condiviso (solo proprietario e
amministratori), la ricerca ora interroga tutta la cronologia nel core e i
gruppi hanno un link invito copiabile e un flusso per unirsi dalla schermata
Chat.

## 2026-11-02 — Chat: cerca e inoltra

Aggiunte la ricerca dei messaggi dentro una conversazione e l'inoltro di un
messaggio, con allegati, verso un'altra chat o gruppo. La ricerca filtra solo i
messaggi già caricati; l'inoltro usa il percorso E2E esistente.

## 2026-11-02 — Nuova chat in stile Telegram

La creazione di una chat ora è una schermata dedicata: scegli un contatto
reciproco (ordinato per interazioni o alfabeticamente) e apri subito la
conversazione, oppure seleziona più persone e crea un gruppo con nome. Nei
gruppi sono disponibili ruoli proprietario/amministratore/membro e la rinomina.

## 2026-11-02 — Scopri e notifiche raggruppate

La scheda Scopri ora mostra hashtag in tendenza e account suggeriti con follow
in un tap. Le notifiche ripetute dello stesso autore vengono raggruppate e
possono essere espanse.

## 2026-11-02 — Post salvati e disponibili offline

I segnalibri ora conservano una copia locale del post: la nuova schermata
«Salvati» (Configurazioni o rail destro) li mostra anche senza rete e permette
di rimuoverli con uno swipe.

## 2026-11-02 — I tag aprono davvero i post

Corretto il percorso dei tag: i conteggi considerano solo tag con post
esistenti, i nomi sono normalizzati e cliccando un hashtag si apre la ricerca
con i post corrispondenti, senza risultati vuoti.

## 2026-11-01 — Scopri gli hashtag in tendenza

Il rail destro mostra ora gli hashtag più usati nel Fediverso e li apre
direttamente nella ricerca. Corretta anche la ricerca hashtag, che prima
restituiva sempre zero risultati.

## 2026-11-01 — Notifiche e composer più usabili

Le notifiche ora sono righe compatte con avatar, stato di presenza e azioni
rapide. Il composer accetta il trascinamento dei file e la selezione nativa, con
anteprima degli allegati; la chat ha un campo di scrittura arrotondato in stile
Telegram con pulsante di invio a gradiente.

## 2026-11-01 — Chat in stile Telegram e presenza

Le chat hanno bolle con coda, avatar e spunte di consegna: singola, doppia e
doppia con indicatore quando tutti i membri hanno letto. Lo stato online,
occupato, AFK e offline viaggia tra utenti Fedi3 anche su relay diversi tramite
buste cifrate, scade automaticamente ed è visibile sui avatar del client.

## 2026-11-01 — Restyling del client

Nuova passata grafica: logo Fedi3 fisso con particelle luminose, bagliore aurora
nel rail, banner profilo con mesh e avatar sovrapposto, lista chat a schede,
composer in evidenza nella sidebar e azioni dei post più compatte. La ricerca è
raggiungibile dalla barra della timeline e gli stati vuoti sono brandizzati.

## 2026-11-01 — Composer avanzato e nuovo client

Il client Fedi3 introduce un composer completo in stile editor: toolbar per
titoli, caratteri, dimensioni, allineamento, liste, citazioni, link e media,
anteprima live e statistiche del documento. La pubblicazione resta compatibile
con il Fediverso.

- Nuovo rail con effetti particellari e logo fisso.
- Scorciatoie Ctrl+1..7, Ctrl+N, Ctrl+F ed Esc.
- Rimozione del composer rapido: la sidebar apre direttamente l'editor.

## 2026-10-30 — Specifica OpenAPI e interoperabilità

Pubblicata la specifica OpenAPI 3.1 di core e relay (`docs/openapi.yaml`), con
66 endpoint documentati. I test di interoperabilità con Mastodon e Misskey reali
restano verdi.

## 2026-10-25 — Sicurezza e decentralizzazione

Identità ancorata separata dal trasporto, spool cifrato e mesh dei relay sono in
produzione. Il client mostra lo stato del core e del tunnel in tempo reale.

---

Per segnalazioni e proposte: https://github.com/redhunt07/Fedi3/issues
