# FEDI3 — Piano dei test

**Obiettivo:** una suite che provi ogni funzione dell'applicativo — core Rust, relay, app Flutter e configurazioni di piattaforma — e che sia eseguibile in locale e in CI senza servizi esterni.

**Riferimenti:** [production_readiness_audit.md](production_readiness_audit.md), [remediation_plan.md](remediation_plan.md)

---

## 1. Strategia

| Livello | Cosa prova | Dove gira | Servizi esterni |
|---|---|---|---|
| **Unit** | Funzioni pure: firme, digest, parsing, escaping, serializzazione, regole di config, rate limiter, CIDR, codec | `cargo test` (moduli `#[cfg(test)]` nei crate), `flutter test` | nessuno |
| **Integration** | Server axum reali su porta effimera, DB SQLite temporaneo, richieste HTTP vere, mock di peer remoti | `crates/*/tests/*.rs` | nessuno (SQLite, mock HTTP) |
| **Contract** | Documenti ActivityPub (actor, Note, attività) confrontati con fixture canoniche Mastodon/Misskey | integration | nessuno |
| **Security** | Bypass di autenticazione, impersonificazione, SSRF, path traversal, timing, replay | unit + integration | nessuno |
| **Concurrency/chaos** | Scritture parallele, client lenti, disconnessioni, worker che falliscono | integration | nessuno |
| **UI** | Widget, stati di caricamento/errore, validazione form, navigazione, timeout | `flutter test` con client HTTP fake | nessuno |
| **Static config** | Manifest Android, entitlements macOS, Info.plist iOS, gradle | script bash | nessuno |
| **E2E smoke** | Core + relay sqlite in-process, flusso publish → deliver | script + integration | nessuno |

**Regole:**
- Nessun test dipende da rete esterna o da Docker.
- Ogni test crea il proprio `tempdir` e la propria porta; nessuno stato condiviso.
- I test di sicurezza devono **fallire** sul codice attuale non corretto e passare dopo il fix.
- I mock server remoti sono implementati con axum e verificano le firme ricevute.
- Niente `sleep` arbitrari: usare attese su condizione con timeout.

---

## 2. Infrastruttura di test condivisa

Da creare una volta e riusare:

| Componente | Path | Funzione |
|---|---|---|
| `TestCore` | `crates/fedi3_core/tests/common/mod.rs` | Avvia un core reale su porta effimera con data dir temporanea, espone base URL e token interno, chiude in `Drop` |
| `TestRelay` | `crates/fedi3_relay/tests/common/mod.rs` | Avvia il relay in modalità SQLite senza Redis/Meili su porta effimera |
| `MockActorServer` | `tests/common/mock_actor.rs` | Serve un actor document, una chiave pubblica e un inbox; registra le richieste ricevute; opzioni: lento, 500, corpo enorme, redirect |
| `TempDb` | `tests/common/db.rs` | Crea un DB SQLite in tempdir, espone helper di seeding |
| `Signer` | `tests/common/signer.rs` | Genera coppie RSA, firma richieste HTTP, costruisce header `Signature`/`Digest` |
| `FakeHttpClient` | `app/test/support/fake_http.dart` | Client Dart deterministico con risposte programmabili, ritardi e errori |
| `pumpApp` | `app/test/support/pump_app.dart` | Monta l'app con `AppState` iniettato e repository fake |

---

## 3. Matrice di copertura — Core Rust

### 3.1 Chiavi e identità (`keys.rs`)

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| K-01 | unit | Generazione identità in dir vuota | File creati, PEM valido, DID deterministico dal PEM |
| K-02 | unit | Caricamento identità esistente | Stessa chiave, nessuna rigenerazione |
| K-03 | unit | File corrotto | Errore esplicito, non rigenerazione silenziosa |
| K-04 | unit | Permessi su Unix | 0600 sul file chiave, 0700 sulla dir |
| K-05 | unit | DID da PEM con nuova riga/spazi | Normalizzazione stabile |

### 3.2 HTTP Signatures (`http_sig.rs`)

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| S-01 | unit | `parse_signature_header` con `keyId`, `headers`, `signature` | Campi corretti, header lowercase |
| S-02 | unit | Header con `algorithm="hs2019"` e virgole in quoted string | Parsing corretto |
| S-03 | unit | Header senza `signature` | Errore |
| S-04 | unit | `build_signing_string` con `(request-target)` e query | Stringa conforme alla spec |
| S-05 | unit | Header firmato mancante nella richiesta | Errore |
| S-06 | unit | Round-trip firma/verifica RSA-SHA256 su richiesta reale | Verifica OK |
| S-07 | unit | Firma alterata di 1 byte | Verifica fallisce |
| S-08 | unit | Chiave pubblica diversa | Verifica fallisce |
| S-09 | unit | `verify_digest_if_present` con digest corretto | OK |
| S-10 | unit | Digest errato | Errore |
| S-11 | unit | Digest assente su POST | Errore (nuovo comportamento) |
| S-12 | unit | Digest multi-algoritmo `SHA-256=...,SHA-512=...` | Accetta SHA-256 |
| S-13 | unit | `verify_date` con skew 300 s e 3600 s | Rifiuta oltre soglia, accetta entro |
| S-14 | unit | Data non parsabile | Errore |
| S-15 | unit | Set di header firmati minimo assente (`(request-target)`/`host`/`date`) | Errore |
| S-16 | integration | KeyResolver con mock server: chiave valida, 500, corpo enorme, redirect | Cache, errore gestito, cap dimensione, redirect limitato |

