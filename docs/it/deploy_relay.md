# Deploy del relay (plug & play)

> Per un'installazione automatica su Debian/Ubuntu o derivate Arch, vedi
> [installazione del server](deploy_server.md). Questa guida descrive la
> configurazione manuale del relay.

## 1) Configurazione

```bash
cp .env.example .env
```

Il template imposta `COMPOSE_PROFILES=relay,p2p,caddy` (stack completo). Per una
installazione solo relay rimuovi `p2p`; su YunoHost rimuovi anche `caddy`;
aggiungi `turn` e/o `search` solo se servono coturn o Meilisearch. `scripts/install_server.sh` scrive questo valore
automaticamente.

Compila i campi essenziali:

- `FEDI3_DOMAIN=relay.example.org` — il dominio reale del relay.
- `FEDI3_RELAY_ADMIN_TOKEN=<token>` — almeno 32 caratteri (`openssl rand -hex 32`). Il relay rifiuta di partire se è più corto.
- `FEDI3_RELAY_TELEMETRY_TOKEN=<token>` — opzionale; autentica gli scambi di telemetria tra relay (`X-Fedi3-Telemetry-Token`). Non sostituisce il token admin per le metriche.
- `FEDI3_RELAY_PG_PASSWORD=<password>` — obbligatoria, il compose si ferma se manca.
- `FEDI3_RELAY_DB_DRIVER=postgres`; lasciare `FEDI3_RELAY_DB_URL` vuoto per il servizio Compose.
  L’URL viene composto da `FEDI3_RELAY_PG_USER`, `FEDI3_RELAY_PG_PASSWORD` e
  `FEDI3_RELAY_PG_DB`, con escaping dei caratteri speciali. Un URL completo è
  un override avanzato per database esterni; non duplicare la password nel template.
- Media backend (S3/WebDAV o local).
- Mesh dei relay (opzionale): `FEDI3_RELAY_SEED_RELAYS`, `FEDI3_RELAY_PEER_SYNC_SECS` e
  `FEDI3_RELAY_PEER_MAX_HOPS`; i peer si possono gestire anche da `/admin/peers`.
- `TURN_REALM`, `TURN_PASS`, `TURN_EXTERNAL_IP` (IP pubblico) — necessarie per coturn. TURN parte solo quando `turn` è aggiunto a `COMPOSE_PROFILES`.
  Con il profilo `turn` attivo, il relay pubblica anche le credenziali coturn
  agli utenti autenticati (`GET /_fedi3/relay/turn/{username}`), ricavando gli
  URL da `FEDI3_DOMAIN` e `TURN_PORT`. Si possono sovrascrivere con
  `FEDI3_RELAY_TURN_URLS`/`FEDI3_RELAY_TURN_PORT`/`FEDI3_RELAY_TURN_USERNAME`/
  `FEDI3_RELAY_TURN_CREDENTIAL`, oppure montare un pool multi-operatore con
  `FEDI3_RELAY_TURN_POOL_FILE`. Senza TURN le chiamate falliscono quando
  entrambi gli interlocutori sono dietro NAT simmetrico o VPN.
  Il container TURN riceve soltanto il proprio parametro di porta nell’ambiente,
  non le credenziali del DB o del relay; le credenziali TURN sono passate al servizio.
  Healthcheck STUN e limite memoria sono presenti anche nel compose standalone.

## 2) Avvio

```bash
docker compose up -d --build
docker compose ps
```

Il servizio `relay` ha un healthcheck: Caddy parte solo quando il relay risponde su `/healthz`. Se il relay non diventa healthy, `docker compose ps` mostra lo stato e `docker compose logs relay` la causa.

## 3) TLS / ACME

Caddy richiede un dominio reale (no `relay.example.com`).
Se ACME fallisce, controlla:

- DNS A/AAAA
- porte 80/443
- `FEDI3_DOMAIN` corretto

Log: `docker compose logs --tail=200 caddy`

## 4) Test rapido

```bash
scripts/relay_smoke_test.sh https://relay.example.org <ADMIN_TOKEN>
```

## 5) Monitoraggio

- `GET /healthz` — pubblico, nessuna autenticazione, nessun accesso al DB. Usalo per il probe di liveness.
- `GET /readyz` — pubblico; risponde 503 se il database non è raggiungibile. Usalo per il probe di readiness.
- `GET /_fedi3/relay/metrics.prom` — richiede `Authorization: Bearer <FEDI3_RELAY_ADMIN_TOKEN>`.
- `GET /_fedi3/relay/stats` — statistiche pubbliche della mesh dei relay.
- `POST /_fedi3/relay/telemetry` — telemetria tra relay; quando `FEDI3_RELAY_TELEMETRY_TOKEN` è impostato, il mittente deve includere `X-Fedi3-Telemetry-Token`.

Gli endpoint di salute non scrivono righe di audit: sono pensati per essere interrogati ogni pochi secondi.

## 6) Diagnosi

```bash
scripts/relay_diagnose.sh
```

