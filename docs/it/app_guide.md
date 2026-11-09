# Guida all'app FEDI3

FEDI3 è composta da due parti che girano sulla tua macchina:

- l'**app** Flutter, che è l'interfaccia;
- il **core** Rust locale, che gestisce identità, crittografia, cache e federazione.

Il core ascolta su `127.0.0.1` ed è protetto da un token interno generato automaticamente. Il relay è opzionale: senza relay puoi comunque usare P2P e chat, ma non sei raggiungibile dalle istanze del fediverso tradizionale.

## Primo avvio

1. Apri l'app. Se non esiste una configurazione, viene mostrata la schermata di benvenuto.
2. Compila:
   - **Dominio pubblico**: l'hostname con cui vuoi essere raggiungibile (es. `mio.nome`). Deve puntare alla tua istanza o al relay che usi.
   - **Username**: solo lettere, cifre e `_`, da 1 a 64 caratteri. Non può essere cambiato senza migrare l'account.
   - **Bind locale**: lascia `127.0.0.1` a meno che tu non voglia esporre il core in LAN.
   - **Relay**: URL del relay (`https://relay.example.org`) e token ottenuto dalla registrazione. Puoi lasciarlo vuoto per una modalità solo-P2P.
3. Premi **Avvia**. L'app attende che il core sia effettivamente in ascolto prima di mostrare la timeline. Se il core non parte, l'errore viene mostrato nella schermata di setup.

## Marchio

Il marchio di Fedi3 è la F neon retrò su un globo orbitale. Compare nella
schermata di benvenuto e nella shell, ed è l'icona di avvio su desktop, Android,
iOS, macOS e Windows.

## Salvataggio della configurazione

La configurazione viene salvata in modo atomico nella directory dati dell'app, con permessi `600` su POSIX. Se il file è corrotto, l'app prova a recuperarlo dal backup e, se non ci riesce, ti avvisa invece di sovrascrivere silenziosamente.

## Timeline

- Le timeline federate, locali e "following" sono separate.
- I filtri rapidi permettono di vedere solo media, solo risposte o solo boost.
- Le azioni (like, boost, risposta, elimina) sono disponibili dal menu della nota.
- I media si aprono nel viewer integrato: swipe per navigare, tap per il toggle audio.

## Pubblicare

- Il composer accetta testo, media e visibilità (pubblico, non in elenco, follower, diretto).
- Il testo viene sempre convertito in HTML con escaping: i caratteri `<`, `>`, `&`, `"` e `'` non rompono il rendering né aprono injection.
- La sintassi MFM (`**grassetto**`, `:emoji:`) è supportata.
- Le note dirette non compaiono nella outbox pubblica.

## Chat

- Le chat sono 1:1 o di gruppo. La consegna preferisce il circuito P2P; quando non è disponibile l'envelope viene inviato via HTTPS al percorso del peer sul relay (`/users/<nome>/_fedi3/chat/inbox`), così la chat funziona anche nelle installazioni solo-relay. In entrambi i casi il contenuto resta cifrato end-to-end.
- I messaggi della chat usano AES-256-GCM con chiavi per messaggio derivate da Kyber768 tramite HKDF-SHA256 e firme RSA-SHA256. Il relay non vede il contenuto della chat. RSA-OAEP è usato separatamente per envelope di compatibilità/spool.
- Supporta reazioni, file, GIF e stato di lettura.
- Il servizio notifiche dichiara `remoteMessaging` su Android 14+ (con fallback a `dataSync`
  sotto): Android 15 limita i foreground service `dataSync` a sei ore al giorno.
- Su Android il backup delle impostazioni viene scritto in un file JSON temporaneo
  e passato al pannello di condivisione di sistema (File, Drive, mail); gli appunti
  restano come fallback quando la condivisione non è disponibile.
- Seguire qualcuno importa anche i suoi post pubblici recenti nella timeline home;
  i peer Fedi3 vengono interrogati tramite il relay come i server classici.
- Gli envelope di chat per un membro offline vengono accodati nel core del mittente
  e ritentati finché gira (backoff limitato); il destinatario deduplica per message id.
- Le animazioni decorative (alone di presenza, anello di selezione, marchio)
  girano per pochi cicli e poi si fermano: una schermata statica non viene
  ridisegnata a 60fps, quindi la CPU in idle resta vicina a zero.
- Le notifiche di attività, chat e chiamata usano tre suoni distinti inclusi nell'app, basati sui toni di messaggio, menzione e squillo di Discord.
  Le notifiche di attività sono emesse anche dalla shell, così arrivano nel
  pannello di notifica mentre l'utente è su un'altra scheda.
  (`res/raw/fedi3_activity`, `fedi3_chat`, `fedi3_call`).
- Le chiamate in arrivo suonano con una notifica di sistema mentre l'app è in background;
  il prompt di chiamata si apre appena l'app torna in primo piano.