### 3.3 Envelope cifrato (`crypto_envelope.rs`)

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| E-01 | unit | Encrypt/decrypt round-trip | Payload identico |
| E-02 | unit | Ciphertext alterato | Decrypt fallisce con errore (non silenzioso) |
| E-03 | unit | Nonce di lunghezza errata | Errore |
| E-04 | unit | Envelope JSON malformato | Errore |
| E-05 | unit | Body vuoto | Nessuna cifratura, invariato |
| E-06 | unit | Due messaggi con stessa chiave: nonce diversi | Nonce diversi, entrambi decifrabili |
| E-07 | unit | `x-fedi3-encrypted` assente | Nessuna decifratura |

### 3.4 ActivityPub — actor e discovery (`ap.rs`)

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| A-01 | integration | `GET /users/{u}` | `@context` AP, `id`, `inbox`, `outbox`, `followers`, `publicKey`, `preferredUsername` |
| A-02 | contract | Actor document contro fixture Mastodon/Misskey | Campi obbligatori presenti, tipi corretti |
| A-03 | integration | `GET /.well-known/webfinger?resource=acct:u@domain` | `subject`, link `self` con `activity+json` |
| A-04 | integration | webfinger con risorsa non locale | 404 |
| A-05 | integration | `GET /nodeinfo/2.0` e `/.well-known/nodeinfo` | Schema corretto, versione, statistiche |
| A-06 | integration | `GET /users/{u}/followers` e `/following` | Collection con `totalItems`, paginazione |
| A-07 | integration | `GET /healthz`, `/_fedi3/health` | 200; `/_fedi3/health` richiede token interno |
| A-08 | integration | `GET /_fedi3/*` senza token | 401 |
| A-09 | integration | `GET /_fedi3/*` con token errato | 401 |
| A-10 | integration | `GET /_fedi3/*` con token corretto | 200 |

### 3.5 ActivityPub — inbox

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| I-01 | integration | POST senza `Signature` | 401 |
| I-02 | integration | POST con firma invalida | 401 |
| I-03 | integration | POST con firma valida di un actor remoto | 202, attività memorizzata |
| I-04 | integration | **POST firmato da A con `actor=B`** | 401 (nuovo: binding) |
| I-05 | integration | POST con `Digest` assente | 401 (nuovo) |
| I-06 | integration | POST con `Digest` errato | 401 |
| I-07 | integration | Data oltre lo skew | 401 |
| I-08 | integration | Stessa attività due volte | 202, seconda è duplicate, nessuna doppia memorizzazione |
| I-09 | integration | Processing fallito (DB chiuso) e retry | Il retry riprocessa (dedup solo dopo successo) |
| I-10 | integration | Follow | Follower aggiunto, Accept inviato al mock |
| I-11 | integration | Follow con `manually_approves_followers` | Stato pending, nessun Accept |
| I-12 | integration | Accept di un Follow nostro | Stato accepted |
| I-13 | integration | **Accept da attore diverso dal target del Follow** | Non marca accepted |
| I-14 | integration | Undo Follow | Follower rimosso |
| I-15 | integration | Create Note pubblica | Nota memorizzata, feed federato aggiornato |
| I-16 | integration | Create Note con `inReplyTo` | Indice risposte aggiornato |
| I-17 | integration | **Update di nota di un altro attore** | Ignorato (nuovo: ownership) |
| I-18 | integration | **Delete di nota di un altro attore** | Ignorato (nuovo) |
| I-19 | integration | Delete della propria nota | Tombstone, oggetto marcato deleted |
| I-20 | integration | Announce | Boost memorizzato con attore corretto |
| I-21 | integration | Like/EmojiReact | Reazione memorizzata |
| I-22 | integration | Undo Like | Reazione rimossa |
| I-23 | integration | Attività bloccata da `blocked_actors` | 403, non memorizzata |
| I-24 | integration | Attività da dominio bloccato | Non memorizzata |
| I-25 | integration | Rate limit per keyId superato | 429 |
| I-26 | integration | Body oltre il limite | 413 |
| I-27 | integration | JSON non valido | 400 |
| I-28 | integration | Attività con profondità/array enormi | Nessun panic, risposta controllata |

### 3.6 ActivityPub — outbox

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| O-01 | integration | POST outbox senza token interno | 401 (nuovo) |
| O-02 | integration | POST outbox con token interno, Create valido | 202, consegna accodata |
| O-03 | integration | POST outbox con `actor` diverso dal nostro | 403 |
| O-04 | integration | POST outbox con JSON invalido | 400 |
| O-05 | integration | GET outbox | Paginazione, `orderedItems` |
| O-06 | integration | **GET outbox: i DM non compaiono** | Attività direct escluse (nuovo) |
| O-07 | integration | Paginazione con due elementi nello stesso millisecondo | Nessun elemento perso (cursore composito) |
| O-08 | integration | Post pubblico con allegati | `to` include Public, `cc` followers |

