# Rete Fedi3 e infrastrutture pubbliche

Questa pagina descrive la discovery e il trasporto P2P implementati nel core
Rust, l'infrastruttura IPFS pubblica usata solo come supporto e le impostazioni
che li controllano. Per il lato relay, vedere [decentralization.md](decentralization.md)
e [network_resilience.md](network_resilience.md).

**Stato (0.11.9):** discovery e trasporto sono implementati nel core locale; il peering tra relay è su HTTP (vedi [relay_directory.md](relay_directory.md)) e non esiste un runtime mesh libp2p dei relay. La DHT pubblica IPFS è usata solo come ausilio alla discovery, insieme agli annunci firmati tra peer Fedi3.

## Discovery implementata

Il core usa quattro sorgenti indipendenti: contatti conservati sul dispositivo,
bootstrap Fedi3 configurabili, mDNS sulla LAN e discovery sulla DHT pubblica IPFS.
I client Fedi3 collegati scambiano inoltre annunci firmati tramite
`/fedi3/peers/1`. Nessuna VPN è richiesta.

Il file `p2p_known_peers.json`, nella directory dati del profilo, contiene fino
a 256 peer con indirizzi e data dell'ultima connessione riuscita. Viene scritto
atomicamente ogni minuto e allo spegnimento, con permessi 0600 su Unix. I contatti
scadono dopo sette giorni. La riconnessione prova al massimo quattro peer ogni
30 secondi, con backoff fino a 30 minuti e sospensione dei tentativi automatici
quando ci sono già almeno 32 connessioni. Questo non limita le connessioni
necessarie alle query interne Kademlia o alle richieste dell'utente.

Lo scambio trasmette al massimo 16 annunci per messaggio e 64 KiB per frame.
Ogni annuncio lega Peer ID, indirizzi e timestamp alla chiave libp2p del nodo;
scade dopo 24 ore. Un contatto ricevuto da terzi viene ricondiviso solo dopo una
connessione al nodo annunciato. Gli indirizzi LAN, loopback, DNS e riservati non
vengono propagati via PEX; i bootstrap configurati e i contatti appresi
direttamente possono invece usare DNS e indirizzi LAN.

Gli annunci provano il controllo del Peer ID, non la titolarità di un account
ActivityPub. Non contengono account, chat, credenziali o chiavi private. Gli
indirizzi pubblici e il Peer ID sono visibili ai partecipanti alla discovery.

## IPFS pubblico come supporto

Il client contatta quattro bootstrap pubblicati dalla documentazione IPFS,
usando DNSADDR per seguirne gli indirizzi correnti. È abilitata anche la verifica
delle identità RSA usate dai bootstrap. I protocolli DHT restano separati:

- `/fedi3/kad/1`: record applicativi Fedi3, Peer ID e DID;
- `/ipfs/kad/1.0.0`: client della DHT pubblica, senza record applicativi Fedi3.

Sulla DHT pubblica il client pubblica e cerca **provider record** per il multihash
SHA-256 di `fedi3/network/providers/v1`. Questo permette a nuovi client Fedi3 di
incontrarsi senza una directory del nostro relay. È un identificatore pubblico
del servizio di rete, non il contenuto di una conversazione. Il provider record
è una candidatura alla connessione: il protocollo e l'identità del nodo vengono
verificati durante il collegamento.

Se un nodo incontrato offre Circuit Relay v2, il client può provare fino a due
prenotazioni pubbliche. Sono opportunistiche: capacità, durata e disponibilità
dipendono dal gestore. I circuit relay IPFS servono soprattutto a facilitare
il passaggio a connessioni dirette, non a offrire banda media illimitata.

