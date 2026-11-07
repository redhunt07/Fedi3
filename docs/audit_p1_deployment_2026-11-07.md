# Audit P1: correzioni e distribuzione del 7 novembre 2026

Interventi sui P1 A1–A20 dell'audit ricevuto il 6 novembre, inclusi quelli già
corretti insieme ai C1–C10. Le build sono state distribuite sul client Linux
locale di Alice e sui servizi `relay` e `p2p_infra` del nostro VPS. Non è stata
eseguita una distribuzione sul computer di Peppenamir.

## Interventi

| Rilievo | Risultato |
| --- | --- |
| A1 | SQL del relay estratto in `db.rs`; facade asincrona in `db_dispatch.rs`, operazioni su `spawn_blocking` e concorrenza limitata dal pool PostgreSQL. Rimosso il mutex globale. Readiness usa una capacità separata e un timeout; prova su runtime a un solo thread con un'operazione bloccata e una query concorrente riuscita. |
| A2 | SIGTERM/Ctrl-C attivano shutdown HTTP e chiusura dei tunnel; infrastruttura P2P drena gli handler mailbox con deadline di cinque secondi. Prove SIGTERM sul binario reale: uscita pulita e utente ancora nel DB; una richiesta già nel tunnel completa con 200 anche dopo il segnale. Reader drena fino a cinque secondi, writer viene cancellato e atteso. |
| A3 | SVG rifiutato; controllo dei magic bytes per i MIME supportati. CSP sandbox e nosniff anche al download. MIME sconosciuti non sono accettati come immagini/audio/video. |
| A4 | Tunnel nuovi con Bearer; tracing relay registra soltanto il path. Compatibilità con query token dei vecchi client mantenuta. Il Caddyfile in uso non abilita access log. Log storici e token già eventualmente esposti non sono stati cancellati/ruotati. |
| A5 | Prima di memorizzare la chiave di un relay viene interrogato il suo endpoint stats sull'origine dichiarata, verificando URL, firma e chiave. Pin già presenti restano vincolanti. Test con payload validamente firmato che tenta di attribuirsi l'origine di un altro relay: respinto, nessuna chiave memorizzata. |
| A6 | Header di autorizzazione, cookie, header interni e hop-by-hop esclusi dal forwarding; eliminate anche le intestazioni nominate da Connection e Set-Cookie nelle risposte. Host ricostruito per il destinatario HTTP. Test con peer HTTP reale e credenziali sentinella. |
| A7 | Discovery rilegge il file peer ID con timeout anche se appare dopo l'avvio del relay. Dipendenza Compose dall'infrastruttura aggiunta senza cambiare identità o indirizzi pubblici. Test del file creato dopo l'avvio. |
| A8 | Handler mailbox eseguiti in task separati, massimo 64; lo swarm continua a elaborare ping, identify e circuiti. Sovraccarico restituisce 503. |
| A9 | Edit/delete vincolati all'autore originale; ricevute e reazioni vincolate alla conversazione. Correzione precedente inclusa nelle build distribuite. |
| A10 | Sync P2P verifica attore, proprietà degli oggetti e moderazione. Like/Announce fra istanze diverse conservano il comportamento federato legittimo. |
| A11 | Firme device richiedono target, host, date e digest; aggiornato anche chi genera le richieste GET di device sync. Test negativo per set incompleti e positivo per firma completa. |
| A12 | Token vuoto o composto da spazi genera un segreto casuale. Entrambi i percorsi di autorizzazione interna rifiutano token vuoti e usano confronto constant-time. Test HTTP senza credenziali. |
| A13 | Tunnel con 64 handler simultanei, 503 oltre il limite e cancellazione al disconnect/shutdown. Test con 65 richieste bloccate e verifica rilascio degli handler. |
| A14 | Versioni SQLite indipendenti per modulo social e delivery, con rifiuto di versioni future; marker legacy non sovrascritto dai moduli. Migrazione verificata anche sul DB installato. |
| A15 | Export delle impostazioni cifrato v2: AES-256-GCM, PBKDF2-HMAC-SHA256 con 600.000 iterazioni, salt/nonce casuali. Import v1 compatibile. Password errata e alterazione rifiutate. UI esplicita: non comprende chiavi d'identità, messaggi o media; non è una migrazione completa dell'account. |
| A16 | Cinque campi sensibili in flutter_secure_storage; config e backup contengono riferimenti al portachiavi. Scritture nel vault verificate prima della migrazione, nessun fallback in chiaro; portachiavi indisponibile impedisce l'avvio preservando il file. Migrazione reale Linux verificata. |
| A17 | Permessi Android per microfono, camera e audio; richieste runtime media affidate al plugin WebRTC esistente. Richiesta runtime notifiche e tap sul thread locale. Assenza del servizio notifiche non blocca runApp. |
| A18 | Risorse asincrone delle chiamate rilasciate anche se la schermata viene chiusa durante l'inizializzazione; incluso il test di cattura microfono ritardata. |
| A19 | Un'unica query globale dei segnali, con cursore per ID e controllo membership/destinatario, sostituisce la scansione di 100 chat. Polling da 3 a 15 secondi quando inattivo e risveglio tramite SSE; vecchi inviti scaduti/terminati ignorati. |
| A20 | Osservatore lifecycle condiviso: polling UI e SSE sospesi in background, timer di una chiamata attiva preservati. All'uscita si attende lo stop con timeout. Attesa FFI su isolate separato; avvii serializzati con lo stop per evitare sovrapposizioni sul DB. |

