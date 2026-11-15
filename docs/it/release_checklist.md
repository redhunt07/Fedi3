# Verifica ripetibile prima del rilascio

Questa procedura verifica i sorgenti e distingue i test automatici dalle prove
sui pacchetti distribuiti. Un risultato positivo dei test non sostituisce le
prove sui dispositivi reali. I commenti e le docstring del codice sono in inglese;
le guide mantenute hanno una versione italiana e una inglese.

## Ambiente e controlli automatici

Usa Rust indicato in `rust-toolchain.toml`, Flutter **3.50.0** (come la CI),
Python 3 con PyYAML, Docker con Compose, ShellCheck e `cargo-audit`.
Installa le dipendenze Linux descritte in [build_linux.md](build_linux.md),
poi esegui dalla radice del repository:

```bash
export PATH="$HOME/.cargo/bin:$HOME/flutter/bin:$PATH"
cargo install cargo-audit --locked
(cd app && flutter pub get)
bash scripts/verify_release.sh
```

Lo script non incrementa versioni e non crea commit, tag o release. Si ferma al
primo errore e stampa la cartella temporanea dei log; puoi sceglierla tramite
`FEDI3_VERIFY_LOG_DIR`. Ogni script `scripts/tests/test_*.py` viene eseguito
direttamente: la sola discovery di unittest non esegue i controlli con `main()`.
Il controllo dei link verifica i file locali, non gli URL esterni né le ancore.

La suite include federazione con relay/core reali, sincronizzazione account,
spool, sicurezza ActivityPub, RSA/WebRTC con un client precedente e widget Flutter.
Il test responsive controlla primo avvio, creazione account, impostazioni e
abbinamento a 320/390/1280 pixel, in italiano/inglese, tema chiaro/scuro e testo
al 200%. Gli HTTP dei widget sono simulati: questo non prova videocamera,
notifiche del sistema operativo o federazione su Internet.

Per il test PostgreSQL crea un database **isolato e sacrificabile**, con un
utente dedicato, e passa la sua URL tramite `FEDI3_TEST_POSTGRES_URL` allo stesso
script. Non usare il database di produzione: il test crea dati e verifica
prenotazioni e cancellazioni. Senza questa variabile lo script dichiara il test
non eseguito; la CI lo esegue con un servizio PostgreSQL dedicato. Il test dei
bootstrap IPFS pubblici resta separato e richiede connettività esterna.
`cargo audit` deve riuscire; eventuali avvisi di manutenzione vanno riesaminati,
anche quando non costituiscono vulnerabilità bloccanti.

## Protocollo e conservazione dei dati

- Verifica [sincronizzazione multi-dispositivo](multi_device_sync.md),
  [rete P2P](p2p_network.md), [interoperabilità](interop.md) e
  [OpenAPI](../openapi.yaml) quando cambi un contratto.
- Aggiorna prima il relay e poi i client che negoziano nuovi record di replica.
  L'identità è URL completo dell'attore e chiave RSA; il token del relay non
  sostituisce la prova della chiave. Non ricreare un'identità per recuperare
  un account esistente.
- Prova aggiornamento e ripristino con copie di dati di test: identità, chiavi,
  post propri, chat, relazioni e credenziali devono restare leggibili.
- Su desktop verifica anche il keyring bloccato: deve apparire il recupero e
  il profilo deve restare intatto. La cache privata in chiaro per il servizio
  Android non deve essere creata o usata su desktop. Le copie desktop legacy
  vengono eliminate dopo l'accesso riuscito al keyring; se il keyring è
  indisponibile restano conservate per il recupero e non vengono usate.

## Git e pacchetti

1. Controlla `git status --short`, `git diff --check` e il diff completo. Se ci
   sono modifiche già presenti, distingui il loro contenuto dal lavoro nuovo.
   Non includere `.env`, chiavi, database, log o output generati nel commit.
2. Allinea versione/build in `app/pubspec.yaml`, changelog e note. Il tag
   `vX.Y.Z` deve corrispondere alla versione e identificare il commit verificato.
   Riesegui i controlli se cambi i sorgenti dopo la verifica.
3. Costruisci core e client dello stesso commit. La sola compilazione Flutter
   Linux non include automaticamente il core appena compilato: usa gli script
   di packaging, oppure ricopia il core con `scripts/build_core.sh release`
   dopo `flutter build linux --release`.
4. Un tag produce una bozza tramite CI; `codex/release/vX.Y.Z` richiede invece
   la pubblicazione automatica e deve puntare al `main` corrente. Non usare quel
   ramo come semplice area di lavoro. Vedi [aggiornamenti](updates.md).
5. La CI produce AppImage, ZIP Windows, APK Android e DEB. I segreti di firma
   Android devono essere presenti e mantenere la stessa identità di firma
   degli aggiornamenti precedenti. Un APK con chiave debug non equivale a un
   pacchetto di distribuzione firmato per quel canale.
6. Verifica i checksum dei pacchetti scaricati e prova installazione pulita,
   aggiornamento e riavvio su Linux/Windows/Android. La CI controlla il primo
   frame dell'AppImage su Ubuntu 22.04 e 24.04. Le build di altri sistemi e i
   canali Flatpak/AUR/F-Droid richiedono le verifiche specifiche in
   [distribuzione](distribution.md).

## Prove reali prima della pubblicazione

Usa account di test controllati. Segui [interoperabilità](interop.md) con
Mastodon/Misskey e almeno due dispositivi associati: follow/accept/unfollow,
post e risposte, visibilità privata, modifiche/cancellazioni, allegati, chat,
chiamate e notifiche. Prova perdita della rete, riavvio, sospensione Android e
ripresa con un altro dispositivo ancora connesso. Verifica fotocamera, microfono,
audio, tastiera, focus, scroll e testo grande sui pacchetti effettivi.

Registra commit, ambiente, comandi, risultati e limiti in un rapporto datato in
`docs/audit/` con controparte italiana in `docs/it/audit/`. Le prove precedenti
sono evidenza storica: non devono essere presentate come nuove verifiche di un
pacchetto differente. Pubblica solo dopo aver chiuso i problemi bloccanti dei
canali scelti e verificato backup/ripristino.

Per eseguire tutta la CI prima della release senza pubblicarla, usa il ramo
`codex/verify-release/vX.Y.Z` corrispondente al pubspec. Esegue build Windows,
Android, AppImage e avvio su entrambe le versioni Ubuntu, con audit bloccante;
il job di pubblicazione resta disabilitato. Questi rami servono solo alla verifica.
