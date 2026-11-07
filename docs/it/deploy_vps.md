# Deploy relay e infrastruttura P2P su VPS

Questa guida descrive il deployment attivo su `fedi3.redhunt07.ovh`
(IPv4 `51.210.42.24`). Il relay è raggiungibile solo via HTTPS su Caddy;
PostgreSQL, Redis/Dragonfly e Meilisearch restano sulla rete Docker interna.
P2P e TURN sono esposti pubblicamente.

## DNS e TLS

- Record A: `fedi3.redhunt07.ovh -> 51.210.42.24`.
- Il certificato è emesso e rinnovato automaticamente da Caddy (Let's Encrypt).
- Non è configurato un record AAAA: il server ha IPv6, ma il dominio usa solo
  IPv4. Aggiungere `AAAA -> 2001:41d0:404:200::9721` solo dopo aver verificato
  che Caddy e il firewall siano raggiungibili su IPv6.
- `FEDI3_RELAY_BASE_DOMAIN` è vuoto: gli attori vivono sul dominio principale
  (`https://fedi3.redhunt07.ovh/users/<user>`). In questo modo non serve un
  wildcard DNS `*.fedi3.redhunt07.ovh`.

## Stack

Il deployment usa `docker-compose.yml` nella directory
`/home/rocky/fedi3` sul VPS:

- `caddy`: TLS e reverse proxy verso il relay.
- `relay`: servizio Fedi3, database PostgreSQL, media locali.
- `postgres`, `dragonfly`, `meilisearch`: backend interni.
- `p2p_infra`: nodo libp2p con relay server, Kademlia e mailbox.
- `turn`: coturn con credenziali, IP esterno e range UDP.

Porte aperte in `firewalld`: `22`, `80`, `443`, `4001/tcp`, `4001/udp`,
`3478/tcp`, `3478/udp`, `49152-65535/udp`.

## Configurazione e segreti

- Configurazione: `/home/rocky/fedi3/.env` (permessi `600`).
- Token applicativi: `/home/rocky/fedi3/relay-secrets.txt` (permessi `600`).
- Non committare `.env` né `relay-secrets.txt`.
- L'utente `alice` è registrato sul relay con il token applicativo presente in
  `relay-secrets.txt`; il client locale usa lo stesso token.
- TURN usa `relay` e la password in `relay-secrets.txt`; il client locale ha
  gli stessi valori in `~/.config/fedi3/config.json`.

## Client

Il client locale è configurato con:

- `publicBaseUrl = https://fedi3.redhunt07.ovh`
- `relayWs = wss://fedi3.redhunt07.ovh`
- ICE: `stun:fedi3.redhunt07.ovh:3478`,
  `turn:fedi3.redhunt07.ovh:3478?transport=udp`,
  `turn:fedi3.redhunt07.ovh:3478?transport=tcp`
- `p2pEnable = true`, `webrtcEnable = true`

Dopo una modifica al relay o al token, riavviare il core/client e verificare
che il log mostri `tunnel connected`.

## Operazioni

```bash
cd /home/rocky/fedi3

# stato
sudo docker compose ps
sudo docker compose logs --tail=100 relay caddy p2p_infra turn

# riavvio mirato
sudo docker compose restart relay
sudo docker compose up -d --force-recreate caddy

# smoke test pubblico
bash scripts/relay_smoke_test.sh https://fedi3.redhunt07.ovh "$(grep ADMIN_TOKEN relay-secrets.txt | cut -d= -f2)"
```

Verifiche rapide:

```bash
curl -fsS -H "Authorization: Bearer <ADMIN_TOKEN>" \
  https://fedi3.redhunt07.ovh/healthz

curl -fsS -H "Authorization: Bearer <ADMIN_TOKEN>" \
  https://fedi3.redhunt07.ovh/_fedi3/relay/stats

curl -fsS -H 'Accept: application/activity+json' \
  https://fedi3.redhunt07.ovh/users/alice
```

## Backup

Sono attivi backup giornalieri con retention dello script:

- `03:15` PostgreSQL;
- `03:45` media locali;
- destinazione `/home/rocky/backups`;
- log `/var/log/fedi3-backup.log`, con logrotate settimanale.

```bash
sudo cat /etc/cron.d/fedi3-backups
ls -lh /home/rocky/backups
```

I backup non sono cifrati: proteggere la directory o copiarla su storage
cifrato. Per un ripristino seguire `docs/ops_backup_restore.md`.

## Manutenzione

- `dnf-automatic.timer` applica gli aggiornamenti di sicurezza.
- Il timezone del server è `Europe/Rome`.
- Swap: 2 GB in `/swapfile` per evitare OOM con i limiti memoria dello stack.
- I container hanno `restart: unless-stopped`, healthcheck e log rotation.
- Dopo un aggiornamento del codice: `git pull --ff-only`, rebuild del servizio
  interessato, `docker compose up -d`, poi smoke test.

## Limiti noti

- Il relay non è ancora dietro a un WAF/CDN: Caddy applica TLS, header di
  sicurezza e limiti di body, ma non c'è protezione DDoS a monte.
- Il certificato dipende dalla raggiungibilità delle porte 80/443 durante il
  rinnovo.
- Il relay è single-node: PostgreSQL e media non sono replicati.
- Le chiamate di gruppo usano una mesh P2P; oltre ~5–6 partecipanti conviene
  valutare un SFU opzionale.