- La condivisione schermo richiede un foreground service `mediaProjection` su Android;
  l'app avvia `ScreenShareService` prima di chiedere la proiezione e lo ferma alla fine.
- Le chiamate usano WebRTC tra client. Android richiede `ACCESS_NETWORK_STATE` e `ACCESS_WIFI_STATE`: senza questi permessi il network monitor di WebRTC lancia un'eccezione e manda in crash il processo alla prima chiamata.
- Le chiamate richiedono un relay TURN raggiungibile quando un interlocutore è dietro una VPN o un NAT dell'operatore: STUN scopre solo gli indirizzi e non può ritrasmettere i media. Il client ricava gli endpoint `turn:` dall'host del relay quando sono configurate le credenziali ICE, e il relay pubblica il proprio pool coturn agli account autenticati (`FEDI3_RELAY_TURN_*`). Offerte e risposte includono i candidati raccolti in una breve finestra; un percorso media fallito avvia fino a due riavvii ICE automatici e poi propone «Riprova». Il signaling può concludersi mentre i media non passano: la schermata di chiamata mostra lo stato ICE, non solo «risposta ricevuta».
- Su desktop, **Impostazioni > Audio** permette di scegliere microfono e uscita usati dalle chiamate e di provarli. Il test del microfono registra cinque secondi con un indicatore di livello e riproduce la registrazione sull'uscita scelta; il test dell'uscita riproduce un breve tono senza cambiare l'audio predefinito di sistema. Un dispositivo mancante ricade sul predefinito al momento della chiamata. Su Linux l'AppImage usa lo stack GStreamer e PulseAudio/PipeWire dell'host per riproduzione e routing; installa `gstreamer` con i plugin base/good e `pipewire-pulse` o `pulseaudio`. La condivisione schermo mantiene il microfono e aggiunge l'audio di sistema del PC come seconda traccia, così l'interlocutore sente entrambi. L'audio di sistema viene preso dall'uscita predefinita: se non hai scelto un'uscita della chiamata, l'app sposta la chiamata su un altro dispositivo per la durata della condivisione, così l'interlocutore non risente la propria voce attraverso il loopback; un'uscita scelta esplicitamente viene mantenuta. Il video della webcam rispetta il formato nativo invece di essere tagliato alla forma del riquadro. Su Android e iOS le videochiamate hanno un pulsante per passare dalla fotocamera frontale a quella posteriore.
- Foto, file, note vocali e videomessaggi si inviano nella chat. Un messaggio può contenere solo l'allegato, senza testo. Gli allegati audio si riproducono in linea con barra di avanzamento e durata; i videomessaggi mostrano un poster e si riproducono nella bolla. Le note video vengono registrate solo video (la voce è coperta dalle note audio) perché altrimenti il muxer Android scrive file vuoti; una registrazione vuota viene scartata con un messaggio chiaro invece di caricare un body vuoto. Un invio fallito conserva la bozza e mostra l'errore con l'azione Riprova.
- La creazione di un gruppo lo annuncia ai membri scelti con un payload di sistema (elenco membri e titolo), così ogni partecipante riceve la conversazione prima del primo messaggio. Gli eventi di gruppo (creato, rinominato, membro aggiunto/rimosso, uscita, eliminazione) compaiono come avvisi centrati nella conversazione.
- Solo i gruppi si possono rinominare; le chat dirette mostrano sempre l'handle dell'altra persona. I ruoli sono owner/admin/member e le azioni sui messaggi (reagisci, rispondi, inoltra, fissa, modifica, elimina) sono disponibili con una pressione prolungata.
- Limite noto: non c'è *forward secrecy*. Se una chiave privata viene compromessa, i messaggi passati diventano leggibili.

## Notifiche

