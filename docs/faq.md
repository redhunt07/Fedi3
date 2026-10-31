# FAQ

## Il relay è un server social completo?

No. Il relay fornisce compatibilità ActivityPub (actor, inbox/outbox, webfinger, nodeinfo) e routing verso i core degli utenti tramite tunnel. Non tiene le chiavi private e non è un'istanza Mastodon: è un ponte.

## Posso usare solo P2P senza relay?

Sì. Il core si avvia anche senza relay: la configurazione accetta un `relay_ws` vuoto e il tunnel viene semplicemente disattivato. In questa modalità hai chat e sincronizzazione P2P, ma **non** sei raggiungibile dalle istanze del fediverso tradizionale, che parlano solo ActivityPub su HTTPS. Per la federazione con Mastodon/Misskey serve un relay (tuo o pubblico).

## Chi vede il mio token del relay?

Nessuno tranne il relay. Il client lo invia come bearer token sul tunnel; il relay ne conserva solo l'hash SHA-256. Il token non viene mai scritto nei log.

## Dove sono salvate le chiavi private?

Nella directory dati del core, con permessi `0600` su POSIX. Non vengono mai inviate al relay: la firma avviene in locale. Fai un backup separato di quella directory se vuoi poter migrare l'account.

## La chat è cifrata?

Sì: AES-256-GCM con chiavi per messaggio, avvolte con RSA-OAEP per ogni destinatario. Limite noto: non c'è forward secrecy. Se la chiave privata viene compromessa, i messaggi già scambiati diventano leggibili.

## Telemetria?

È opzionale e attivabile dall'utente. Il relay raccoglie solo dati aggregati (numero di utenti, peer, versione) e i log locali si possono cancellare.

## Dove sono salvati i media?

Sul relay: backend locale, S3 o WebDAV, configurabile con `FEDI3_RELAY_MEDIA_BACKEND`. Il core mantiene una cache locale con limiti di spazio e TTL.

## Perché alcune richieste federate vengono rifiutate con 401?

Dal 0.1.1 il relay e il core verificano obbligatoriamente la firma HTTP su ogni POST federato, richiedono un `Digest` valido e rifiutano le attività firmate da un attore diverso da quello dichiarato. Se un'istanza remota non firma correttamente, viene rifiutata: è il comportamento corretto, non un bug.

## Infrastruttura P2P/ICE avanzata?

Sono disponibili stack opzionali in `deploy/p2p_infra` e `deploy/turn` (coturn per NAT traversal).

## Posso esporre la porta del relay direttamente?

Meglio di no. Metti il relay dietro un reverse proxy TLS e lascia `FEDI3_RELAY_BIND=0.0.0.0:8787` raggiungibile solo dalla rete interna di Docker. Se esponi la porta, abilita comunque TLS e imposta `FEDI3_RELAY_TRUST_PROXY_HEADERS` di conseguenza.

## Come faccio un backup?

Vedi [ops_backup_restore.md](ops_backup_restore.md). Gli script leggono la configurazione da `.env`, verificano l'archivio e applicano una retention. Il backup dell'app esporta solo configurazione e preferenze, non la chiave privata.
