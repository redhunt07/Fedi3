# Relay Fedi3 su YunoHost

YunoHost usa già nginx sulle porte 80/443 e gestisce i certificati TLS, quindi
l'installer normale di Fedi3 non deve avviare una propria istanza di Caddy. Il
wrapper `scripts/install_yunohost.sh` esegue l'installer con `--no-caddy`,
pubblica il relay in loopback attraverso una location nginx di YunoHost,
aggira l'SSO di YunoHost per il traffico ActivityPub/WebSocket, apre le porte
P2P nel firewall di YunoHost e verifica l'endpoint HTTPS pubblico.

Un'installazione `full` fa contribuire il server a Fedi3 in due modi:

- il relay entra nella mesh HTTP dei relay (`FEDI3_RELAY_SEED_RELAYS`) e
  conserva post, media e traffico del relay per i suoi utenti;
- `fedi3_p2p_infra` esegue un nodo libp2p con mailbox, Kademlia e
  circuit-relay v2, così i client possono usarlo come bootstrap e i messaggi
  restano disponibili per i peer offline.

## Prerequisiti

- Un server YunoHost con accesso root/sudo.
- Un sottodominio dedicato, ad esempio `relay.example.org`, aggiunto in
  YunoHost: `yunohost domain add relay.example.org`.
- Un certificato Let's Encrypt valido per quel dominio:
  `yunohost domain cert install relay.example.org` (o l'azione equivalente
  nell'interfaccia di amministrazione YunoHost). Il record DNS deve già
  puntare al server.
- Nessun'altra app YunoHost installata sulla radice di quel sottodominio:
  Fedi3 ha bisogno di `/` per ActivityPub, WebFinger, NodeInfo, media e il
  WebSocket del relay.
- Docker e Docker Compose: l'installer aggiunge il repository Debian ufficiale
  di Docker e installa il plugin Compose. Usa `--skip-docker-install` se Docker
  è già installato e non deve essere toccato.
- IPv4 pubblico raggiungibile da Internet, porte 4001/tcp e 4001/udp aperte e,
  facoltativamente, porte TURN.

## Installazione

```bash
sudo git clone https://github.com/redhunt07/Fedi3.git /opt/fedi3
cd /opt/fedi3

# Nodo completo: relay + infrastruttura P2P
sudo bash scripts/install_yunohost.sh --domain relay.example.org --mode full
```

Il wrapper pone le stesse domande dell'installer normale (TURN, backend di
ricerca, relay seed) e poi:

1. scrive `.env` con `COMPOSE_PROFILES=relay,p2p` e
   `FEDI3_EDGE_PROXY=external`;
2. avvia relay, PostgreSQL, Dragonfly e `p2p_infra` senza Caddy;
3. scrive `/etc/nginx/conf.d/relay.example.org.d/fedi3.conf`;
4. esegue `nginx -t` e ricarica nginx;
5. apre 4001/tcp e 4001/udp nel firewall di YunoHost;
6. verifica `https://relay.example.org/healthz` e `/readyz`.

Esempio non interattivo con TURN e un relay seed:

```bash
sudo bash scripts/install_yunohost.sh \
  --domain relay.example.org --mode full \
  --seed-relays https://relay.example.net \
  --turn --non-interactive --yes
```

Opzioni utili:

- `--mode relay`: solo relay, senza nodo di infrastruttura P2P.
- `--mode p2p`: solo P2P; non configura nginx/TLS, apre solo la porta 4001.
- `--seed-relays LIST`: seed della mesh HTTP dei relay. Quando eseguono un nodo
  completo, il wrapper/l'installer scopre anche i loro multiaddr
  `/_fedi3/relay/p2p_infra` e imposta `FEDI3_P2P_BOOTSTRAP`.
- `--p2p-bootstrap LIST`: multiaddr P2P espliciti di nodi Fedi3 esistenti.
- `--public-ip IP`: sovrascrive il rilevamento dell'IP pubblico.
- `--dir PATH`: directory di deploy (predefinita: il checkout, altrimenti
  `/opt/fedi3`).
- `--dry-run`: scrive `.env` e la configurazione nginx, valida, ma non avvia né
  ricarica.
- `--skip-firewall`: non modificare il firewall di YunoHost.

## Nginx e SSO

La location generata è:

```nginx
location / {
    # Fedi3 autentica i propri utenti e i client ActivityPub.
    access_by_lua_file /dev/null;

    proxy_pass http://127.0.0.1:8787;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_set_header X-Forwarded-Host $host;
    proxy_set_header X-Forwarded-Port $server_port;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection $connection_upgrade;
    proxy_read_timeout 3600s;
    proxy_send_timeout 3600s;
    client_body_timeout 3600s;
    client_max_body_size 64m;
    proxy_buffering off;
    proxy_request_buffering off;
    proxy_redirect off;
}
```

`access_by_lua_file /dev/null;` disabilita l'SSO di YunoHost per questa
location: i server ActivityPub classici e il tunnel WebSocket dell'app non
possono passare dal portale YunoHost. Fedi3 ha i propri token admin/utente,
quindi è intenzionale. Non aggiungere
`include conf.d/yunohost_panel.conf.inc;` qui, a meno che tu non voglia il
pannello utente YunoHost nelle pagine Fedi3.

La porta 8787 del relay è pubblicata solo su `127.0.0.1`; non esporla
direttamente.

## Entrare nella rete P2P/relay

Un nodo completo è subito utilizzabile come bootstrap dai client che aggiungono
i suoi multiaddr, ma per entrare nella DHT Fedi3 più ampia serve almeno un peer
esistente. Aggiungi relay seed o multiaddr di bootstrap espliciti:

