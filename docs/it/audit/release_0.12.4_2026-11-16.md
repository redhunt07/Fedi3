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

**360 test Rust superati** (4 ignorati nella suite ordinaria). Superati i gate Rust, l'oracolo di compatibilità, l'audit dipendenze, i contratti
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

Il sito Pages è stato pubblicato dal workflow `43772826777` e verificato via
HTTP. Il relay 0.12.4 è stato provato con database temporaneo e rete disabilitata,
poi aggiornato in produzione: health/readiness e nodeinfo confermano la versione.
Volumi, credenziali, identità e configurazione risultano conservati dopo il deploy.

La prima esecuzione CI ha rilevato una corsa nel test preesistente dello spool:
attendeva 100 ms e leggeva il file prima della fine della scrittura. Il test ora
attende la comparsa del cursore con un limite di 5 secondi; il comportamento del
servizio non cambia. La pipeline viene rieseguita sul commit con il test corretto.

La release [v0.12.4](https://github.com/redhunt07/Fedi3/releases/tag/v0.12.4) è
pubblicata come ultima stabile dal commit `5fb980e`. Tutti i 14 job della
[CI di release](https://github.com/redhunt07/Fedi3/actions/runs/43773343370) sono
superati, incluse build dei pacchetti e avvio AppImage su Ubuntu 22.04 e 24.04.

Scaricati tutti e cinque i pacchetti: SHA256 coincidenti con `checksums.txt` e
con i digest degli asset GitHub. Archivio Windows con CRC valido, eseguibile e
DLL del core presenti; intestazione AppImage valida. Le firme dei due APK sono
valide e coincidono con il certificato della 0.12.3. VersionCode 6020 (arm64-v8a)
e 8020 (x86_64), versionName 0.12.4 e rispettivi core nativi presenti.

I siti aggiornati superano il controllo HTTP/ancore su 46 URL senza errori.
FAQ e pannello live verificati nel browser senza errori console. Il feed usa la
data effettiva di pubblicazione della release. Rimossi i container temporanei di
ripristino e prova; conservati i backup privati e l'immagine di rollback.

| Package | SHA256 |
| --- | --- |
| `Fedi3-linux-x86_64.AppImage` | `e5b2136c6ff8c204f78fcfce6df9b475c6028f9dcaab1dcde8f48ef067d577d7` |
| `Fedi3-windows-x64.zip` | `7a70d5ea4b456fc3c2cece97ab4b057904e8c81130055ee09c6dc943b6e77320` |
| `app-arm64-v8a-release.apk` | `9218d0425a9b3f530d19417c1ad836a91b6a5322f34b7991cc11d60364e02780` |
| `app-x86_64-release.apk` | `030f618ba0b88f29e081f92b3f025e4a989f5352211971bad7b69840abec884e` |
| `fedi3_0.12.4-1_amd64.deb` | `dc10c34965709738fc47529880b49566dad3df3d0461bb66cf71ee0f3e743a68` |

I test widget audio simulano il plugin nativo: non provano decodifica e uscita
audio del file sul dispositivo dell'utente che ha segnalato il problema.
Non sono stati eseguiti nuovi test con account di altre persone. Rimane da
provare aggiornamento e riproduzione su dispositivo reale. Nessun pacchetto
iOS/macOS.