Durante la verifica installata è stato inoltre corretto un panic Rustls del
WebSocket WSS: provider crittografico scelto esplicitamente quando le dipendenze
abilitano sia ring sia AWS-LC. Test del builder TLS e connessione al relay reale
riusciti dopo la correzione.

## Verifiche

La suite workspace completa ha passato **259 test Rust**, zero falliti e un
test IPFS pubblico intenzionalmente ignorato. Dopo l'ultimo affinamento dello
shutdown sono stati ripetuti tutti i **77 test del relay**, incluso un nuovo
test sul drenaggio della richiesta in volo: tutti passati. Il totale delle
regressioni Rust distinte eseguite è **260**. Comandi con `--locked --offline`
e `--test-threads=4`.

- Flutter: **134 test passati**, analyze con `--fatal-infos` senza segnalazioni.
- Test nativo aggiuntivo: handle FFI arrestato da un isolate separato, server
  chiuso e timer del chiamante rimasto attivo durante l'attesa.
- Parser dotenv e backup/restore: prova automatica con Docker simulato passata;
  non equivale a una prova di restore del dump di produzione.
- Configurazione piattaforme, formattazione Rust, controllo whitespace e
  ShellCheck 0.11.0 (`-x -P scripts`) verificati.
- Upload sul relay pubblico installato: HTML dichiarato PNG e SVG entrambi
  respinti con 415, senza creare media.
- Sul VPS PostgreSQL reale: 36 richieste stats con 12 worker, tutte 200;
  `/healthz` e `/readyz` 200, zero riavvii dei container dopo la distribuzione.
- Sul client installato: DB healthy, tunnel WSS connesso, peer P2P connessi;
  attore pubblico Alice con la stessa chiave RSA locale. I peer pubblici IPFS
  non rappresentano altrettanti utenti Fedi3.
- Chiavi RSA e libp2p del profilo e peer ID infrastrutturale confrontati con
  le copie precedenti: invariati. Nessun segreto nei JSON config e `.bak`.

## Backup e ripristino

VPS: `/home/rocky/fedi3-backups/audit-p1-20261107/`, directory 0700.
Dump PostgreSQL custom verificato con `pg_restore --list`: 402.869 byte,
SHA256 `0933b0f1bafeab3debcc92c54496456799a450d7fab999c7c6d222942dd882ed`.
Immagini precedenti conservate come
`fedi3-relay:before-audit-p1-20261107` e
`fedi3-p2p_infra:before-audit-p1-20261107`.
Conservate anche configurazione Compose e sorgenti precedenti. Gli archivi
sorgente e il manifest della versione installata restano accanto al dump.

Client: copia consistente SQLite e file del profilo salvati prima della
migrazione in `/home/morpheus/.cache/fedi3-audit/alice-before-p1-20261107.aesgcm`.
Archivio AES-GCM, con chiave casuale nel portachiavi di sistema, attributi
`application=fedi3`, `backup=fedi3-audit-p1-20261107`. Non è cifrato con la
passphrase dei nuovi backup UI: è una copia operativa locale per il rollback.
Non divulgarne la chiave. Il ripristino richiede prima l'arresto del client.

## Limiti delle verifiche

Non è una certificazione di production readiness e non chiude i rilievi medi.
Nessuna nuova chiamata audio/video fra il nostro PC e quello di Peppenamir è
stata provata in questa distribuzione. L'hardware Android e i backend cloud
S3/WebDAV reali non sono stati verificati sul campo. I test mailbox e tunnel
non riproducono tutte le condizioni NAT/CGNAT possibili.

Il backup UI è deliberatamente un backup delle impostazioni cifrato: per
trasferire la stessa identità occorre anche una copia protetta della cartella
dati del core. I vecchi backup v1 restano leggibili ma sono in chiaro.
La coda locale contiene anche delivery dead-letter storiche: non sono state
cancellate né reinviate manualmente durante questa manutenzione.


## Artefatti distribuiti

Build Rust con target GNU e baseline GLIBC 2.34, compatibile con il runtime
Debian Bookworm del VPS; AppImage Linux con core aggiornato. Immagini server
costruite dalle immagini runtime precedenti, sostituendo i binari verificati.
Database, volumi e configurazione `.env` preservati; Compose aggiornato per
ordine di avvio e tempo di arresto dell'infrastruttura. Sorgenti e script
operativi sincronizzati sul VPS per i successivi rebuild.

| Artefatto | SHA256 |
| --- | --- |
| Relay | `6c6c14afe9ac88bf88f6cbe526ca17595079756267d950f101751ed3b93a9395` |
| Infrastruttura P2P | `fa35f2c46bc7a6092894723c087085534c0f2812acb20dd371c77eebd2b34dab` |
| Core Linux | `0c73696c9f7651c1cade0638e40201ff60723fbf0dcdd3514f9a77717b7fb16b` |
| AppImage Linux | `b22361f4edd3b1f2d304c1b926f403849c361d126729270f66d9c8ae677a214a` |

I log di verifica locali sono in `/home/morpheus/Documenti/`, prefisso
`fedi3-p1-`: `validated-workspace.log`, `relay-complete.log`,
`flutter-final-verified.log`, `analyze-final.log`, `native-stop.log`,
`installed-checks.json`. Le prove iniziali non riuscite sono state conservate
separatamente: il resoconto sopra usa soltanto gli esiti finali verificati.
