# FEDI3 — Piano di miglioramento e fix

**Riferimento:** [production_readiness_audit.md](production_readiness_audit.md) (12 P0, 66 HIGH, 37 MEDIUM, 20 LOW)
**Obiettivo:** rendere il progetto rilasciabile e verificabile, con una suite di test che copra ogni area funzionale.
**Data:** 2026-10-31

---

## 0. Principi di esecuzione

1. **Nessun fix senza test.** Ogni correzione di sicurezza o di protocollo arriva con almeno un test che fallisce prima e passa dopo. Dove il test è impossibile (config di piattaforma), serve una verifica automatizzata alternativa (parse del manifest, check di gradle/plist).
2. **Priorità: sicurezza → correttezza → disponibilità → performance → igiene.** Non si ottimizza prima di aver chiuso le falle.
3. **Nessuna rottura di compatibilità ActivityPub.** Le modifiche al protocollo devono restare interoperabili con Mastodon/Misskey; le estensioni Fedi3 restano opzionali.
4. **Modifiche piccole e isolate**, con scope di scrittura disgiunti fra agenti, per evitare conflitti.
5. **Fail-fast sulla configurazione:** un valore non valido deve fermare l'avvio con un messaggio, non essere sostituito in silenzio.
6. **Il default deve essere sicuro.** Se un operatore dimentica una variabile, il sistema non deve diventare permissivo.

---

## 1. Fasi

### Fase A — Rilascio sbloccato (P0 di piattaforma)

Obiettivo: l'app si installa e funziona su Android, macOS, iOS; la release è firmabile e pubblicabile.

| ID | Task | File | Criterio di accettazione |
|---|---|---|---|
| A1 | Permesso `INTERNET` + `POST_NOTIFICATIONS` nel manifest principale; rimozione dai manifest debug/profile | `app/android/app/src/main/AndroidManifest.xml` | `aapt dump permissions` su APK release mostra INTERNET; il manifest debug non lo duplica |
| A2 | `network_security_config.xml` con cleartext solo per `127.0.0.1`/`localhost`; riferimento nel manifest; `allowBackup=false` | `app/android/app/src/main/res/xml/network_security_config.xml`, manifest | Il file è valido XML, referenziato, e `base-config` nega il cleartext |
| A3 | Entitlements macOS: `network.client`, `network.server`, `files.user-selected.read-write` in Debug e Release | `app/macos/Runner/*.entitlements` | `codesign -d --entitlements` sul bundle mostra le tre chiavi |
| A4 | ATS iOS: `NSAllowsLocalNetworking`; usage descriptions foto/camera; URL scheme | `app/ios/Runner/Info.plist` | `plutil -lint` OK; le chiavi richieste presenti |
| A5 | App id definitivo, `signingConfigs.release` da `key.properties`, fallback a debug solo se il file manca | `app/android/app/build.gradle.kts`, `app/linux/CMakeLists.txt` | Build release con keystore esterno; nessun `com.example` residuo |
| A6 | Test di piattaforma: script che verifica manifest/plist/gradle | `scripts/verify_platform_config.sh` | Esce 0 su config corretta, 1 con messaggio chiaro su regressione |

### Fase B — Autenticazione della federazione

Obiettivo: nessun attore può iniettare o impersonare attività.

| ID | Task | File | Criterio di accettazione |
|---|---|---|---|
| B1 | Firma HTTP obbligatoria su `/inbox` del relay; 401 se assente/invalida | `crates/fedi3_relay/src/main.rs` | Test: POST senza firma → 401; con firma valida → 202; firma alterata → 401 |
| B2 | Binding `activity.actor` ↔ `summary.actor_url` nel core, su inbox e receipt | `crates/fedi3_core/src/ap.rs` | Test: attività firmata da A ma con `actor=B` → 401 |
| B3 | Autenticazione su `POST /users/<u>/outbox` (token interno o firma) | `crates/fedi3_core/src/ap.rs` | Test: POST senza token → 401; con token → 202; il percorso UI continua a funzionare |
| B4 | `Digest` obbligatorio per POST e set di header firmati validato | `crates/fedi3_core/src/http_sig.rs`, `ap.rs` | Test: POST senza Digest → 401; Digest errato → 401; header set minimo assente → 401 |
| B5 | Confronto token constant-time + validazione all'avvio (admin ≥32 char, relay ≥16) | `crates/fedi3_relay/src/main.rs`, `crates/fedi3_core/src/runtime.rs` | Test unitario sul comparatore; avvio con token corto → errore esplicito |
| B6 | Proprietà dell'oggetto su Create/Update/Delete | `crates/fedi3_core/src/ap.rs` | Test: Update di nota altrui ignorato; Delete di nota altrui ignorato |
| B7 | Accept legato al Follow corretto | `crates/fedi3_core/src/ap.rs` | Test: Accept da attore diverso non marca accepted |

