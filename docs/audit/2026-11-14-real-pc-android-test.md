# Prova reale PC e Android — 14 novembre 2026

Client PC Flatpak e Android 0.12.1 (Android 6017), relay pubblico fedi3.redhunt07.ovh aggiornato. Dispositivo Android Xiaomi 2201123G collegato via ADB. Il forwarding ADB è stato usato soltanto per interrogare il core locale; chat e replica hanno usato il relay pubblico e le normali connessioni dei client. Account ausiliario temporaneo controllato, nessun messaggio a utenti terzi.

## Risultati

- Profilo: identici ID, nome, biografia, avatar, banner, campi/link e chiave pubblica sui due dispositivi.
- Follower e seguiti: confronto degli URL completi, non solo dei contatori; 5 follower e 5 seguiti iniziali identici. Follow reciproco con il solo account di prova comparso sui due dispositivi. Undo eseguiti al termine.
- Post esistenti: stessi ID, contenuti e allegati. Una differenza riguarda soltanto l’arricchimento locale `fedi3InReplyToObject` della nota di risposta, non il contenuto pubblicato.
- Post nuovo: nota privata creata dal telefono e indirizzata soltanto all’account di prova; oggetto identico leggibile nel core PC. Cancellazione dal PC propagata su entrambi come Tombstone.
- Chat: 176 messaggi iniziali sui due dispositivi, snapshot non troncati e stessi ID e contenuti. Messaggio dall’account di prova e risposte da PC e telefono ricevuti da tutti i client. Retry reale con medesimo client_message_id restituisce il medesimo message_id.
- Background normale: Home Android, schermo spento, core mobile inizialmente fermo. Nuovo messaggio ricevuto sul telefono dopo circa 11 secondi; verificata notifica Android 1001/canale fedi3_chat_v3 contenente il marcatore del test.
- Doze forzato breve: verificato stato IDLE con schermo spento; secondo messaggio presente dopo circa 15,5 secondi e notifica contenente il marcatore verificata successivamente. Il core era già attivo all’inizio di questa seconda prova: non equivale a una prova di risveglio da Doze prolungato con processo terminato.

## Problema riprodotto: notifiche social fra dispositivi

Il Follow firmato dall’account di prova è apparso nelle notifiche del telefono ma non in quelle del PC, benché il follower sia stato replicato correttamente. Sul PC non esiste la relativa riga Follow in inbox_items. La replica in crates/fedi3_core/src/account_sync.rs include relazioni, oggetti e outbox, ma non le attività delle notifiche personali; emette eventi account/outbox. La consegna del tunnel raggiunge un solo core dell’account e l’importazione della relazione sugli altri dispositivi non ricrea la notifica.

Esito: la prova non è interamente superata. Serve una replica selettiva e deduplicata delle notifiche personali, distinta dall’intera inbox della timeline, con eventi appropriati per UI e notifiche di sistema. Nessuna modifica funzionale applicata durante questa prova.

## Conservazione e pulizia

I contenuti dei 176 messaggi iniziali sono stati verificati invariati su entrambi prima della pulizia della sola chat ausiliaria. Nessuna identità dell’utente rigenerata e nessuna cancellazione dei dati applicativi. Verificati infine su entrambi: profilo/identità invariati, elenchi originali di follower e seguiti ripristinati e tutti i 176 messaggi iniziali conservati. Chat ausiliaria rimossa da entrambi, account di prova eliminato dal relay; Doze forzato disattivato, impostazione schermo ripristinata al valore 0, forwarding ADB e credenziali temporanee rimossi. Le notifiche storiche dell’azione di test possono restare nella cronologia.

Le evidenze grezze sono conservate localmente nella cache privata fedi3-audit; il documento contiene soltanto esiti e dettagli tecnici, non credenziali né contenuti delle chat personali.