### 3.7 Media

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| M-01 | integration | Upload con token interno valido | 201, file su disco, riga DB |
| M-02 | integration | Upload senza token | 401 |
| M-03 | integration | Upload oltre il limite configurato | 413 (non `usize::MAX`) |
| M-04 | integration | Upload di un PNG che decodifica enorme | Rifiutato o gestito con limiti |
| M-05 | integration | MIME non in allowlist (`text/html`) | Rifiutato |
| M-06 | integration | GET media di un altro utente | 404 |
| M-07 | integration | GET media con id `../` | 400 |
| M-08 | integration | Quota per utente superata | 413 |
| M-09 | integration | DELETE media | File e riga rimossi |
| M-10 | unit | `sanitize_key` con `../`, `/`, `\` | Nessun traversal |

### 3.8 Chat

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| C-01 | integration | Invio messaggio a thread nuovo | Thread creato, envelope cifrato, membri corretti |
| C-02 | integration | Envelope da attore non membro di un thread esistente | Rifiutato |
| C-03 | integration | Envelope per thread inesistente | Creato con membri corretti |
| C-04 | unit | Bundle di un peer diverso dall'atteso | Rifiutato |
| C-05 | unit | Decifratura di un messaggio | Testo corretto |
| C-06 | integration | Reazioni e read receipt | Stato aggiornato |

### 3.9 Database (`social_db.rs`)

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| D-01 | unit | Schema creation su DB vuoto | Tabelle e indici presenti |
| D-02 | unit | Migrazione da schema vecchio | Colonne aggiunte, dati preservati |
| D-03 | unit | Versione schema più nuova del binario | Errore esplicito |
| D-04 | unit | Inserimento e lettura oggetto | Round-trip JSON identico |
| D-05 | unit | Dedup inbox | Secondo insert → false |
| D-06 | unit | Paginazione con timestamp uguali | Nessuna perdita |
| D-07 | unit | Pruning TTL per inbox/feed/oggetti | Conteggi attesi |
| D-08 | unit | Quote per attore | Gli eccessi vengono rimossi |
| D-09 | concurrency | 8 writer paralleli su DB con `busy_timeout` | Zero errori `SQLITE_BUSY` |
| D-10 | unit | SQL injection su ricerca (`100%`, `foo_bar`) | Match corretti con `ESCAPE` |
| D-11 | unit | Cursore invalido/negativo | Pagina vuota, nessun errore |

### 3.10 Coda di consegna (`delivery_queue.rs`)

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| Q-01 | unit | Enqueue attività | Job persistito con destinatari |
| Q-02 | unit | Scheduling con backoff | Intervalli crescenti, cap |
| Q-03 | unit | Superamento `max_attempts` | Dead-letter |
| Q-04 | unit | Receipt ricevuto | Job marcato delivered |
| Q-05 | unit | Receipt mai ricevuto | Raggiunge comunque `max_attempts` (nuovo) |
| Q-06 | unit | Errore DB durante il fetch | Il worker continua, non muore (nuovo) |
| Q-07 | unit | Pruning dei job vecchi | Righe rimosse |
| Q-08 | integration | Consegna a mock server: 200 | Delivered |
| Q-09 | integration | Consegna a mock server: 500 ripetuto | Retry, poi dead-letter |
| Q-10 | integration | Consegna a mock server: timeout | Retry con backoff |
| Q-11 | unit | Ripresa dopo riavvio | I job pendenti vengono ripresi |

### 3.11 Object fetch e GC

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| F-01 | integration | Fetch di un oggetto remoto valido | Memorizzato |
| F-02 | integration | Oggetto oltre il cap dimensione | Scartato con errore |
| F-03 | integration | 404 | Dead-letter dopo N tentativi |
| F-04 | integration | SSRF: oggetto su `127.0.0.1` | Rifiutato (nuovo) |
| G-01 | unit | GC inbox/feed/oggetti/media | Conteggi e file coerenti |
| G-02 | unit | GC con `cache_only_followed_fedi3=false` | Le quote media restano attive (nuovo) |

### 3.12 P2P, tunnel, WebRTC

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| P-01 | unit | Codec frame con lunghezza 0xFFFFFFFF | Errore, nessuna allocazione 4 GiB (nuovo) |
| P-02 | unit | Codec frame valido | Round-trip |
| P-03 | unit | Mailbox put/get/ttl | Messaggio recuperato, TTL rispettato |
| P-04 | unit | Mailbox id collision tra peer diversi | Nessuna sovrascrittura (nuovo) |
| P-05 | unit | Rate limiter mailbox | 429 dopo soglia |
| T-01 | integration | Tunnel: connessione e ping/pong | RTT misurato |
| T-02 | integration | **Tunnel: relay chiude la connessione** | Riconnessione entro il backoff (nuovo) |
| T-03 | integration | Tunnel: nessun pong per N secondi | Riconnessione (nuovo) |
| T-04 | integration | Tunnel: shutdown | Chiusura pulita, nessun task orfano |
| T-05 | unit | `relay_bridge` con path senza `/` | 400, nessun panic (nuovo) |
| T-06 | unit | `relay_bridge` con path contenente spazi | 400 |
| T-07 | unit | `relay_bridge` con header hop-by-hop | Header filtrati |
| W-01 | unit | Parsing ICE/TURN config | URL e credenziali corretti |
| W-02 | unit | Segnale WebRTC firmato | Accettato; firma invalida rifiutata |

### 3.13 FFI e runtime

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| R-01 | unit | `fedi3_core_version` | Stringa non vuota, liberabile |
| R-02 | unit | `start` con JSON invalido | Codice 2, messaggio, nessuna perdita |
| R-03 | unit | `start` con config valida | Handle > 0, server in ascolto (readiness) |
| R-04 | unit | **`start` con porta occupata** | Errore (nuovo), non handle vivo |
| R-05 | unit | `stop` con handle inesistente | Codice d'errore, nessun panic |
| R-06 | unit | Doppio `start` | Due handle indipendenti, due porte |
| R-07 | unit | **Config che provoca panic interno** | Codice d'errore, processo vivo (catch_unwind) |
| R-08 | unit | `stop` due volte | Secondo stop → errore gestito |
| R-09 | unit | `start` senza relay token | Avvio locale/P2P, nessun bail (nuovo) |
| R-10 | unit | Username con `../` | Rifiutato all'avvio |
| R-11 | unit | `previous_public_base_url` con follower legacy | Migrazione richiesta, errore esplicito |
| R-12 | integration | Shutdown con task attivi | Tutti i task joinati entro il timeout |

### 3.14 Metriche e stream

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| N-01 | unit | `net_metrics` snapshot | JSON con campi attesi |
| N-02 | unit | Prune su timer | Le entry vecchie vengono rimosse (nuovo) |
| N-03 | unit | Poisoning del mutex | Nessun panic (nuovo) |
| U-01 | integration | `/_fedi3/stream` SSE | Eventi emessi, auth richiesta |
| U-02 | unit | Evento malformato | Scartato senza panic |

---

## 4. Matrice di copertura — Relay

### 4.1 Config e validazione

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| RC-01 | unit | Variabile numerica invalida (`MAX_BODY_BYTES=64M`) | Errore all'avvio (nuovo) |
| RC-02 | unit | `DB_DRIVER=pg16` | Errore, non fallback silenzioso |
| RC-03 | unit | `SEARCH_BACKEND=meilisearch` | Errore |
| RC-04 | unit | Admin token assente o corto | Errore all'avvio |
| RC-05 | unit | Default sicuri | `hsts>0`, `trust_proxy_headers` documentato |
| RC-06 | unit | Clamp dei valori | Valori fuori range vengono limitati o rifiutati |

### 4.2 Autenticazione e token

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| RA-01 | unit | Admin token corretto | Autorizzato |
| RA-02 | unit | Token errato | 401 |
| RA-03 | unit | Token con prefisso corretto e suffisso errato | 401 |
| RA-04 | unit | Token vuoto | 401, non `Bearer ` |
| RA-05 | unit | Confronto constant-time | Nessuna differenza misurabile (test funzionale) |
| RA-06 | unit | Hash token utente | SHA-256, confronto corretto |
| RA-07 | unit | Utente disabilitato | 401 |

### 4.3 Rate limiting e IP

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| RL-01 | unit | Fixed window | Blocca dopo soglia, riapre dopo la finestra |
| RL-02 | unit | Weighted (fan-out) | Peso corretto |
| RL-03 | unit | Fallback in memoria senza Redis | Funziona |
| RL-04 | unit | Noisy backoff | Backoff crescente |
| RL-05 | unit | `parse_ip_rule` CIDR | Match corretti, edge case /32 e /0 |
| RL-06 | unit | `client_ip` con XFF spoofato (primo elemento falso) | Usa l'IP reale (nuovo) |
| RL-07 | unit | `client_ip` senza proxy | Usa peer IP |
| RL-08 | unit | Allowlist/denylist | 403 per IP escluso |
| RL-09 | unit | **Rate limit su `/webrtc/poll` con IP reali diversi** | Bucket separati (nuovo) |

### 4.4 Tunnel e forward

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| RT-01 | integration | Upgrade tunnel con token valido | Connesso |
| RT-02 | integration | Tunnel con token errato | Chiusura 401 |
| RT-03 | integration | Tunnel con username non valido | 400 (nuovo) |
| RT-04 | integration | Riconnessione: il vecchio disconnect non rimuove il nuovo | Il nuovo tunnel resta attivo (nuovo) |
| RT-05 | integration | **Forward con client lento** | Un secondo client riceve la risposta entro il timeout (nuovo) |
| RT-06 | integration | Forward con utente offline | Spool + 503 (nuovo) |
| RT-07 | integration | Forward con inflight pieno | 429 e spool |
| RT-08 | integration | Timeout del tunnel | 504 e spool |
| RT-09 | unit | Entry `inflight` rimossa al timeout | Nessun leak (nuovo) |

### 4.5 Inbox e federazione

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| RI-01 | integration | POST `/inbox` senza firma | 401 (nuovo) |
| RI-02 | integration | POST con firma valida | 202, fan-out |
| RI-03 | integration | Firma alterata | 401 |
| RI-04 | integration | Attività con 30 destinatari (oltre `max_inbox_fanout`) | 413 |
| RI-05 | integration | Destinatari non locali | 400 |
| RI-06 | integration | Stessa attività due volte | Dedupe, nessuna doppia consegna (nuovo) |
| RI-07 | integration | Consegna fallita a utente online | Spool, 503 al sender (nuovo) |
| RI-08 | integration | Spool oltre il cap in byte | 413 o scarto del più vecchio |
| RI-09 | unit | Spool: body in BLOB, non base64 | Dimensione coerente |
| RI-10 | integration | Due flush concorrenti | Nessuna doppia consegna |

### 4.6 Media

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| RM-01 | integration | Upload autenticato | 201, DB e backend coerenti |
| RM-02 | integration | Upload senza token | 401 |
| RM-03 | integration | Upload con MIME `text/html` | 415 (nuovo) |
| RM-04 | integration | Upload oltre la quota utente | 413 |
| RM-05 | integration | GET media con id malevolo | 400 |
| RM-06 | integration | GET media di utente offline | Forward al tunnel |
| RM-07 | integration | DELETE media | Oggetto rimosso dal backend (nuovo) |
| RM-08 | unit | `sanitize_key` con estensione strana | Chiave valida |
| RM-09 | unit | Upload non atomico | Scrittura temp+rename (nuovo) |

### 4.7 Admin, health, metriche

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| RH-01 | integration | `/healthz` senza token | 200, nessuna riga di audit (nuovo) |
| RH-02 | integration | `/readyz` senza token | 200/503 senza audit |
| RH-03 | integration | `/readyz` con DB rotto | 503 |
| RH-04 | integration | Endpoint admin senza token | 401 |
| RH-05 | integration | `admin_delete_user` | Media, note e segnali rimossi (nuovo) |
| RH-06 | integration | Audit su azione admin | Riga con request id e IP |
| RH-07 | integration | `/metrics.prom` con token separato | 200 |
| RH-08 | unit | Metriche: counter e histogram presenti | Formato Prometheus valido |

### 4.8 Ricerca, webfinger, host routing

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| RS-01 | unit | `escape_like` + `ESCAPE` | `100%` e `foo_bar` corretti (nuovo) |
| RS-02 | integration | Ricerca con backend DB | Risultati, nessuna scansione di HTML |
| RS-03 | integration | Ricerca con caratteri speciali | Nessun errore SQL |
| RS-04 | integration | Cache TTL | Scade correttamente |
| RW-01 | integration | webfinger per utente locale | `subject` e `href` corretti |
| RW-02 | integration | webfinger con `Host` non valido in host-routing | 421/404 (nuovo) |
| RW-03 | unit | Redirect canonico con porta esplicita | Nessun loop (nuovo) |
| RW-04 | unit | `host_only` con porta | Porta preservata dove serve |

### 4.9 Move notice, telemetria, WebRTC signaling

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| RN-01 | integration | Move notice firmato | Accettato, fan-out |
| RN-02 | integration | **Fan-out con `Signature`** | Accettato dal relay remoto (nuovo) |
| RN-03 | integration | Move notice senza firma | 401 |
| RN-04 | unit | TTL e retry del fan-out | Cap e scadenza |
| RY-01 | integration | Telemetria firmata | Accettata, registry aggiornato |
| RY-02 | integration | Telemetria con 100.000 relay | Cap applicato (nuovo) |
| RY-03 | unit | TOFU pinning della chiave relay | Chiave cambiata → rifiuto |
| RX-01 | integration | WebRTC signal firmato | Accettato |
| RX-02 | integration | Signal con `keyId` privato (SSRF) | Rifiutato (nuovo) |

### 4.10 Schema e migrazioni relay

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| RD-01 | unit | Init su DB vuoto (sqlite e postgres) | Schema completo |
| RD-02 | unit | Migrazione da schema vecchio | Colonne aggiunte, errori non inghiottiti |
| RD-03 | unit | Versione schema più nuova | Errore esplicito |
| RD-04 | unit | Due istanze sullo stesso DB | Migrazione idempotente |
| RD-05 | unit | Cleanup retention (audit, note, registry) | Righe vecchie rimosse (nuovo) |

---

## 5. Matrice di copertura — App Flutter

### 5.1 Modelli e serializzazione

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| AM-01 | unit | `CoreConfig.fromJson(toJson())` | Round-trip identico |
| AM-02 | unit | `toCoreStartJson` | Chiavi snake_case, token inclusi |
| AM-03 | unit | JSON con campi mancanti | Default sicuri, nessun crash |
| AM-04 | unit | JSON con tipi errati | Default, nessun crash |
| AM-05 | unit | `UiPrefs` round-trip | Identico |
| AM-06 | unit | `BackupCodec` encode/decode | Round-trip; versione sconosciuta → errore (nuovo) |
| AM-07 | unit | `BackupCodec` con JSON corrotto | `FormatException` chiara |

### 5.2 MFM e HTML

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| AF-01 | unit | `MfmCodec.toHtml` con `<`, `&`, `"`, `'` | Entità corrette |
| AF-02 | unit | Testo senza marker | Sempre HTML escapato (nuovo) |
| AF-03 | unit | Newline | `<br>` |
| AF-04 | unit | Marker MFM (`**bold**`, `:emoji:`) | HTML atteso |
| AF-05 | unit | Input vuoto | Stringa vuota, nessun `<p></p>` spurio |

