# Installazione del server (Debian e derivate Arch)

`scripts/install_server.sh` installa un server Fedi3 in un solo passaggio e
permette di scegliere cosa eseguire:

| Modalità | Servizi | Federazione ActivityPub | Dominio pubblico e TLS | Porte pubbliche |
| :--- | :--- | :--- | :--- | :--- |
| `full` | relay, PostgreSQL, Dragonfly, Caddy, infrastruttura P2P | sì | obbligatorio | 80/tcp, 443/tcp, 4001/tcp, 4001/udp |
| `relay` | relay, PostgreSQL, Dragonfly, Caddy | sì | obbligatorio | 80/tcp, 443/tcp |
| `p2p` | solo infrastruttura P2P (mailbox, Kademlia, circuit relay) | no | non necessario | 4001/tcp, 4001/udp |

Componenti opzionali:

- **TURN** (`coturn`): inoltra i media audio/video quando un percorso WebRTC
  diretto non è possibile. Richiede un IPv4 pubblico e le UDP
  `49152-65535`; aggiungere anche `3478/tcp` e `3478/udp`.
- **Meilisearch**: ricerca full-text più rapida per il relay. Il backend
  predefinito è PostgreSQL; Meilisearch richiede circa 1 GB di RAM in più.

L'installer supporta le derivate Debian e Ubuntu (Debian, Ubuntu, Linux Mint,
Pop!_OS, Zorin OS, Kali, Raspbian, Devuan, MX Linux, ...) e le derivate Arch
(Arch Linux, Manjaro, EndeavourOS, Garuda, CachyOS, ...). Installa Docker dai
repository ufficiali sulle distribuzioni della famiglia Debian e da `pacman`
sulle distribuzioni della famiglia Arch.

## A cosa serve ogni modalità

- **full** è il server pubblico consigliato: federa con le istanze ActivityPub
  classiche e gestisce anche l'infrastruttura P2P che conserva i messaggi per i
  peer offline e offre capacità di circuit relay.
- **relay** è pensato per chi ha bisogno solo della federazione e del tunnel
  verso il relay. I client possono comunque collegarsi direttamente tra loro;
  su questo server non c'è un nodo mailbox o circuit relay.
- **p2p** è un nodo bootstrap senza federazione. È utile per una comunità che
  vuole un punto di incontro/mailbox indipendente senza esporre un dominio
  ActivityPub né un certificato TLS.

Un server può essere cambiato di modalità in seguito: l'installer ferma i
container precedenti, conserva i volumi nominati (database del relay, media,
identità P2P) e avvia la nuova selezione.

## Requisiti

- Un VPS o un server dedicato con systemd e accesso root/sudo.
- Debian 12+ / Ubuntu 22.04+ (o una derivata compatibile) con `apt`, oppure una
  derivata Arch con `pacman`.
- Almeno 1 GB di RAM per `p2p`, 4 GB per `relay`, 6 GB consigliati per `full`
  (il file Compose applica limiti di memoria). Almeno 20 GB liberi per la prima
  build delle immagini e per i volumi di database e media.
- Per `full` e `relay`: un dominio reale con un record A/AAAA che punta al
  server, le porte 80 e 443 raggiungibili da Internet e nessun altro web server
  in ascolto su quelle porte. Caddy ottiene e rinnova il certificato tramite
  ACME.
- Per `p2p`: un indirizzo IPv4 pubblico e le porte 4001/tcp e 4001/udp
  raggiungibili.
- Aprire le stesse porte nel security group o nel firewall del provider cloud.

Il DNS deve essere configurato **prima** di avviare una modalità relay;
altrimenti Caddy non può completare la sfida ACME. L'installer avvisa se il
dominio non risolve sull'indirizzo pubblico rilevato, ma non blocca
l'installazione.

## Procedura passo passo: Debian e derivate

```bash
# 1. Aggiorna il sistema e installa git/curl
sudo apt update
sudo apt full-upgrade -y
sudo apt install -y git curl

# 2. Clona il repository nella directory di deploy
sudo git clone https://github.com/redhunt07/Fedi3.git /opt/fedi3
cd /opt/fedi3

# 3. Esegui l'installer
sudo bash scripts/install_server.sh
```

L'installer chiede, nell'ordine:

1. **Modalità**: `1` relay + P2P, `2` solo relay, `3` solo P2P.
2. **Dominio** per le modalità 1 e 2 (ad esempio `relay.example.org`). Per la
   modalità 3 il dominio è opzionale e può essere lasciato vuoto.
