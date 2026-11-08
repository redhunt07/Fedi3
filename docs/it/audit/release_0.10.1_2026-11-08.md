# Verifica pubblicazione client 0.10.1 — 2026-11-08

Release: [v0.10.1](https://github.com/redhunt07/Fedi3/releases/tag/v0.10.1).
Commit sorgente: `dfb98c277eccc1dcd7091700edc51b3354a157df`.

## Regressione e correzione

L’AppImage 0.10.0 superava checksum e verifica FFI del core, ma su un desktop Arch
recente falliva prima della GUI: la vecchia `libmount` inclusa nascondeva quella
richiesta dalla GLib di sistema (`MOUNT_2_40`). Rimuovendola emergeva un secondo
conflitto con `libsystemd` (`LIBSYSTEMD_251`). La vecchia Fontconfig rifiutava anche
la configurazione font del sistema.

Una politica unica di packaging esclude le librerie del desktop sia dalla copia
delle dipendenze sia dai file già presenti nel bundle Flutter: dipendenze GLib
mount/blkid/systemd/udev/PCRE2/FFI e stack font del sistema. L’immagine locale
ricreata ha aperto la finestra sul desktop interessato, confermata visibile dal
manutentore. Profili, database, credenziali nel portachiavi e formati cifrati non
sono stati migrati o azzerati.

## Controlli di release

- [CI main](https://github.com/redhunt07/Fedi3/actions/runs/42461970385): superata.
- [CI release](https://github.com/redhunt07/Fedi3/actions/runs/42461972792): superata,
  incluse build native Linux/Windows e avvio GUI obbligatorio dell’AppImage esatta
  su Ubuntu 22.04 e 24.04 con Xvfb e sessione D-Bus nuova.
- Il controllo GUI richiede una finestra Fedi3 visibile, mostrata dal runner Linux
  solo dopo il primo frame Flutter, e verifica che il processo resti attivo.
- Il test ABI compila due fixture `libmount.so.1` incompatibili con simboli
  versionati e riproduce il fallimento `MOUNT_2_40` prima di applicare la politica.
- Superati 304 test Rust standard e 184 Flutter, interoperabilità RSA/WebRTC
  precedente, PostgreSQL, Clippy/analyzer rigorosi e contratti di deployment.
- Audit dipendenze: zero vulnerabilità, cinque avvisi di manutenzione, nessun
  advisory ignorato. Implementazione crittografica e formati profili/messaggi invariati.

Gli utenti Linux con GUI 0.10.0 non avviabile devono scaricare manualmente la
0.10.1. L’updater torna utilizzabile avviando il client corretto. Pacchetti
macOS/mobile e test interattivi Windows restano fuori da questa verifica.

La prova su Ubuntu 24.04 ha rilevato anche la vecchia Xlib del bundle, che
impediva a Mesa di creare il contesto EGL. Xlib e XCB seguono ora i driver del
sistema. I runner installano esplicitamente il runtime OpenGL ES e Wayland.

## Controllo dei download pubblici

Scaricati entrambi i pacchetti e `checksums.txt` dagli URL pubblici: SHA256
coincidenti con checksum e digest GitHub. Lo ZIP Windows supera il CRC e contiene
eseguibile e DLL core/Flutter. Il client completo estratto dall’AppImage supera
`ldd -r` sul desktop Arch interessato, senza librerie mancanti o simboli
incompatibili; le copie vietate dalla politica sono assenti dal pacchetto.

| Asset | SHA256 |
| --- | --- |
| `Fedi3-linux-x86_64.AppImage` | `70db90896c430ad73bb01361079b200bcb0dad5d72a040c86de0e51e4c686ae1` |
| `Fedi3-windows-x64.zip` | `becc29514f7188a700ecd2fdfee98ff4bf424938f2e43c3ad8c02c637f142555` |