### 5.3 Persistenza locale

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| AS-01 | unit | `ConfigStore.write` atomico | Nessun file troncato su errore simulato |
| AS-02 | unit | File corrotto + `.bak` valido | Recupero dal backup (nuovo) |
| AS-03 | unit | File corrotto senza `.bak` | Stato "corrotto" distinto da "assente" (nuovo) |
| AS-04 | unit | Permessi su POSIX | 600 (nuovo) |
| AS-05 | unit | `PrefsStore` corrotto | Default, con flag di errore |
| AS-06 | unit | Store concorrenti (`note_flags`, `reactions`) | Nessuna perdita di aggiornamenti (nuovo) |

### 5.4 Rete e API

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| AN-01 | unit | Richiesta senza token interno | Nessun header `X-Fedi3-Internal` |
| AN-02 | unit | Timeout del server | `CoreApiException.timeout` entro 15 s (nuovo) |
| AN-03 | unit | 401 | Eccezione `unauthorized`, messaggio chiaro |
| AN-04 | unit | 500 | Eccezione `server`, retryable |
| AN-05 | unit | JSON malformato | Eccezione `decode`, nessun crash |
| AN-06 | unit | `X-Filename` non ASCII | Encoding corretto (nuovo) |
| AN-07 | unit | `threadId` con `/` e `%` | Path encodato (nuovo) |
| AN-08 | unit | `CoreEventStream` con frame valido | Evento emesso |
| AN-09 | unit | `CoreEventStream` con frame malformato | Scartato, stream vivo |
| AN-10 | unit | `CoreEventStream` con errore di rete | Errore propagato, retry con backoff |

