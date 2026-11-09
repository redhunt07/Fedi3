# Per iniziare

Questa guida porta da un checkout pulito a un relay e un client funzionanti nel minor tempo possibile.

## Cos'è FEDI3

FEDI3 è un social federato con componente P2P. L'app usa un core locale (Rust) e comunica con un relay pubblico per la compatibilità ActivityPub. Il relay è opzionale: senza di esso hai P2P e chat, ma non la federazione con le istanze tradizionali.

## Prerequisiti

- Docker Engine e Docker Compose v2.20 o successivo per il relay (l'installer verifica la versione di Compose).
- Flutter 3.38 o successivo e Rust stabile per compilare app e core.
- Per i pacchetti precompilati vedi [pacchetti di release](#pacchetti-di-release).

## Installazione del server con un comando

Su Debian/Ubuntu o derivate Arch, l'installer configura Docker, TLS, segreti e
la modalità scelta (relay + P2P, solo relay o solo P2P):

```bash
sudo git clone https://github.com/redhunt07/Fedi3.git /opt/fedi3
cd /opt/fedi3
sudo bash scripts/install_server.sh
```

Vedi [installazione del server](deploy_server.md) per la guida passo passo; su
YunoHost usa l'[installazione su YunoHost](deploy_yunohost.md).

## Avvio rapido (relay)

```bash
cp .env.example .env
```

Compila almeno: `FEDI3_DOMAIN`, `FEDI3_RELAY_ADMIN_TOKEN` (>= 32 caratteri), `FEDI3_RELAY_PG_PASSWORD`, `FEDI3_RELAY_DB_DRIVER` e il backend media. Il template avvia `relay,p2p,caddy`; aggiungi `turn` a `COMPOSE_PROFILES` (e imposta `TURN_REALM`, `TURN_PASS`, `TURN_EXTERNAL_IP`) per le chiamate, oppure `search` per Meilisearch. Su YunoHost usa `relay,p2p` (senza Caddy). Lascia `FEDI3_RELAY_DB_URL` vuoto per il servizio Compose: il relay ricava l'URL PostgreSQL da `FEDI3_RELAY_PG_USER`, `FEDI3_RELAY_PG_PASSWORD` e `FEDI3_RELAY_PG_DB`.

```bash
docker compose up -d --build
scripts/relay_smoke_test.sh https://<tuo-dominio> <ADMIN_TOKEN>
```

## Avvio rapido (app)

```bash
cd app
flutter pub get
flutter run
```

Per compilare il core nativo e l'AppImage vedi [build_linux.md](build_linux.md).

## Pacchetti di release

I pacchetti precompilati sono pubblicati nella [release v0.11.10](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.10): `Fedi3-linux-x86_64.AppImage` per Linux x86_64 e `app-arm64-v8a-release.apk` oppure `app-x86_64-release.apk` per Android. Verifica ogni file con `checksums.txt` prima di eseguirlo. Gli APK Android sono build per installazione manuale, firmate con la chiave di debug locale; vedi [app_guide.md](app_guide.md) per i dettagli sull'installazione.

## Verifica dell'ambiente

```bash
cargo test --workspace --locked
cd app && flutter analyze && flutter test
scripts/verify_platform_config.sh
```

## Documentazione principale

- [deploy_server.md](deploy_server.md) — installazione automatica del server per Debian e Arch
- [deploy_yunohost.md](deploy_yunohost.md) — installazione su YunoHost con nginx esterno
- [deploy_relay.md](deploy_relay.md) — mettere in produzione il relay
- [deploy_core.md](deploy_core.md) — core locale e modalità P2P
- [build_linux.md](build_linux.md) — compilare app e AppImage
- [app_guide.md](app_guide.md) — guida utente
- [ops_backup_restore.md](ops_backup_restore.md) — backup e ripristino
- [troubleshooting.md](troubleshooting.md) — diagnosi
- [faq.md](faq.md) — domande frequenti
- [test_plan.md](test_plan.md) — piano dei test
- [SECURITY.md](../../SECURITY.md) — segnalazione vulnerabilità
