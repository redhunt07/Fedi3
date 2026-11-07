# Verifica e correzioni dell'audit — 6 novembre 2026

L'audit ricevuto è stato usato come elenco di rilievi da verificare nel codice,
non come prova che ogni conclusione o soluzione proposta fosse corretta. Questo
intervento copre i dieci rilievi critici C1–C10 e alcuni problemi collegati.
Le modifiche sono nei sorgenti: non sono state distribuite sui server o nei
client in esecuzione durante questo intervento.

## Critici corretti

| Rilievo | Correzione ed evidenza |
| --- | --- |
| C1 — frame infrastrutturali | Rifiuto di lunghezze nulle o superiori a 4 MiB prima dell'allocazione, sia per request sia per response; limite anche in scrittura. Il valore mantiene la compatibilità con il limite del core. Test con `u32::MAX`, limiti e round trip. |
| C2 — backup e dotenv | CSP quotata nel template; cinque script usano un parser letterale senza `source` del file `.env` o `eval`. I valori non sovrascrivono variabili già definite né vengono esportati a Compose, che conserva la propria interpolazione. Retention validata prima dell'aritmetica. Backup/restore provati con Docker simulato, template reale e valori con metacaratteri. ShellCheck e regressione aggiunti in CI. La diagnostica redige anche DB URL e chiavi. |
| C3 — oggetti remoti | Fetch sincrono e worker usano GET con validazione prima di ogni redirect e lettura con limite di 2 MiB. Anche i dati ricevuti via P2P sono limitati prima del parsing; l'ID restituito deve appartenere all'origine richiesta. |
| C4 — media remoti nel core | Fetch HTTP sotto la stessa policy, lettura limitata a 32 MiB, controllo anche dei risultati P2P. Un URL privato rifiutato non viene riproposto al client mediante redirect. Test dell'endpoint pubblico con contatore delle richieste al bersaglio privato: zero contatti e risposta 502 senza Location. |
| C5 — firma inbox relay | Il mittente dichiarato, anche nella forma oggetto con `id`, deve corrispondere all'attore verificato dalla firma. Test con firma valida e attore contraffatto: 401 e nessuno spool. |
| C6 — recupero core | `markCoreDead` tenta di arrestare l'handle prima di abbandonarlo. Watchdog installato anche dopo il primo fallimento; dopo il massimo dei tentativi applica un raffreddamento di cinque minuti. Retry manuale riapre il budget. Il latch UI non crea un ciclo di avvii e riconosce nuove configurazioni. Onboarding resta aperto se il core non parte. Test del primo avvio fallito e del recupero manuale. |
| C7 — bozza chat | Testo e allegati rimangono disponibili durante l'invio e su errore. Dopo la conferma vengono rimossi soltanto i contenuti inviati, preservando un eventuale nuovo testo. Test 503 → bozza integra → retry riuscito. |
| C8 — link | Note senza CW, biografia e campi del profilo usano la policy URL. Test del tap su un link di una nota e dell'effettiva chiamata al launcher simulato. |
| C9 — media relay | Upload conserva `Bytes` senza duplicare l'intero buffer in `Vec`; backend locale, WebDAV e S3 restituiscono stream. Range singolo, suffisso, 206/416, ETag, 304 e If-Range implementati; metadati media isolati con CSP sandbox e nosniff. Prove HTTP del backend locale e prova WebDAV con risposta che rimane aperta mentre il chiamante riceve il primo chunk. |
| C10 — dedup e sync | Claim con `INSERT OR IGNORE`; concorrenza provata con 16 thread. Sync P2P salva claim, inbox e feed nella stessa transazione e mantiene il cursore su errori. Un errore DB simulato tramite trigger deve annullare anche il claim e consentire il retry. |

La soluzione suggerita nell'audit di vincolare gli oggetti di Like/Announce
all'origine di chi reagisce o ricondivide **non è stata applicata**: romperebbe
la federazione fra istanze diverse. Il controllo del documento recuperato e la
policy sui suoi URL restano distinti dall'identità dell'autore dell'Announce.

## Ulteriori interventi collegati

- **A3, mitigazione al download:** CSP `default-src 'none'; sandbox` e `nosniff`
  sulle risposte media. La validazione dei magic bytes e la politica di upload
  SVG restano da completare.
- **A4, nuovi tunnel:** il client invia il token in `Authorization: Bearer` e
  l'URL loggato non contiene più il token. La compatibilità server con i vecchi
  client a query string rimane; i log storici e la loro eventuale rotazione dei
  segreti non sono stati modificati.
- **A9:** edit/delete ricevuti richiedono il mittente originale; ricevute e
  reazioni devono riferirsi a messaggi della conversazione dichiarata.
  Regressione con due thread e più membri, incluso un edit legittimo.
- **A10:** sync verifica l'attore atteso, la moderazione locale e la proprietà
  degli oggetti modificabili. Like e Announce verso altre istanze rimangono
  validi. Test di attribuzione contraffatta e ricondivisione federata.
- **A18:** le risorse di cattura, renderer e peer connection restituite dopo
  la chiusura della schermata vengono liberate. Test con apertura del microfono
  ritardata: track e stream rilasciati, nessun invito inviato.

## Verifiche eseguite

- `cargo test --workspace --locked --offline -- --test-threads=4`: **245 passati**,
  zero falliti, un test IPFS pubblico intenzionalmente ignorato.
- Successivamente, sulla versione con il controllo del redirect media:
  `cargo test -p fedi3_core --test media_fetch_policy --locked --offline`:
  **1 passato**. Il totale delle regressioni Rust distinte eseguite è **246**.
- `flutter test --no-pub`: **130 passati**.
- `flutter analyze --fatal-infos --no-pub`: nessuna segnalazione.
- `cargo fmt --all -- --check`, `git diff --check` e
  `scripts/verify_platform_config.sh`: superati.
- ShellCheck 0.11.0 sui cinque script operativi e sul parser: superato.
- `python3 scripts/tests/test_backup_scripts.py`: superato. È una prova dei
  comandi e degli archivi con Docker simulato, non un restore su PostgreSQL reale.

Una prima esecuzione dei test relay in concorrenza non limitata ha avuto una
chiusura HTTP transitoria in `inbox_requires_digest_header`. Con i quattro thread
usati dalla CI, il gruppo e la suite workspace sono passati.

## Aggiornamento del 7 novembre

I P1 rimasti sono stati affrontati nella fase successiva, con nuove build e
installazione sul client locale e sul VPS. Il dettaglio degli interventi,
delle verifiche e dei backup è in [audit_p1_deployment_2026-11-07.md](audit_p1_deployment_2026-11-07.md).
Restano i rilievi medi e le verifiche audio/video fra due PC su reti diverse.


## Verifica dei residui

La dichiarazione iniziale sui P1 era troppo ampia: `markCoreDead`, la schermata
keyring, deploy/configurazione, pulizia e consistenza avevano ancora residui.
La verifica e le correzioni successive sono documentate in
[audit_remaining_fixes_2026-11-07.md](audit_remaining_fixes_2026-11-07.md).
