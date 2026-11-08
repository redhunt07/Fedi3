# Verifica client 0.11.0 — 2026-11-08

Release: [v0.11.0](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.0).
Versione Flutter: `0.11.0+3`. Questa release integra il lavoro Android successivo
alla 0.10.0 e tutte le correzioni AppImage pubblicate nella 0.10.1.

## Verifiche locali dei sorgenti

- 309 test Rust standard superati, zero fallimenti; quattro test speciali ignorati
  dalla suite standard. Verificati separatamente PostgreSQL e interoperabilità.
- 189 test Flutter superati; analisi statica senza problemi, formattazione Rust e Dart.
- Clippy workspace e tutti i target con `-D warnings` superato.
- Interoperabilità RSA e WebRTC con il client precedente verificata con l'oracolo isolato.
- Prenotazione quote PostgreSQL concorrente verificata su PostgreSQL 16 temporaneo.
- Contratti deployment/OpenAPI/release, backup, directory relay, configurazione di
  piattaforma, ShellCheck e regressione delle librerie AppImage superati.
- `cargo audit`: zero vulnerabilità, cinque avvisi di manutenzione, nessun advisory ignorato.

## Pacchetti e limiti

Linux x86_64 usa il nome stabile `Fedi3-linux-x86_64.AppImage`. Il workflow di
release compila su Ubuntu 22.04 e richiede l'avvio della GUI su Ubuntu 22.04 e
24.04; la build locale viene provata anche sul desktop Arch del maintainer.
Le note della release e i relativi workflow registrano l'esito dei pacchetti.

Gli APK Android `Fedi3-android-arm64-v8a.apk` e `Fedi3-android-x86_64.apk` includono
il core Rust release ricompilato per la rispettiva ABI, richiedono Android 7/API 24
o successivo e sono per installazione manuale. ARM a 32 bit non è supportato.
La versione Android è `0.11.0`, numero build base `3` (codici APK separati: ARM64 `2003`, x86_64 `4003`).

Non è configurato `app/android/key.properties`: gli APK usano la chiave di debug
locale già esistente. Conservare quella chiave per gli aggiornamenti; un certificato
diverso richiede backup prima della reinstallazione. Non sono pacchetti Play Store.
Gli avvisi Gradle su future versioni minime AGP/Kotlin non sono errori di build.

SHA256 dei pacchetti in `checksums.txt` nella release. I profili, i database,
le chiavi del core e le credenziali del keyring non vengono migrati o cancellati.
Il collaudo completo su telefoni fisici descritto negli annunci precedenti è
una verifica precedente del maintainer, distinta dalle prove di questa release.
