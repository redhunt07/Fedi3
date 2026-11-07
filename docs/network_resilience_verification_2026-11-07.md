# Verifica e distribuzione — 7 novembre 2026

Client locale e relay pubblico aggiornati. Backup verificati prima della sostituzione.

## Correzioni operative

- Directory pubblicata anche su `https://fedi3.redhunt07.ovh/_fedi3/relay/directory`.
  Il pulsante Scopri usa cache/copia inclusa quando tutte le fonti remote falliscono.
  Il vecchio dominio ritirato non è ripristinato e TLS resta verificato.
- SSRF: resolver condiviso core/relay filtra gli indirizzi effettivamente usati dal
  connettore, anche dopo redirect a hostname. Test verifica che localhost non sia
  contattato. Proxy HTTP ambientali disabilitati per la federazione; servizi di
  storage/ricerca configurati dall'operatore usano client separati senza redirect.
- Spool: scadenza verificata prima della consegna; quota/evizione atomica con
  transazioni SQLite e lock della riga utente in PostgreSQL. L'inbox condivisa
  offline passa dal percorso cifrato quando la chiave dell'utente è disponibile.
- Riconnessioni: una vecchia autenticazione completata in ritardo non sostituisce
  un tunnel più recente. Regressione emersa nella suite e corretta.
- Pagina pubblica `https://fedi3.redhunt07.ovh/transparency` e snapshot di politica
  firmato su `/_fedi3/relay/transparency`, senza elenchi utenti o contenuti.

## Predisposizioni e limiti

- Push: API autenticata di registrazione/revoca e contratto gateway configurabile,
  disattivati in produzione come richiesto. I connettori nativi UnifiedPush/APNs,
  i relativi callback/background handler e il gateway provider restano da implementare
  e provare su dispositivi. Nessuna garanzia di tunnel mobile permanente.
- TURN: file privato con più operatori e credenziali distinte, endpoint autenticato
  e integrazione ICE nelle chiamate Flutter. Nessun nuovo operatore comunitario o
  servizio gratuito è stato attivato. Rotazione, quote e disponibilità richiedono
  accordi con operatori reali; manca ICE restart automatico per chiamate già avviate.
- Gossip: era già presente; verificata trasmissione fra due nodi libp2p locali e
  persistenza su SQLite senza relay. Il test copre trasporto e storage del ricevente;
  non costituisce prova tra due client completi dietro NAT fisici differenti né
  di disponibilità storica completa con autore offline. Le firme del trasporto
  non equivalgono alla verifica indipendente dell'attore ActivityPub.
- Trasparenza: lo snapshot firmato rende verificabile la politica dichiarata.
  Un registro append-only dei forward, testimoni indipendenti e ricevute firmate
  dai destinatari restano da realizzare; la pagina non li presenta come esistenti.

## Verifiche

- Workspace Rust: 285 passati, 2 ignorati (bootstrap pubblico IPFS e PostgreSQL).
- PostgreSQL 16 isolato su VPS: test esplicito passato; quota media, 8 inserimenti
  spool concorrenti, password con caratteri speciali e cancellazione account.
  Container e tunnel di prova rimossi. Il database di produzione non è stato usato
  per questi test. Il primo stress con 20 richieste via SSH ha raggiunto il limite
  di attesa del dispatcher; la prova finale usa 8 scritture concorrenti.
- Flutter: suite completa 154 passati; 12 test directory ripetuti dopo l'aggiunta
  del mirror. Analyze fatal-infos e Clippy all-targets senza errori.
- Compose/OpenAPI validati; immagine candidata provata prima del deploy:
  directory, firma, retention, autorizzazione e credenziali TURN distinte,
  registrazione push disabilitata. Firma ricontrollata indipendentemente.
- Produzione: health/readiness, directory e trasparenza HTTP 200 con TLS valido;
  firma verificata; client riconnesso; chiave dell'attore uguale a quella locale.
  Relay/P2P/TURN healthy, nessun panic o errore di inizializzazione ricerca.

## Distribuzione e recupero

- AppImage: `dist/Fedi3-release-x86_64.AppImage`, in esecuzione con il nuovo core.
- Relay: immagine `fedi3-relay:network-resilience-20261107`.
- Rollback: `fedi3-relay:before-network-resilience-20261107`.
- Backup VPS: `/home/rocky/fedi3-backups/network-resilience-20261107/`;
  dump PostgreSQL verificato, configurazione privata e sorgenti precedenti.
- Backup locale cifrato: `~/.cache/fedi3-audit/alice-before-network-resilience-20261107.aesgcm`;
  chiave nel Secret Service, attributi application=fedi3,
  backup=fedi3-network-resilience-20261107. Formato nonce 12 byte + AES-GCM,
  AAD `fedi3-network-resilience-20261107`.
- Identità locale, identità P2P infra e `.env` del server preservati.
  P2P infra e TURN non richiedevano una nuova immagine e non sono stati ricreati.

Configurazione e protocolli: [network_resilience.md](network_resilience.md).

## Hash artefatti

- Core: `1746c56dc3b8eae221aba302bd34b73ac998df2236a592918bff4cdf3545db53`
- AppImage: `d9a4fc697524fc57499fc71bbf62a563b2ce714303f3b508787c2263500ba5e4`
- Relay: `359bc442425de415d4c86d969758540c1fa4b3ab0be17e32f7528bada1539993`
