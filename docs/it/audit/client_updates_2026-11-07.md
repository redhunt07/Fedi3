# Verifica client, repository e aggiornamenti — 7 novembre 2026

> Aggiornamento dell’8 novembre 2026: i due blocchi registrati in questo rapporto storico sono stati risolti. Vedi [la verifica della migrazione compatibile](crypto_compatibility_2026-11-08.md).

Ambito: working tree locale sul branch `feat/relay-directory`, comprese le
modifiche non ancora committate a core, relay e mobile; controllo in sola lettura
di release e CI pubbliche di `redhunt07/Fedi3`. Il lavoro preesistente è stato
conservato. Nessun commit, push, tag, pubblicazione o deploy di produzione.

## Esito

I controlli funzionali passano e il sistema di aggiornamento è implementato.
**La release non è ancora approvabile dal punto di vista della sicurezza:**
restano due vulnerabilità RustSec e sette avvisi di dipendenze non mantenute.
La CI ora blocca i tag di release se l’audit fallisce, anche con test funzionali verdi.

## Correzioni

- La [CI pubblicata](https://github.com/redhunt07/Fedi3/actions/runs/42417030565)
  falliva per lo slash finale nell’URL del relay Palermo e per il test RTT del
  tunnel. La directory locale e la copia inclusa nell’app ora contengono Palermo
  con URL normalizzato. Le misure RTT sotto il millisecondo sono arrotondate a
  uno, anziché scartate come valore zero/non misurato. Aggiunta una regressione.
- Corretti tre errori Clippy. Aggiornato libp2p da 0.53 a 0.57 adattando codec
  async, eventi e allocazione dell’evento Identify. L’infrastruttura mantiene
  i protocolli DHT privato e legacy con archivi separati; non copia i record
  privati nella DHT pubblica.
- Aggiornate le dipendenze Rust e rimosso il client HTTPS legacy di S3. Le
  vulnerabilità segnalate da RustSec passano da **39 a 2**, senza ignorare advisory.
- I log diagnostici locali tollerano righe JSON troncate o corrotte, serializzano
  le scritture e ruotano a 2 MiB. Anche i log precedenti troppo grandi vengono
  limitati. Svuota elimina entrambe le copie; gli errori disco non interrompono l’app.
- Sistemati script di release, checksum dei soli pacchetti correnti, permessi di
  esecuzione non necessari grazie a Bash, rilevamento Flutter, build debug Windows, gestione degli errori di
  compilazione e versione Windows con `-NoBump`. Aggiunta la dipendenza WebKit 4.1
  necessaria alla build Linux della CI.

## Telemetria e Git

Non è stato trovato un trasporto automatico di telemetria verso GitHub o un push
Git nel client, negli script tracciati o nel workflow. La diagnostica del client
è locale, disattivata per impostazione predefinita ed esportabile manualmente.
Interfaccia e FAQ ora lo descrivono correttamente. Un test verifica che la
registrazione dei log non generi richieste tramite il client HTTP condiviso.
Le statistiche firmate e la discovery tra relay via HTTP restano funzioni separate.

La CI ha `contents: read` come impostazione predefinita. Solo il job delle release
in bozza riceve `contents: write` e carica pacchetti/checksum, non log diagnostici.
Non sono stati cancellati log esistenti o cronologia Git.

## Aggiornamenti implementati

Configurazioni → Aggiornamenti del client offre controllo manuale, note,
avanzamento del download e verifica SHA256. Le build release controllano dopo
l’avvio e ogni sei ore in primo piano. Bozze, prerelease, downgrade, URL esterni,
metadati invalidi, risposte troppo grandi e architetture incompatibili sono esclusi.
Le richieste non inviano credenziali del profilo o diagnostica.

Linux x86_64 AppImage supporta sostituzione atomica, copia `.previous` e riavvio
manuale. Windows x64 scarica lo ZIP verificato e guida alla sostituzione del
pacchetto completo. Installazioni da sorgenti/pacchetti, macOS e mobile usano
la loro fonte di installazione o la pagina della release; non c’è installazione
automatica mobile/macOS.

Preparata la versione `0.10.0+1`, superiore alla release storica `0.9.19`. La CI
verifica il tag `vX.Y.Z`, compila client e core per Linux e Windows, calcola i
checksum e crea una bozza solo dopo i controlli obbligatori. La pubblicazione
resta manuale. Vedi [la guida aggiornamenti](../updates.md).

## Blocchi di sicurezza

| Advisory | Percorso | Intervento necessario |
| --- | --- | --- |
| [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html) | `rsa` 0.9.10, envelope di compatibilità e firme | Migrazione delle operazioni con chiave privata/provider, con verifica di interoperabilità e timing. Upstream non indica versioni corrette. |
| [RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html) | WebRTC DTLS → P-256/P-384 → `crypto-bigint` 0.5.5 | Migrazione dello stack DTLS/provider con aritmetica corretta; non forzare versioni transitive incompatibili. WebRTC P2P è disabilitato per impostazione predefinita. |

Gli avvisi di manutenzione riguardano bincode, paste, pin-utils,
pqcrypto-internals, pqcrypto-kyber, pqcrypto-traits e rustls-pemfile. Cambiare il
KEM della chat richiede lavoro di compatibilità del protocollo. Non è stato
riprodotto un exploit pratico dell’app. Dettagli in [SECURITY.md](../../../SECURITY.md).

## Verifiche

| Controllo | Risultato |
| --- | --- |
| `flutter test --no-pub` | 184 superati |
| `flutter analyze --no-pub` | Nessun problema |
| `cargo test --workspace --locked --offline -- --test-threads=4` | 297 superati; i due test opt-in eseguiti separatamente |
| Test P2P con `--include-ignored` | 12 superati, incluso bootstrap IPFS pubblico, anche dopo l’ultima modifica dell’evento Identify |
| Quota atomica PostgreSQL con `--ignored` | Superato su una nuova istanza PostgreSQL temporanea dopo l’aggiornamento delle dipendenze |
| Regressione upload S3 firmato con client attuale | Superata, inclusa anche nella suite workspace |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | Superato |
| Formattazione Rust/Dart e `git diff --check` | Superati |
| `flutter build linux --release --no-pub` | Superata |
| Test Python deploy/OpenAPI, directory relay, backup/ripristino, ricevute di inoltro e contratto release | Superati |
| Controlli configurazione piattaforme, ShellCheck release e preflight Linux | Superati |
| RustSec con cargo-audit 0.22.2 | **Fallito: 2 vulnerabilità, 7 avvisi di manutenzione; nessun advisory ignorato** |

I due test Rust opt-in (bootstrap pubblico P2P e quota PostgreSQL) sono passati:
**299 test Rust distinti superati** tra suite workspace ed esecuzioni esplicite.
I test di sostituzione AppImage usano file temporanei e verificano integrità,
permessi, backup, pulizia e conservazione dell’app corrente se il pacchetto è
alterato. Non sostituiscono l’applicazione installata dell’utente.


## Limiti

In questa sessione Linux non sono state eseguite build native Windows, macOS,
Android o iOS. I controlli statici delle piattaforme non sostituiscono prove su
dispositivi, store, notifiche e chiamate. Il workflow di release è verificato
localmente, ma non eseguito o pubblicato su GitHub. Federazione e account di
produzione non sono stati modificati. I test non dimostrano l’assenza di ogni bug.
