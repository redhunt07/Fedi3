# Deploy relay e infrastruttura P2P su VPS

Questa guida descrive il deployment attivo su `fedi3.redhunt07.ovh`
(IPv4 `51.210.42.24`). Integra l'[installazione automatica del server](deploy_server.md)
e la [configurazione manuale del relay](deploy_relay.md).

Il relay è raggiungibile via HTTPS su Caddy; il DNS pubblico è proxato da
Cloudflare (WAF gestito, TLS minimo 1.2, HSTS), mentre l'origine resta su
`51.210.42.24`. PostgreSQL, Dragonfly (compatibile Redis) e Meilisearch restano
sulla rete Docker interna. P2P e TURN sono esposti pubblicamente.

## DNS e TLS

- Record A: `fedi3.redhunt07.ovh -> 51.210.42.24`.
- Il certificato è emesso e rinnovato automaticamente da Caddy (Let's Encrypt).
- Record AAAA: `fedi3.redhunt07.ovh -> 2001:41d0:404:200::9721`, anch'esso
  proxied. I resolver pubblici vedono gli indirizzi anycast Cloudflare (per
  esempio `188.114.96.3` e `2a06:98c1:3120::3`), non l'origine.
- La zona è proxata da Cloudflare (vedi [Sicurezza](#sicurezza)); l'origine
  resta su `51.210.42.24` ed è comunque raggiungibile direttamente con SNI.
- `FEDI3_RELAY_BASE_DOMAIN` è vuoto: gli attori vivono sul dominio principale
  (`https://fedi3.redhunt07.ovh/users/<user>`). In questo modo non serve un
  wildcard DNS `*.fedi3.redhunt07.ovh`.

## Stack

Il deployment usa `docker-compose.yml` nella directory
`/home/rocky/fedi3` sul VPS:

- `caddy`: TLS e reverse proxy verso il relay.
- `relay`: servizio Fedi3 e media locali.
- `postgres`, `dragonfly`, `meilisearch`: backend interni (database, cache
  compatibile Redis e ricerca opzionale).
- `p2p_infra`: nodo libp2p con relay server, Kademlia e mailbox.
- `turn`: coturn con credenziali, IP esterno e range UDP.
- `push_gateway` e `ntfy` (profilo `push`): gateway Web Push self-hosted ed
  endpoint UnifiedPush/ntfy per le notifiche in background.

I servizi Compose sono selezionati tramite profili. Questo deployment imposta
`COMPOSE_PROFILES=relay,caddy,p2p,turn,search,push` in `.env`; senza quel
valore un normale `docker compose up -d` non avvia alcun servizio.

Porte aperte in `firewalld`: `22`, `80`, `443`, `4001/tcp`, `4001/udp`,
`3478/tcp`, `3478/udp`, `49152-65535/udp` (TURN), più `4444/tcp` per l'hub
dell'agente di supporto (vedi [Sicurezza](#sicurezza)). Solo Caddy pubblica
80/443; `relay` ascolta su `127.0.0.1:8787` e PostgreSQL/Dragonfly/Meilisearch
restano sulla rete Docker interna. `rpcbind`/porta 111 è disabilitato perché
NFS non è usato.

## Configurazione e segreti

- Configurazione: `/home/rocky/fedi3/.env` (permessi `600`).
- Token applicativi: `/home/rocky/fedi3/relay-secrets.txt` (permessi `600`).
- Non committare `.env` né `relay-secrets.txt`.
- L'utente `alice` è registrato sul relay con il token applicativo presente in
  `relay-secrets.txt`; il client locale usa lo stesso token.
- TURN usa `relay` e la password in `relay-secrets.txt`; il client locale ha
  gli stessi valori in `~/.config/fedi3/config.json`.

## Sicurezza

L'host è Rocky Linux 9 con SELinux in enforcing e `firewalld` attivo. La
postura attuale è verificata dal vivo, non presunta:

- **SSH**: solo chiave. `PasswordAuthentication no`, `PermitRootLogin no`,
  `KbdInteractiveAuthentication no`, `GSSAPIAuthentication no`,
  `MaxAuthTries 3`, `LoginGraceTime 30`. L'hardening sta in
  `/etc/ssh/sshd_config.d/00-fedi3-hardening.conf`; va tenuto
  alfabeticamente per primo, così i drop-in successivi
  (`50-cloud-init.conf`, `50-redhat.conf`) non possono riattivare la password.
  L'utente `rocky` ha sudo senza password; root non ha chiavi autorizzate.
- **Login da workstation**: la chiave privata è protetta da passphrase. Usare
  l'helper askpass (la passphrase sta in `~/.ssh/askpass_fedi3.sh`, mode 600):
  ```bash
  export SSH_ASKPASS=$HOME/.ssh/askpass_fedi3.sh SSH_ASKPASS_REQUIRE=force DISPLAY=:0
  ssh -i ~/.ssh/id_ed25519_fedi3_vps_redhunt07 rocky@51.210.42.24
  ```
- **fail2ban**: installato e attivo, **senza jail SSH per scelta**. L'IP di
  login è dinamico e SSH rifiuta già le password, quindi un ban IP
  bloccherebbe solo l'operatore. Su questo host la lista jail è
  intenzionalmente vuota; il brute force non può riuscire perché sono
  accettate solo chiavi pubbliche. Gli endpoint HTTP del relay usano token,
  non password.
- **Firewall**: solo le porte elencate in [Stack](#stack) sono aperte. Il
  socket Docker è `root:docker 660`; nessuna porta di database è pubblicata
  sull'host.
- **Hub agente di supporto**: `fedi3-agent-server` ascolta su `4444/tcp` per le
  connessioni in uscita degli agenti e su `127.0.0.1:4445` per l'admin. Il
  token dell'agente sta in `/etc/fedi3-agent.env` (mode 640 `root:rocky`). Il
  traffico è TCP in chiaro per scelta: gli agenti si connettono da reti utente
  arbitrarie e il token viene verificato su `HELLO`.
- **Aggiornamenti**: `dnf-automatic-install.timer` applica gli aggiornamenti
  di sicurezza (`upgrade_type = security`, `apply_updates = yes`).
  `reboot = never`: dopo aggiornamenti di kernel/glibc eseguire
  `sudo needs-restarting -r` e riavviare in una finestra scelta. Il relay
  riparte con l'host.
- **TLS e CDN**: il traffico pubblico passa da Cloudflare — A/AAAA proxied,
  `ssl=strict`, `min_tls_version=1.2`, TLS 1.3, ruleset WAF gestiti (Free
  Managed Ruleset + DDoS L7) e HSTS preload attivo sull'edge. L'origine Caddy
  serve TLS 1.2/1.3 con certificato Let's Encrypt; è raggiungibile anche
  direttamente su `51.210.42.24:443`, comodo per il debug ma potenziale
  bypass DDoS se l'IP viene scoperto.
- **Token API Cloudflare**: il token è valido. È un token **account-owned**,
  quindi `GET /user/tokens/verify` risponde 401 per definizione; verificalo con
  `GET /accounts/1881cf0f52c1c5acabb570f924fca5c6/tokens/verify`
  (`status: active`). Ha permessi DNS, zone settings, SSL, WAF e cache.
- **Attenzione ai test locali**: questa workstation mappa
  `51.210.42.24 fedi3.redhunt07.ovh` in `/etc/hosts`, quindi `curl`/`openssl`
  qui colpiscono l'origine e saltano Cloudflare. Togli quella riga oppure usa
  `curl --resolve fedi3.redhunt07.ovh:443:188.114.96.3` per testare il
  percorso pubblico.

## Notifiche push (opzionale)

Il profilo Compose `push` avvia `push_gateway` e `ntfy`. Questo deployment usa
hostname dedicati configurati in `.env`:

```text
FEDI3_NTFY_DOMAIN=ntfy.redhunt07.ovh
FEDI3_PUSH_DOMAIN=push.redhunt07.ovh
FEDI3_RELAY_PUSH_GATEWAY_URL=https://push.redhunt07.ovh/push
FEDI3_RELAY_PUSH_GATEWAY_TOKEN=<almeno 32 caratteri>
FEDI3_RELAY_PUSH_PROVIDERS=unifiedpush
FEDI3_PUSH_GATEWAY_PATH=/push
FEDI3_NTFY_BASE_URL=https://ntfy.redhunt07.ovh
```

Il Caddyfile contiene i site block per `{$FEDI3_NTFY_DOMAIN}` e
`{$FEDI3_PUSH_DOMAIN}`. Se le variabili non sono impostate i blocchi cadono su
`*.invalid` e non ricevono traffico, quindi un deployment senza push continua a
funzionare. VAPID è opzionale per il trasporto ntfy puro: lascia vuoto
`FEDI3_RELAY_PUSH_VAPID_PUBLIC_KEY` se non usi un distributore Web Push.

Verifica dello stack:

```bash
curl -fsS https://push.redhunt07.ovh/healthz
curl -fsS https://ntfy.redhunt07.ovh/v1/health
curl -fsS https://fedi3.redhunt07.ovh/_fedi3/relay/push-config
# wake end-to-end verso un topic ntfy (token = FEDI3_RELAY_PUSH_GATEWAY_TOKEN)
curl -s -X POST -H "Authorization: Bearer <TOKEN>" -H 'content-type: application/json' \
  -d '{"provider":"unifiedpush","token":"https://ntfy.redhunt07.ovh/e2epush","event":"sync","ttl_secs":86400}' \
  https://push.redhunt07.ovh/push
curl -s "https://ntfy.redhunt07.ovh/e2epush/json?poll=1"
```

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
curl -fsS https://fedi3.redhunt07.ovh/healthz
curl -fsS https://fedi3.redhunt07.ovh/readyz
curl -fsS https://fedi3.redhunt07.ovh/_fedi3/relay/stats

curl -fsS -H "Authorization: Bearer <ADMIN_TOKEN>" \
  https://fedi3.redhunt07.ovh/_fedi3/relay/metrics.prom | head

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
# il backup più recente deve essere di oggi o ieri
sudo ls -1t /home/rocky/backups | head -2
sudo tail -20 /var/log/fedi3-backup.log
```

Gli script devono mantenere il bit eseguibile, altrimenti cron fallisce con
`Permission denied` mentre la voce di cron sembra comunque corretta:

```bash
sudo chmod +x /home/rocky/fedi3/scripts/*.sh /home/rocky/fedi3/scripts/*.py
```

Le voci di cron invocano esplicitamente `bash ./scripts/backup_relay_*.sh`,
così un `git reset --hard` che ripristina il mode 644 del repository non può
fermare di nuovo i backup in silenzio.

Il 2026-11-07 si è verificato un buco silenzioso nei backup (script senza `+x`
dopo un aggiornamento dell'albero); controllare sempre la data del file più
recente, non solo la voce di cron.

I backup non sono cifrati: proteggere la directory o copiarla su storage
cifrato. Per un ripristino vedi [ops_backup_restore.md](ops_backup_restore.md).

## Manutenzione

- `dnf-automatic.timer` applica gli aggiornamenti di sicurezza.
- Il timezone del server è `Europe/Rome`.
- Swap: 2 GB in `/swapfile` per evitare OOM con i limiti memoria dello stack.
- I container hanno `restart: unless-stopped`, healthcheck e log rotation.
- Dopo un aggiornamento del codice: scarica il commit di release, ricompila il
  servizio interessato, `docker compose up -d`, poi smoke test. Non fidarti di
  un HEAD locale vecchio: verifica che l'albero deployato corrisponda alla
  release voluta (`git log -1 --format='%h %s'`,
  `sha256sum crates/fedi3_relay/src/main.rs`).
- `deploy/relay/Dockerfile` deve copiare la crate `fedi3_push_gateway`: dalla
  0.12.0 è un membro del workspace e un Dockerfile senza quel `COPY` fallisce
  con `failed to read /src/crates/fedi3_push_gateway/Cargo.toml`.
- Verifica che il nuovo relay risponda a `/_fedi3/account/<user>/state` con
  `401` (route presente) e non `404` (build vecchia).

## Limiti noti

- Il relay è dietro Cloudflare per il DNS pubblico (A/AAAA proxied, WAF
  gestito, TLS minimo 1.2, HSTS preload). L'IP origine `51.210.42.24` resta
  raggiungibile direttamente, quindi può essere usato per bypassare la CDN.
- Il certificato dipende dalla raggiungibilità delle porte 80/443 durante il
  rinnovo.
- Il relay è single-node: PostgreSQL e media non sono replicati.
- Le chiamate di gruppo usano una mesh P2P; oltre ~5–6 partecipanti conviene
  valutare un SFU opzionale.
