# FEDI3 — Audit di production readiness

**Data:** 2026-10-31
**Ambito:** intero repository — `crates/*` (28.358 righe Rust), `app/lib` (27.244 righe Dart), `deploy/`, `scripts/`, `.github/`, docs, configurazioni di piattaforma.
**Metodo:** revisione statica del codice e della configurazione, condotta in parallelo su quattro aree (relay, core Rust, app Flutter, ops/repo). In questo ambiente **non sono installati né Rust né Flutter/Dart**, quindi nulla è stato compilato o eseguito: ogni conclusione deriva dalla lettura del sorgente. I punti marcati `⚠️ da verificare` richiedono un test runtime.

---

## 1. Verdetto

Il progetto ha una base architetturale seria e non banale: ActivityPub reale, HTTP Signatures implementate, relay con Postgres/Redis/Meilisearch, P2P libp2p/WebRTC, cifratura E2E, i18n completo IT/EN, GC dello storage, coda di consegna persistente. Non è un prototipo.

**Non è però pronto per la produzione.** I blocker sono di tre nature:

1. **Le build release non possono funzionare.** Android release non ha il permesso `INTERNET`; il traffico verso il core locale è bloccato dal cleartext policy di Android 9+; macOS release non ha né `network.client` né `network.server`; iOS non ha eccezioni ATS; l'app id è `com.example.fedi3` e la release è firmata con la debug key.
2. **La federazione ha falle di autenticazione.** L'endpoint `/inbox` del relay **non verifica alcuna firma HTTP**; il core verifica la firma ma **non lega l'`actor` dell'attività al firmatario**; l'`outbox` accetta POST non autenticati anche attraverso il tunnel pubblico. Chiunque può iniettare o impersonare attività.
3. **L'affidabilità non è garantita.** Il tunnel verso il relay non si riconnette mai; un panic Rust attraversa il confine FFI e uccide l'app; non esiste un solo test Rust e non esiste alcuna CI.

Sotto: blocker (P0), HIGH, MEDIUM, LOW, roadmap, e cosa è già fatto bene.

---

## 2. Blocker di rilascio (P0)

### P0-01 — Android release: manca il permesso `INTERNET`
`app/android/app/src/main/AndroidManifest.xml` non dichiara `<uses-permission android:name="android.permission.INTERNET"/>`. Il permesso esiste solo in `app/android/app/src/debug/AndroidManifest.xml:6` e `app/android/app/src/profile/AndroidManifest.xml:6`, che il manifest merger applica **solo** alle build debug/profile.

**Effetto:** una build `--release` non ha accesso alla rete. Ogni chiamata a `127.0.0.1:8788` e al relay fallisce con `SocketException`. L'app appare permanentemente offline.

**Fix:** aggiungere in `src/main/AndroidManifest.xml` `INTERNET` (e `POST_NOTIFICATIONS`, vedi M-04), rimuovere il duplicato dai manifest debug/profile, e aggiungere in CI un check su una APK release (`aapt dump permissions`).

### P0-02 — Android: il cleartext verso il core locale è bloccato
L'app usa `http://$bind` (`app/lib/model/core_config.dart:83`, consumato da `app/lib/core/core_api.dart:24-48`). Non esiste `network_security_config.xml` (`find app/android -name "network_security_config*"` → vuoto) e il manifest non imposta `android:usesCleartextTraffic`. Con `targetSdk >= 28` Android blocca il cleartext **anche su loopback**.

**Effetto:** `CLEARTEXT communication to 127.0.0.1 not permitted` su ogni endpoint `/_fedi3/*`. In più `media_kit`/ExoPlayer su Android applica la stessa policy ai media `http://` federati (mentre `dart:io` la ignora), creando un comportamento incoerente.

**Fix:** `app/android/app/src/main/res/xml/network_security_config.xml` con `cleartextTrafficPermitted="true"` limitato a `127.0.0.1`/`localhost`, referenziato da `android:networkSecurityConfig`.

### P0-03 — macOS release: entitlements senza rete e senza accesso ai file
`app/macos/Runner/Release.entitlements` contiene solo `com.apple.security.app-sandbox`; `DebugProfile.entitlements:5-11` ha `app-sandbox`, `allow-jit` e `network.server` ma **non** `network.client`. Nessuno dei due ha `files.user-selected.read-write`.

**Effetto:** con App Sandbox, la release non può fare connessioni uscenti (core + relay irraggiungibili), né fare `TcpListener::bind` (`crates/fedi3_core/src/runtime.rs:695`); `file_selector` (export/import backup, avatar, download media) fallisce. **La release macOS è non funzionale.**

**Fix:** aggiungere `com.apple.security.network.client`, `com.apple.security.network.server` e `com.apple.security.files.user-selected.read-write` a entrambi i file; verificare con `codesign -d --entitlements`.

### P0-04 — iOS: nessuna eccezione ATS
`app/ios/Runner/Info.plist` non contiene `NSAppTransportSecurity`. L'app parla con il core su `http://127.0.0.1:8788`. ⚠️ da verificare su device, ma su iOS il traffico `http://` verso il loopback non è garantito esente su tutte le versioni.

**Fix:** `NSAppTransportSecurity` → `NSAllowsLocalNetworking=true` (preferibile a `NSAllowsArbitraryLoads`). Aggiungere anche `NSPhotoLibraryUsageDescription`/`NSCameraUsageDescription` (mancano entrambe, vedi M-08).

### P0-05 — Identità applicativa placeholder e firma con chiavi di debug
`app/android/app/build.gradle.kts:24` `applicationId = "com.example.fedi3"`; `:34-38` la release usa `signingConfigs.getByName("debug")`. Lo stesso id compare in `app/linux/CMakeLists.txt:11` (`APPLICATION_ID "com.example.fedi3"`).

**Effetto:** Google Play rifiuta/limita `com.example.*` (deve essere unico e immutabile) e una release firmata con la debug key non può essere aggiornata da una keystore diversa.

**Fix:** id definitivo (es. `org.fedi3.client`), `key.properties` + keystore di release, `signingConfig = signingConfigs.getByName("release")`, allineare bundle id macOS e metadati Windows.

### P0-06 — Il relay non verifica alcuna firma HTTP su `/inbox`
`crates/fedi3_relay/src/main.rs:2763-2857`: `shared_inbox` riceve `headers: HeaderMap` ma **non chiama mai** una verifica di firma. `grep` di `Signature|verify` nel corpo dell'handler → nessun risultato. `headers` è usato solo per `client_ip` e rate limiting.

**Effetto:** chiunque su internet può `POST` un'attività arbitraria con `to` verso qualsiasi utente locale. L'attività viene (a) inoltrata nel tunnel dell'utente, (b) indicizzata in `relay_notes` e Meilisearch (`index_activity_bytes_for_search`, riga 2806), (c) accodata nello spool per 7 giorni se l'utente è offline. Un singolo attaccante può avvelenare l'indice di ricerca condiviso e gonfiare lo spool di qualsiasi utente.

**Fix:** chiamare `verify_actor_signature_for_request(...)` all'inizio dell'handler (la funzione esiste già ed è usata da `webrtc_signal_send`), restituire 401 in caso di fallimento, e renderla obbligatoria (vedi anche H-11).

### P0-07 — Il core non lega l'`actor` dell'attività al firmatario (impersonificazione)
`crates/fedi3_core/src/ap.rs:4793` risolve la chiave da `sig.key_id`, `ap.rs:4803` verifica la firma contro `summary.actor_url`, poi `ap.rs:4871` passa l'attività a `process_inbox_activity`, che usa `activity.get("actor")` per **tutto** (storage, follower, target degli Accept: `ap.rs:5183`, `5205`, `5263`). **Non esiste alcun confronto** `activity.actor == summary.actor_url` in tutto il percorso inbox/receipt.

**Effetto:** un server qualsiasi, con la propria chiave valida, può inviare `{"type":"Create","actor":"https://good.example/users/alice","object":{...}}` e Fedi3 lo memorizza come se fosse di Alice. Impersonificazione nei feed, `Delete` di note altrui, `Accept` inviati a un URL scelto dall'attaccante; i blocchi vengono valutati contro il firmatario (`ap.rs:4815`) e non contro l'attore dichiarato.

**Fix:** dopo la risoluzione della chiave, richiedere che `activity.actor` (stringa o `object.id`) coincida con `summary.actor_url`, e che la parte attore di `sig.key_id` coincida; 401 altrimenti. Applicare lo stesso controllo a `receipt_post`, ingest P2P/gossip e legacy sync.

### P0-08 — `POST /users/<u>/outbox` non è autenticato ed è raggiungibile dal relay e dal P2P
`crates/fedi3_core/src/ap.rs:4071-4073`: `outbox_post` non chiama `require_internal` né verifica una firma (a differenza di `media_upload` a `ap.rs:528`). Il router del tunnel (`runtime.rs:741-746`) e quello P2P (`runtime.rs:758-763`) usano lo stesso `handle_request`, quindi l'endpoint è raggiungibile dall'esterno. L'unica guardia è l'uguaglianza dell'actor (`ap.rs:4094-4106`), che non è una difesa contro chi conosce lo username.

**Effetto:** chiunque raggiunga il relay può pubblicare attività **come l'utente locale**, con `id` scelti da lui: post falsi nel proprio outbox, `Delete` arbitrari, `Undo` di follow.

**Fix:** richiedere `X-Fedi3-Internal` (o una firma HTTP valida) per `POST` sull'outbox; mantenere un percorso UI-only separato se i peer remoti non possono inviare il token.

### P0-09 — Il tunnel verso il relay non si riconnette mai
`crates/fedi3_core/src/runtime.rs:750-752` lancia il tunnel **una sola volta**. `tunnel.rs:41-47` fa un singolo `connect_async` senza timeout; `tunnel.rs:74` `let msg = msg?;` e `tunnel.rs:110` `ws_tx.send(...).await?` propagano qualsiasi errore fuori dalla funzione; nessuno riavvia il task. L'errore viene perfino scartato (`runtime.rs:820` `let _ = tunnel.await;`, eseguito solo allo shutdown).

**Effetto:** un riavvio del relay, un cambio di rete, un NAT timeout o un token ruotato rendono l'istanza **irraggiungibile dal fediverso per sempre**, mentre la UI mostra "connesso". Per un'app federata è il difetto più grave in assoluto.

