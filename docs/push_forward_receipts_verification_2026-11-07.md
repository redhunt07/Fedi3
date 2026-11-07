# Push nativi e ricevute forward — verifica del 7 novembre 2026

Client Linux e relay aggiornati; identità, chiave di firma del relay e configurazione
privata conservate. Il client si è riconnesso dopo la sostituzione del relay.

## Risultato

- Android: connettore nativo ufficiale UnifiedPush 3.5.0, selezione distributore,
  rinnovo/revoca endpoint, chiavi Web Push e gestione messaggi cifrati. Avvio
  headless con avviso generico; l'apertura della UI sincronizza la coda senza
  avviare core concorrenti in motori Flutter differenti.
- iOS: callback APNs per token, rotazione/errori e background fetch; completamento
  singolo con timeout nativo; bridge Dart e sincronizzazione breve. Entitlement,
  modalità background e ambiente APNs configurabili. Aggiunta una fase Xcode per
  compilare/linkare il core Rust statico, preservando i simboli FFI.
- Impostazioni push opt-in nella sicurezza mobile. Scope distinto per account e
  relay; credenziali nel portachiavi. Rinnovo al rientro nell'app e retry limitato
  sul relay; invio push separato dall'accettazione dei messaggi.
- Ricevute Ed25519 per tentativi mutanti verso core locali e inserimenti nello
  spool; conferma indipendente RSA-SHA256 del core con nonce e digest di richiesta
  e risposta. Stati distinti per accodamento, risposta verificata e rifiuto.
- Registro pubblico di commitment concatenati, checkpoint firmati e ricevute
  private autenticate. Retention con ancoraggio del prefisso rimosso, scritture
  atomiche e protezione dagli eventi tardivi durante la cancellazione account.
- Verificatore indipendente `scripts/verify_forward_receipt.py` per ricevute e
  registri pubblici completi, con chiavi fissate indipendentemente.

## Prove completate

- Workspace Rust: **291 passati**, 2 ignorati (IPFS pubblico e fixture PostgreSQL).
- Fixture PostgreSQL 16 eseguita separatamente: **passata**, compresi quota media,
  spool concorrente, creazione concorrente della chiave di firma, catena del
  registro e assenza di ricevute private dopo cancellazione/eventi tardivi.
  Database di produzione separato dai test; container e tunnel temporanei rimossi.
- Flutter: **158 passati**; ulteriori 7 test mirati sugli ultimi ritocchi dei
  connettori/API passati. Analyze fatal-infos e Clippy all-targets passati.
- Verificatore Python: 2 test passati; firme contraffatte/chiavi inattese e binding
  errati rifiutati. Catena completa verificata anche su candidato e produzione.
- E2E con relay e core reali: risposta 202 e rifiuto 404, entrambe con firme
  relay/core verificate. Test di nonce diverso, corpo alterato e attore diverso.
- APK Android ARM64 compilato con core incluso; binario contenuto nell'APK uguale
  al core Rust dopo lo stripping applicato da Gradle. Risolti desugaring e conflitto
  Tink desktop/Android. APK debug di validazione, con API minima 24.
- iOS: plist ed entitlement validi; progetto Xcode analizzato con parser OpenStep;
  fase Rust e riferimenti di linking presenti; script passati a ShellCheck.
  Bridge APNs verificato con test Dart. UIKit, toolchain Apple, firma e consegna
  APNs **richiedono ancora compilazione e prova su macOS/dispositivo provisionato**.
- Compose e OpenAPI validati. Candidato Docker verificato prima del deploy:
  limiti JSON push, autenticazione, configurazione disattivata, ricevute private,
  registro pubblico e firme indipendenti.

## Verifica in produzione

`https://fedi3.redhunt07.ovh/transparency` e
`https://fedi3.redhunt07.ovh/_fedi3/relay/forward-log` sono disponibili.

Un probe privato dell'account locale ha prodotto 404 e `core_rejected`:
verificati firma Ed25519 con la chiave fissata prima del deploy, firma RSA con la
chiave pubblica derivata dall'identità locale, digest dei byte di richiesta e
risposta e catena pubblica completa. L'endpoint privato delle ricevute richiede
il token dell'account. Relay/P2P/TURN healthy, nessun restart o panic osservato.

## Limiti operativi e configurazione

I provider esterni restano **disattivati**: occorrono un gateway conforme al
contratto, distributore UnifiedPush e/o credenziali/provisioning APNs. Il repository
implementa il contratto del gateway; non ne attiva un deployment provider.
La consegna effettiva su telefoni non è stata provata con servizi reali.
L'APK non è stato installato su un dispositivo dell'utente. Per una release Android
firmata occorre la chiave di firma del progetto; la build debug serve alla verifica.

Le ricevute attestano le operazioni osservate dal relay e le risposte del core.
Le firme non costituiscono ricevute di lettura umana o prova di elaborazione da
parte di un'altra istanza ActivityPub. GET/HEAD/OPTIONS e trasporti diversi dal
tunnel locale/spool hanno scope separato. Testimoni indipendenti e confronto
automatico tra checkpoint di relay differenti restano ulteriori sviluppi.

Configurazione e protocollo: [push_forward_receipts.md](push_forward_receipts.md).

## Artefatti e rollback

- Client attivo: `dist/Fedi3-release-x86_64.AppImage`.
- APK di prova: `app/build/app/outputs/flutter-apk/app-debug.apk`.
- Relay: `fedi3-relay:push-receipts-20261107`.
- Rollback: `fedi3-relay:before-push-receipts-20261107`.
- Backup VPS: `/home/rocky/fedi3-backups/push-receipts-20261107/`, con dump
  PostgreSQL verificato, configurazione precedente, sorgenti e manifest.
- Client precedente: `~/.cache/fedi3-audit/Fedi3-before-push-receipts.AppImage`.
- Backup profilo cifrato: `~/.cache/fedi3-audit/alice-before-push-receipts-20261107.aesgcm`;
  nonce di 12 byte seguito da ciphertext/tag AES-GCM, AAD
  `fedi3-push-receipts-20261107`. Chiave nel Secret Service con attributi
  application=fedi3, backup=fedi3-push-receipts-20261107.
- Le tabelle journal sono additive; il rollback del binario non richiede di
  eliminare dati o ripristinare il dump.

Fingerprint SHA256 della chiave pubblica del relay conservata: `510f37d2a922a0518089961729b51ebdb50b90c5569fe7a8fffeff615201e90c`.

- core_sha256: `401fd528a0a12cc576f2f825fd5610e32f752e3e99a9b18140ac15670d1f3607`
- appimage_sha256: `83a5645053bc0cdbd1fec0edd4aaa1fdb29f1592962ce90997f8de674bce3be2`
- android_core_sha256: `b227840c01927be8c02f6249df9959e29480933234e1443b66b3c1fb76c34eeb`
- android_apk_sha256: `f4f4126001e1dda57a08ab09b19e326b47a18e130ebb0f6137a786cbb5de864a`
- relay_sha256: `af7407611e0ddb5d867a2cf24c5680ce6a8ace9394942082e5b851d0e10b1620`
