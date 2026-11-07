# Residui dell'audit: seconda verifica del 7 novembre 2026

Questa verifica corregge e sostituisce l'affermazione troppo ampia del primo
follow-up: lo stop ordinario era asincrono, ma `markCoreDead` non lo era;
portachiavi e CI non erano verificati end-to-end, e diversi rilievi di deploy,
pulizia e consistenza restavano aperti.

## Ordine seguito e risultati

1. **CI:** esaminati i job GitHub falliti del run 41549436948. Corrette la
   formattazione Dart/Rust, le deprecazioni libp2p, i borrow e gli avvisi dei
   moduli estratti. Le annotazioni per gli argomenti numerosi restano limitate
   alle due facade che mantengono il contratto SQL, senza sopprimere warning
   globali. Il test timeout del tunnel attende ora la registrazione effettiva
   dopo il 101; il client HTTP dei test non riusa connessioni chiuse. Aggiunti
   test Compose/OpenAPI e un job PostgreSQL con database effimero.
2. **Freeze:** `markCoreDead` avvia lo stop asincrono, conserva la causa del
   guasto e registra il future di cleanup. Ogni nuovo start attende quel future;
   anche dispose non blocca il thread UI. Test con stop ritardato, timer UI
   ancora eseguito e riavvio impedito fino al rilascio.
3. **Keyring:** stato `credentialsUnavailable` distinto da JSON corrotto;
   schermata di recupero con Riprova, senza onboarding che possa sovrascrivere
   il profilo. Test del file conservato e del recupero dopo lo sblocco.
   Prerequisiti libsecret/Secret Service/jsoncpp documentati e aggiunti al job
   di build Linux.
4. **Deploy:** Compose passa gli stessi PG_USER/PG_PASSWORD/PG_DB a PostgreSQL
   e relay; DB_URL vuoto è composto nel relay con percent encoding sicuro, senza
   password duplicata nel template. L'override completo resta supportato per
   database esterni. README/AGENTS aggiornati. TURN root/standalone allineati,
   immagine fissata, IP esterno richiesto, healthcheck STUN e limite memoria;
   niente `.env` completo nel container TURN. P2P standalone allineato a
   indirizzi esterni, limiti circuiti, peer-ID file e healthcheck reale TCP/ID.
   Il bind mount standalone resta invariato per non rigenerare le identità.
5. **Pulizia:** rimossi i prototipi non compilati relay_mesh.rs/relay_notes.rs,
   nat.rs, l'entrypoint P2P inutilizzato e `docs/linux error`. Rimane il peering
   HTTP operativo fra relay. OpenAPI aggiornato per autenticazione, media raw,
   Range/ETag, cancellazioni e segnali globali; eliminato il percorso media
   ambiguo. Validazione YAML, riferimenti e template dei path automatica.
6. **Residui di consistenza e sicurezza:** descritti sotto con prove specifiche.

## Correzioni di consistenza e sicurezza

- **Blob e account:** disabilitazione prima della raccolta; eliminazione dei
  blob prima dei metadati; fallimenti conservano account e riferimenti per
  ripetere DELETE. La transazione finale elimina anche blocchi, move, indici
  locali, note/tag e segnali riconducibili all'identità. Un upload in volo non
  può ripristinare media dopo la cancellazione. Test con storage guasto e retry,
  più prova HTTP completa upload/eliminazione dei file.
- **XFF:** il core usa l'IP della connessione TCP. Gli header inoltrati vengono
  interpretati soltanto con una provenienza interna inserita dal tunnel relay
  autenticato, mai da una intestazione HTTP o da un peer P2P. Si usa l'ultimo IP
  valido aggiunto dal proxy. Test socket diretto, P2P senza provenienza e relay.
- **Dead-letter:** classificazione tramite status tipizzato HTTP/P2P/WebRTC e
  catena degli errori reqwest. Solo 404/410 sono permanenti; 429 è ritentabile.
  Stringhe nel body o nell'URL non determinano il destino del job. Test 429/5xx
  contenenti testo «404/410» e falsi errori con numeri nell'URL.
- **Legacy sync:** limite prima del parsing anche per actor e collection,
  oltre alle pagine; policy verificata prima di ogni redirect. Test di risposta
  sovradimensionata per tutte e tre le letture.
- **Quota:** prenotazione atomica con transazione SQLite IMMEDIATE o lock della
  riga utente PostgreSQL; nessun fail-open sul DB. I byte prenotati contano nella
  quota. Prenotazioni incomplete persistono e vengono pulite dopo 15 minuti,
  eliminando prima gli eventuali blob; upload completati restano intatti.
  Test paralleli SQLite/PostgreSQL, otto upload HTTP concorrenti e recupero di
  una prenotazione scaduta.