**Fix:** loop di riconnessione con backoff esponenziale + jitter fino allo shutdown, timeout esplicito sul connect, watchdog sui pong (oggi si mandano ping ogni 5 s senza mai verificare l'assenza di pong, `tunnel.rs:51`), e stato di connessione esposto alla UI via `NetMetrics`.

### P0-10 — Panic Rust attraverso il confine FFI e registry con `Mutex::lock().unwrap()`
`crates/fedi3_core/src/ffi.rs:20` e `:46` sono `extern "C"` senza `catch_unwind`; `runtime.rs:181` e `:195` usano `REGISTRY.lock().unwrap()`.

**Effetto:** da Rust 1.81 un unwind che attraversa `extern "C"` **aborta il processo**. Un panic in `start`/`stop`, o un mutex avvelenato da un panic precedente, uccide l'intera app Flutter. È il crash totale più probabile in produzione.

**Fix:** `std::panic::catch_unwind(AssertUnwindSafe(...))` su ogni entry point FFI, con codice d'errore e messaggio; sostituire `.unwrap()` con `lock().unwrap_or_else(|e| e.into_inner())`.

### P0-11 — `start()` restituisce successo prima che il core sia avviato
`runtime.rs:175-190` spawna il thread e ritorna `Ok(handle)` subito; identità, DB e `TcpListener::bind` avvengono dentro `run_core` (`runtime.rs:695`), e i fallimenti finiscono solo in un `error!` (`runtime.rs:177`). Lato Dart `app/lib/state/app_state.dart:236` tratta il ritorno come "core attivo".

**Effetto:** porta già occupata, bind non valido, DB corrotto o il `bail!` sul token relay (vedi H-09) producono un handle vivo con nessun server. La UI parte, le richieste falliscono con `connection refused`, e l'utente vede una timeline vuota con errori a ripetizione.

**Fix:** validare la config, aprire il DB e fare il `bind` **prima** di ritornare (o creare il listener sul thread chiamante e passarlo al runtime), con un oneshot di readiness atteso da `start()` e timeout; restituire `Err` con messaggio.

### P0-12 — `edit_config` ferma il core e non lo riavvia
`app/lib/ui/screens/edit_config_screen.dart:412-416`: `await stopCore(); await saveConfig(cfg); Navigator.pop();` — **nessun** `startCore()`. Confronto: `security_settings_screen.dart:146-148` fa `stop → save → start` correttamente.

**Effetto:** dopo aver modificato username/relay/bind l'utente torna alla timeline con `isRunning == false`; ogni colonna è vuota (`_loadMore` esce subito se il core non gira) e `AutoStartCore` non riavvia perché `_started` è già `true` (`auto_start_core.dart:46-52`). L'app sembra rotta fino al riavvio del processo.

**Fix:** centralizzare in `AppState.replaceConfig(cfg)` che fa stop → persist → start → attesa readiness, usato da tutte le schermate di impostazioni e dall'import del backup.

---

## 3. Problemi HIGH

### Sicurezza — federazione e relay

**H-01 — Digest non obbligatorio e set di header firmati non validato.** `crates/fedi3_core/src/http_sig.rs:213-215`: se `Digest` manca, `verify_digest_if_present` ritorna `Ok(())`. `http_sig.rs:159-162` usa `"date"` come default per `headers`. L'inbox (`ap.rs:4749`) non richiede che `digest` sia tra gli header firmati. Un sender che firma solo `date` lascia corpo e path non autenticati: un MITM può riscrivere il body e il `Date` mentre la firma RSA continua a verificare. **Fix:** per le POST richiedere `Digest` valido e che `sig.headers` contenga almeno `(request-target)`, `host`, `date`, `digest`.

**H-02 — L'inbox non verifica la proprietà dell'oggetto.** `ap.rs:5191-5205`: `Create|Update` fa `upsert_object_with_actor(...)` senza verificare `object.attributedTo == actor`; `ap.rs:5212-5220`: `Delete` fa `mark_object_deleted` senza verificare che il firmatario possieda l'oggetto. Qualsiasi attore federato può sovrascrivere la nota in cache di un altro, o cancellare oggetti arbitrari (incluse le note dell'utente locale) con un `Delete` firmato. **Fix:** richiedere la corrispondenza `attributedTo`/actor per Create/Update e l'uguaglianza con l'attore memorizzato per Delete.

**H-03 — SSRF su URL controllati dall'attaccante, senza guardia sugli IP privati.** `http_sig.rs:38` `reqwest::Client::new()` e `http_sig.rs:60-71` fetchano `actor_url` derivato da `keyId`; `delivery.rs:36`/`79-105` fetchano l'actor e si fidano di `endpoints.sharedInbox`; `ap.rs:6030-6054` fetcha `object_url`; `legacy_sync.rs:155-189` segue i `next` remoti. Da nessuna parte c'è `.redirect(...)` o un controllo su loopback/private/link-local. Un attore può puntare a `http://127.0.0.1:8788/...` o `http://169.254.169.254/...` e il core lo fetcha (reqwest segue fino a 10 redirect). **Fix:** client condiviso con policy di redirect limitata a http(s) e rifiuto degli IP non pubblici; applicarlo a KeyResolver, Delivery, object fetch, media e legacy sync.

**H-04 — Corpi di risposta in ingresso illimitati (OOM).** `delivery.rs:92`, `http_sig.rs:73`, `ap.rs:6054`, `legacy_sync.rs:180`: `resp.text()`/`resp.bytes()` senza cap. Un server che restituisce un documento da 2 GB viene bufferizzato interamente in RAM. **Fix:** `Content-Length` + streaming con cap (256 KiB per actor/chiavi, 2 MiB per oggetti).

**H-05 — Upload media che bypassa il body limit e può decodificare una bomba.** `ap.rs:531` `to_bytes(body, usize::MAX)` annulla il `DefaultBodyLimit` del router (`runtime.rs:691`, 20 MB); `ap.rs:700-701` chiama `image::load_from_memory` prima di qualsiasi limite dimensionale. Un PNG piccolo e compresso può decodificare in centinaia di MB. **Fix:** usare il limite configurato, validare i magic bytes e usare `image::io::Reader::with_limits`.

**H-06 — Il codec P2P alloca la lunghezza dichiarata dal peer (fino a 4 GiB).** `crates/fedi3_core/src/p2p.rs:312-313`: `let len = u32::from_be_bytes(...) as usize; let mut buf = vec![0u8; len];`. Qualsiasi peer libp2p può inviare un header `0xFFFFFFFF` e forzare 4 GiB di allocazione azzerata. **Fix:** rifiutare `len > MAX_P2P_FRAME` (2–4 MiB).

