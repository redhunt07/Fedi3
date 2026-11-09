# Interoperabilità reale: Fedi3 ↔ Mastodon ↔ Misskey

Questo documento descrive il test di interoperabilità eseguito contro due implementazioni
ActivityPub reali e indipendenti: **Mastodon 4.7.4** e **Misskey 2026.10.1**. Lo scopo è
verificare che un'identità Fedi3 con ancoraggio (`anchor_actor_url`) possa essere seguita,
verificata e scambiare attività con software federato di terze parti, sia in ingresso sia in
uscita.

> Questo report è stato prodotto il 2026-11-07 contro Mastodon 4.7.4 e Misskey 2026.10.1. La matrice si può riprodurre con `scripts/ap_compat_matrix.py`; il client corrente è 0.11.9.

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


## Proxy degli endpoint AP: identità separata dal relay

Il limite "Misskey rifiuta un attore con `inbox` su un host diverso dall'`id`" si risolve
**senza cambiare il modello**: l'host di identità fa da reverse proxy per gli endpoint AP
(`/users/<u>/inbox`, `/users/<u>/outbox`, `/users/<u>/media/...`, `/_fedi3/tunnel`) verso il
relay, mentre il relay resta su un hostname separato. Le istanze classiche vedono un solo host
(quello dell'identità) e accettano l'attore.

Configurazione verificata nel test:

- identità: `https://fedi3.test` (documento attore e WebFinger serviti dal core);
- relay: `https://relay.fedi3.test` (trasporto, media, tunnel), raggiungibile pubblicamente;
- nginx su `fedi3.test`: `/.well-known/webfinger` e `GET /users/alice` → core (8788); tutto il
  resto → relay (8787);
- il documento attore continua a pubblicare `id`, `inbox`, `outbox` e `media` sull'host di
  identità, quindi la divisione identità/trasporto è mantenuta e l'host del relay non compare
  nelle URL dell'attore.

Con questa configurazione, dopo il riavvio del relay su `relay.fedi3.test`:

- la risoluzione di `alice@fedi3.test` da Misskey e Mastodon continua a funzionare;
- follow, `Accept`, note e media sono consegnati attraverso l'host di identità (proxy) e
  verificati dalle due istanze;
- la home timeline di Misskey e Mastodon mostra le note pubblicate da Fedi3.

Nota: i media caricati dal core vengono specchiati sul relay e la URL restituita usa l'host del
relay (`https://relay.fedi3.test/users/alice/media/...`). Le stesse URL sono raggiungibili anche
tramite il proxy dell'host di identità (`https://fedi3.test/users/alice/media/...`), quindi un
deployment può scegliere di riscrivere le URL sull'host di identità se vuole un solo host
pubblico.

## Media: foto, video e audio

Sono stati testati tutti i tipi richiesti, in entrambe le direzioni.

### Fedi3 → Misskey / Mastodon

Caricamento su `POST /users/alice/media` (token interno), poi `Create` con `attachment`:

| Tipo | MIME | Esito Misskey | Esito Mastodon |
|------|------|---------------|----------------|
| Foto | `image/png` | importata e visualizzata (`image/png`) | importata e visualizzata (`image`) |
| Video | `video/mp4` | importato e visualizzato (`video/mp4`) | importato e visualizzato (`gifv`) |
| Audio | `audio/mpeg` | importato e visualizzato (`audio/mpeg`) | importato e visualizzato (`audio`) |

Le URL dei media restituite dal core sono servite con il content-type corretto e i byte
corrispondono al file caricato (`200`, `image/png` / `video/mp4` / `audio/mpeg`), sia via
`relay.fedi3.test` sia via il proxy `fedi3.test`.

### Misskey / Mastodon → Fedi3

Upload tramite le API delle istanze e pubblicazione con media:

| Origine | Tipo | Esito Fedi3 |
|---------|------|-------------|
| Misskey | foto `image/png` | `objects` contiene `attachment` con `mediaType=image/png`, `width/height` |
| Misskey | video `video/mp4` | `attachment` con `mediaType=video/mp4` |
| Misskey | audio `audio/mpeg` | `attachment` con `mediaType=audio/mpeg` |
| Mastodon | foto `image/png` | `attachment` con `mediaType=image/png`, `blurhash`, dimensioni |
| Mastodon | video `video/mp4` | `attachment` con `mediaType=video/mp4`, `duration` |
| Mastodon | audio `audio/mpeg` | `attachment` con `mediaType=audio/mpeg`, `duration` |

Tutte le 9 URL di media presenti nella timeline Fedi3 (3 Fedi3, 3 Misskey, 3 Mastodon) sono
state scaricate con successo (`200`) e con il content-type atteso.

Nota di deployment: Mastodon serve i media da `/system/` con un alias statico nel reverse proxy
(non da Rails). Nel test nginx è stato configurato con
`location /system/ { alias .../public/system/; }` e con `include mime.types;`, altrimenti le URL
dei media Mastodon rispondono `404` o con content-type errato. Questo non riguarda Fedi3 ma va
previsto nel proxy che ospita Mastodon.

## Coerenza delle federazioni

Verifica di coerenza "vista da client" (timeline e thread delle tre implementazioni):

- la timeline federata Fedi3 contiene le note pubbliche di Misskey e Mastodon (8 elementi
  osservati), con autore e contenuto corretti;
- la home timeline Fedi3 contiene tutte le 9 note con media (3 Fedi3, 3 Misskey, 3 Mastodon),
  con `mediaType` corretti;
- la timeline Misskey mostra le note di `alice@fedi3.test` con i media re-hostati da Misskey;
- la home timeline Mastodon mostra le note di `alice@fedi3.test` con i media cachati da Mastodon;
- thread: una risposta Misskey a una nota Fedi3 arriva a Fedi3 con `inReplyTo` corretto; una
  risposta Fedi3 a una nota Mastodon compare nel thread Mastodon con `in_reply_to_id` corretto e
  nel thread Fedi3 (`/_fedi3/note/replies`).

Non sono state osservate duplicazioni, note mancanti o media non risolti nei tre client.

## Client Flutter: verifica end-to-end

Il client Fedi3 è stato avviato contro lo stack federato reale (relay, core, Misskey,
Mastodon) con la stessa identità `alice` usata dal dev client:

```bash
FEDI3_ALLOW_PRIVATE_ACTOR_FETCH=true \
FEDI3_DATA_DIR=/tmp/fedi3-live4/data/fedi3/dev_client \
DISPLAY=:0 WAYLAND_DISPLAY=wayland-1 \
./app/build/linux/x64/debug/bundle/fedi3
```

Nota: `FEDI3_ALLOW_PRIVATE_ACTOR_FETCH=true` serve solo nei test locali, dove
`fedi3.test`/`misskey.test`/`mastodon.test` risolvono su loopback e la guardia SSRF
del core rifiuterebbe gli host (`resolves to non-public address ::1`). In produzione
gli host pubblici non richiedono la variabile. Il tunnel WebSocket usa ora i root CA
di sistema (`rustls-tls-native-roots`), quindi `wss://fedi3.test` funziona anche con
una CA privata installata nel trust store.

Verifiche eseguite dal client:

- **Timeline**: le quattro viste (Home, Locale, Sociale, Federata) si aprono; Home e
  Locale mostrano le note federate e le interazioni, Federata le note pubbliche di
  Misskey/Mastodon. Sociale resta vuota con P2P disattivato (comportamento atteso).
- **Pubblicazione**: una nota scritta dal client compare nella timeline Mastodon
  (`alice@fedi3.test`) e in quella Misskey, senza duplicazioni.
- **Nidificazione**: il thread apre padre e risposta annidata; una risposta inviata
  dal client arriva a Mastodon con `in_reply_to_id` corretto e compare nel thread.
- **Reazioni**: Like, Announce e EmojiReact dal client sono persistiti e federati;
  i conteggi si aggiornano nella UI.
- **Follow/unfollow**: funzionano sia dalla lista follower/following sia dal profilo;
  lo stato è coerente sul lato Mastodon (`followed_by`/`following`). Le liste
  follower e seguiti sono raggiungibili dai contatori del profilo e mostrano
  handle, avatar e azione follow/unfollow.
- **Media**: immagini e video federati vengono renderizzati; il player audio
  (media_kit/mpv) viene istanziato per gli MP3.
- **Schede**: Cerca (post e utenti), Chat, Notifiche, Relay e Configurazioni si
  aprono senza errori; la ricerca post/utenti restituisce risultati.

Due correzioni emerse da questa verifica sono incluse nel codice:

- la ricerca rispondeva sempre `503 search coverage incomplete` su relay standalone
  senza relay peer o con indice utenti vuoto: la copertura ora è considerata
  vacua in quei casi (test unitari in `crates/fedi3_core/src/ap.rs`);
- la ricerca utenti lato client non trovava gli attori perché l'indice del core
  contiene solo oggetti attività: il client ora integra gli attori già visti e,
  come fallback, la rete follow/follower locale.

## Limiti noti emersi

- **Host con porta**: Misskey usa `domainToASCII` sul dominio e ottiene stringa vuota se il
  dominio contiene una porta; per questo gli host federati devono essere raggiungibili sulla
  porta standard (443). In produzione è normale; nei test locali serve un reverse proxy TLS.
- **Stesso host per id e inbox**: risolto con il proxy degli endpoint AP descritto sopra.
  L'host di identità deve esporre e inoltrare al relay `inbox`, `outbox`, `media` e il tunnel.
- **HTTPS obbligatorio**: Mastodon assume `https` per WebFinger; il supporto `http` esiste solo
  per `.onion`. Il test locale usa una CA dedicata.
- **Media Mastodon**: il reverse proxy dell'istanza Mastodon deve servire `/system/` come alias
  statico e includere i MIME type, altrimenti i media federati risultano non raggiungibili.
- **Node 26**: Misskey è stato eseguito con Node 26.12 (oltre la versione consigliata); nessun
  problema osservato nel test, ma per produzione conviene la versione LTS supportata.
- **URL media sul relay**: il core specchia i media sul relay e restituisce URL sull'host del
  relay; se serve un solo host pubblico, riscrivere l'host delle URL o servire i media tramite
  il proxy dell'identità (entrambe le strade sono state verificate).

## Documenti correlati

- [decentralization.md](decentralization.md) — identità ancorata e mesh dei relay.
- [test_plan.md](test_plan.md#12-interoperabilità-reale-manuale) — copertura automatica e
  gate manuale di interoperabilità.
- [p2p_network.md](p2p_network.md) — trasporto e discovery dietro le verifiche del client.
