# Verifica pubblicazione client 0.10.0 — 2026-11-08

Release: [v0.10.0](https://github.com/redhunt07/Fedi3/releases/tag/v0.10.0).
Commit sorgente: `ad15842ce13d10740073ecf6b9e4803bcaf63a5a`.

- [CI main](https://github.com/redhunt07/Fedi3/actions/runs/42456638207): superata.
- [CI release](https://github.com/redhunt07/Fedi3/actions/runs/42456653252): superata,
  incluse build AppImage Linux x86_64 e ZIP Windows x64 su runner nativo.
- Superati 304 test Rust standard e 184 test Flutter, Clippy rigoroso, analisi
  Flutter, formattazione, quote PostgreSQL e contratti di backup e deployment.
- Superato l'oracolo isolato RSA 0.9 e WebRTC 0.12, in entrambi i ruoli SDP.
  Formati delle chiavi, DID, firme e buste cifrate restano compatibili. Il lockfile
  della release non contiene `rsa` o `webrtc-dtls`.
- Audit RustSec obbligatorio: zero vulnerabilità, cinque avvisi di dipendenze
  non più mantenute e nessun advisory ignorato.
- Asset stabili: `Fedi3-linux-x86_64.AppImage`, `Fedi3-windows-x64.zip`,
  `checksums.txt`. Il workflow calcola gli hash dei pacchetti esatti prima dell'upload.
- La diagnostica del client resta locale; nessun push Git automatico di telemetria.

Il test delle ricevute inoltrate imposta ora l'URL pubblico del proprio relay
isolato e confronta la chiave dell'attore con quella del core di test. Questo
impedisce ai redirect verso la porta predefinita di raggiungere un altro core locale.

I vecchi client richiedono un primo aggiornamento manuale per ricevere l'updater.
Dati dei profili, chiavi e credenziali nel portachiavi devono essere conservati.
Pubblicazione di pacchetti Android/iOS/macOS e test interattivi Windows non fanno
parte di questi risultati.

La CI Linux installa i pacchetti di sviluppo GStreamer e rimuove il conflitto
con `libunwind-14-dev` preinstallato sul runner Ubuntu 22.04.

## Controlli sui pacchetti pubblicati

Entrambi i pacchetti e `checksums.txt` sono stati scaricati dagli URL pubblici.
Gli SHA256 coincidono con i checksum e con i digest degli asset GitHub.
Lo ZIP Windows supera il controllo CRC e contiene eseguibile, DLL del core e
DLL Flutter. Dall’AppImage sono stati estratti eseguibile, core e libreria Flutter;
il core pubblicato si carica e la funzione FFI di versione risponde correttamente.

| Asset | SHA256 |
| --- | --- |
| `Fedi3-linux-x86_64.AppImage` | `9fb3dea2bd191566f31ae01e254899908b0d1b7c9bdecfefffa473696240b334` |
| `Fedi3-windows-x64.zip` | `ea3127251cf208fe641f8f7aa82bee5898677c0ad350b95960f30b95f02838b7` |

## Verifica successiva: regressione nell’avvio GUI Linux

L’avvio su un desktop Arch recente ha rilevato `MOUNT_2_40` e poi
`LIBSYSTEMD_251`: vecchie dipendenze incluse nascondevano quelle della GLib di
sistema. I controlli precedenti verificavano pacchetti e FFI del core, ma non
l’avvio della GUI completa. Politica di packaging corretta e controllo GUI
obbligatorio su due distribuzioni sono pubblicati nella 0.10.1. I dati dei profili
restano invariati.
