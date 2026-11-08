# Annunci Fedi3

Aggiornamenti ufficiali dai maintainer. Questa pagina è letta direttamente dal
repository Git e il client la controlla periodicamente. La controparte inglese è
[ANNOUNCEMENTS.md](ANNOUNCEMENTS.md).

## 2026-11-09 — Chiamate: la condivisione schermo mantiene il microfono e il formato della webcam

Condividere lo schermo non silenzia più chi parla: la chiamata invia microfono
e audio di sistema del PC come tracce separate, così l’interlocutore sente
entrambi. Su Linux l’audio di sistema viene preso dall’uscita predefinita;
se non è scelta un’uscita della chiamata, l’app la sposta su un altro
dispositivo per la durata della condivisione, così l’interlocutore non risente
la propria voce attraverso il loopback.
Le immagini della webcam ora rispettano il formato nativo invece di essere
tagliate alla forma del riquadro, e le chiamate uno-a-uno occupano lo spazio
disponibile. **Impostazioni > Audio** su desktop mostra lo stesso suggerimento
di routing.
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
