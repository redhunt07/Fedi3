# Far conoscere Fedi3

Come portare Fedi3 davanti a più persone senza trasformare il fediverso in un
cartellone pubblicitario. Questo documento è il kit di lancio operativo: cosa
inviare e dove, cosa pubblicare, in quale ordine, e come capire se ha funzionato.

## Principi

1. **Il fediverso premia la partecipazione, non la pubblicità.** I post che
   rispondono a una domanda ("perché la mia istanza costa così tanto", "perché il
   telefono si prosciuga la batteria") viaggiano; gli annunci di lancio da soli no.
2. **Ogni link deve portare da qualche parte di utile.** Ora il relay serve una
   landing page su `/` (inglese) e `/it` (italiano), quindi un post può puntare a
   una pagina vera invece che a un 404.
3. **Mai promettere più di quanto c'è.** Fedi3 è pre-1.0. Diciamo cosa funziona e
   nominiamo cosa manca (nessun pacchetto iOS/macOS, nessuna forward secrecy nella
   chat, F-Droid in attesa).

## L'imbuto che controlliamo

| Passo | Asset | Dove |
| :--- | :--- | :--- |
| Landing page | `/` e `/it` servite dal binario del relay | ogni relay |
| Identità leggibile dalle macchine | `nodeinfo/2.0` con `software.name = fedi3` | ogni relay |
| Elenco dei relay pubblici | [`relays/relays.json`](../../relays/relays.json), `/_fedi3/relay/directory` | repo e relay |
| Download | release GitHub con checksum | GitHub |
| Fiducia | `/transparency`, log di forward firmato | relay |

Prima di promuovere qualcosa, verifica su un relay live:

```sh
curl -s https://<relay>/ | head -5
curl -s https://<relay>/nodeinfo/2.0
curl -s https://<relay>/_fedi3/relay/stats
```

La landing page deve restituire HTML, non 404, e `nodeinfo` deve riportare
`"name": "fedi3"` con la versione corrente della release.

## Invio alle directory

Le directory sono il posto dove la gente cerca software per il fediverso. Invia
solo metadati accurati e ricontrolla dopo ogni release.

| Directory | Cosa serve | Note |
| :--- | :--- | :--- |
| [FediDB](https://fedidb.com/) | `nodeinfo/2.0` raggiungibile | indicizza i server; il nome del software deve essere `fedi3` |
| [fediverse.observer](https://fediverse.observer/) | `nodeinfo/2.0`, nome del software | stessi metadati; elenca software e istanze |
| [fediverse.party](https://fediverse.party/) | descrizione breve, screenshot, licenza | elenco curato di progetti del fediverso |
| [awesome-selfhosted](https://github.com/awesome-selfhosted/awesome-selfhosted) | descrizione in una riga, licenza, repo | pull request; sezione "Social Networks" |
| [awesome-fediverse](https://github.com/emilebosch/awesome-fediverse) | link al repo, descrizione | pull request |
| Catalogo app YunoHost | [`packaging`](../../packaging), manifest, screenshot | la vetrina più efficace tra gli self-hoster; vedi [`docs/it/deploy_yunohost.md`](deploy_yunohost.md) |
| AUR | `packaging/aur/PKGBUILD` + `.SRCINFO` | vedi [`docs/it/distribution.md`](distribution.md) |
| Flathub | manifest Flatpak | dopo che il flusso AppImage è stabile |
| F-Droid | metadati in `packaging/fdroiddata` | oggi `Disabled` in attesa della revisione della provenienza |

Tieni una descrizione canonica in [`README.it.md`](../../README.it.md) e riusala
alla lettera: descrizioni incoerenti confondono crawler e revisori.

## Presenza nel fediverso

Il relay è infrastruttura, non pubblico. Apri un account ufficiale su un'istanza
Mastodon o Misskey già affermata (post in italiano e in inglese, o due account) e
usalo come voce del progetto.

Una cadenza sostenibile e utile:

- **Ogni settimana:** una nota tecnica (cosa è cambiato, cosa è stato misurato,
  cosa si è rotto).
- **A ogni release:** un post breve che linka la landing page e le note di
  rilascio.
- **Quando capita:** rispondi nei thread su costi delle istanze ospitate,
  moderazione, privacy, consumo di batteria, "il fediverso è solo Mastodon?" — con
  una risposta concreta, non con un link buttato lì.

Hashtag, con parsimonia: `#FediVerse #ActivityPub #FediDev #OpenSource #Rust`
`#Flutter #P2P`. I post italiani possono aggiungere `#fediverso #softwarelibero`.

Regole pratiche:

- Un solo link per post, alla fine, mai nella prima riga.
- Testo alternativo su ogni screenshot.
- Boost dei messaggi della comunità; non boostare solo i propri annunci.
- Non pubblicare lo stesso testo in cinque comunità lo stesso giorno.

## Comunità e finestre di lancio

L'ordine conta: prima sistema l'imbuto, poi semina qualche utente reale, poi
allarga.

1. **Prima le comunità italiane** — [Le Alternative](https://lealternative.org/)
   recensisce software self-hosted e federato, e le istanze italiane
   (mastodon.uno, Poliverso) hanno local timeline attive. Un progetto mantenuto
   in Italia è una storia, lì.
2. **Canali per sviluppatori del fediverso** — `#FediDev`, la mailing list
   ActivityPub, FediForum, le track sulla decentralizzazione a FOSDEM. Una
   proposta di talk vale più di dieci post.
3. **Pubblico tecnico** — Show HN, Lobsters, r/rust, r/FlutterDev,
   `!fediverse@lemmy.world`, r/fediverse. Guida con l'architettura (core locale,
   P2P, gestione delle chiavi), non con "nuovo social network".
4. **Ecosistemi di pacchetti** — AUR, Flathub, F-Droid, YunoHost: ognuno è un
   canale di scoperta permanente.

Non lanciare di lunedì mattina: una finestra tra martedì e giovedì, con un
maintainer disponibile a rispondere per qualche ora, funziona meglio.

## Testi pronti da pubblicare

### Annuncio di release (italiano, breve)

> È uscita Fedi3 0.12.4: un'app social federata in cui l'account vive in un core
> Rust locale sul tuo dispositivo. Le chiavi restano in locale, i messaggi di chat sono cifrati
> end-to-end e un livello P2P ti rende raggiungibile senza server. ActivityPub ti
> tiene collegato a Mastodon e Misskey. Linux, Windows e Android con checksum.
> Pre-1.0, e lo diciamo.
> https://fedi3.redhunt07.ovh/it — la pagina del relay ora mostra la rete in tempo reale.
> Correzioni audio/video e allegati nelle risposte ai post. I file media non sono E2EE.
> #fediverso #ActivityPub #softwarelibero

### Annuncio di release (inglese, breve)

> Fedi3 0.12.4 is out. A federated social app where the account lives in a local
> Rust core on your device: keys stay local, chat messages are end-to-end encrypted, and a
> P2P layer keeps you reachable without a server. ActivityPub keeps you
> connected to Mastodon and Misskey. Linux, Windows, Android, checksums
> published. Still pre-1.0, and honest about it.
> https://fedi3.redhunt07.ovh/
> Voice/video playback fixes and media attachments in post replies. Media files are not E2EE.
> #FediVerse #ActivityPub #OpenSource

### Show HN

> Show HN: Fedi3 – federated social with the account on your device
>
> Fedi3 is an ActivityPub client backed by a local Rust core instead of a hosted
> instance. Signing keys and E2E chat keys never leave the device; a libp2p layer
> (DHT, DCUtR, relay reservation) carries posts and chat between peers, and an
> optional relay handles federation with classic instances and offline delivery.
> It is pre-1.0, AGPL-3.0, with Linux/Windows/Android packages. I am happy to
> answer questions about the signature verification path and the relay threat
> model.

### Risposta tipo nelle comunità (usala solo se è pertinente)

> Un'istanza ospitata è un server che paghi e di cui ti fidi. Fedi3 sposta
> l'account in un core locale: il relay instrada traffico firmato ma non tiene mai
> le tue chiavi private. Parla comunque ActivityPub, quindi federа con Mastodon
> invece di sostituirlo.

## Checklist del primo mese

- [ ] Landing page verificata su entrambi i relay pubblici (`/` e `/it`).
- [ ] `nodeinfo` riporta `fedi3` e la versione corrente su entrambi i relay.
- [ ] Account ufficiale aperto su un'istanza affermata, profilo completo, post
      fissato con il link alla landing page.
- [ ] FediDB e fediverse.observer mostrano Fedi3 (ricontrolla dopo ogni release).
- [ ] Una pull request ad `awesome-selfhosted`, una ad `awesome-fediverse`.
- [ ] Invio al catalogo YunoHost preparato con screenshot e manifest.
- [ ] Pacchetto AUR pubblicato; `.SRCINFO` rigenerato.
- [ ] Un video demo (60 s) e il testo alternativo pronto per ogni screenshot.
- [ ] Note di rilascio bilingui e linkate dalla landing page.
- [ ] Rotazione delle risposte: almeno una risposta sostanziale a settimana nei
      thread del fediverso su costi di hosting, moderazione o privacy.

## Cosa misurare

Misura solo ciò che dice se l'imbuto funziona:

| Segnale | Dove | Obiettivo |
| :--- | :--- | :--- |
| Richieste alla landing page | log di accesso del relay | crescita settimana su settimana |
| Nuovi account sui relay pubblici | `/_fedi3/relay/stats` (`total_users`) | utenti reali, non iscrizioni |
| Provenienza dal fediverso | post che linkano `/` o `/it` | almeno uno per release |
| Interesse per il repository | stelle, fork e issue da sconosciuti su GitHub | le issue valgono più delle stelle |
| Presenza nelle directory | FediDB, fediverse.observer | elencato con il nome software corretto |
| Installazioni dai pacchetti | voti AUR, installazioni Flathub, presenza F-Droid | un canale nuovo al mese |

Non ottimizzare per le stelle. Un progetto con 20 stelle e 5 persone che
pubblicano davvero da Fedi3 è più sano di uno con 500 stelle e nessun utente.

## Cose da evitare

- Non chiamarlo "il nuovo Mastodon": è un client per lo stesso protocollo.
- Non promettere pacchetti iOS o macOS prima che esistano.
- Non nascondere lo stato pre-1.0: costruisce fiducia e filtra le aspettative.
- Non pubblicare lo stesso annuncio in ogni comunità lo stesso giorno.
- Non comprare pubblicità: il fediverso cresce con i boost e il passaparola.
