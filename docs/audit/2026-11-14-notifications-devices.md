# Correzioni notifiche, profili e dispositivi — 14 novembre 2026

Versione client 0.12.1+4017, sorgenti funzionali a74aae1aa8bca6735c9d96e82dcaafc5a79920bb.

- Notifiche personali: Follow diretti al proprio account; Like, Announce ed EmojiReact soltanto sui propri oggetti; risposte ai propri post e menzioni esplicite. Le attività degli account seguiti restano nei dati della timeline senza generare notifiche personali. Il medesimo filtro determina gli eventi per le notifiche Android. Paginazione conservata anche per pagine prive di notifiche personali.
- Navigazione: chiavi stabili e invalidazione delle richieste asincrone impediscono di riutilizzare profili di altri autori nelle notifiche singole e raggruppate.
- Profili: canonicalizzazione HTML e conversione in testo degli editor impediscono escaping ricorsivo durante la sincronizzazione; link sicuri cliccabili e anteprime coerenti. Le modifiche al profilo conservano tutti i campi di identità e configurazione.
- Presenza: associazione all’URL completo dell’attore, mai al solo username; nessun pallino per account classici senza presenza Fedi3. Il relay associa connessione e heartbeat al singolo peer, con scadenza di 150 secondi. I contatti storici restano conosciuti ma offline. Le connessioni dei dispositivi associati non si espellono a vicenda.
- Chat: salvataggio atomico di messaggio e coda prima della risposta HTTP, attesa della consegna immediata limitata a due secondi, ritentativi persistenti; identificatore della richiesta conservato dalla UI nei retry per evitare duplicati.

## Verifiche

252 test Flutter e analisi statica senza errori; suite Rust e clippy verdi anche in CI. Test mirati per collisione dei nomi fra domini, profili ricorsivamente escaped, reazioni ai post altrui, sostituzione/raggruppamento delle notifiche e retry chat. Test di integrazione con due core e reset di un dispositivo: 2 peer online, poi 3 conosciuti/2 online, infine 3 conosciuti/1 online. Tre test di consegna chat P2P passati; rollback SQLite e idempotenza verificati.

Aggiornamento locale con backup cifrato: cinque file di identità invariati, 172 messaggi originali verificati tramite hash di identificatore e contenuto, integrità SQLite valida. Aggiornamento Android ADB in place, stessa firma e nessuna cancellazione dati. Relay aggiornato dopo backup PostgreSQL e verifica su candidato isolato: credenziali degli otto account, chiavi di firma, identità sincronizzate, spool, media e configurazione conservati. Endpoint pubblico readiness verificato dal client.

La conservazione dei contatti dei dispositivi precedenti è intenzionale: il reset di un dispositivo non equivale alla revoca di identità o credenziali. La presenza indica esclusivamente connessioni vive, senza eliminare lo storico.
