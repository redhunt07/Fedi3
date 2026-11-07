# Annunci Fedi3

Aggiornamenti ufficiali dai maintainer. Questa pagina è letta direttamente dal
repository Git e il client la controlla periodicamente. La controparte inglese è
[ANNOUNCEMENTS.md](ANNOUNCEMENTS.md).

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
- Repository: documentazione inglese con controparte italiana, CI verde (Rust,
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