3. **TURN**: se installare coturn. Per le modalità relay il valore predefinito
   è sì quando viene rilevato un IPv4 pubblico.
4. **Meilisearch**: se usarlo al posto del backend di ricerca PostgreSQL.
5. **Relay seed**: URL di relay separati da virgola, opzionali, per la mesh HTTP.
6. **Conferma** del piano.

Poi:

- aggiunge il repository Docker ufficiale e installa Docker Engine, Buildx e il
  plugin Compose (oppure `docker`, `docker-compose` e `docker-buildx` su Arch);
- avvia Docker e aggiunge l'utente che esegue l'installer al gruppo `docker`;
- rileva l'IPv4 pubblico e controlla DNS, porte, RAM e disco;
- scrive `.env` e `relay-secrets.txt` con permessi `600` e segreti casuali;
- seleziona i profili Compose in `.env`, così `docker compose up -d` avvia
  esattamente la modalità scelta;
- consente le porte necessarie se `ufw` o `firewalld` sono già attivi;
- compila le immagini, avvia lo stack e attende gli health check;
- stampa l'URL del relay, il peer id e i multiaddr P2P e le impostazioni per il
  client.

La prima build delle immagini Rust può richiedere diversi minuti.

## Procedura passo passo: Arch e derivate

```bash
# 1. Aggiorna il sistema e installa git/curl
sudo pacman -Syu --needed git curl

# 2. Clona il repository nella directory di deploy
sudo git clone https://github.com/redhunt07/Fedi3.git /opt/fedi3
cd /opt/fedi3

# 3. Esegui l'installer
sudo bash scripts/install_server.sh
```

Le domande e lo stack risultante sono identici al percorso Debian. L'installer
usa `pacman` per Docker, Docker Compose e Buildx e abilita `docker.service`.

## Installazione non interattiva

Utile per automazione, Ansible o uno script cloud-init:

```bash
# Server completo (relay + P2P + TURN)
sudo bash scripts/install_server.sh \
  --mode full --domain relay.example.org --turn \
  --non-interactive --yes

# Solo relay, senza TURN
sudo bash scripts/install_server.sh \
  --mode relay --domain relay.example.org --no-turn \
  --non-interactive --yes

# Nodo bootstrap solo P2P
sudo bash scripts/install_server.sh \
  --mode p2p --non-interactive --yes
```

Aggiungi `--public-ip 203.0.113.10` quando il rilevamento dell'IP pubblico
fallisce (comune dietro alcuni NAT o VPN), `--seed-relays https://relay1.example,https://relay2.example`
per entrare in una mesh di relay e `--search meili` per abilitare Meilisearch.

`--dry-run` scrive `.env` e `relay-secrets.txt`, valida il file Compose e stampa
il piano senza avviare i container.

## File generati e segreti

- `.env` (permessi `600`): profili Compose, dominio, password del database,
  token admin e telemetria, credenziali TURN, indirizzo esterno P2P e relay
  seed. A ogni esecuzione viene conservata una copia `.env.bak.*` con data e ora.
- `relay-secrets.txt` (permessi `600`): i valori necessari ai client e agli
  operatori, tra cui token admin, token telemetria, password PostgreSQL,
  credenziali TURN, chiave Meilisearch e, dopo il primo avvio, peer id e
  multiaddr P2P.
- Volumi Docker nominati: database del relay, media, certificati Caddy e
  identità P2P. Sopravvivono a `docker compose down`; vengono eliminati solo con
  `docker compose down -v`.

Rieseguire l'installer conserva i segreti esistenti, a meno che non si passi
`--rotate-secrets`. Il token admin può essere impostato anche con
`--admin-token`.

Non committare `.env` né `relay-secrets.txt`; `.gitignore` esclude già `.env` e
`.env.*`.

## Verifica dell'installazione

Modalità relay:

```bash
cd /opt/fedi3
sudo docker compose ps
curl -fsS https://relay.example.org/healthz
curl -fsS https://relay.example.org/readyz

# Metriche admin (token in relay-secrets.txt)
curl -fsS -H "Authorization: Bearer <ADMIN_TOKEN>" \
  https://relay.example.org/_fedi3/relay/metrics.prom | head

# Smoke test end-to-end opzionale
sudo bash scripts/relay_smoke_test.sh https://relay.example.org <ADMIN_TOKEN>
```

Modalità P2P:

```bash
cd /opt/fedi3
sudo docker compose ps p2p_infra
sudo docker compose logs --tail=100 p2p_infra
sudo grep FEDI3_P2P_PEER_ID relay-secrets.txt
sudo grep FEDI3_P2P_MULTIADDRS relay-secrets.txt
```

Se Caddy non ottiene il certificato, controlla DNS, porte 80/443 e
`docker compose logs --tail=200 caddy`. Per un test locale con dominio
`localhost`, Caddy usa una CA interna: usare `curl -k`.

## Collegare l'app

### Modalità relay (`full` e `relay`)

La registrazione autonoma è disabilitata per impostazione predefinita, quindi
registra l'utente prima di aspettarti che il tunnel del relay si colleghi:

1. Nel primo avvio usa:
   - URL base pubblico: `https://<tuo-dominio>`
   - WebSocket del relay: `wss://<tuo-dominio>`
   - token relay: almeno 16 caratteri (l'app può generarne uno; conservalo).
2. Dopo che il profilo esiste, apri **Impostazioni → Rete → Relay admin** e
   incolla il token admin da `relay-secrets.txt`.
3. Registra lo stesso nome utente con il token relay usato nel primo avvio. Il
   token admin serve solo per l'amministrazione; il token utente appartiene
   all'account.
4. Il tunnel si collega dopo la registrazione. Se il relay non è raggiungibile,
   controlla `wss://<tuo-dominio>` e il token registrato in
   **Impostazioni → Sicurezza**.

### Bootstrap P2P (`full` e `p2p`)

Nell'app vai in **Impostazioni → Rete → Nodi bootstrap Fedi3 indipendenti** e
aggiungi i multiaddr stampati dall'installer (sono anche in
`relay-secrets.txt` come `FEDI3_P2P_MULTIADDRS`), ad esempio:

```text
/ip4/203.0.113.10/tcp/4001/p2p/12D3KooW...
/ip4/203.0.113.10/udp/4001/quic-v1/p2p/12D3KooW...
```

Un server solo relay non esegue un nodo di infrastruttura P2P; i client possono
comunque usare connessioni P2P dirette o un altro nodo bootstrap.

### TURN

Se TURN è abilitato, usa i valori in `relay-secrets.txt` nella configurazione
ICE dell'app:

```text
turn:<tuo-dominio>:3478?transport=udp
turn:<tuo-dominio>:3478?transport=tcp
utente: relay
credenziale: <TURN_PASS>
```

STUN non è obbligatorio quando è configurato TURN, ma i server STUN pubblici
possono comunque aiutare a trovare un percorso diretto.

## Operazioni

```bash
cd /opt/fedi3

# Stato e log
sudo docker compose ps
sudo docker compose logs --tail=200 relay
sudo docker compose logs --tail=200 caddy
sudo docker compose logs --tail=200 p2p_infra

# Riavvia un servizio
sudo docker compose restart relay

# Ferma lo stack (volumi e dati sono conservati)
sudo docker compose down

# Aggiorna il codice e ricompila
sudo git pull --ff-only
sudo docker compose up -d --build
# oppure riesegui l'installer per aggiornare .env e la selezione dei profili:
sudo bash scripts/install_server.sh
```

Cambio di modalità:

```bash
sudo bash scripts/install_server.sh --mode p2p --no-turn
```

L'installer ferma i container della modalità precedente e avvia la nuova.
I volumi nominati sono conservati, quindi tornando indietro si ritrovano
database e media del relay.

### Backup

Il repository include gli script di backup di PostgreSQL e dei media:

```bash
sudo bash scripts/backup_relay_pg.sh
sudo bash scripts/backup_relay_media.sh
```

Per backup giornalieri, aggiungi un file cron come `/etc/cron.d/fedi3-backups`:

```cron
15 3 * * * root cd /opt/fedi3 && bash scripts/backup_relay_pg.sh
45 3 * * * root cd /opt/fedi3 && bash scripts/backup_relay_media.sh
```

Gli script applicano la retention (`BACKUP_KEEP`, predefinito 7) e scrivono in
`backups/` per impostazione predefinita. I backup non sono cifrati: proteggi la
directory o copiala su storage cifrato. Vedi
[ops_backup_restore.md](ops_backup_restore.md).

## Firewall

L'installer configura `ufw` o `firewalld` solo se uno dei due è già attivo.
Altrimenti stampa le porte e i comandi da eseguire. Usa `--enable-firewall` per
abilitare automaticamente `ufw`/`firewalld`; `ufw` viene abilitato con la porta
SSH 22 consentita, quindi verifica la tua porta SSH prima di usarlo.