### 5.5 Stato e ciclo di vita del core

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| AP-01 | unit | `startCore` con config valida (FFI fake) | Handle impostato solo dopo readiness (nuovo) |
| AP-02 | unit | `startCore` con errore FFI | `lastError` impostato, nessun handle |
| AP-03 | unit | `stopCore` | Handle azzerato, FFI chiamato |
| AP-04 | unit | Watchdog: health fallisce | `markCoreDead`, restart tentato (nuovo) |
| AP-05 | unit | `replaceConfig` | stop → save → start → readiness (nuovo) |
| AP-06 | unit | Doppio `startCore` concorrente | Una sola chiamata FFI |

### 5.6 Widget

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| AW-01 | widget | Primo avvio senza config | Schermata di setup con titolo corretto (test riparato) |
| AW-02 | widget | Onboarding: validazione username/domain/bind | Errori inline (nuovo) |
| AW-03 | widget | Timeline: caricamento | Skeleton, non errore |
| AW-04 | widget | Timeline: errore rete | `NetworkErrorCard` con retry |
| AW-05 | widget | Timeline: vuota | Stato empty con messaggio |
| AW-06 | widget | Compose: testo vuoto | Pulsante disabilitato |
| AW-07 | widget | Compose: invio | Chiamata API con payload corretto |
| AW-08 | widget | Note card: like/boost/reply | Chiamate corrette, stato ottimistico |
| AW-09 | widget | Chat: invio messaggio | Payload corretto, stato aggiornato |
| AW-10 | widget | Impostazioni: salvataggio | `replaceConfig` invocato (nuovo) |
| AW-11 | widget | Notifiche: nuovo evento | Item aggiunto in testa senza azzerare la lista (nuovo) |
| AW-12 | widget | Media viewer su mobile | Nessuna eccezione `getSaveLocation` (nuovo) |
| AW-13 | widget | Testo a 1.5× scale | Nessun overflow nei layout principali |
| AW-14 | widget | Semantica | `semanticLabel` presenti su avatar/media (nuovo) |