- **Preferenze:** coda non avvelenata dagli errori per i file temporanei in
  PrefsStore; mutazioni UI accodate su snapshot aggiornato nell'AppState.
  Le ricevute di lettura si fondono per thread e avanzano monotonamente.
  Test di 25 scritture simultanee, errore filesystem seguito da retry e 20
  conversazioni aggiornate insieme a un'impostazione UI.

## Evidenze e distribuzione

Il codice del commit `90378fff16ab49c4e34026c8f054e9f64e78b5bd` è passato
su [GitHub Actions, run 175](https://github.com/redhunt07/Fedi3/actions/runs/42355489694):
otto job verdi (Rust fmt/Clippy/test, Flutter analyze/test, piattaforme,
script/deploy e PostgreSQL). Il job AppImage, riservato ai tag, è escluso dalla
PR; l'AppImage è stata compilata e verificata localmente.

[PR #93](https://github.com/redhunt07/Fedi3/pull/93) aperta in bozza, senza merge
su main. La documentazione operativa e questo resoconto sono completati dopo
la verifica del codice e la distribuzione.

- Suite workspace: **269 test Rust passati**, zero falliti; due test opzionali
  ignorati (IPFS pubblico e PostgreSQL isolato). Quest'ultimo è stato eseguito
  separatamente con successo, per **270 regressioni Rust distinte**.
- Flutter: **139 test passati**; analyze con fatal infos e formattazione puliti.
- Clippy con `-D warnings`, ShellCheck, configurazioni piattaforma,
  backup/restore simulati e contratti Compose/OpenAPI superati.
- Candidato Docker su PostgreSQL isolato: URL composto da componenti con
  password speciale, otto upload concorrenti (uno ammesso e sette 413 con quota
  di 32 byte) ed eliminazione account/blob verificati.
- Installato sul client Linux e sui container relay, p2p_infra e TURN del VPS:
  tutti healthy, zero riavvii automatici, health/readiness 200. Trentasei
  richieste stats concorrenti sul DB PostgreSQL di produzione: tutte 200.
- Tunnel pubblico connesso; attore Alice risponde con la chiave locale corretta.
  Chiavi RSA/libp2p e identità dell'infrastruttura preservate; nessun panic nel
  log del client aggiornato. Config e `.bak` contengono riferimenti al keyring.
- La produzione usa ora PG_USER/PG_PASSWORD/PG_DB con DB_URL vuoto, dopo avere
  trasferito le stesse credenziali funzionanti dall'override precedente.
  Indirizzi pubblici e limiti P2P conservati. TURN non contiene più segreti del
  DB o token admin nell'ambiente.

 I log locali sono in `/home/morpheus/Documenti/` con
 prefisso `fedi3-residual-`. Il test PostgreSQL aggiuntivo usa un container
 PostgreSQL 16 isolato, rimosso insieme al forward SSH dopo la prova; non usa
 il database di produzione.

Non sono state eseguite chiamate audio/video con Peppenamir. Le verifiche sui
trasporti, sui servizi e sulla coda non sostituiscono una prova audio/video fra
quei due PC. La rimozione dei vecchi file non riscrive la storia Git.


## Ripristino e artefatti

VPS: `/home/rocky/fedi3-backups/audit-residual-20261107/` (0700), dump PostgreSQL
verificato con pg_restore, Compose/dotenv e sorgenti precedenti, manifest e
risultati del candidato. Immagini precedenti conservate con tag
`before-residual-20261107`; rollback automatico previsto se le verifiche falliscono.

Client: backup consistente SQLite/profilo/preferenze cifrato in
`/home/morpheus/.cache/fedi3-audit/alice-before-residual-20261107.aesgcm`, chiave
nel keyring con attributi `application=fedi3`, `backup=fedi3-residual-20261107`.
AppDir precedente in `client-before-residual.AppDir` nella stessa directory
protetta. Nessun reset del profilo o cancellazione delle dead-letter storiche.

| Artefatto installato | SHA256 |
| --- | --- |
| fedi3_relay | `cfe69f7cb217b3e2c275ba9ae5e5318f463463b63d7d06a943f8ca631606dc2f` |
| fedi3_p2p_infra | `8c574a609db5f9700df6965cb619adfe8dce8649ef9f22e8941718f6da0c3d63` |
| Core Linux | `cf0e789177e381ce1d565371ff60809677e0c24a0d94e6195184afd9e58044e7` |
| AppImage Linux | `6e8e2921ad62066653d80713961a8d500737bc36d9d27d0c09041d8ff4709249` |
