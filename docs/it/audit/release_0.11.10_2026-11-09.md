# Verifica del client 0.11.10 — 2026-11-09

Release: [v0.11.10](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.10).
Versione Flutter: `0.11.10+2013`. Questa release corregge il ciclo di rifiuto
delle prenotazioni relay che impediva ai client P2P di mantenere un circuito e
fa partire il P2P su Android anche quando il resolver DNS di sistema non è
utilizzabile.

## Cosa è cambiato

- Relay: budget di prenotazioni per peer portato a 8 (configurabile fino a 128)
  e rate limiter di libp2p sostituito con token bucket che si ricaricano nel
  tempo, così i client che si riconnettono non vengono più rifiutati in ciclo.
- Core: una sola prenotazione per peer relay (la risposta contiene già gli
  indirizzi TCP e QUIC), retry esponenziale da 5 secondi fino a 2 minuti e
  riscoperta periodica dell'infrastruttura relay, così un telefono avviato
  prima che la rete sia pronta si riprende senza riavviare l'app.
- Android: il resolver DNS di sistema (hickory) va in panico quando il contesto
  NDK non è inizializzato; il core intercetta il panico e continua con
  indirizzi IP. I fallimenti di mDNS e di listen non fermano più l'intero stack.
- Diagnostica: il tracing del core è replicato su logcat Android con il tag
  `fedi3-core` e i panico sono riportati tramite tracing.

## Pacchetti

| Pacchetto | Piattaforma | Dimensione (byte) | SHA256 |
| :--- | :--- | ---: | :--- |
| `Fedi3-linux-x86_64.AppImage` | Linux x86_64 | 177198272 | `c29d6edd9d0070f4b939d64f647ac4912c73730ac040477c099adcdb6a6c00c4` |
| `Fedi3-windows-x64.zip` | Windows x64 | 57068313 | `110c99f0f5544075dd314b684be3d9ed565f565a89d19e43c82b899232729fbb` |
| `app-arm64-v8a-release.apk` | Android arm64-v8a | 73216215 | `ead7db4e82055e98049c49a94a71f7f9a9e7fb9d6cef1d726d98fa8331c982ae` |
| `app-x86_64-release.apk` | Android x86_64 | 85739589 | `f581fdcc0780a8c05b9c698e2ebee2e7bb3c7e962b14e126f3d27717af00f57a` |

L'AppImage e lo ZIP Windows sono prodotti dal workflow CI sul commit di release
e provati prima della pubblicazione (`AppImage startup` su Ubuntu 22.04 e
24.04). Gli APK Android sono compilati localmente con
`scripts/build_android.sh release`; `aapt` riporta `versionName=0.11.10`,
version code `4013` (arm64-v8a) e `6013` (x86_64).

## Verifiche eseguite

- `cargo test --workspace --locked` passa; `cargo fmt --check` e
  `cargo clippy --workspace --all-targets --locked -- -D warnings` sono puliti.
- `flutter analyze --fatal-infos` non segnala problemi e l'intera suite di test
  Flutter passa (205 test).
- Relay ridistribuito dal commit di release su entrambi i relay. I limiti di
  prenotazione riportano `max_reservations_per_peer: 8` con due rate limiter che
  si ricaricano, e il log dell'infrastruttura non mostra voci
  `relay reservation denied` per nessuno dei due client nella finestra
  osservata.
- Client reali: il client Linux mantiene una sola prenotazione
  (`renewed=false`, poi nessun rinnovo continuo) e il client Android ottiene la
  propria prenotazione e pubblica l'indirizzo relayed. I circuiti tra i due
  client vengono accettati dal relay in entrambe le direzioni.
- Android: `adb logcat -s fedi3-core` mostra `p2p configuration enable=true`,
  `p2p task starting`, `p2p transport ready`, `p2p enabled` e `p2p listening`
  con il multiaddress relayed; prima della correzione il task P2P moriva sul
  panico del resolver hickory e non esisteva alcun listener.

## Limiti noti

- Gli APK Android sono build per installazione manuale, firmate con la chiave di
  debug locale, non pacchetti Play Store. Android accetta aggiornamenti solo se
  il certificato di firma coincide, quindi conserva la chiave locale per le
  build future.
- Gli indirizzi basati su DNS (`/dns4`, `/dnsaddr`) non vengono risolti su
  Android finché il resolver di sistema resta inutilizzabile; gli indirizzi IP,
  compreso quello dell'infrastruttura relay, continuano a funzionare.
- iOS e macOS sono compilabili dai sorgenti ma non fanno parte di questa
  release.
- Lo ZIP Windows è compilato e verificato dalla CI; non viene eseguito su una
  macchina Windows fisica nella checklist di release.