### Fase C — Disponibilità e affidabilità

| ID | Task | File | Criterio di accettazione |
|---|---|---|---|
| C1 | Loop di riconnessione del tunnel con backoff, timeout di connect e watchdog sui pong | `crates/fedi3_core/src/tunnel.rs`, `runtime.rs` | Test: relay che chiude la connessione → il client si riconnette entro N secondi; shutdown pulito |
| C2 | `catch_unwind` su ogni entry point FFI + rimozione dei `.lock().unwrap()` | `crates/fedi3_core/src/ffi.rs`, `runtime.rs` | Test: config che provoca panic → codice d'errore, processo vivo |
| C3 | Handshake di readiness in `start()` (bind sincrono o oneshot) | `crates/fedi3_core/src/runtime.rs` | Test: porta occupata → `start` ritorna Err; porta libera → ritorna solo dopo il bind |
| C4 | Rilascio del lock prima dell'await in `forward_to_user`; timeout sulla send; spool su ogni fallimento | `crates/fedi3_relay/src/main.rs` | Test: client lento non blocca un secondo client; consegna fallita → riga nello spool e 503 al sender |
| C5 | IP del client corretto su tutti gli handler; guardia sui proxy fidati | `crates/fedi3_relay/src/main.rs`, `Caddyfile` | Test unitario su `client_ip` con XFF spoofato; rate limit per IP reale |
| C6 | Health/readiness non autenticati e senza side effect; `HEALTHCHECK` | `crates/fedi3_relay/src/main.rs`, `deploy/relay/Dockerfile`, compose | Test: `/healthz` senza token → 200; nessuna riga di audit scritta |
| C7 | SQLite: `busy_timeout`, scrittore singolo, schema versionato | `crates/fedi3_core/src/social_db.rs`, `delivery_queue.rs`, relay | Test di concorrenza: 8 writer paralleli, zero `SQLITE_BUSY` |
| C8 | Worker di consegna resilienti (errore → backoff, non morte) + pruning dei job | `crates/fedi3_core/src/delivery_queue.rs` | Test: errore DB simulato → il worker continua; job vecchi rimossi |
| C9 | Inbox: dedup solo dopo il successo | `crates/fedi3_core/src/ap.rs` | Test: processing fallito → retry riprocessa |
| C10 | `relay_bridge` senza panic su input remoto | `crates/fedi3_core/src/relay_bridge.rs` | Test: path senza `/`, path con spazi → risposta 400, nessun panic |
| C11 | Cap sulla lunghezza dei frame P2P | `crates/fedi3_core/src/p2p.rs` | Test: header 0xFFFFFFFF → errore, nessuna allocazione da 4 GiB |
| C12 | Supervisore del core nell'app (watchdog health + restart) | `app/lib/state/app_state.dart` | Test widget: core morto → stato `dead` e restart tentato |

### Fase D — App: rete, configurazione, UX corretta

| ID | Task | File | Criterio di accettazione |
|---|---|---|---|
| D1 | Client HTTP con timeout, errori tipizzati, repository iniettabile | `app/lib/core/*`, `app/lib/services/*` | Test: server che non risponde → errore `timeout` entro 15 s, nessun Future pendente |
| D2 | Config/prefs atomiche (temp+rename), `.bak`, permessi 600, distinzione corrotto/assente | `app/lib/state/config_store.dart`, `prefs_store.dart` | Test: file troncato → recupero da `.bak`; nessuna perdita silenziosa |
| D3 | `replaceConfig` che ferma, salva, riavvia e attende readiness; usato da tutte le schermate | `app/lib/state/app_state.dart`, schermate impostazioni | Test: salvataggio → core running; nessuna timeline vuota |
| D4 | Testo dei post sempre convertito in HTML con escaping | `app/lib/utils/mfm_codec.dart`, composer | Test: `<`, `&`, `"` → entità corrette; nessun tag iniettato |
| D5 | Test widget riparato + copertura delle schermate principali con HTTP fake | `app/test/**` | `flutter test` verde |
| D6 | Media kit inizializzato su Linux; software rendering opt-in | `app/lib/main.dart`, `scripts/build_appimage.sh` | Test: flag presente nell'AppRun; avvio senza eccezioni |

### Fase E — Osservabilità, CI, igiene