Porte per modalità:

- `full`: 80/tcp, 443/tcp, 4001/tcp, 4001/udp
- `relay`: 80/tcp, 443/tcp
- `p2p`: 4001/tcp, 4001/udp
- componente TURN: 3478/tcp, 3478/udp, 49152-65535/udp

Apri sempre le stesse porte nel security group del provider cloud. La porta
8787 del relay è pubblicata solo su `127.0.0.1` e non deve essere aperta.

## Diagnostica

| Sintomo | Causa e soluzione |
| :--- | :--- |
| `Unsupported distribution` | L'installer supporta solo derivate Debian/Ubuntu e Arch. Installa Docker manualmente ed esegui con `--skip-docker-install`. |
| `Docker Compose >= 2.20 is required` | Aggiorna `docker-compose-plugin` (Debian) o `docker-compose` (Arch); i profili e le dipendenze `required: false` richiedono una versione recente di Compose. |
| `apt-get update` fallisce dopo l'aggiunta del repository Docker | Alcune derivate riportano un codename che Docker non pubblica. L'installer mappa i codename Debian sconosciuti su `bookworm`; se fallisce ancora, segui le istruzioni ufficiali di Docker e riesegui con `--skip-docker-install`. |
| Avviso DNS / errore ACME | Crea prima il record A/AAAA, attendi la propagazione, verifica che le porte 80/443 siano raggiungibili e non occupate da un altro server. |
| Porta già in uso | Ferma il web server o il container esistente sulla porta 80/443, poi riesegui l'installer. |
| `permission denied` su Docker | Esci e rientra dopo che l'installer ha aggiunto il tuo utente al gruppo `docker`, oppure usa `sudo docker`. |
| Nodo P2P non healthy | Controlla `docker compose logs p2p_infra`; non eliminare il volume dell'identità P2P. Serve un IPv4 pubblico per multiaddr raggiungibili. |
| TURN non healthy | Verifica `TURN_EXTERNAL_IP` in `.env`, l'IPv4 pubblico e che l'intervallo UDP sia aperto. |
| Segreti persi | Sono in `relay-secrets.txt`; ripristina da un backup o ruota con `--rotate-secrets`. |

Per la diagnostica specifica del relay vedi [deploy_relay.md](deploy_relay.md) e
[troubleshooting.md](troubleshooting.md).

## Note di sicurezza

- Mantieni `relay-secrets.txt` e `.env` con permessi `600`; conservali in un
  backup separato dal database.
- La registrazione autonoma è disabilitata per impostazione predefinita
  (`FEDI3_RELAY_ALLOW_SELF_REGISTER=false`): un amministratore registra gli
  utenti dall'app.
- Il relay è raggiungibile solo tramite Caddy; PostgreSQL, Dragonfly e
  Meilisearch restano sulla rete Docker interna.
- Mantieni il sistema aggiornato e ricompila dopo un `git pull`.
- TLS, HSTS e limiti sul body sono applicati da Caddy; per impostazione
  predefinita non c'è un WAF/CDN a monte.

## Riferimento dell'installer

| Flag | Effetto |
| :--- | :--- |
| `--mode full\|relay\|p2p` | Seleziona la modalità di installazione. |
| `--domain DOMAIN` | Dominio pubblico del relay. Obbligatorio per `full`/`relay`. |
| `--turn` / `--no-turn` | Abilita o disabilita coturn. |
| `--search db\|meili` | Backend di ricerca. |
| `--seed-relays LIST` | URL di relay separati da virgola per la mesh. |
| `--public-ip IP` | Sovrascrive il rilevamento dell'IPv4 pubblico. |
| `--dir PATH` | Directory di deploy (predefinita: il checkout, altrimenti `/opt/fedi3`). |
| `--admin-token TOKEN` | Riutilizza un token admin di almeno 32 caratteri. |
| `--rotate-secrets` | Rigenera tutti i segreti invece di riusare quelli in `.env`. |
| `--non-interactive` | Nessuna domanda; le opzioni omesse usano i valori predefiniti. |
| `--yes` | Salta la conferma. |
| `--skip-docker-install` | Non installare Docker; fallisce se manca. |
| `--skip-firewall` | Non modificare il firewall dell'host. |
| `--enable-firewall` | Abilita `ufw`/`firewalld` se installato ma inattivo. |
| `--skip-dns-check` | Ignora gli avvisi di mismatch DNS. |
| `--dry-run` | Scrive `.env`/segreti e valida Compose senza avviare. |