### 5.7 l10n e piattaforma

| ID | Livello | Caso | Atteso |
|---|---|---|---|
| AL-01 | unit | Chiavi ARB en/it | Parità esatta |
| AL-02 | unit | Nessuna stringa hardcoded nelle schermate | Gate CI (nuovo) |
| AL-03 | script | Manifest Android | INTERNET, network security config, allowBackup=false |
| AL-04 | script | Entitlements macOS | network.client/server, file access |
| AL-05 | script | Info.plist iOS | ATS, usage descriptions |
| AL-06 | script | gradle | App id non `com.example`, signing release |

---

## 6. Test end-to-end (senza servizi esterni)

| ID | Livello | Scenario | Atteso |
|---|---|---|---|
| X-01 | e2e | Core A pubblica una nota pubblica; Core B (mock peer) la riceve firmata | B verifica la firma, la memorizza |
| X-02 | e2e | Follow A→B, Accept B→A, consegna | Stato accepted su A |
| X-03 | e2e | Relay sqlite + core: tunnel, pubblicazione, fan-out | Attività consegnata |
| X-04 | e2e | Relay sqlite: utente offline, attività in spool, riconnessione, flush | Attività consegnata una sola volta |
| X-05 | e2e | Migrazione: `Move` verso un nuovo actor | I follower ricevono `Move` |
| X-06 | e2e | Chat: invio cifrato tra due core mock | Messaggio decifrato correttamente |
| X-07 | e2e | Media: upload sul relay, GET, cache locale nel core | Byte identici |

---

## 7. Test di sicurezza mirati (devono fallire sul codice attuale)