Lo script non assume che l'immagine del relay abbia `curl` o `wget`: usa lo stato di healthcheck del container e, se sul host è disponibile un client HTTP, interroga `127.0.0.1:8787` (la porta è pubblicata solo su loopback).

## 7) Note

In produzione:

- `FEDI3_RELAY_ALLOW_SELF_REGISTER=false`
- `FEDI3_RELAY_HSTS_MAX_AGE_SECS` maggiore di zero (il default del compose è un anno)
- token admin conservato in un vault, mai in git
- non pubblicare la porta 8787 su `0.0.0.0`: il compose la espone solo su `127.0.0.1` per la diagnosi locale
- fai girare il relay dietro Caddy e lascia `FEDI3_RELAY_TRUST_PROXY_HEADERS=true` solo perché il proxy sovrascrive `X-Forwarded-For`
- tieni `.env` e `relay-secrets.txt` a mode 600 e fuori da git

### Pagina pubblica e nodeinfo

Il binario del relay serve una propria pagina pubblica, così un link pubblicato
nel fediverso non finisce su un 404:

- `GET /` — pagina in inglese;
- `GET /it` — pagina in italiano;
- `GET /en` — redirect permanente a `/`;
- `GET /assets/landing/*` — foglio di stile, banner, favicon e screenshot
  incorporati nel binario (nessun volume o mount statico necessario);
- `GET /nodeinfo/2.0` — riporta `software.name = fedi3`, la versione della
  release e `metadata.repository`, così directory come FediDB e
  fediverse.observer classificano il relay come Fedi3 invece che come prodotto
  sconosciuto.
- `GET /_fedi3/relay/live` — istantanea aggregata per la vista live: utenti
  online, peer, raggiungibilità dei relay e contatori di traffico a livello di
  processo. Non contiene username né peer id; le risposte sono
  `Cache-Control: no-store` e hanno un limite per IP.

In modalità base domain (`FEDI3_RELAY_BASE_DOMAIN`) le richieste a
`<utente>.<dominio>` continuano a essere inoltrate al core dell'utente, incluso
`/`. Non serve cambiare Caddy: il sito è già in proxy verso il relay. La pagina
non carica font esterni; la vista live usa un solo script same-origin
(`/assets/landing/live.js`) e ripiega sul JSON grezzo di
`/_fedi3/relay/live` quando JavaScript è disabilitato. La CSP documentata in
`.env.example` lo consente perché `script-src` ricade su `default-src 'self'`;
una policy più restrittiva che blocca gli script same-origin disattiva solo
l'animazione, non la pagina.

Checklist di hardening dell'host (verificata sul VPS di riferimento, vedi
[deploy_vps.md](deploy_vps.md#sicurezza) e
[deploy_yunohost.md](deploy_yunohost.md#sicurezza)):

- SSH solo chiave (`PasswordAuthentication no`, `PermitRootLogin no`,
  `MaxAuthTries 3`) con il drop-in di hardening tenuto alfabeticamente primo;
- nessun ban IP su SSH quando l'IP dell'operatore è dinamico e SSH rifiuta già
  le password; mantieni fail2ban per gli altri servizi esposti;
- il firewall consente solo 80/443, 4001, TURN e la porta dell'agente di
  supporto;
- aggiornamenti di sicurezza automatici attivi; verifica con
  `dnf needs-restarting -r` / `apt-get -s upgrade` e riavvia solo in una
  finestra scelta;
- backup pianificati **ed** eseguibili (`chmod +x scripts/*.sh`), con un
  controllo che l'archivio più recente sia di oggi o ieri;
- TLS solo 1.2+ al proxy; verifica che l'albero deployato corrisponda al
  commit di release prima di ricompilare.

## 8) Documentazione correlata

- [installazione del server](deploy_server.md) — installer automatico per Debian/Arch
- [deploy su VPS](deploy_vps.md) — deployment di riferimento su `fedi3.redhunt07.ovh`
- [backup e ripristino](ops_backup_restore.md) — backup di PostgreSQL e media
- [promozione e kit di lancio](promotion.md) — invio alle directory e presenza nel fediverso

### P2P tramite HTTPS (v0.12.5)

L'infrastruttura Compose ascolta anche sulla porta WebSocket interna 4002.
Caddy inoltra `/_fedi3/p2p` a questo listener nella rete privata Compose.
Impostare `FEDI3_P2P_INFRA_WSS_ENABLE=true` per annunciare il multiaddress WSS
con verifica del certificato sulla porta pubblica 443: i client lo preferiscono
alla porta 4001. Peer ID e volume dati restano invariati. Non pubblicare la
porta 4002 sull'host.

Con TLS terminato da un proxy esterno, incluso YunoHost, aggiungere prima la
regola equivalente per WebSocket e poi abilitare l'annuncio. Lasciare il flag
false finché l'inoltro non funziona. TCP/QUIC 4001 rimane disponibile per i
client esistenti e per le installazioni senza WSS.
