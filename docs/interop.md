# Interoperabilità reale: Fedi3 ↔ Mastodon ↔ Misskey

Questo documento descrive il test di interoperabilità eseguito contro due implementazioni
ActivityPub reali e indipendenti: **Mastodon 4.7.4** e **Misskey 2026.10.1**. Lo scopo è
verificare che un'identità Fedi3 con ancoraggio (`anchor_actor_url`) possa essere seguita,
verificata e scambiare attività con software federato di terze parti, sia in ingresso sia in
uscita.

> Esito: **test superato in entrambe le direzioni** con entrambe le implementazioni.
> Il test ha inoltre individuato un bug reale nella firma HTTP in uscita, corretto in
> `crates/fedi3_core/src/delivery.rs`.

## Topologia usata

Nessun Docker: i servizi sono stati eseguiti nativamente su Arch Linux con PostgreSQL 18 e
Redis/Valkey locali.

| Host | Ruolo | Porta interna |
|------|-------|---------------|
| `https://fedi3.test` | identità Fedi3 ancorata + relay AP | core 8788 (actor, webfinger), relay 8787 (inbox, outbox, tunnel) |
| `https://misskey.test` | istanza Misskey reale | 3000 |
| `https://mastodon.test` | istanza Mastodon reale | 3001 (web) + Sidekiq |

Nginx fa da reverse proxy TLS per tutti e tre gli host e instrada `fedi3.test` in modo che:

- `/.well-known/webfinger` e `GET /users/alice` siano serviti dal core (documento attore ancorato);
- tutto il resto (`/users/alice/inbox`, `/users/alice/outbox`, `/_fedi3/tunnel`, …) sia servito dal relay.

Questo è il punto architetturale importante emerso dal test: **Misskey rifiuta un attore il cui
`inbox` non è sullo stesso host dell'`id`** (`invalid Actor: inbox has different host`). In
produzione l'host di ancoraggio deve quindi fare da proxy verso il relay per gli endpoint AP,
oppure relay e anchor devono condividere lo stesso hostname. Un anchor statico su un dominio
diverso dal relay non è compatibile con Misskey.

## Perché serve TLS

Mastodon costruisce l'URL WebFinger come `https://<domain>/.well-known/webfinger` (unica
eccezione: i domini `.onion`). Per il test locale è stata quindi generata una CA locale,
installata nel trust store di sistema (`/etc/ca-certificates/trust-source/anchors/` +
`update-ca-trust`), e un certificato con SAN per `fedi3.test`, `misskey.test`, `mastodon.test`.
Il core e il relay usano `rustls-tls-native-roots`, così rispettano il trust store di sistema
(necessario per CA interne/enterprise; in produzione si usano normali CA pubbliche).

Per far accettare i certificati locali ai runtime:

- Node/Misskey: `NODE_EXTRA_CA_CERTS=/path/ca.crt`;
- Ruby/Mastodon: trust store di sistema aggiornato;
- Mastodon: `ALLOWED_PRIVATE_ADDRESSES=127.0.0.0/8,::1/128` per consentire la risoluzione di
  host loopback;
- Misskey: `allowedPrivateNetworks: ['127.0.0.1/32', '::1/128']`.

## Preparazione delle istanze

### Misskey

```bash
# PostgreSQL e Redis locali (porte 5433 e 6380 nel test)
psql -c "CREATE USER misskey WITH PASSWORD 'misskey';" -c "CREATE DATABASE misskey OWNER misskey;"

# build da sorgente (pnpm) e migrazioni
pnpm install --frozen-lockfile
pnpm build
pnpm migrate

# .config/default.yml: url https://misskey.test/, db su 127.0.0.1:5433, redis su 127.0.0.1:6380
NODE_EXTRA_CA_CERTS=/path/ca.crt pnpm start
```

Alla prima installazione Misskey disabilita la registrazione (`disableRegistration: true`) e
richiede un primo utente amministratore: nel test è stato abilitato via DB
(`UPDATE meta SET "disableRegistration" = false; UPDATE meta SET federation = 'all';`) e
l'utente è stato creato con `POST /api/signup` (la risposta contiene già un token).

### Mastodon

```bash
# Ruby 3.4 + yarn 4 (corepack) + PostgreSQL 18 + Redis
bundle install
RAILS_ENV=production bundle exec rails db:schema:load
yarn install --immutable
RAILS_ENV=production bundle exec rails assets:precompile
RAILS_ENV=production bundle exec bin/tootctl accounts create feditester \
  --email=feditester@gmail.com --confirmed
RAILS_ENV=production bundle exec rails s -b 127.0.0.1 -p 3001
RAILS_ENV=production bundle exec sidekiq
```

In `.env.production`: `LOCAL_DOMAIN=mastodon.test`, `WEB_DOMAIN=mastodon.test`,
`LOCAL_HTTPS=true`, `ALLOWED_PRIVATE_ADDRESSES=127.0.0.0/8,::1/128`, più
`SECRET_KEY_BASE`, `OTP_SECRET`, chiavi `ACTIVE_RECORD_ENCRYPTION_*` e `VAPID_*`.
L'account va approvato (`account.user.approve!`) prima di poter seguire account remoti.