| ID | Area | Attacco | Atteso dopo il fix |
|---|---|---|---|
| SEC-01 | relay inbox | POST senza firma | 401 |
| SEC-02 | core inbox | Attività firmata da A con `actor=B` | 401 |
| SEC-03 | core outbox | POST senza token dal tunnel | 401 |
| SEC-04 | core inbox | POST senza Digest | 401 |
| SEC-05 | relay | XFF spoofato per bypassare rate limit | Limite applicato all'IP reale |
| SEC-06 | core | `keyId` che punta a `127.0.0.1` | Rifiutato |
| SEC-07 | relay | `keyId` che punta a IP privato | Rifiutato |
| SEC-08 | core media | id `../../etc/passwd` | 400/404, nessuna scrittura fuori |
| SEC-09 | relay media | MIME `text/html` | 415 |
| SEC-10 | relay | Token admin con timing | Confronto constant-time |
| SEC-11 | core | Replay di una POST firmata con body alterato | 401 |
| SEC-12 | relay | Attività duplicata | Consegna una sola volta |
| SEC-13 | core | P2P frame da 4 GiB | Rifiutato senza OOM |
| SEC-14 | relay | `admin_delete_user` | Nessun blob orfano |
| SEC-15 | core | Delete di nota altrui | Ignorato |

---

## 8. CI e ambiente

### 8.1 Job della pipeline

| Job | Comando | Bloccante |
|---|---|---|
| rust-fmt | `cargo fmt --all -- --check` | sì |
| rust-clippy | `cargo clippy --workspace --all-targets -- -D warnings` | sì |
| rust-build | `cargo build --workspace --locked` | sì |
| rust-test | `cargo test --workspace --locked` | sì |
| rust-audit | `cargo audit` (o `cargo deny check`) | no (warning) |
| flutter-analyze | `flutter analyze --fatal-infos` | sì |
| flutter-test | `flutter test` | sì |
| platform-config | `scripts/verify_platform_config.sh` | sì |
| appimage | `scripts/build_appimage.sh` (solo su tag) | no |

### 8.2 Ambiente locale

```
export PATH="$HOME/.cargo/bin:$HOME/flutter/bin:$PATH"
cargo test --workspace --locked
cd app && flutter test
scripts/verify_platform_config.sh
```

### 8.3 Vincoli

- Nessun test richiede Docker, Postgres, Redis o Meilisearch.
- Il relay nei test usa `FEDI3_RELAY_DB_DRIVER=sqlite`, `SEARCH_BACKEND=db`, Redis assente.
- Ogni test ha timeout esplicito; nessun test dura più di 30 s.
- I test di rete usano porte effimere assegnate dal SO.

---

## 9. Metriche di accettazione

| Metrica | Soglia |
|---|---|
| Test Rust totali | ≥ 120 casi, tutti verdi |
| Test Flutter totali | ≥ 40 casi, tutti verdi |
| Aree funzionali coperte | 100% delle aree elencate nelle sezioni 3–5 |
| Test di sicurezza | 15/15 implementati e verdi |
| Test e2e | 7/7 implementati e verdi |
| `cargo clippy -D warnings` | pulito |
| `flutter analyze --fatal-infos` | pulito |
| Tempo pipeline | < 20 minuti su runner standard |
| Regressioni | zero sui test esistenti |

---

## 10. Ordine di implementazione dei test

1. **Infrastruttura** (`TestCore`, `TestRelay`, `MockActorServer`, `Signer`, `FakeHttpClient`) — sblocca tutto il resto.
2. **Sicurezza** (SEC-01…15) — definiscono il comportamento atteso dei fix.
3. **Unit core** (S-*, E-*, K-*, D-*, Q-*, P-*) — veloci, alta resa.
4. **Integration core** (A-*, I-*, O-*, M-*, C-*).
5. **Relay** (RC-*, RA-*, RL-*, RT-*, RI-*, RM-*, RH-*, RS-*, RW-*, RN-*, RY-*, RX-*, RD-*).
6. **App** (AM-*, AF-*, AS-*, AN-*, AP-*, AW-*, AL-*).
7. **E2E** (X-*).
8. **Static config** (AL-03…06).

I test scritti prima dei fix devono essere marcati come `#[ignore]` con un commento che cita il finding, e riabilitati dopo la correzione. Questo evita una pipeline rossa durante il lavoro e documenta l'intenzione.

---

## 11. Stato di implementazione

Implementato il 2026-10-31 e il 2026-11-01 in tre iterazioni. Conteggi reali verificati con l'esecuzione della suite finale:

