# Verifica rilascio Fedi3 0.12.4 — 16 novembre 2026

Versione sorgenti: **0.12.4+4020**. Il rapporto distingue i controlli sui
sorgenti dalle verifiche di pacchetti e deploy. Note: [v0.12.4](../../releases/v0.12.4.md).

## Ambito

- Play, nuovo tentativo, pausa e riascolto delle note vocali ricevute; metadati
  MIME precedenti; audio delle videonote e allegato selezionato a schermo intero.
- Editor condiviso delle risposte da timeline/attività: fino a otto allegati,
  risposte senza testo, rimozione e riutilizzo dei caricamenti dopo errori.
- Metadati, traduzioni, istruzioni relay, archivio delle chiavi locali e
  descrizione della migrazione corretti nei siti. I file nel backend media
  restano esclusi dalla cifratura end-to-end; ora questo limite è esplicito.
- Versione/build allineate fra client, crate, sito, metadati F-Droid, annunci,
  changelog Android e note bilingui.

## Controlli sui sorgenti

Superati i gate Rust, l'oracolo di compatibilità, l'audit dipendenze, i contratti
Python, i controlli piattaforme e ShellCheck. La suite Flutter completa supera
**278 test**, inclusi layout stretto al 200%, nuovo tentativo e rimozione degli
allegati. Il test Rust aggiuntivo della risposta solo multimediale supera la
prova con core locale reale, PNG caricato e lettura dell'outbox. Format e analisi
superati. La build del sito risolve i link interni e genera 14 pagine di contenuto.

La suite Rust ordinaria salta il bootstrap IPFS esterno, la quota PostgreSQL
isolata e due test che richiedono oracoli precedenti; il gate separato esegue
questi ultimi. Per la quota PostgreSQL locale serve una URL di test isolata;
la CI di release fornisce un proprio servizio PostgreSQL. Restano cinque avvisi
di manutenzione ammessi: paste, pqcrypto-internals, pqcrypto-kyber,
pqcrypto-traits e rustls-pemfile.

## Preparazione del deploy

Database, media e configurazione sono stati salvati prima di sostituire il
relay. L'archivio del database è stato ripristinato in un container PostgreSQL
16 isolato e senza esposizione di rete. I checksum delle credenziali account
e delle identità cifrate coincidono con gli originali. Verificata l'integrità
dell'archivio media; conservata un'immagine di rollback. Il candidato relay
viene costruito separatamente dalla produzione.

## Pubblicazione e limiti

Al commit di release sono ancora in corso build finali GitHub Actions, deploy
Pages e sostituzione del relay pubblico. Il maintainer ha richiesto commit,
push, aggiornamento dei siti/relay e preparazione della nuova release. Gli
esiti finali sono registrati dai controlli di deploy/CI e dal resoconto conclusivo.

I test widget audio simulano il plugin nativo: non provano decodifica e uscita
audio del file sul dispositivo dell'utente che ha segnalato il problema.
Non sono stati eseguiti nuovi test con account di altre persone. Rimane da
provare aggiornamento e riproduzione su dispositivo reale. Nessun pacchetto
iOS/macOS.