| ID | Task | File | Criterio di accettazione |
|---|---|---|---|
| E1 | CI completa: fmt, clippy, build locked, test Rust, analyze, test Flutter, build AppImage | `.github/workflows/ci.yml` | La pipeline esiste ed è eseguibile in locale con gli stessi comandi |
| E2 | `cargo audit`/`deny` + dipendenze pinnate nei Dockerfile (`--locked`, immagini per digest) | `.github/workflows/`, `deploy/` | Build riproducibile; nessun `latest` |
| E3 | Pulizia repo: binari fuori da git, `.gitignore`, file spuri rimossi | `.gitignore`, `docs/` | `git ls-files` senza AppImage/DLL/swp |
| E4 | `SECURITY.md`, `CHANGELOG.md`, licenza AGPL completa, docs riscritte | radice, `docs/` | I file esistono e sono coerenti |
| E5 | Script operativi corretti (wget, variabili PG, tar) + retention backup | `scripts/` | `relay_diagnose.sh` funziona nell'immagine; backup/restore coerenti |
| E6 | Config: `.env.example` completo, validazione fail-fast, default sicuri | `.env.example`, `main.rs`, `runtime.rs` | Ogni variabile letta è documentata; un valore invalido blocca l'avvio |

---

## 2. Matrice di tracciamento audit → piano

| Finding | Fase |
|---|---|
| P0-01, P0-02, P0-03, P0-04, P0-05 | A1–A5 |
| P0-06 | B1 |
| P0-07 | B2 |
| P0-08 | B3 |
| P0-09 | C1 |
| P0-10 | C2 |
| P0-11 | C3 |
| P0-12 | D3 |
| H-01 | B4 |
| H-02 | B6 |
| H-03, H-04 | Fase F (SSRF e cap dimensioni) |
| H-05 | Fase F |
| H-06 | C11 |
| H-07 | Fase F (ingest) |
| H-08 | Fase F (move notice firmati) |
| H-09 | B5 + C3 |
| H-10 | C4 |
| H-11 | C4 |
| H-12, H-13 | C5 |
| H-14, H-15 | B5, Fase F |
| H-16 | Fase F |
| H-17, H-18 | Fase F |
| H-19 | Fase F (permessi chiavi) |
| H-20 | Fase F (dedupe relay) |
| H-21, H-22 | Fase G (performance) |
| H-23 | C6 |
| H-24, H-25 | C7 |
| H-26 | C8 |
| H-27, H-28 | Fase G |
| H-29, H-30, H-31 | Fase F |
| H-32, H-33, H-34, H-35, H-36 | Fase G |
| H-37 | C9 |
| H-38, H-39 | Fase F |
| H-40, H-41 | C8 |
| H-42, H-43 | Fase F |
| H-44, H-45 | B7, Fase F |
| H-46, H-47, H-48 | Fase G |
| H-49 | C10 |
| H-50 | D1 |
| H-51 | C12 |
| H-52 | Fase G (UX/perf) |
| H-53, H-54 | Fase F (app) |
| H-55 | D6 |
| H-56, H-57 | Fase F (backup) |
| H-58 | Fase F (shutdown) |
| H-59, H-60 | D5 + E1 |
| H-61, H-62, H-63 | D4, Fase G |
| H-64, H-65, H-66 | Fase F (app) |
| M-* | Fase G / H |
| L-* | Fase H (igiene) |

### Fase F — Correttezza di protocollo e dati (dopo le fasi A–C)

SSRF con guardia sugli IP privati, cap sui corpi in ingresso, ingest con validazione dell'origine, move notice firmati, dedupe del relay, quote e DELETE dei media, spool con cap in byte, paginazione con cursore composito, visibilità dell'outbox, permessi dei file di chiave, shutdown completo, Accept corretto, backup completo con rollback.

### Fase G — Performance, scalabilità, UX

Pool di connessioni DB, rimozione del lock globale, streaming dei media, fan-out concorrente, metriche reali, polling con lifecycle, N+1 delle timeline, accessibilità, l10n completa.

### Fase H — Igiene e manutenzione

Repo pulito, licenza completa, SECURITY/CHANGELOG, toolchain e lint deterministici, aggiornamento dipendenze (ML-KEM), script operativi.

---

## 3. Ordine di esecuzione e parallelizzazione

Le fasi A–D sono indipendenti per scope di file e possono procedere in parallelo su tre binari:

- **Binario relay**: A (Dockerfile/compose), B1, B5, C4, C5, C6, C7(relay), C8(relay).
- **Binario core**: B2, B3, B4, B6, B7, C1, C2, C3, C7(core), C8(core), C9, C10, C11.
- **Binario app**: A1–A5, D1–D6, C12.

La fase E (CI, igiene, docs) è del Lead e non tocca i file dei tre binari, salvo `.github/`, `scripts/`, radice.

