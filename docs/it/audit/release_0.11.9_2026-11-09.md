# Verifica del client 0.11.9 — 2026-11-09

Release: [v0.11.9](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.9).
Versione Flutter: `0.11.9+2012`. Questa release raccoglie le correzioni
pubblicate dopo la 0.11.1 (pulizia del composer, task Android unico, indirizzi
P2P relayed, normalizzazione dell'outbox migrato, banner del profilo, logo neon,
particelle di navigazione continue e avvio Linux pulito).

## Pacchetti

| Pacchetto | Piattaforma | Dimensione (byte) | SHA256 |
| :--- | :--- | ---: | :--- |
| `Fedi3-linux-x86_64.AppImage` | Linux x86_64 | 177185984 | `40c5138c4db4434802eaa2d8fa2b4597fcedbcfe4dd0aeb7103f37ef8d0ef0d9` |
| `Fedi3-windows-x64.zip` | Windows x64 | 57047939 | `76f78cdf4265fefbe0aca52f6930fc9962dc3bbbf3ebe58613a2b8ce245e2f5b` |
| `app-arm64-v8a-release.apk` | Android arm64-v8a | 73183443 | `df003663fc4144fbf29873ff70542a849cc9e3b3e9b6ff4866dcd3faebb4e9a9` |
| `app-x86_64-release.apk` | Android x86_64 | 85706817 | `1e6022f1c88bc821918fad5b8b4b46a9dfdb2ad12ebbf2f7124fd2ff8d2f81b4` |

L'AppImage e lo ZIP Windows sono prodotti dal workflow CI sul tag `v0.11.9` e
verificati prima della pubblicazione (`AppImage startup` su Ubuntu 22.04 e 24.04).
Gli APK Android sono compilati in locale con `scripts/build_android.sh release`;
`aapt2 dump badging` riporta `versionName=0.11.9`, version code `4012`
(arm64-v8a) e `6012` (x86_64).

## Verifiche eseguite

- CI sul commit della release: Rust fmt, clippy, test, Flutter analyze e test,
  configurazione di piattaforma, contratti deploy/OpenAPI/release, test delle
  quote PostgreSQL, build AppImage e smoke test del primo frame, build del
  pacchetto Windows.
- AppImage Linux: si avvia sul desktop di sviluppo, si collega al core e mostra la
  timeline; nessun critical ATK nel log di avvio.
- APK Android: installazione e avvio su un dispositivo arm64 fisico e sul target
  emulatore x86_64; timeline, chat, profilo e impostazioni vengono renderizzati.
- Interoperabilità del relay: timeline federata, indirizzi P2P relayed,
  normalizzazione dell'outbox migrato e coerenza dei profili sono coperti dai test
  del relay nella suite del workspace.

## Limiti noti

- Gli APK Android sono build per installazione manuale firmate con la chiave di
  debug locale, non pacchetti Play Store. Android accetta aggiornamenti solo con
  lo stesso certificato di firma: conserva la chiave locale per le build future.
- iOS e macOS si compilano dai sorgenti ma non fanno parte di questa release.
- La chat è cifrata end-to-end senza forward secrecy (limite documentato).
- Il pacchetto Windows è compilato e verificato dalla CI; non viene eseguito su
  una macchina Windows fisica nella checklist della release.