- Le notifiche sono locali; non serve un servizio push esterno.
- Su Android serve il permesso `POST_NOTIFICATIONS` (richiesto all'avvio) e il permesso `INTERNET`.
- Android mantiene attivo un servizio in primo piano mentre l'app è in background. Il servizio ospita un motore Flutter headless che adotta il core locale, ascolta il flusso di eventi e pubblica le notifiche, così attività e chat arrivano anche senza un provider push. Si controlla in **Impostazioni > Sicurezza > Notifiche in background**; durante l'esecuzione viene mostrata una piccola notifica persistente.
- UnifiedPush (quando il relay espone un provider push) copre il caso in cui Android recupera il processo.
- La lista delle notifiche si aggiorna in testa senza azzerare quelle già lette.

## Impostazioni

- **Sicurezza**: token del relay, rotazione della chiave, visibilità del profilo.
- **Privacy**: consenso alla telemetria (opzionale), cache dei media, blocco per dominio.
- **Moderazione**: liste di attori e domini bloccati.
- **Backup**: esporta e importa configurazione e preferenze in JSON. Il backup **non** contiene la chiave privata: quella vive nella directory dati del core e va salvata separatamente se vuoi poter migrare.
- **Lingua**: italiano e inglese.

Ogni modifica alla configurazione riavvia il core e attende che sia pronto. Se il nuovo valore non funziona, l'app ripristina la configurazione precedente.

## Installazione Android 0.11.0

Dalla [release](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.0) scarica
`Fedi3-android-arm64-v8a.apk` per telefoni ARM a 64 bit o
`Fedi3-android-x86_64.apk` per emulatori x86_64 e verifica il relativo SHA256 in
`checksums.txt`. Sono build per installazione manuale, firmate con la chiave di
debug locale; non sono distribuzioni Play Store. Per aggiornare una vecchia
installazione serve lo stesso certificato. Esporta prima un backup dalle
impostazioni se devi cambiare certificato e reinstallare. I dispositivi ARM a
32 bit non sono supportati.

## Build Android ed emulatore

- `scripts/build_android.sh debug` compila il core per `arm64-v8a` e `x86_64` e produce un APK di debug. `profile` fa lo stesso con una build AOT, utile per misurare prestazioni realistiche su un emulatore.
- `scripts/build_android.sh release` compila il core per `arm64-v8a` e `x86_64` e produce APK separati per ABI. L'APK `arm64-v8a` è quello da distribuire (circa 70 MB); l'APK `x86_64` si installa su un emulatore; l'APK `armeabi-v7a` viene scartato perché il core non è compilato per ARM a 32 bit. Senza `android/key.properties` l'APK di release è firmato con la chiave di debug.
- Le librerie native rimaste da una build precedente vengono rimosse prima di ogni build, così un APK di release non trasporta ABI pensate solo per l'emulatore.
- Requisiti: Android SDK, NDK 28.2.13676358 e un JDK 17 completo (`javac` è necessario; il solo JRE fa fallire la build Gradle).
- Android non espone la directory `HOME`, quindi l'app salva la configurazione nella directory di supporto dell'app e passa un `data_dir` esplicito al core. Entrambi i percorsi restano nella sandbox dell'app.
- Emulatore: avvia un AVD `x86_64` ed esegui `flutter run -d emulator-5554` (oppure installa l'APK di debug con `adb install`). L'APK di debug contiene già il core x86_64.
- Il relay registra un username alla prima connessione del tunnel. Se l'username esiste già con un altro token, il tunnel viene rifiutato: usa un nuovo username o ruota il token tramite l'API admin del relay.
- Misure su build di release (emulatore x86_64, 4 GB di RAM): avvio a freddo circa 0,5 s, PSS circa 130 MB in primo piano e 145 MB con il servizio di notifiche in background, CPU in background circa 0,2% di un core. Il thread raster di Flutter segnala circa il 50% di CPU sull'emulatore perché il guest usa il rendering software; quel valore non è rappresentativo di un dispositivo reale con GPU.

## Problemi comuni

| Sintomo | Causa probabile | Rimedio |
|---|---|---|
| "Core non avviato" | porta occupata o username non valido | cambia il bind o correggi lo username |
| Timeline vuota | relay non raggiungibile o token errato | verifica il token nelle impostazioni |
| Nessuna federazione in ingresso | dominio pubblico non risolve verso il relay | controlla DNS e il tunnel |
| Spinner infinito | il core non risponde | l'app rileva il guasto entro pochi secondi e tenta il riavvio |
| Media non visibili | backend media del relay non configurato | controlla `FEDI3_RELAY_MEDIA_BACKEND` |
| Messaggio di chat fermo sull'orologio | peer irraggiungibile sia via P2P sia via relay | controlla il tunnel del peer e `fedi3.log` nella directory dati del core |

La diagnostica nativa del core viene aggiunta a `fedi3.log` nella directory dati del core (accanto a `fedi3.db`); su Android è l'unico posto dove i log del core sono visibili.

Per il relay vedi [deploy_relay.md](deploy_relay.md) e [troubleshooting.md](troubleshooting.md). Per compilare l'app da sorgente vedi [build_linux.md](build_linux.md).

### Credenziali e portachiavi Linux

Le build Linux richiedono `libsecret` e un Secret Service attivo nella sessione
utente, per esempio GNOME Keyring oppure KWallet con il servizio Secret Service
abilitato. Per compilare, installare `libsecret-1-dev` e `libjsoncpp-dev` su
Debian/Ubuntu, oppure `libsecret` e `jsoncpp` su Arch.

Se il portachiavi è bloccato o non disponibile, Fedi3 mostra una schermata di
recupero: sbloccare il portachiavi e premere **Riprova**. Il programma conserva
configurazione e account e non propone la creazione di un nuovo profilo in
questa condizione. Un JSON danneggiato viene segnalato separatamente. Non
cancellare `config.json` per risolvere un problema del portachiavi.