### Fedi3

```bash
# relay
FEDI3_RELAY_DB=/tmp/fedi3/relay.db \
FEDI3_RELAY_PUBLIC_URL=https://fedi3.test \
FEDI3_RELAY_BASE_DOMAIN=fedi3.test \
FEDI3_RELAY_ALLOW_SELF_REGISTER=true \
FEDI3_RELAY_ALLOW_PRIVATE_ACTOR_FETCH=true \
FEDI3_ALLOW_PRIVATE_REMOTE_FETCH=true \
FEDI3_RELAY_ADMIN_TOKEN=<token 32+ caratteri> \
./target/debug/fedi3_relay

# registrazione utente
curl -X POST http://127.0.0.1:8787/register \
  -H 'Content-Type: application/json' \
  -d '{"username":"alice","token":"alice-token-0123456789abcdef"}'

# core con identità ancorata
XDG_DATA_HOME=/tmp/fedi3/data \
FEDI3_USER=alice FEDI3_DOMAIN=fedi3.test \
FEDI3_BASE_URL=https://fedi3.test \
FEDI3_RELAY_WS=ws://127.0.0.1:8787 \
FEDI3_RELAY_TOKEN=alice-token-0123456789abcdef \
FEDI3_ANCHOR_URL=https://fedi3.test \
FEDI3_BIND=127.0.0.1:8788 \
FEDI3_ALLOW_PRIVATE_ACTOR_FETCH=true \
./target/debug/dev_client
```

## Verifiche eseguite

1. **Risoluzione dell'identità Fedi3 da Misskey**: `POST /api/ap/show` con
   `https://fedi3.test/users/alice` → utente `alice@fedi3.test` creato correttamente.
2. **Follow Misskey → Fedi3**: `POST /api/following/create` → il core accetta e consegna
   `Accept`; `GET /users/alice/followers` riporta `totalItems: 1` (poi `2` con Mastodon).
3. **Nota Fedi3 → Misskey**: `Create` pubblicata dal core, consegnata e firmata; la nota
   compare nella timeline di Misskey come `alice@fedi3.test`.
4. **Nota Misskey → Fedi3**: dopo che alice segue l'utente Misskey, una nota pubblica viene
   consegnata all'inbox Fedi3, verificata (202) e salvata in `objects`; compare nella home
   timeline Fedi3.
5. **Follow Mastodon → Fedi3**: risoluzione via WebFinger `https`, follow accettato
   (`following: true`), `Accept` consegnato e verificato da Mastodon.
6. **Nota Fedi3 → Mastodon**: `Create` consegnata all'inbox Mastodon, firma verificata,
   visibile nella home timeline Mastodon come `alice@fedi3.test`.
7. **Nota Mastodon → Fedi3**: nota con menzione consegnata all'inbox Fedi3, verificata (202) e
   salvata in `objects`.
8. **Moderazione e spool** non sono stati ri-testati in questa sessione: coperti dagli e2e
   esistenti (`e2e_anchor_spool`, `mesh_moderation`).

## Bug trovato e corretto

Durante il primo follow da Mastodon, l'`Accept` veniva rifiutato con:

```
401 Unauthorized {"error":"Verification failed for alice@fedi3.test
https://fedi3.test/users/alice https://fedi3.test/users/alice#main-key"}
```

Causa: `Delivery::deliver_json` inseriva `Content-Type` nell'`HeaderMap` firmata e poi lo
aggiungeva una seconda volta alla richiesta reqwest. Il server riceveva due header
`Content-Type`, che Rack unisce in un unico valore
(`application/activity+json, application/activity+json`), diverso da quello firmato; la firma
risultava quindi invalida. La firma era matematicamente corretta: verificata offline con la
chiave pubblica dell'attore, ma il valore dell'header ricevuto era diverso.

Fix: rimossa l'aggiunta duplicata in `crates/fedi3_core/src/delivery.rs`; il `Content-Type`
viene inviato una sola volta, dal set di header firmati. Dopo il fix Mastodon accetta sia
`Accept` sia le note in uscita.

Il fix è coperto indirettamente dai test di consegna esistenti; non è stato aggiunto un test
automatico contro un server reale perché richiederebbe Mastodon/Misskey in CI. Il test manuale
è documentato qui ed è riproducibile con i passi sopra.

## Limiti noti emersi

- **Host con porta**: Misskey usa `domainToASCII` sul dominio e ottiene stringa vuota se il
  dominio contiene una porta; per questo gli host federati devono essere raggiungibili sulla
  porta standard (443). In produzione è normale; nei test locali serve un reverse proxy TLS.
- **Stesso host per id e inbox**: come detto, Misskey rifiuta attori con inbox su host diverso.
  L'anchor deve quindi essere pubblicato sullo stesso hostname del relay (o proxato).
- **HTTPS obbligatorio**: Mastodon assume `https` per WebFinger; il supporto `http` esiste solo
  per `.onion`. Il test locale usa una CA dedicata.
- **Node 26**: Misskey è stato eseguito con Node 26.12 (oltre la versione consigliata); nessun
  problema osservato nel test, ma per produzione conviene la versione LTS supportata.