**H-07 — Ingest gossip/globale: primo che scrive vince.** `ap.rs:1074-1099` accetta una `Create`/`Announce` pubblica da qualsiasi peer (il token interno protegge il trasporto, non l'autore); `social_db.rs:795-810` usa `INSERT OR IGNORE`, quindi la prima copia di un `id` vince e le copie legittime successive vengono scartate. Un peer malevolo può iniettare una nota attribuita a chiunque e "occupare" l'id di una nota reale con contenuto falso. **Fix:** validare l'origine di actor/id prima di memorizzare e preferire la versione dal dominio dell'attore in caso di conflitto.

**H-08 — Il fan-out dei `move_notice` non firma le richieste, quindi fallisce sempre.** `main.rs:8549-8562` invia `POST {relay}/_fedi3/relay/move_notice` senza header `Signature`; il ricevitore (`main.rs:7486-7505`) accetta solo se `verify_move_notice_signature` è vero (che ritorna `false` quando `Signature` manca, `main.rs:8770-8773`) o con admin token. Ogni consegna prende 401, registra `sent_ok=0` e viene ritentata ogni 60 s con backoff per 7 giorni (`move_notice_ttl_secs`). **Effetto:** la migrazione fra relay è una feature morta che genera traffico e righe DB inutili. **Fix:** firmare con la chiave Ed25519 del relay (già usata per la telemetria) e verificare contro la chiave nel registry.

**H-09 — Il token relay obbligatorio uccide anche l'API locale.** `runtime.rs:715-716` `if relay_token.len() < 16 { anyhow::bail!(...) }` avviene **dopo** il bind e lo spawn del server (`runtime.rs:695-706`) ma **dentro** `run_core`: il `bail` fa uscire il thread, il runtime tokio viene distrutto e il server axum abortito. Un utente che vuole solo P2P o solo locale resta senza UI. Nota: il valore di default `"devtoken"` è di 8 caratteri, quindi anche i default falliscono. **Fix:** rendere il relay opzionale (salta check e tunnel se `relay_ws` è vuoto) e segnalare l'errore tramite l'handshake di readiness.

**H-10 — `forward_to_user` tiene il lock globale dei tunnel attraverso una `send` senza timeout e 15 s di attesa.** `main.rs:2606-2637`: `let tunnels = state.tunnels.read().await;` resta vivo fino a fine funzione, `tunnel.tx.send(msg).await` non ha timeout e blocca quando il canale da 64 slot è pieno (client che non legge), poi c'è `timeout(15s, resp_rx)`. **Effetto:** un singolo client lento blocca ogni nuova connessione tunnel e ogni altro forward. È un head-of-line blocking che mette in ginocchio il servizio. **Fix:** clonare il `Sender` ed **rilasciare il guard prima** di attendere; mettere un timeout sulla send e trattare lo scaduto come 503/spool.

**H-11 — Una consegna fallita verso un utente online viene scartata e il relay risponde comunque 202.** `main.rs:2810-2827`: se `is_online`, chiama `forward_to_user` e su `429/503/504/502` fa `continue` **senza** accodare nello spool. Poi `main.rs:2852-2855` risponde `202 ACCEPTED` se almeno un destinatario ha avuto successo. **Effetto:** perdita permanente di messaggi proprio sotto carico (limite inflight raggiunto, o tunnel in riconnessione). **Fix:** su qualsiasi risposta non-2xx cadere nello spool; rispondere 202 solo se ogni destinatario è stato consegnato **o** accodato, altrimenti 503.

**H-12 — Rate limit keyed sull'IP del proxy, non del client.** `main.rs:7534-7537` (e `7601`, `7362`, `7411`, `7450`, `6982`, `7266`) usano `peer_ip(&peer)`, che dietro Caddy è **sempre lo stesso** per tutti i client internet, mentre `shared_inbox`/`webfinger` usano correttamente `client_ip(...)`. Con `FEDI3_RELAY_RL_FORWARD_PER_MIN=240` un pugno di istanze esaurisce il bucket condiviso e **tutti** ricevono 429, incluso `/webrtc/poll` che ogni client chiama ogni pochi secondi. **Fix:** `client_ip(&state.cfg, &peer, &headers)` in tutti gli handler e bucket separati per route.

**H-13 — `X-Forwarded-For` spoofabile su due livelli.** `main.rs:6404-6427` prende il **primo** valore di XFF; Caddy appende l'IP reale a un XFF già esistente, quindi il primo elemento è controllato dal client. In più `docker-compose.yml` pubblica `0.0.0.0:8787:8787` oltre a Caddy e imposta `FEDI3_RELAY_TRUST_PROXY_HEADERS=true`: chi colpisce direttamente la 8787 può inviare qualsiasi `X-Real-IP`/`X-Forwarded-For` (Caddy non imposta `X-Real-IP` di default e lascia passare quello del client). **Effetto:** rate limit, allow/deny list IP e audit `ip` diventano attacker-controlled; inoltre la porta 8787 serve plain HTTP senza TLS. **Fix:** usare l'ultimo elemento di XFF o configurare Caddy con `trusted_proxies` + sovrascrittura; non pubblicare 8787 sull'host.

**H-14 — Confronto dei token non constant-time e nessuna validazione all'avvio.** `main.rs:3262-3267` `token == expected`; `main.rs:4218-4245` confronta gli hash con `==`; `main.rs:1217` `std::env::var("FEDI3_RELAY_ADMIN_TOKEN").ok()` senza trim né controllo di lunghezza. Con `FEDI3_RELAY_ADMIN_TOKEN` vuoto (`docker-compose.yml:24` lo passa come stringa vuota se manca nel `.env`) `expected == ""` e solo un `Authorization: Bearer ` con spazio finale passa, mentre `bearer_token()` tratta l'empty come `None`: due nozioni di "vuoto" in conflitto, e tutti gli endpoint admin/health/metrics rispondono 401 senza alcun errore all'avvio. **Fix:** `subtle::ConstantTimeEq`, `bail!` all'avvio se il token è assente o < 32 caratteri.

**H-15 — `verify_actor_signature_for_request` accetta firme che coprono quasi nulla e solo RSA.** `main.rs:8609` default `headers="date"`; `main.rs:8722-8734` il Digest è opzionale; `main.rs:8763` verifica solo RSA PKCS#1 v1.5. Ed25519 (usato da molte implementazioni moderne e dal progetto stesso per la telemetria) non può essere verificato, quindi il signaling WebRTC fallisce silenziosamente con 401 per quegli attori. Il parsing dell'header è split su `,` e `keyId=` è case-sensitive. **Fix:** richiedere `(request-target)`, `host`, `date` (+ `digest` se c'è un body), supportare Ed25519 (`ed25519-dalek` è già dipendenza), parsing con quoted-string e chiavi case-insensitive.

**H-16 — Il token del tunnel viaggia in query string e finisce nei log.** `main.rs:571-573` `struct TunnelQuery { token: Option<String> }`; `tunnel.rs:37-39` costruisce `{relay_ws}/tunnel/{user}?token={token}` e lo logga con `info!(%url, ...)`. Caddy registra la URI completa negli access log. Il token è la credenziale long-lived dell'utente (usata anche per media, search, move, WebRTC). **Fix:** accettare `Authorization: Bearer` sull'upgrade come meccanismo primario, tenere `?token=` come fallback, o usare ticket monouso a breve scadenza; non loggare la query.

**H-17 — Il chat inbox accetta envelope da qualsiasi attore in qualsiasi thread.** `ap.rs:2618-2636` decifra e chiama `chat::store_incoming_payload`; `chat.rs:260-279` fa `create_chat_thread(thread_id, "dm", None)` e `upsert_chat_member(...)` per `op == "message"` senza verificare che il mittente sia membro di un thread esistente. Gli id dei thread DM sono un hash degli URL dei membri, non un segreto: chiunque conosca l'id può iniettare messaggi e creare thread fantasma. **Fix:** richiedere che il thread esista e che `env.sender_actor` sia membro.

**H-18 — I bundle chat non sono legati al peer che li serve.** `ap.rs:3544-3563` restituisce qualsiasi `ChatBundle` ricevuto da `peer_id`; `chat.rs:167-181` verifica solo che la firma del bundle validi contro la chiave di `bundle.actor`, senza controllare che `bundle.actor` sia l'attore atteso, che `bundle.peer_id` sia il peer che ha servito, o che `bundle.did` corrisponda. Un peer può restituire il bundle valido di un altro attore e il messaggio diventa indecifrabile. **Fix:** validare `peer_id`, `actor` e `did` dopo il fetch.

**H-19 — I file di chiave e il DB non hanno permessi ristretti.** `crates/fedi3_core/src/keys.rs:40-48` scrive `identity_private_key.pem` con `fs::write` senza `set_permissions`; nessun `chmod`/`PermissionsExt` esiste nel crate (verificato con grep). `p2p.rs:1136-1145` scrive `p2p_keypair.pb` allo stesso modo. SQLite crea `fedi3.db`/`-wal` con l'umask del processo (tipicamente 0644). Su un desktop multi-utente la chiave privata RSA, il keypair P2P e i segreti Kyber sono leggibili da altri utenti locali. **Fix:** 0600 sui file di chiave e sul DB/WAL/SHM, 0700 sulla directory dati, scrittura atomica (temp + rename).

**H-20 — Nessuna deduplica delle attività nel relay.** `main.rs:2763-2857` legge solo `to`/`cc`/`bcc`/`audience`; l'`id` dell'attività non viene mai registrato né controllato. Combinato con H-11 (202 anche su fallimento parziale) e con i retry dei sender, ogni retry riconsegna la stessa `Create`/`Announce` all'utente, generando post e notifiche duplicati. **Fix:** tabella di dedupe `(activity_id, recipient)` con TTL breve e `ON CONFLICT DO NOTHING`.

### Affidabilità e scalabilità del relay

**H-21 — Un unico `Mutex<Db>` globale serializza ogni operazione e SQLite apre una connessione per query.** `main.rs:223` `db: Arc<Mutex<Db>>`; `main.rs:3299-3303` `open_sqlite_conn` fa `Connection::open` + 5 PRAGMA (incluso `journal_mode=WAL`, che prende un lock) **per ogni query**; per Postgres `block_on_result` usa `block_in_place(|| handle.block_on(...))` (`main.rs:547-558`) parcheggiando un worker **mentre tiene il mutex globale**. La concorrenza DB effettiva è 1, indipendentemente da `FEDI3_RELAY_PG_POOL_MAX_SIZE=16`; `/healthz` prende lo stesso mutex e può bloccarsi dietro una query lenta. **Fix:** pool di connessioni (r2d2/deadpool), rimozione del `Mutex` esterno, `spawn_blocking` per il percorso sync, o `Db` async-native con `deadpool-postgres`.

**H-22 — IO bloccante (`std::fs`) dentro handler async, con più copie del body.** `media_store.rs:58-63` e `:71-74` usano `std::fs::create_dir_all/write/read` dentro `async fn`; `main.rs:1967` fa `body.to_vec()`, `media_store.rs:179` fa un'altra `to_vec()` per S3. Un upload da 64 MB blocca un worker tokio per tutta la durata dell'I/O e occupa fino a 3 copie in RAM; con `rt-multi-thread` (worker = core CPU) pochi upload grandi affamano il runtime: i tunnel smettono di rispondere e scattano i timeout da 15 s. **Fix:** `spawn_blocking`/`tokio::fs`, `ByteStream::from(bytes)` per S3 senza copia, streaming in download (`ReaderStream` + `Body::from_stream`).

**H-23 — `/healthz` e `/readyz` richiedono il token admin e scrivono una riga di audit per probe.** `main.rs:2067-2079` chiama `admin_guard` (rate limit + token) e poi `insert_admin_audit`. Un probe ogni 10 s scrive ~8.640 righe/giorno; senza token il probe riceve 401 e l'orchestratore riavvia il container in loop; se probe e Prometheus condividono il bucket da 120/min, un 429 fa credere al sistema che il relay sia giù. Inoltre **non esiste alcun `HEALTHCHECK`** né nel Dockerfile né nel servizio `relay` del compose, e `caddy` dipende da `relay` senza `condition`. **Fix:** `/healthz` non autenticato, senza DB e senza side effect; `/readyz` con un ping read-only e senza audit; aggiungere `HEALTHCHECK`.

**H-24 — Migrazioni assenti e `ALTER TABLE` con errori inghiottiti.** `main.rs:3533-3541` esegue `let _ = conn.execute("ALTER TABLE ...")` — un fallimento per un motivo diverso da "colonna già esistente" passa inosservato. Non esiste alcuna tabella `schema_version` in nessuno dei due backend. `sql/postgres_schema.sql` e le DDL inline sono **due fonti di verità** (il file è incluso con `include_str!` a `main.rs:3568`), e `docs/ops_backup_restore.md` dice di applicarlo a mano anche se il relay lo fa già. **Fix:** tabella di versione + migrazioni numerate in transazione, errori loggati e fail-fast, una sola fonte di verità.

**H-25 — Lo schema del core è senza versione e senza `busy_timeout`.** `social_db.rs:106-112` e `delivery_queue.rs:638` aprono lo stesso `fedi3.db` con WAL ma senza `busy_timeout`; due writer concorrenti (handler HTTP + worker coda) producono immediatamente `SQLITE_BUSY`. **Fix:** `busy_timeout=5000`, `synchronous=NORMAL`, scrittore singolo, e versione dello schema.

**H-26 — I worker di consegna muoiono definitivamente su un errore DB transitorio.** `delivery_queue.rs:164` `let jobs = self.fetch_due_jobs(40).await?;` dentro il loop; su `Err` il task ritorna e logga solo un `warn!` (`delivery_queue.rs:143`). `object_fetch.rs:77-81`/`129`/`151` usano `.await??`. Con H-25 basta un `SQLITE_BUSY` per fermare **tutta** la federazione in uscita fino al riavvio dell'app. **Fix:** gestire l'errore dentro il loop con backoff e continuare; supervisore che riavvia il worker se termina inaspettatamente.

**H-27 — `admin_delete_user` lascia blob, note e segnali orfani.** `main.rs:4552-4562` cancella righe da `inbox_spool`, `user_cache`, `user_collection_cache`, `media_items`, `peer_directory` ma **non** gli oggetti reali in S3/local/WebDAV (`MediaBackend` non ha `delete`), né `relay_notes`, `user_moves`, `relay_outbox_index`, `webrtc_signals`; e non c'è transazione. **Effetto:** storage pagato per sempre e note dell'utente cancellato ancora nell'indice di ricerca pubblico (problema GDPR/privacy). **Fix:** `delete` nel trait del backend, cancellazione dei blob, transazione, e rimozione delle note dell'attore dall'indice.

**H-28 — Media senza quota, senza DELETE, senza rate limit, bufferizzati interi.** `main.rs:2017-2059`: `media_get` non ha rate limiter (a differenza di `media_upload`), non fa conditional GET, carica l'intero oggetto in `Vec<u8>`; non esiste alcuna rotta `DELETE`. Un utente autenticato può caricare 64 MB all'infinito e nessuno li cancella mai. **Fix:** quota per utente (`SUM(size)`), rotta DELETE, rate limit separato per IP, streaming/presigned URL per S3.

**H-29 — Il relay accetta nel tunnel username che tutti gli altri handler poi rifiutano.** `main.rs:1683-1714` `handle_tunnel` non chiama `is_valid_username` (a differenza di `register`, `media_upload`, `forward_to_user`). Con `allow_self_register=true`, `GET /tunnel/Alice?token=…` crea l'utente `Alice`; poi `forward_to_user("Alice")` risponde 400, `media_upload` 400, `register` rifiuta. L'utente esiste (consuma spool, documenti Meili, telemetria) ma non può mai servire una richiesta. **Fix:** validare lo username prima dell'upgrade.

**H-30 — Race di riconnessione: il disconnect del vecchio tunnel rimuove il nuovo.** `main.rs:1722` inserisce `TunnelHandle { tx }`; `main.rs:1818` fa `remove(&user)` senza generazione/connection id. Se l'utente si riconnette, la cleanup della vecchia connessione cancella l'entry della nuova, che diventa invisibile a `forward_to_user` → 503 → attività scartate (H-11). **Fix:** memorizzare un connection id e rimuovere solo se coincide (`Arc::ptr_eq`).

**H-31 — Le entry `inflight` non vengono rimosse al timeout.** `main.rs:1769-1782` inserisce `id → oneshot::Sender`; `main.rs:2632-2634` al timeout ritorna senza rimuoverla. Viene rimossa solo quando il client invia la risposta corrispondente o chiude. Un client che smette di rispondere a certi id accumula entry per tutta la vita del tunnel. **Fix:** guard di `Drop` che rimuove l'id, o pruning periodico.

**H-32 — Lo spool non ha cap in byte, salva base64 nel DB e i flush concorrenti possono duplicare.** `main.rs:4611-4631` limita solo il numero di righe (`spool_max_rows_per_user`, default 5.000): 5.000 × 64 MB ≈ 320 GB per utente, e `body_len` è registrato ma mai usato. Il body è base64 in una colonna TEXT (+33% e costo di decode). `flush_spool_for_user` (`main.rs:3044-3113`) lista, consegna, poi cancella: due flush concorrenti (uno per riconnessione) possono consegnare le stesse righe due volte. **Fix:** cap in byte (`SUM(body_len)`), BLOB/BYTEA, mutex per utente o `FOR UPDATE SKIP LOCKED`.

**H-33 — WebSocket non irrobustito.** `main.rs:1790-1806` legge solo `Message::Text`; nessun ping/pong keepalive, nessun idle timeout, nessun `max_message_size` esplicito. Una connessione half-open resta "viva" per sempre, i forward aspettano 15 s e poi falliscono, l'utente è riportato online mentre è morto. Inoltre il body base64 di una risposta può superare il limite inbound di default di axum. **Fix:** keepalive + chiusura dopo N pong mancanti, limiti espliciti coerenti con `MAX_BODY_BYTES`, cap su `resp.body_b64`.

**H-34 — Nessun timeout applicativo, nessun limite di concorrenza, nessuna gestione CORS.** `main.rs:1150-1174` ha solo `DefaultBodyLimit`, `TraceLayer` e `enforce_ip_policy`. Gli handler possono attendere 15 s (o indefinitamente su `tx.send`, H-10) e i socket si accumulano senza limite; un client browser su origine diversa non può chiamare le API del relay (nessun `Access-Control-Allow-Origin`, `OPTIONS` → 405). **Fix:** `TimeoutLayer` (30 s), `ConcurrencyLimitLayer` dimensionato sul pool DB, `CorsLayer` con allowlist.

**H-35 — Il relay accetta URL arbitrari da telemetria e dai `next` degli outbox.** `main.rs:2981-2983` e `:7299-7302` verificano solo il prefisso `http://`/`https://`; poi `sync_relays_once` (`main.rs:8168-8171`) fa GET su `{relay_url}/_fedi3/relay/sync/notes`. Chiunque può registrarsi via telemetria (firma self-signed) e annunciare `http://127.0.0.1:5432`: il relay lo interrogherà per sempre ogni 120 s con timeout 30 s. **Fix:** stessa guardia sugli IP privati di H-03, e cap sul numero di relay annunciati (H-36).

**H-36 — La telemetria accetta liste illimitate di relay/utenti/peer e le scrive sotto il lock globale.** `main.rs:7286-7339`: un POST firmato può annunciare 100.000 relay (body limit 64 MB), ognuno diventa una riga permanente in `relay_registry` e un futuro target HTTP, tutto mentre si tiene il mutex DB. Il fan-out periodico itera fino a 500 relay **in sequenza** con timeout 30 s: un relay morto blocca il ciclo per 30-90 s, e 500 relay morti lo portano a ore (starvation alfabetica). **Fix:** cap per payload (500/1000/1000), scritture in transazione, fan-out con `buffer_unordered(8)` e timeout 5 s.

### Core — affidabilità

**H-37 — L'inbox marca l'attività come vista prima di elaborarla: i fallimenti perdono l'attività per sempre.** `ap.rs:4859` `mark_inbox_seen(&dedup_id)` poi `ap.rs:4871` `process_inbox_activity(...)`; se fallisce ritorna 502 ma la riga di dedup resta, quindi il retry del sender riceve `202 duplicate` (`ap.rs:4861-4864`) e l'attività non viene mai riprocessata. Stesso pattern in `p2p.rs:880-885` e `device_sync.rs:246-248`. **Fix:** dedup e processing transazionali (inserire "seen" solo dopo il successo), o cancellare la riga e rispondere 5xx per far ritentare il sender.

**H-38 — Paginazione che perde elementi con lo stesso timestamp.** `social_db.rs:1978-1981`/`1999-2005` usano `WHERE created_at_ms < ?1`; gli inserimenti bulk condividono il millisecondo (`social_db.rs:630-648`). La pagina 2 salta per sempre tutte le righe rimaste con lo stesso ms: follower, outbox e feed perdono elementi silenziosamente. **Fix:** cursore composito `(created_at_ms, id)`.

**H-39 — L'outbox pubblico espone anche i DM.** `ap.rs:4042` `list_outbox` e `social_db.rs:1974-2009` selezionano **tutte** le righe di `outbox_items` senza filtro di visibilità; `outbox_get` (`ap.rs:4013-4070`) le restituisce verbatim. `/users/<u>/outbox` è pubblico: chiunque può paginare i messaggi diretti dell'utente. **Fix:** filtrare per visibilità (Public sempre; Followers solo a un follower firmato; direct omessi o serviti solo via inbox).

**H-40 — I job in attesa di receipt non raggiungono mai `max_attempts`.** `delivery_queue.rs:340`/`350`/`372` chiamano `mark_awaiting_receipt(...)`; `delivery_queue.rs:270` seleziona `status IN (0, 3)`; solo il ramo di errore HTTP (`delivery_queue.rs:405-408`) applica `attempt_no >= max_attempts`. Se un peer non manda mai il receipt, il job resta status 3 e viene reinviato per sempre (30 s → 6 h), crescendo il DB e generando traffico duplicato. Inoltre non esiste alcun `DELETE FROM delivery_jobs` (verificato): i job non vengono **mai** ripuliti. **Fix:** applicare `max_attempts` anche allo stato 3, dead-letter, e `prune_delivery_jobs` nel GC.

**H-41 — Un fallimento di risoluzione attore non riprogramma il job: hot loop ogni 2 s.** `delivery_queue.rs:194-203` e `219-221` fanno `else { ... continue; }` senza reschedule, mentre il tick è 2 s (`delivery_queue.rs:158`). Un attore irraggiungibile resta "due" per sempre e viene ri-risolto ogni 2 s, martellando il remoto e affamando gli altri job (40 per tick). **Fix:** riprogrammare con `next_backoff` e dead-letter dopo N tentativi.

**H-42 — Shutdown incompleto.** `runtime.rs:809-822` attende solo `tunnel` e `server`; i worker di delivery queue, object fetch, storage GC, relay sync, legacy sync, device sync, P2P sync e WebRTC sono spawnati e mai joinati. `stop()` (`runtime.rs:193-209`) rimuove l'handle, invia il watch e fa il join su un thread detached, ritornando subito. **Effetto:** scritture in volo interrotte, DB non checkpointato all'uscita, e un riavvio immediato può correre sul vecchio listener (bind fallito, silenzioso per P0-11). **Fix:** `JoinSet` con await e timeout, `stop()` bloccante o con segnale di completamento, checkpoint esplicito del DB.

**H-43 — Username e domain non sono validati; lo username finisce in un path.** `runtime.rs:227` `default_data_dir()?.join("app").join(&cfg.username)`. `username = "../other"` o `"a/b"` esce dalla directory dati o rende 404 ogni rotta; uno spazio rompe gli URL. **Fix:** validare `^[A-Za-z0-9_]{1,64}$` e un domain hostname-shaped all'avvio.

**H-44 — `manually_approves_followers` è pubblicizzato ma ignorato.** `runtime.rs:377` lo passa, `ap.rs:3810` lo pubblica nell'actor document, ma il ramo Follow di `process_inbox_activity` (`ap.rs:5263-5280`) fa sempre `add_follower` e invia `Accept`. Un account "locked" accetta tutti. **Fix:** stato `pending` con approvazione/rifiuto dal UI.

**H-45 — Accept può accettare un follow dall'attore sbagliato.** `ap.rs:5310-5313`: se l'oggetto è una stringa che non si risolve, **qualsiasi** `Accept` da `accept_actor` marca quell'attore come accepted; il ramo embedded (`ap.rs:5328-5331`) fa `following(obj_object)` accepted senza verificare `accept_actor == obj_object`. **Fix:** richiedere `accept_actor == follow.object` e che il Follow embedded abbia `actor` = noi.

**H-46 — `KeyResolver`: cache illimitata, binding del keyId lasso, amplificazione di fetch su GET.** `http_sig.rs:24` `HashMap` senza cap né eviction globale; le entry scadono solo se lo stesso actor viene riaccesso (`http_sig.rs:49-58`, `104-127`). Il `publicKey.id` restituito non deve coincidere con il `keyId` richiesto, e `actor.id` non è verificato contro l'URL fetchato. Un attaccante può usare keyId sempre nuovi (il suo server può generare path infiniti): ogni richiesta crea una entry permanente e un fetch. **Fix:** LRU limitata (10k), validare `pk.id == key_id` e `actor.id == actor_url` (o stessa origine), rate limit sui GET.

**H-47 — Limiter dell'inbox che può superare il proprio limite di 50k.** `ap.rs:5082`/`5098-5100`: `if guard.len() > 50_000 { retain(|_, v| now - v.window_start_ms < 10*60_000) }`. Sotto un flood di chiavi uniche nella finestra di 10 minuti il `retain` non rimuove nulla e la mappa continua a crescere. **Fix:** cap rigido con eviction LRU.

**H-48 — Metriche di rete con mappe illimitate e prune solo quando la UI interroga.** `net_metrics.rs:98-102` (con `.lock().unwrap()`); `prune_seen` (`net_metrics.rs:178-199`) è chiamato solo da `snapshot_json`. Un flood di peer cresce tre HashMap senza limite quando la UI è chiusa. **Fix:** prune su timer, cap/LRU, gestione del poisoning.

**H-49 — `relay_bridge` panica su path malformato.** `relay_bridge.rs:18` costruisce `http://localhost{path}{query}` e `:31` fa `request.body(...).unwrap()`. Un path senza `/` iniziale o con spazi rende la URI invalida e il `.unwrap()` panica. In P2P questo avviene nel loop del swarm (`p2p.rs:819`), quindi il panic abbatte l'intero task P2P; nel tunnel abbatte il tunnel (che non si riavvia, P0-09). Un peer malevolo può uccidere il P2P con una sola richiesta. **Fix:** normalizzare il path, costruire la URI con `?` e rispondere 400.

### App Flutter

**H-50 — Nessun timeout HTTP in tutto il layer di rete Dart.** `app/lib/core/core_api.dart:88`, `:123`, `:135`, `:292`; `actor_repository.dart:225`; `relay_admin_api.dart:37,51,61`; `core_event_stream.dart:61`. `package:http` non ha timeout di default. Se il core si blocca o un host remoto non risponde, il Future non completa mai: spinner infiniti, pulsanti disabilitati, nessuna cancellazione. Ogni schermata reimplementa il retry con confronti di stringhe (`timelines_screen.dart:563`, `notifications_screen.dart:175`, `chat_screen.dart:170`). **Fix:** un `http.Client` iniettato con `connectionTimeout` + `.timeout(15s)` su ogni richiesta, `CoreApiException` tipizzata con `statusCode`/`retryable`, token di cancellazione.

**H-51 — Il core è considerato "attivo" prima del bind e `markCoreDead` non viene mai chiamato.** `app/lib/state/app_state.dart:38-47` definisce `markCoreDead` ma `grep` non trova altri usi; `app_state.dart:236` imposta l'handle subito. In Rust `run_core` logga solo l'errore (`runtime.rs:177`). **Effetto:** se il core muore, `isRunning` resta `true` per sempre, la UI continua a fare polling su una porta morta, nessun banner "core fermo", nessun restart. **Fix:** watchdog che interroga `/_fedi3/health` ogni 5 s, chiama `markCoreDead` e tenta un restart con backoff; in Rust, callback di stato sul thread.

**H-52 — Polling storm senza consapevolezza del ciclo di vita.** `nerd_status_bar.dart:51` 2 s; `timelines_screen.dart:410` 8 s × 4 timeline; `right_sidebar.dart:47` 10 s; `relays_screen.dart:42` 10 s; `peer_presence_store.dart:29` 10 s; `shell.dart:47-54` costruisce tutte e sei le schermate in `IndexedStack` all'avvio; 7 siti `CoreEventStream(` aprono SSE; `grep "WidgetsBindingObserver|didChangeAppLifecycleState"` → **nessun risultato**. **Effetto:** ~30-60 richieste/minuto e 6 socket SSE anche in background su mobile: batteria, radio, e lavoro costante sul core. **Fix:** un `WidgetsBindingObserver` in `AppRoot` che sospende tutto ciò che non è `resumed`, un solo stream SSE condiviso (broadcast), polling 5-10 s solo quando il widget è visibile, lazy tab.

**H-53 — Il refresh delle notifiche azzera lista e cursore a ogni evento.** `notifications_screen.dart:116-124`: `setState(() { _items.clear(); _cursor = null; ... }); await _loadMore();` chiamato dal debounce SSE (`:92-95`) e dal listener di app state (`:67-71`). Se arriva una notifica mentre l'utente sta leggendo la pagina 3, la lista torna a pagina 1 e lo scroll salta; se un load è in volo, `_loadMore` esce subito (`if (_loading) return;`, `:139-141`) e il refresh diventa un no-op lasciando dati stantii. **Fix:** inserire i nuovi item in testa senza azzerare il cursore; generation counter per scartare risposte vecchie.

**H-54 — Race nel refresh/load delle timeline e `setState` dopo dispose.** `timelines_screen.dart:466-472` azzera `_items`/`_cursor` e chiama `_loadMore`, che inizia con `if (_loading) return;` (`:474`) e finisce in `finally { setState(...) }` (`:512-514`) senza guardia `mounted`. Un pull-to-refresh durante un load svuota la lista e ritorna; se la Shell viene smontata (logout/cambio config) il `finally` chiama `setState` su uno State dismesso → eccezioni e log spam. **Fix:** generation counter, `if (!mounted) return;` prima di ogni `setState`, refresh disabilitato durante il load.

**H-55 — `media_kit` non è mai inizializzato su Linux: i video sono rotti nell'AppImage.** `app/lib/main.dart:22` `if (!Platform.isLinux || Platform.environment['FEDI3_ENABLE_MEDIA_KIT_LINUX'] == '1') { MediaKit.ensureInitialized(); }`; l'AppRun generato da `scripts/build_appimage.sh` e `AppDir/AppRun` esportano `LD_LIBRARY_PATH`, `GDK_BACKEND`, `LIBGL_*`… ma **non** `FEDI3_ENABLE_MEDIA_KIT_LINUX` (verificato con grep). `fedi_media_player.dart:37` crea `Player()` in `initState`. **Effetto:** toccare un video/audio lancia un'eccezione nel widget, pur essendo `libmpv` correttamente bundlato dallo script. In più `--enable-software-rendering` è attivo di default (`FEDI3_SOFTWARE_RENDERING:-1`) e degrada la riproduzione su macchine con GPU. **Fix:** rimuovere il gate o impostare la variabile in AppRun; rendere il software rendering opt-in.

**H-56 — Il "backup" non contiene né la chiave privata né i dati, ed è in chiaro.** `app/lib/services/backup_codec.dart:21-28` esporta solo `coreConfig` + `uiPrefs`. Mancano la chiave RSA (nel data dir del core, `keys.rs`), `fedi3.db`, media e cronologia chat. Il file è JSON in chiaro e contiene `relayToken`/`internalToken`; `decode` (`:30-44`) ignora il campo `v`. Condividere il backup = condividere i segreti, e ripristinarlo su un altro device non ripristina l'identità. **Fix:** includere chiave e DB (o un export logico), cifrare con password (Argon2 + AES-GCM), validare `v`.

**H-57 — Import del backup senza rollback.** `app/lib/ui/screens/backup_screen.dart:169-175`: `stopCore(); saveConfig(bundle.config); savePrefs(bundle.prefs); startCore();` in un unico `try`; il `catch` imposta solo `_status`. Se `savePrefs` fallisce (disco pieno, path_provider) la config è già sovrascritta e il core è fermo: l'utente ha un altro account e un'app morta, con un messaggio "import failed". **Fix:** snapshot di config/prefs, validazione di `v`, applicazione con restore dello snapshot e restart in caso di errore.

**H-58 — Nessun arresto del core alla chiusura dell'app.** `main.dart`/`app_root.dart` non hanno hook di lifecycle; chiudendo la finestra il processo termina con SQLite e la coda di consegna in scrittura, senza checkpoint. **Fix:** `AppLifecycleListener`/`onWindowClose` che attende `stopCore()` con timeout.

**H-59 — Il test widget esistente asserisce un testo che l'app non mostra mai, e non c'è CI.** `app/test/widget_test.dart:23-24` `expect(find.text('Fedi3 setup'), findsOneWidget)`; `'Fedi3 setup'` è `onboardingTitle` (`app_en.arb:573`), mentre `FirstRunScreen` rende `firstRunTitle` = "Welcome" (`app_en.arb:203`, usato in `first_run_screen.dart:115`). **Il solo test del progetto è rosso o non viene mai eseguito** (non c'è CI: `.github/` contiene solo `copilot-instructions.md`). Inoltre il test pompa un frame mentre `FirstRunScreen.initState` fa vere chiamate HTTP (`first_run_screen.dart:47-75`), quindi è flaky. **Fix:** asserire "Welcome", iniettare un `http.Client` fake, `pumpAndSettle`, e aggiungere la pipeline.

**H-60 — Zero test Rust.** `grep -rn "#\[test\]\|#\[tokio::test\]" crates/` → **0** su 28.358 righe. Per un progetto con firme HTTP, crittografia, consegna asincrona e schema DB, è il rischio numero uno: nessun refactor è verificabile. **Fix minimo:** test su `parse_signature_header`/`build_signing_string` (round-trip con firma reale), `verify_digest_if_present` (assente/presente/errato), `verify_date`, `activity_dedup_id`, `sanitize_key`, `parse_ip_rule`, `user_from_host`, `CoreConfig`↔JSON round-trip, `BackupCodec`.

**H-61 — Contenuto federato che genera richieste dirette verso host arbitrari.** `actor_repository.dart:222-229`, `object_repository.dart:21-31`, `link_preview_repository.dart:68-93` (cap 256 KB ma nessun timeout), `status_avatar.dart:58` e `inline_media_tile.dart:77` (`Image.network`). Ogni post renderizzato può far connettere il client a host terzi arbitrari (anche `http://192.168.1.1/...`), rivelando IP/User-Agent e permettendo tracking pixel via URL unici. Un host che non risponde appende il Future in `_inflight` per sempre (`actor_repository.dart:198-209`), e ogni hover successivo attende lo stesso Future mai completato. **Fix:** proxy dei media remoti attraverso il core o almeno richiedere https, timeout e cap; rispettare `blockedDomains`; toggle "carica immagini remote".

**H-62 — Il testo dei post è inviato come HTML grezzo quando non ci sono marker MFM.** `compose_screen.dart:384` e `inline_composer.dart:399`: `final content = MfmCodec.hasMarkers(text) ? MfmCodec.toHtml(text) : text;`. `MfmCodec.toHtml` esegue correttamente l'escaping (`mfm_codec.dart:20-27`), ma il ramo senza marker lo bypassa. **Effetto:** scrivere `1 < 2 & 3` pubblica `content: "1 < 2 & 3"` come `text/html`; Mastodon/Misskey lo interpretano male o lo perdono, e un `<b>` letterale mette in grassetto. È un bug di correttezza visibile agli altri server. **Fix:** produrre sempre HTML (`plainToHtml` con solo escaping), con test su `<`, `&`, virgolette e newline.

**H-63 — N+1 per ogni nota nella timeline.** `note_card.dart:139-155`: per ogni card un post-frame callback chiama `_loadFlags(note.id)`, `_loadMyReactions(api, note.id)` e `EmojiStore.addAll(note.emojis)`; `_loadActors()` (`:120-122`) fetcha autore e booster. `emoji_store.dart:35-55` `addAll` fa un `read()` (parse JSON completo) + `writeAsString` dell'intera mappa per ogni nuovo emoji. Una timeline da 50 item genera ~50 richieste `/_fedi3/reactions/me`, 50+ fetch di actor e fino a 50 read/write su file. **Fix:** endpoint batch per le reazioni, `EmojiStore` in memoria con flush serializzato e debounced, cache dei profili.

**H-64 — Il refresh delle impostazioni non ricostruisce il core e le schermate salvano snapshot stantii.** `security_settings_screen.dart:31`, `privacy_settings_screen.dart:30`, `moderation_settings_screen.dart:30` catturano `_cfg = widget.appState.config!` in `initState`, poi ricostruiscono un `CoreConfig(...)` con 25 campi copiati a mano. Se la config cambia altrove (import backup, altra schermata) mentre la schermata è aperta, il salvataggio sovrascrive con lo snapshot vecchio; aggiungere un campo richiede di toccare 5 costruttori e un campo dimenticato viene silenziosamente resettato. **Fix:** `CoreConfig.copyWith` e lettura di `widget.appState.config` al momento del salvataggio.

**H-65 — Il compose completo non ha file picker, non ha limiti e i draft perdono gli allegati.** `compose_screen.dart:33` `_filePath` è un `TextEditingController`, `:217-220` un `TextField` "Media file path" che richiede un path assoluto; `:341-352` `_addMediaFromPath` legge `File(path)`. `ComposeDraft` (`draft_store.dart:11-53`) non ha campo media. La schermata è raggiungibile solo da `right_sidebar.dart:333` (desktop). **Effetto:** su desktop bisogna digitare un path; su mobile il compose completo non esiste; nessun cap di dimensione lato client; gli allegati si perdono al cambio tab. **Fix:** `openFile`/`desktop_drop`, limiti coerenti con l'inline composer (`inline_composer.dart:327-331`), media nel draft.

**H-66 — Header e path interpolati senza encoding.** `core_api.dart:657-661` imposta `'X-Filename': filename` con il nome grezzo da `XFile.name`; `:264` e `:248` interpolano `threadId` senza `Uri.encodeComponent`. `dart:io` rifiuta header con byte non ASCII (`HttpException: Invalid header field value`): caricare `città.jpg` fallisce con un errore opaco. **Fix:** percent-encoding o filename nel body JSON, sanificazione ASCII di fallback, `Uri.encodeComponent` per gli id.

---

## 4. Problemi MEDIUM

**Relay**
- **M-01** Il `LIKE` su SQLite è rotto: `escape_like` (`main.rs:8243-8245`) inserisce backslash ma nessuna query aggiunge `ESCAPE '\'`; cercare `100%` o `foo_bar` non trova nulla. Inoltre si cerca anche in `content_html` (`main.rs:5152`), quindi `p` o `div` corrispondono a quasi tutto, e `LIKE '%…%'` è un full table scan sotto il mutex globale. **Fix:** `ESCAPE`, colonna di testo puro, FTS5.
- **M-02** Redirect canonico in loop infinito con porta esplicita: `canonical_origin` (`main.rs:8442-8448`) include la porta, `host_only` (`main.rs:8390-8398`) la rimuove → `FEDI3_RELAY_PUBLIC_URL=http://localhost:8787` genera 308 verso sé stesso. Gli URL degli attori perdono la porta. **Fix:** confronto con authority normalizzata.
- **M-03** In host-routing gli URL degli attori derivano dall'header `Host` del client (`main.rs:8431-8439`): con `Host: evil.example` il webfinger restituisce `acct:alice@evil.example` e un `href` su quel dominio. **Fix:** validare `Host` contro `*.{base_domain}` e derivare l'origine da `public_url`.
- **M-04** Android 13+ non chiede mai il permesso notifiche: manca `POST_NOTIFICATIONS` nel manifest e `notification_service.dart:14-27` non chiama `requestNotificationsPermission()`; le notifiche esistono solo con l'app in foreground (nessun foreground service/push). **Fix:** permesso + richiesta esplicita; documentare il limite del background.
- **M-05** Tabella `admin_audit`, `relay_notes`, `peer_registry`, `relay_registry`, `relay_user_directory` mai ripulite (`main.rs:1045-1067` pulisce solo spool, move notices, webrtc signals, peer directory). `relay_notes` conserva il `note_json` completo per sempre: l'indice di ricerca è una copia monotona crescente di ogni nota mai vista. **Fix:** retention configurabile nel loop di cleanup.
- **M-06** `FEDI3_RELAY_HSTS_MAX_AGE_SECS` ha default 0 (`main.rs:1272-1275`) e non è in `.env.example`; un deployment senza compose serve plain HTTP senza enforcement. **Fix:** default 31.536.000 e documentazione.
- **M-07** Config: errori di parsing silenziosamente sostituiti con default (`main.rs:1268-1271` e ~40 occorrenze). `FEDI3_RELAY_DB_DRIVER=pg16` diventa SQLite, `SEARCH_BACKEND=meilisearch` cade su DB (`main.rs:920`), `MAX_BODY_BYTES=64M` diventa 64 MiB. **Fix:** parser che fallisce con messaggio sulle variabili security-relevant.
- **M-08** iOS: mancano `NSCameraUsageDescription`/`NSPhotoLibraryUsageDescription` e nessun URL scheme/deep link registrato (`Info.plist` completo, 44 righe). **Fix:** usage descriptions + `CFBundleURLTypes`.
- **M-09** `.env.example` non documenta `TRUST_PROXY_HEADERS`, `HSTS_MAX_AGE_SECS`, `CSP`, `BASE_DOMAIN`, `IP_ALLOWLIST/DENYLIST`, `TUNNEL_TIMEOUT_SECS`, `SPOOL_*`, `MAX_INBOX_FANOUT`, `MAX_INFLIGHT_PER_USER`, `MEDIA_DIR`, `MEDIA_WEBDAV_*`, `TURN_*`, `P2P_UID/GID`, `P2P_*`. Chi copia il file gira con default non sicuri (`trust_proxy_headers=false` rompe l'IP del client dietro proxy; `hsts=0`). Nel `.env` locale lo **stesso token** è usato per admin e Meilisearch.
- **M-10** Coturn: `--lt-cred-mech` con `--user=${TURN_USER}:${TURN_PASS}` statici condivisi da tutti i client, `TURN_PASS:-CHANGE_ME` come default, nessun `--external-ip` (dietro NAT annuncia IP privati), nessun TLS, immagine `latest`. **Fix:** credenziali temporanee (TURN REST), `external-ip`, TLS 5349, immagine pinnata.
- **M-11** Compose senza limiti di risorsa, senza log rotation, senza `HEALTHCHECK` per il relay, `caddy` che dipende da `relay` senza condizione; `POSTGRES_PASSWORD:-fedi3` e `MEILI_MASTER_KEY:-` (vuoto = Meilisearch senza auth nella rete compose). **Fix:** `mem_limit`, `logging.options.max-size`, immagini pinnate per digest, valori obbligatori.
- **M-12** `Db::init` per Postgres non ritenta se lo schema fallisce: `main.rs:3565-3570` `conn.batch_execute(include_str!(...))?` ritorna subito e `main` panica con `.expect("db init")` (`main.rs:968`). **Fix:** trattare lo schema come la connessione (retry con backoff).
- **M-13** Observability sottile: 5 gauge senza HELP né istogrammi, gated dal token admin, che ricalcolano la telemetria completa a ogni scrape (8+ query DB + firma Ed25519); `TraceLayer` logga a DEBUG mentre il filtro di default è `info`, quindi **nessuna riga per richiesta** e nessun log dei 5xx. **Fix:** metriche reali (counter/histogram per route/status, latenza tunnel, profondità spool), log `on_response` a INFO, endpoint metriche separato.
- **M-14** Media upload non atomico e orfani: `std::fs::write` tronca e scrive senza fsync/rename (`media_store.rs:63`); se l'upsert DB fallisce (`main.rs:2001-2005`) il file resta per sempre senza riferimento. **Fix:** temp + fsync + rename, delete best-effort su fallimento DB.
- **M-15** L'estensione da `X-Filename` non è validata né limitata (`main.rs:1972-1980`): `#`, `?`, spazi rompono le chiavi WebDAV, che non vengono percent-encoded (`media_store.rs:112`). **Fix:** allowlist `^[a-z0-9]{1,8}$` e `urlencoding` nelle URL WebDAV.
- **M-16** Il tipo MIME dei media è fidato dal client (`main.rs:1979-1988`, restituito verbatim a `:2043`) e non c'è quota per utente. Un utente può caricare `text/html` e servirlo dal dominio del relay. **Fix:** allowlist + sniffing magic bytes + quota.
- **M-17** Il Dockerfile del relay non è riproducibile: `FROM rust:1-bookworm` (tag mobile), `Cargo.lock` non copiato, build senza `--locked`, nessuno `strip`, nessun `HEALTHCHECK` (`deploy/relay/Dockerfile:3-22`). **Fix:** `rust:1.86-bookworm`, `COPY Cargo.lock`, `cargo build --locked --release`, `[profile.release] strip`, healthcheck.

**Core**
- **M-18** Il relay-controlled `id` dei media può uscire dalla directory: `ap.rs:166-168` `dir.join(&response.id)` e `media_backend.rs:83` `media_dir(data_dir).join(stored_name)` senza validazione. Un relay che restituisce `id: "../../etc/passwd"` scrive/legge fuori. **Fix:** allowlist `[A-Za-z0-9._-]`, rifiutare `..`/`/`/`\`.
- **M-19** Il pruning media per gli attori non seguiti è accidentalmente annidato dentro `if cfg.cache_only_followed_fedi3` (`storage_gc.rs:367-401`), quindi impostare quel flag a `false` — una configurazione valida — disabilita la quota `other_max_media_items_per_actor`/`other_max_media_bytes_per_actor` e resta solo il cap globale della directory (`storage_gc.rs:404-405`): un solo attore non seguito può riempire il disco. **Fix:** sganciare il pruning media da quel flag o usare un flag dedicato.
- **M-20** Chat E2E senza forward secrecy: `chat.rs:451-456` deriva la chiave AES da un segreto Kyber con HKDF, ma ogni messaggio incapsula verso una chiave statica o una prekey del bundle; nessun ratchet, e i segreti delle prekey usate restano nel DB (`social_db.rs:2919-2927` imposta solo `used_at_ms`). Se `identity_private_key.pem` o il DB trapelano, **tutti** i messaggi passati si decifrano. `crypto_envelope.rs:38-45` usa anch'esso una chiave RSA statica. **Fix:** documentare la garanzia esatta (niente FS), aggiungere un ratchet (o ibrido X25519+ML-KEM con chiavi effimere) e cancellare i segreti usati; valutare SQLCipher.
- **M-21** `pqcrypto-kyber 0.7` è archiviato e Kyber è stato sostituito da ML-KEM (FIPS 203); nessun ibrido con X25519. **Fix:** migrare a `ml-kem` con ibrido.
- **M-22** `net_metrics`/`inbox_limiter` (H-47/H-48) e cache KeyResolver (H-46): tre mappe non limitate con prune opportunistico.
- **M-23** Nessuna rotazione delle chiavi dell'attore: `ap.rs:3766` usa un solo `#main-key`; se `identity_private_key.pem` sparisce, `keys.rs:41-48` ne genera una nuova e **cambia il DID**, senza pubblicare la chiave precedente. **Fix:** lista di chiavi (corrente + precedenti) nell'actor document e storia delle chiavi.
- **M-24** `tokio` dichiarato senza `sync`/`time`/`net` (`Cargo.toml:28`): dipende dall'unificazione transitiva delle feature, fragile. Nessun `rust-version`, nessun `zeroize` sui materiali di chiave.

**App**
- **M-25** Config/prefs scritte in modo non atomico e parse error silenziosamente `null` (`config_store.dart:32-35`, `:20-29`; `prefs_store.dart:19-29`). Un crash a metà scrittura fa credere all'app che non esista configurazione: l'utente torna al primo avvio, con l'identità ancora nel data dir del core. Nessun `chmod 600`: `relayToken`, `internalToken`, `translationAuthKey` (DeepL) e `relayAdminToken` sono in chiaro e leggibili da altri utenti locali. **Fix:** temp+rename, `.bak`, distinguere "assente" da "corrotto", permessi 600, `flutter_secure_storage` per i segreti.
- **M-26** Il token relay può viaggiare su `ws://`: `app_state.dart:116-127` `normalizeRelayWs` aggiorna a `wss://` solo se `publicBaseUrl` è https; con `ws://` il token (e il bearer admin in `relay_admin_api.dart:16-25`) è leggibile sul filo. `bind=0.0.0.0` espone gli endpoint `/_fedi3/*` sulla LAN in chiaro, e `http://0.0.0.0` come target client non è portabile. **Fix:** avvisare/rifiutare `ws://`, default `127.0.0.1`, connettersi a `127.0.0.1` quando il bind è `0.0.0.0`.
- **M-27** Discoverability fragile della libreria FFI: `fedi3_core.dart:64-85` aggiunge il nome nudo (`DynamicLibrary.open('fedi3_core.dll')` usa il search path dell'OS) e risale 6-8 directory padre; nessun handshake di versione con `fedi3_core_version()`. Un DLL stantio da un'altra installazione può essere caricato silenziosamente. **Fix:** sole posizioni documentate, confronto di versione, rimozione del DLL da 28 MB dal repo.
- **M-28** Il compose/UI mostra circa 45 stringhe hardcoded, molte solo in italiano in un'app English-default: `onboarding_screen.dart:148,152,162,163,172` (`'P2P + Relay (fallback dopo 5s)'`, `'Solo P2P (mai relay)'`, …), `edit_config_screen.dart:140,144,154,155,164`, `notifications_screen.dart:279-304`, `timelines_screen.dart:882-906`, `media_viewer_screen.dart:190,193`, `note_card.dart:499,502,645,1256`. Gli ARB sono completi e in parità (489 chiavi per en e it). **Fix:** spostare tutto negli ARB e aggiungere un gate in CI.
- **M-29** Store di preferenze con read-modify-write non coordinato: `note_flags_store.dart:55-67`, `reaction_store.dart:32-44`, `direct_recipient_store.dart:32-44`, `emoji_recent_store.dart:32-44` leggono, mutano e riscrivono l'intero JSON senza serializzazione. Due azioni rapide si sovrascrivono a vicenda e un crash lascia JSON invalido che viene silenziosamente letto come `{}` (bookmark/mute/pin spariscono). **Fix:** store in memoria con scritture serializzate e atomiche.
- **M-30** Schermata Relay: `relays_screen.dart:141` `Text(_stats.toString())` stampa la mappa Dart grezza; `:168-176` cerca a ogni tasto senza debounce (e il guard `_loadingPeers` scarta le battute, quindi i risultati possono essere di un prefisso precedente); `:300-310` pinga ogni relay in parallelo a ogni refresh. **Fix:** campi tipizzati con l10n, debounce 300 ms con generation token, cache delle latenze.
- **M-31** Download media rotto su mobile: `media_viewer_screen.dart:176-183` chiama `getSaveLocation` **fuori** dal `try`; su Android/iOS lancia `UnimplementedError` → eccezione non gestita, il pulsante sembra non fare nulla. Il nome suggerito è `parts.last`, quindi `.../image.jpg?x=1` produce `image.jpg?x=1`. **Fix:** branch per piattaforma (`Share`/`XFile.saveTo` su mobile), nome da `Uri.pathSegments.last` decodificato.
- **M-32** Nessuna accessibilità: `grep -rn "Semantics(" lib` → 0; `Image.network` in `status_avatar.dart:58`, `inline_media_tile.dart:77`, `note_card.dart:1664`, `media_viewer_screen.dart:143` senza `semanticLabel`; stato online indicato solo dal colore (`status_avatar.dart:88-99`); l'unica scorciatoia è Ctrl+Enter in compose. **Fix:** `Semantics`/`semanticLabel`, scorciatoie per tab/ricerca/refresh, test con `SemanticsTester`, verifica a 1.5× text scale.
- **M-33** Config non validata e chiamate relay non autenticate contro `publicBaseUrl`: `core_api.dart:923-941` `Uri.parse(config.publicBaseUrl.trim())` lancia `FormatException` su un valore vuoto o senza schema; `fetchRelayList`/`fetchRelayStats`/`fetchRelayPeers` non inviano alcun token mentre `fetchRelayCoverage`/`fetchRelayMe` sì. **Fix:** validare URL e bind in onboarding/edit_config con errori inline, normalizzare lo schema, mappare 401/403 su messaggi chiari.
- **M-34** Link federati inerti o non validati: `note_card.dart:1851-1859` passa `onTapUrl` che ritorna `false` per ogni URL non-attore (`:353-372`), e `_QuotedPreview` (`:1826`) non passa `onTapUrl` affatto; `open_url.dart:10-17` non filtra gli schemi (`javascript:`, `file:`). **Fix:** un solo handler che allowlista `http/https/mailto` e chiama `openUrlExternal`, applicato anche ai preview quotati.
- **M-35** Telemetria locale senza rotazione e senza crash reporting remoto: `telemetry_service.dart:60-88` appende una riga JSON per evento con stack trace, senza limite di dimensione; `loadRecent` (`:97-108`) parsa l'intero file e può lanciare su una riga corrotta. **Fix:** rotazione 2 MB × 3, parsing per riga in try/catch, export manuale ben visibile, Sentry/GlitchTip opt-in.
- **M-36** Onboarding con default placeholder (`onboarding_screen.dart:56-64`: `alice@example.invalid`, `ws://127.0.0.1:8787`) e token in chiaro nei campi (`:131-136`); `first_run_screen.dart:46-48` ha un solo bootstrap relay hardcoded e chiama `http.get` senza timeout in `initState` (nessun percorso offline). **Fix:** relay reale di default o "local-only" esplicito, `obscureText` sui token, timeout 3-5 s, percorso offline.
- **M-37** `pubspec.lock` richiede `dart >=3.10.0` e `flutter >=3.38.0` mentre `pubspec.yaml:7` dichiara `sdk: ">=3.3.0 <4.0.0"`: il lock committato non è utilizzabile con SDK più vecchi (`pub get --enforce-lockfile` fallisce). `.dart_tool/package_config.json` contiene path Windows (`file:///C:/Users/m0rph/...`) in un checkout Linux. **Fix:** allineare i vincoli, rigenerare `.dart_tool`, decidere se committare la l10n generata.

---

## 5. Problemi LOW / igiene

- **L-01 Repo: 200 MB di binari versionati.** `git ls-files` include `Fedi3-release-x86_64.AppImage` (183 MB), `app/fedi3_core.dll` (28 MB, e risulta modificato), `docs/.linux error.kate-swp` (file di swap di Kate) e `docs/linux error` (con un token casuale `7t2qKx8mE5nQp3LrV9xwAh1B` in fondo). `.gitignore` non esclude `dist/`, `*.AppImage`, `*.dll`, `*.so`, `**/*.swp`. Il repository ha **un solo commit** e `.git` pesa 202 MB: nessuna storia, nessun bisect, nessun provenance. **Fix:** ignorare gli artefatti, `git rm --cached`, pubblicare i binari come release asset.
- **L-02 `LICENSE` non contiene il testo AGPL.** È un avviso di 20 righe: la AGPL richiede che una copia della licenza accompagni il programma, e GitHub/tool di compliance non la riconosceranno. **Fix:** testo ufficiale AGPL-3.0 + `NOTICE`.
- **L-03 Nessun `SECURITY.md`, `CHANGELOG`, `CODE_OF_CONDUCT`, `CODEOWNERS`, template di issue/PR.** `GOVERNANCE.md` promette advisory pubblici ma non esiste un canale di disclosure, per un progetto che gestisce chiavi.
- **L-04 `docs/app_guide.md` è un accumulo.** Contiene tre documenti incollati: la guida vera, un blocco "linux deploy app" con istruzioni **contraddittorie** (installa Flutter con `pacman` e poi clona di nuovo Flutter da GitHub) e una trascrizione di chat che finisce con "Se vuoi, incolla l'output di: … e ti dico il punto esatto che manca". `docs/faq.md` dice "Posso usare solo P2P senza relay? È possibile" ma H-09 contraddice la promessa. **Fix:** riscrivere `app_guide.md`, spostare il deploy Linux in `docs/build_linux.md`.
- **L-05 `analysis_options.yaml` è di 45 byte:** solo `include: package:flutter_lints/flutter.yaml`, nessuna regola extra, nessun `errors:` che promuova i warning, nessun `dart format` in CI. **Fix:** `strict-casts`, `strict-inference`, `strict-raw-types`, warning promossi a errore.
- **L-06 Due funzioni di free per lo stesso tipo di allocazione:** `fedi3_core_string_free` (`lib.rs:41`) e `fedi3_core_free_cstring` (`ffi.rs:57`). `set_err` (`ffi.rs:9-17`) scrive su `out_err` solo in caso di errore: un chiamante che riusa il buffer senza inizializzarlo a `null` legge un puntatore stantio. **Fix:** unificare, documentare l'ownership in un header C, inizializzare sempre.
- **L-07 `openUrlExternal` accetta qualsiasi schema** (`open_url.dart:10-17`); `link_preview_repository.dart:39` crea un `http.Client` mai chiuso e senza timeout; `resolveLocalMediaUrl` (`media_url.dart`) riscrive l'URL verso il core locale senza fallback se il core è spento; `notification_service.dart:33` usa un id fisso `1001`, quindi una notifica chat sovrascrive la precedente.
- **L-08 `DevCoreScreen` mostra il JSON di start del core, token inclusi** (`dev_core_screen.dart:57`), e perde il suo `HttpClient` sul percorso di errore (`:108-121`, `close()` solo in caso di successo). **Fix:** redigere i token, `try/finally`, riusare `CoreApi`.
- **L-09 Versione e metadati di prodotto:** `version: 0.1.0+1`, nessuna sezione `assets:`, icona di default Flutter, `web/index.html` con "A new Flutter project.", nessun deep link, nessun meccanismo di aggiornamento, `DevCoreScreen` visibile agli utenti normali.
- **L-10 Contatori non letti effimeri:** `app_state.dart:27-28` sono solo in memoria e azzerati in `markCoreDead`; `notifications_screen.dart:45` chiama `clearUnreadNotifications()` in `initState`, prima che la lista sia caricata. **Fix:** derivare i contatori dall'API e persistire i timestamp.
- **L-11 Follow ottimistico mai revertito e polling ogni 6 s:** `profile_screen.dart:151-163` non ripristina lo stato in `catch`; `:186-188` polla ogni 6 s indefinitamente se il follow resta pending. **Fix:** revert in `catch`, cap del polling.
- **L-12 Thread view con richieste sequenziali (fino a 200 nodi) prima del primo paint** (`note_detail_screen.dart:55-105`), senza cancellazione. **Fix:** renderizzare la nota focale subito e streammare i parent/risposte.
- **L-13 `escape_like` senza `ESCAPE`** (`social_db.rs:3569-3571` vs query a `:1325/1463/1515`): cercare `100%` o `foo_bar` non trova nulla. **Fix:** `ESCAPE '\'` o rimuovere l'escaping.
- **L-14 Gestione incompleta di `algorithm` nelle firme** (`http_sig.rs:145-180` lo ignora; `:252-262` verifica sempre RSA PKCS#1 v1.5; `:219-230` `split_once('=')` rifiuta digest multipli). Peer con `hs2019`/PSS o Ed25519 ricevono 401. **Fix:** supportare rsa-sha256, hs2019 e ed25519, digest come lista.
- **L-15 `sanitize_key` non rimuove `..`** (`media_store.rs:268-271`): oggi è salvo perché le chiavi sono generate dal server, ma è una dipendenza implicita. **Fix:** rifiutare `..` esplicitamente.
- **L-16 Nel compose il servizio `relay` pubblica `8787` sull'host** oltre a Caddy (due percorsi di ingresso, uno senza TLS); `postgres`/`meilisearch` sono raggiungibili da ogni container della rete compose con password di default.
- **L-17 Gli script di build non usano `--locked`** (`scripts/build_core.sh`, `deploy/*/Dockerfile`) e non c'è un equivalente macOS di `build_core.ps1`.
- **L-18 `scripts/build_appimage.sh:180-186`** scarica `appimagetool` dal tag `continuous` (mutabile) senza verifica di checksum; il bundling di `libmpv` copia la versione dell'host (rischio di mismatch).
- **L-19 `docs/ops_backup_restore.md`** dice di applicare `postgres_schema.sql` a mano, ma il relay lo fa già con `include_str!` (`main.rs:3568`): documentazione fuorviante e rischio di drift fra le due fonti di verità.
- **L-20 Nessun `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`:** formattazione e lint non deterministici, nessun `cargo audit`/`deny` per le CVE.

---

## 6. Script e operatività rotti

- **`scripts/relay_diagnose.sh:50-52`** esegue `docker compose exec -T relay sh -lc 'wget -qO- ...'`, ma l'immagine (`deploy/relay/Dockerfile:16-19`) installa solo `ca-certificates` e `gosu`: **`wget` non esiste**. Il comando stampa `wget: not found` e l'errore è inghiottito da `|| true`: la diagnostica finge di aver controllato la salute.
- **`scripts/backup_relay_pg.sh:13`** e **`restore_relay_pg.sh:17`** hardcodano `-U fedi3 -d fedi3_relay`, ignorando `FEDI3_RELAY_PG_USER`/`FEDI3_RELAY_PG_DB`.
- **`scripts/backup_relay_media.sh:12`** archivia con percorso assoluto (`tar -czf - /data/media`); **`restore_relay_media.sh:17`** estrae con `-C /data/media` → si ottiene `/data/media/data/media/...` (annidamento errato). **Fix:** `tar -czf - -C /data media` e `tar -xzf - -C /data`.
- **Nessuna retention, verifica o schedulazione dei backup:** solo script manuali, nessun cron/systemd timer, nessun test di restore.
- **`relay_smoke_test.sh`** è corretto ma non è mai eseguito automaticamente.

---

## 7. Roadmap consigliata

### Settimana 1 — sbloccare il rilascio e chiudere le falle di autenticazione
1. Manifest Android (`INTERNET`, `POST_NOTIFICATIONS`, network security config), entitlements macOS, ATS iOS (P0-01…04).
2. App id definitivo + keystore di release (P0-05).
3. Firma obbligatoria su `/inbox` del relay (P0-06) e binding `actor`↔firma nel core (P0-07), con test dedicati.
4. Autenticazione sull'`outbox` (P0-08).
5. Loop di riconnessione del tunnel con backoff e watchdog (P0-09).
6. `catch_unwind` su tutti gli entry point FFI (P0-10) e handshake di readiness in `start()` (P0-11).
7. `replaceConfig` che riavvia sempre il core (P0-12).
8. CI minima: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo build --locked`, `cargo test --locked`, `flutter analyze`, `flutter test`, build AppImage.

### Settimane 2-3 — reggere il carico e non perdere dati
9. Relay: `spawn_blocking`/pool per il DB, eliminazione del lock globale, healthcheck non autenticato e senza side effect (H-21, H-23).
10. `forward_to_user`: rilasciare il lock prima dell'await, timeout sulla send, spool su ogni fallimento (H-10, H-11).
11. IP del client corretto in tutti gli handler + Caddy `trusted_proxies` + non pubblicare 8787 (H-12, H-13).
12. Constant-time token, validazione all'avvio, `Digest` obbligatorio, Ed25519 nelle firme (H-14, H-15, H-01).
13. SQLite: `busy_timeout`, schema versionato, GC dei `delivery_jobs`, dedup inbox (H-25, H-24, H-40, H-37).
14. `CoreApi` con timeout, errori tipizzati e repository iniettabile; primi test widget reali (H-50, H-59).
15. Config atomica e permessi 600; watchdog del core con restart; backup completo (M-25, H-51, H-56).

### Settimane 4-6 — osservabilità, hardening, manutenzione
16. Log JSON, metriche reali, endpoint metriche separato, stack Prometheus/Grafana, alert (M-13).
17. Immagini pinnate e build `--locked`, coturn con credenziali temporanee, limiti di risorsa e log rotation (M-17, M-10, M-11).
18. Pulizia repo, `SECURITY.md`, `CHANGELOG`, licenza AGPL completa, docs riscritte (L-01…L-05).
19. Aggiornamento dipendenze (ML-KEM, libp2p, webrtc, rusqlite) con `cargo audit`/`deny` in CI (M-21, M-24, L-20).
20. Quote media, allowlist MIME, streaming, accessibilità, l10n completa (H-28, M-16, H-22, M-32, M-28).

---

## 8. Cosa è già fatto bene

Il quadro non è negativo, e va detto con precisione:

- **Nessuna SQL injection.** Ogni valore passa da `params!`/`$n`; `format!` è usato solo per liste di placeholder `IN (?,?,…)` e filtri Meili, con escaping corretto di `\` e `"`.
- **Token solidi dove esistono:** 24 byte da `OsRng`, salvati come SHA-256, mai loggati; username con allowlist rigida (`^[a-z0-9_-]{1,64}$`) che blocca injection in path e URL.
- **HTTP Signatures reali, non decorative:** RSA-SHA256, Digest, skew temporale, signed headers espliciti, Ed25519 con TOFU pinning per la telemetria.
- **Difese anti-abuso stratificate nel core:** rate limit per IP/keyId/attore, quote giornaliere per keyId/attore/host, strike con blocchi temporanei, audit event. Molto più di molte istanze AP.
- **GC dello storage** con TTL e quote per feed, oggetti e media, differenziate per attori Fedi3 seguiti, con batch.
- **Coda di consegna persistente** con backoff esponenziale e jitter, `max_attempts`, dead-letter per object fetch.
- **Crittografia E2E** con AES-256-GCM, nonce casuale per messaggio, chiavi derivate HKDF, bundle firmati; corretto uso della primitiva (manca solo la forward secrecy).
- **Sicurezza HTTP del relay:** security headers completi, HSTS configurabile, CSP opzionale, request/correlation id, IP allow/deny con CIDR, audit trail con user agent.
- **Prevenzione del path traversal sui media** efficace: id generati dal server, solo l'estensione presa dal client, rifiuto di `..`/`/`/`\`.
- **Configurazione difensiva:** clamp su quasi tutte le variabili numeriche, validazione dei limiti.
- **i18n completo e in parità:** 489 chiavi identiche in en/it, 719 usi di `context.l10n`, solo ~45 stringhe hardcoded.
- **Privacy coerente con l'etica dichiarata:** telemetria opt-in, solo locale, nessun SDK di tracking, nessuna dipendenza da `shared_preferences`.
- **UI curata:** temi chiaro/scuro, densità, scala testo, breakpoint desktop, stati di loading/empty/error con retry, `dispose` corretto di timer e subscription nella maggior parte delle schermate.
- **Documentazione operativa di base** presente (backup, troubleshooting, deploy) e script di build per Linux/Windows.

---

## 9. Limiti di questa verifica

- **Nessuna compilazione.** Rust e Flutter non sono installati: errori di tipo, lint, test e feature-unification non sono stati verificati.
- **Nessun test runtime** di federazione reale (Mastodon/Misskey), WebRTC/TURN, P2P, né misure di performance: le conclusioni su `spawn_blocking`, lock globale e N+1 sono ragionamenti sul codice, non benchmark.
- **iOS ATS** e local network privacy: serve un device.
- **`ap.rs` (6.502 righe) e il `main.rs` del relay (8.836 righe)** sono stati letti integralmente nei percorsi critici (firma, inbox, outbox, media, health, config, tunnel) e scansionati per pattern altrove.
- Da verificare in fase di build: presenza di `wget`/`redis-cli` nelle immagini usate dagli healthcheck del compose; limiti di default di axum per i messaggi WebSocket; comportamento di `flutter_widget_from_html_core` quando `onTapUrl` ritorna `false`.

---

## 10. Riepilogo

| Severità | Conteggio |
|---|---|
| P0 (bloccanti) | 12 |
| HIGH | 66 |
| MEDIUM | 37 |
| LOW | 20 |

**I tre interventi con il miglior rapporto costo/beneficio:**

1. **Fix dei manifest e degli entitlements** (poche righe, sbloccano il rilascio su Android, macOS e iOS).
2. **Loop di riconnessione del tunnel** (poche righe, eliminano il difetto più visibile agli utenti federati e il silenzio totale dopo un blip di rete).
3. **Firma obbligatoria su `/inbox` + binding `actor`↔firma nel core** (due controlli, chiudono le due falle di autenticazione più gravi).

Subito dopo: CI con `cargo test` e `flutter test` (una giornata di lavoro, abilita tutto il resto), e la correzione del lock globale del relay (la differenza fra "regge" e "cade sotto carico").