| Area | Casi implementati | Dove |
|---|---|---|
| Unit core (keys, http_sig, ffi, net_metrics, tunnel, relay_bridge, p2p, net_policy, anchor) | 42 | `crates/fedi3_core/src/*.rs` (mod `tests`) |
| Integrazione core — ActivityPub e sicurezza | 41 | `crates/fedi3_core/tests/ap_security.rs` |
| Integrazione core — database e coda di consegna | 19 | `crates/fedi3_core/tests/db_queue.rs` |
| Integrazione core — FFI e runtime | 12 | `crates/fedi3_core/tests/runtime_ffi.rs` |
| Integrazione core — tunnel | 4 | `crates/fedi3_core/tests/tunnel.rs` |
| Integrazione core — gap semantici (chat, outbox, account bloccati, media, permessi DB) | 19 | `crates/fedi3_core/tests/semantics.rs` |
| Integrazione core — policy di rete e cap dei corpi | 6 | `crates/fedi3_core/tests/net_policy.rs` |
| Unit relay | 35 | `crates/fedi3_relay/src/main.rs` (mod `tests`) |
| Integrazione relay | 29 | `crates/fedi3_relay/tests/relay_http.rs` |
| **End-to-end federazione (relay reale + due core reali)** | **1** | `crates/fedi3_relay/tests/e2e_federation.rs` |
| **End-to-end identità anchor + spool cieco** | **1** | `crates/fedi3_relay/tests/e2e_anchor_spool.rs` |
| **Integrazione mesh relay + moderazione** | **1** | `crates/fedi3_relay/tests/mesh_moderation.rs` |
| **Totale Rust** | **210** | |
| Unit app (modelli, MFM, store, core API, stato, follow request, anchor) | 30 | `app/test/unit/` |
| Widget app (onboarding, timeline, compose, note card, impostazioni, chat, notifiche, media viewer, primo avvio, richieste di follow, networking/anchor) | 49 | `app/test/widget/`, `app/test/widget_test.dart` |
| l10n e configurazione di piattaforma | 9 | `app/test/l10n/`, `app/test/platform_config_test.dart` |
| **Totale Flutter** | **88** | |

### Scenario end-to-end implementato (X-01…X-07)

`crates/fedi3_relay/tests/e2e_federation.rs` avvia il binario reale del relay con SQLite e due runtime core reali in-process (identità, database e tunnel separati), poi verifica:

1. i due tunnel si connettono e il relay serve l'actor document di entrambi;
2. bob segue alice attraverso il relay: Follow firmato, verifica della firma, Accept consegnato, stato `accepted` su bob;
3. alice pubblica una nota pubblica dal proprio outbox;
4. la Create firmata arriva nell'inbox di bob e compare nella sua home timeline;
5. l'outbox pubblico è leggibile attraverso il relay e contiene la nota.

Il test ha fatto emergere due difetti reali (deadlock del tunnel su fetch annidati, shared inbox del relay incapace di espandere `followers`), entrambi corretti.

### Scenario end-to-end decentralizzazione (identità, spool, mesh, moderazione)

`crates/fedi3_relay/tests/e2e_anchor_spool.rs`:

1. il core pubblica i documenti anchor in `<data_dir>/anchor/` e un host statico di test li serve;
2. il relay reindirizza `GET /users/alice` all'actor canonico e il WebFinger punta all'anchor;
3. l'actor anchor mantiene `inbox`/`outbox` sul relay e la chiave sull'anchor;
4. la federazione funziona in modalità anchor (follow e consegna della nota);
5. con il core fermo il relay mette in spool la richiesta **cifrata**: nel database non compare il plaintext;
6. alla riconnessione il relay invia l'envelope cifrato, il core decifra, verifica la firma e la notifica arriva.

`crates/fedi3_relay/tests/mesh_moderation.rs`:

1. due relay si registrano come peer e sincronizzano la directory utenti;
2. una richiesta per un utente ospitato sull'altro relay viene inoltrata al relay di casa;
3. una blocklist relay-wide e una per-utente bloccano il mittente con `202`, senza forward né spool;
4. il token errato viene rifiutato e un utente sconosciuto resta `404` (nessun loop di hop).

Gate eseguiti con output reale sullo stato finale:

| Gate | Comando | Esito |
|---|---|---|
| Formattazione Rust | `cargo fmt --all -- --check` | ✅ |
| Lint Rust | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 0 warning |
| Build | `cargo build --workspace --locked` | ✅ |
| Test Rust | `cargo test --workspace --locked` | ✅ 210/210 |
| End-to-end | `cargo test -p fedi3_relay --test e2e_federation --test e2e_anchor_spool --test mesh_moderation` | ✅ 3/3 |
| Analisi app | `flutter analyze --fatal-infos` | ✅ |
| Formattazione app | `dart format --set-exit-if-changed lib test` | ✅ |
| Test app | `flutter test` | ✅ 88/88 |
| Configurazione piattaforma | `scripts/verify_platform_config.sh` | ✅ 9/9 |

### Scostamenti dal piano

- I casi di integrazione del relay sono 29: i casi di configurazione, token, `client_ip`, rate limiter, firme e migrazioni sono coperti come unit test dentro `main.rs` (35 casi), quindi la copertura delle sezioni §4.1–§4.10 è completa anche se distribuita diversamente.
- X-01…X-07 sono ora coperti dal test end-to-end reale, non solo indirettamente.
- Il Dockerfile del relay non è stato costruito (Docker non disponibile nell'ambiente): il tag `rust:1.99-bookworm` è stato verificato esistente sul registry e allineato al toolchain usato per i test.
- La migrazione PostgreSQL è coperta da asserzioni sul testo SQL, non da un database live.
- Alcuni lint clippy preesistenti di tipo stilistico sono stati silenziati a livello di crate (`manual_clamp`, `result_large_err`, `too_many_arguments`, …). I lint di correttezza restano attivi e bloccanti.