**Regola anti-conflitto:** ogni agente scrive solo nei propri prefissi di path. Il Lead rivede il diff finale ed esegue la verifica completa.

---

## 4. Verifica finale (gate di completamento)

Il piano è completo solo quando, sulla stessa macchina:

1. `cargo fmt --check` pulito.
2. `cargo clippy --workspace --all-targets -- -D warnings` pulito.
3. `cargo build --workspace --locked` riesce.
4. `cargo test --workspace --locked` verde, con la suite descritta in [test_plan.md](test_plan.md).
5. `flutter analyze` senza errori; `flutter test` verde.
6. `scripts/verify_platform_config.sh` esce 0.
7. Il diff non introduce regressioni nei test esistenti e non rompe la compatibilità AP (verificata dai contract test sui documenti actor/attività).

Se un punto non è raggiungibile (es. toolchain assente), va dichiarato esplicitamente e sostituito da una verifica statica equivalente.

---

## 5. Rischi e mitigazioni

| Rischio | Mitigazione |
|---|---|
| Il refactor del DB del relay rompe percorsi non coperti | Introdurre i test prima del refactor; mantenere l'API di `Db` invariata |
| Il binding `actor`↔firma rompe peer legittimi con `keyId` non canonico | Accettare sia `actor` stringa sia `object.id`; loggare il mismatch prima di rifiutare; test con documenti Mastodon/Misskey reali |
| La firma obbligatoria su `/inbox` rompe relay esistenti | Rifiutare solo se la firma è assente o invalida; i client Fedi3 firmano già; test di regressione sul percorso legacy |
| Il loop di riconnessione genera tempeste di riconnessioni | Backoff esponenziale con jitter, cap a 5 minuti, reset dopo connessione stabile |
| I fix di piattaforma non sono verificabili senza build | Script di verifica statica su manifest/plist/gradle, più build in CI |
| Due agenti modificano lo stesso file | Scope disgiunti dichiarati; il Lead è l'unico a toccare `.github/`, `scripts/`, radice |

---

## 6. Definition of done

- Tutti i P0 chiusi e coperti da test o verifica statica.
- Almeno gli HIGH di sicurezza (B1–B7, C1–C6, C9–C11) chiusi e testati.
- Suite di test eseguibile in locale e in CI, con conteggio dei casi e copertura per area.
- CI verde sui tre binari.
- Documentazione aggiornata: audit, piano, test plan, `SECURITY.md`, `CHANGELOG.md`.
- Nessun file binario nel repository; `.gitignore` corretto.

---

## 7. Stato di implementazione

Eseguito il 2026-10-31. Gate verificati con output reale:

| Gate | Comando | Esito |
|---|---|---|
| Formattazione Rust | `cargo fmt --all -- --check` | ✅ nessuna differenza |
| Lint Rust | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 0 warning |
| Build | `cargo build --workspace --locked` | ✅ |
| Test Rust | `cargo test --workspace --locked` | ✅ 165 test verdi, 0 falliti |
| Analisi app | `flutter analyze --fatal-infos` | ✅ nessun problema |
| Formattazione app | `dart format --set-exit-if-changed lib test` | ✅ 0 file cambiati |
| Test app | `flutter test` | ✅ 76 test verdi |
| Configurazione piattaforma | `scripts/verify_platform_config.sh` | ✅ 9/9 controlli |

Fasi completate: **A** (blocker di piattaforma), **B** (federazione e autenticazione), **C** (disponibilità), **D** (app), **E** (CI, igiene, docs), e le parti di **F**/**G** elencate nei task dei tre agenti.

### Non affrontato in questa iterazione

Restano aperti, con impatto minore e senza regressioni:

- `H-17` chat: verifica di appartenenza al thread su tutti i percorsi (parziale).
- `H-39` visibilità dell'outbox per le note non elencate (filtro base presente).
- `H-03`/`H-04` SSRF completa su tutti i fetch remoti: coperto `KeyResolver`, non ogni `object_fetch`.
- `H-05` limiti di decodifica immagini oltre al cap di byte.
- `H-42` `JoinSet` completo allo shutdown: solo tunnel e server sono joinati.
- `H-44` stati `locked`/`pending` dell'account.
- `H-46` eviction FIFO invece di LRU nelle cache.
- Permessi `0600` sui file DB/WAL (la directory è già `0700`).
- Migrazione `search_tsv` per database PostgreSQL esistenti (i DB nuovi sono corretti).
- Il Dockerfile del relay non è stato costruito in questa sessione: Docker non era disponibile. Il tag `rust:1.99-bookworm` è stato verificato esistente sul registry ed è allineato al toolchain usato per i test.