Le [utilities IPFS](https://docs.ipfs.tech/concepts/public-utilities/) sono
esplicitamente offerte best effort, senza garanzia per il percorso critico di
produzione. Affiancano la rete Fedi3: non ne sostituiscono bootstrap indipendenti
e memoria dei contatti. La rete cresce quando nuovi client Fedi3 si annunciano,
si collegano e scambiano contatti; i nodi raggiungibili possono partecipare alla
DHT Fedi3. Un normale client non viene trasformato in un server TURN o in un
circuit relay per terzi.

Il servizio `fedi3_p2p_infra` aggiornato supporta il nuovo protocollo e il nome
DHT precedente per consentire l'aggiornamento progressivo. Va aggiornato insieme
ai nuovi client per mantenere il servizio dei record della rete Fedi3.

## Impostazioni

In **Impostazioni → Rete** si possono attivare/disattivare discovery IPFS e
memoria/scambio dei peer, e inserire bootstrap Fedi3 indipendenti. Queste opzioni
richiedono P2P attivo. La creazione di un nuovo profilo propone ora P2P attivo;
i profili esistenti mantengono la propria scelta.

Configurazione avanzata del core:

```json
{
  "p2p": {
    "enable": true,
    "bootstrap": [],
    "public_discovery_enable": true,
    "peer_exchange_enable": true,
    "mdns_enable": true
  }
}
```

`bootstrap` accetta fino a 32 multiindirizzi terminanti in `/p2p/<peer-id>`.
`public_bootstrap`, opzionale, sostituisce i seed IPFS predefiniti (massimo 8);
una lista vuota elimina quei seed. Per disabilitare tutta la partecipazione
alla DHT pubblica usare `public_discovery_enable: false`. Disabilitare lo scambio
ferma anche salvataggio e riconnessione automatici; non cancella il file esistente.
Le richieste esplicite di chat continuano a poter aprire connessioni.

`/_fedi3/net/metrics`, protetto dal token interno, espone sotto `p2p`:
`known_peers`, `peer_exchange_contacts` e `public_discovery_ready`. Quest'ultimo
indica una query bootstrap pubblica riuscita, non una garanzia di connettività
o disponibilità futura.

## Ridurre la dipendenza dal relay HTTP

La chat conserva nel DB fino a 256 documenti degli attori ottenuti dalla loro
origine HTTP, verificandone l'ID. Per un'ora riusa quel legame account–peer senza
richiamare il relay; per sette giorni può riusarlo se l'origine non risponde o
restituisce 5xx. I rifiuti 4xx e i documenti con identità errata non vengono
mascherati con il fallback scaduto. Un annuncio PEX non può sostituire questo
legame con una propria rivendicazione di identità.

Questo permette a contatti già conosciuti di continuare la chat diretta con il
relay HTTP indisponibile. **Non completa ancora l'indipendenza dal relay per
tutte le funzioni**: il primo contatto con un account richiede ancora la
risoluzione del profilo, e il fallback WebRTC del core Rust usa il signaling
HTTP del relay. Una discovery di rete riuscita non implica automaticamente
la risoluzione di qualunque nome utente.

## Audio/video e bridge esterni

Le chiamate usano WebRTC. Il signaling Fedi3 scambia SDP e ICE; il flusso media
ha una propria connessione. I valori automatici ICE usano gli STUN pubblici
Cloudflare e Google, senza dedurre STUN/TURN dal dominio del relay ActivityPub.
Non viene più compilata automaticamente la lista circuit relay a partire da
quel dominio. I valori esplicitamente salvati nei profili esistenti rimangono
utilizzabili; non sono una prova che il traffico passi fuori dal nostro server.

STUN aiuta a individuare un percorso diretto, ma non inoltra media. Quando quel
percorso è impossibile serve un TURN, oppure la chiamata deve fallire.

| Soluzione | Ruolo e condizioni verificate il 4 novembre 2026 |
| --- | --- |
| [Cloudflare STUN](https://developers.cloudflare.com/realtime/turn/) | Endpoint pubblico `stun:stun.cloudflare.com:3478`; aiuto alla connessione diretta, non bridge media. |
| [Open Relay di Metered](https://www.metered.ca/tools/openrelay/) | TURN utilizzabile con WebRTC, con 20 GB gratuiti al mese dichiarati. Richiede account e credenziali/API; nessuna credenziale condivisa viene incorporata in Fedi3. È un servizio ospitato, non una garanzia comunitaria di disponibilità. |

I [termini Metered](https://www.metered.ca/terms) non assegnano SLA ai piani
gratuiti. Non è stato attivato alcun account esterno.

Per usare Open Relay, ottenere i valori ICE dal proprio account e inserirli nei
campi URL ICE, utente TURN e credenziale TURN della configurazione Fedi3. Inserire
la credenziale TURN restituita dal servizio, non la chiave amministrativa API.
Il modello attuale accetta una coppia utente/credenziale condivisa dagli URL TURN
configurati; provider con credenziali diverse richiedono un modello ICE più ricco.
Il bridge esterno richiede ancora una verifica con una chiamata reale; i test
precedenti fra Alice e Peppenamir non hanno dimostrato un flusso RTP funzionante.

## Verifiche riproducibili

```bash
cargo test -p fedi3_core --lib p2p::discovery
cargo test -p fedi3_core --lib peers_exchange_signed_contacts_and_reconnect_after_restart_without_seeds
cargo test -p fedi3_core --test p2p_chat
# Prova esplicita su Internet: usa la rete IPFS pubblica, non gira nella suite ordinaria.
cargo test -p fedi3_core --lib public_ipfs_bootstrap_smoke -- --ignored --nocapture
```

La prova su Internet ha completato handshake con i bootstrap pubblici e una
query di bootstrap DHT. Le regressioni locali verificano firme, scadenze,
filtraggio degli indirizzi, contatti persistenti, riconnessione senza seed e chat
con il profilo HTTP indisponibile. Non sostituiscono una prova audio/video tra
reti diverse.


## Relay HTTP e prototipi rimossi

La rete fra relay usa peering, directory e sincronizzazione HTTP del servizio
`fedi3_relay`; il nodo `fedi3_p2p_infra` resta un servizio libp2p separato per i
client. I vecchi prototipi non compilati `relay_mesh.rs`/`relay_notes.rs` e il
modulo UPnP inattivo sono stati rimossi. Non esistono opzioni operative
`FEDI3_RELAY_MESH_*` o `upnp_*` del core.

Il compose standalone del nodo usa ancora il bind mount `deploy/p2p_infra/data`
per preservare le installazioni esistenti. Creare la directory con l'UID/GID
configurato in `P2P_UID`/`P2P_GID` prima dell'avvio. Non cambiare volume o
cancellare il keypair per correggere un errore di permessi.