```bash
sudo bash scripts/install_yunohost.sh \
  --domain relay.example.org --mode full \
  --seed-relays https://relay1.example.org,https://relay2.example.org
```

oppure modifica `.env`:

```dotenv
FEDI3_RELAY_SEED_RELAYS=https://relay1.example.org,https://relay2.example.org
FEDI3_P2P_BOOTSTRAP=/ip4/203.0.113.10/tcp/4001/p2p/12D3KooW...,/ip4/203.0.113.10/udp/4001/quic-v1/p2p/12D3KooW...
```

poi riavvia:

```bash
cd /opt/fedi3
sudo docker compose up -d
```

Il peer id e i multiaddr del nodo sono stampati dall'installer e salvati in
`relay-secrets.txt`:

```bash
sudo grep FEDI3_P2P_PEER_ID relay-secrets.txt
sudo grep FEDI3_P2P_MULTIADDRS relay-secrets.txt
```

Condividi questi multiaddr con gli altri operatori, così possono usare questo
nodo come bootstrap. La mesh HTTP dei relay distribuisce post/media tra i
relay; il nodo libp2p gestisce mailbox chat e circuit relay per i client.

## Verifica

```bash
cd /opt/fedi3
sudo docker compose ps
curl -fsS https://relay.example.org/healthz
curl -fsS https://relay.example.org/readyz

# Statistiche solo admin
TOKEN=$(sudo grep '^FEDI3_RELAY_ADMIN_TOKEN=' relay-secrets.txt | cut -d= -f2-)
curl -fsS -H "Authorization: Bearer $TOKEN" \
  https://relay.example.org/_fedi3/relay/stats

# Identità P2P
sudo grep -E '^FEDI3_P2P_(PEER_ID|MULTIADDRS|BOOTSTRAP)=' relay-secrets.txt .env
```

`nginx -t` e `systemctl status nginx` aiutano quando l'endpoint pubblico non
risponde. Un 502 significa che il container del relay è fermo: controlla
`sudo docker compose logs --tail=200 relay`. Un redirect al portale YunoHost
significa che manca il bypass SSO o che la location è oscurata da un'altra app
su `/`.

## Configurazione del client

Modalità relay:

1. Primo avvio: URL base pubblico `https://relay.example.org`, WebSocket del
   relay `wss://relay.example.org` e un token utente di almeno 16 caratteri.
2. **Impostazioni → Rete → Relay admin**: incolla il token admin da
   `relay-secrets.txt` e registra lo stesso nome utente con il token utente.
3. Il tunnel si collega dopo la registrazione.

Bootstrap P2P:

- **Impostazioni → Rete → Nodi bootstrap Fedi3 indipendenti**: aggiungi i
  multiaddr da `relay-secrets.txt`.

TURN, se abilitato:

```text
turn:relay.example.org:3478?transport=udp
turn:relay.example.org:3478?transport=tcp
utente: relay
credenziale: <TURN_PASS>
```

## Operazioni

```bash
cd /opt/fedi3

sudo docker compose ps
sudo docker compose logs --tail=200 relay
sudo docker compose logs --tail=200 p2p_infra
sudo docker compose down          # volumi conservati
sudo docker compose up -d --build # aggiorna/ricompila
```

Riesegui il wrapper dopo aver cambiato modalità, dominio, TURN o relay seed.
Conserva i segreti esistenti, a meno che non si passi `--rotate-secrets`. Il
cambio di modalità ferma i container precedenti e conserva i volumi nominati.

### Backup

Il backup di YunoHost non include `/opt/fedi3` né i volumi Docker per
impostazione predefinita. Salva:

- `/opt/fedi3/.env` e `/opt/fedi3/relay-secrets.txt` (permessi 600);
- i volumi `relay_data`, `caddy_data` (se Caddy è stato usato), `p2p_infra_data`
  e i media;
- facoltativamente gli script e la configurazione del repository.

Gli script `scripts/backup_relay_pg.sh` e `scripts/backup_relay_media.sh` del
progetto funzionano con lo stack Compose; aggiungili a cron o includi i volumi
nel piano di backup di YunoHost.

## Diagnostica

| Sintomo | Soluzione |
| :--- | :--- |
| `Domain ... is not configured in YunoHost` | Aggiungi prima il sottodominio: `yunohost domain add relay.example.org`. |
| Avviso certificato | Installa il certificato: `yunohost domain cert install relay.example.org`; controlla il DNS. |
| Un'altra app serve `/` | Usa un sottodominio dedicato o sposta/rimuovi l'app dalla radice. |
| `nginx -t` fallisce | Ripristina il file `.bak.*` in `/etc/nginx/conf.d/<domain>.d/`; controlla `nginx -t`. |
| Redirect al portale YunoHost | Assicurati che `access_by_lua_file /dev/null;` sia presente in `fedi3.conf`. |
| 502 Bad Gateway | Container relay fermo o non healthy: `docker compose logs relay`. |
| WebSocket non funziona | Verifica `proxy_http_version 1.1`, header `Upgrade`/`Connection` e timeout lunghi. |
| Porte bloccate | Controlla `yunohost firewall list`; consenti 4001 e le porte TURN; verifica anche il firewall del provider. |
| Nodo P2P isolato | Aggiungi multiaddr `FEDI3_P2P_BOOTSTRAP` o relay seed; verifica la raggiungibilità di 4001. |

## Disinstallazione

```bash
cd /opt/fedi3
sudo docker compose down -v   # -v elimina i volumi nominati: prima fai un backup
sudo rm -f /etc/nginx/conf.d/<domain>.d/fedi3.conf
sudo nginx -t && sudo systemctl reload nginx
```

Facoltativamente chiudi le porte con `yunohost firewall disallow Both 4001` e le
porte TURN.
