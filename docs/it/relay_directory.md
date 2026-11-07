# Directory relay

La scoperta dei relay Fedi3 parte da una lista pubblica condivisa:
[`relays/relays.json`](../relays/relays.json). Il file vive in questo repository
ed è mantenuto tramite pull request. Non esiste un server della directory, un
database o un account da creare.

Il client legge la lista, misura ogni relay dal dispositivo dell'utente e mostra
i risultati ordinati per latenza misurata in millisecondi. Il core Rust legge la
stessa lista, così la scoperta dei relay continua a funzionare quando il relay
configurato non è raggiungibile. I relay possono anche inizializzare la propria
mesh HTTP dalla lista.

## Perché esiste la directory

Prima di questo meccanismo il client conosceva un solo relay di bootstrap.
Quando quel relay era offline o il suo certificato TLS non corrispondeva al
nome configurato, la scoperta falliva del tutto e l'app mostrava un errore di
handshake grezzo (ad esempio `TLSV1_ALERT_UNRECOGNIZED_NAME`). La directory
rimuove quel singolo punto di rottura: la lista è servita dalla CDN del
repository e ogni relay viene misurato in modo indipendente.

## Formato del file

```json
{
  "schema": "fedi3-relay-directory/v1",
  "updated_at": "2026-11-07T12:00:00Z",
  "relays": [
    {
      "url": "https://relay.example.com",
      "ws": "wss://relay.example.com",
      "name": "Example relay",
      "operator": "Alice",
      "contact": "https://example.com/@alice",
      "region": "eu",
      "added": "2026-11-07"
    }
  ]
}
```

| Campo | Obbligatorio | Note |
| --- | --- | --- |
| `url` | sì | URL base HTTPS pubblico, senza slash finale. |
| `ws` | no | URL base WebSocket (`wss://`); dedotto da `url` se assente. |
| `name` | sì | Nome breve del relay. |
| `operator` | sì | Persona o organizzazione che gestisce il relay. |
| `contact` | sì | URL o handle dove richiedere il token del relay. |
| `region` | no | Indicazione libera della regione. |
| `added` | sì | Data ISO di inserimento. |

Il token del relay non è nella directory: ogni relay rilascia i token ai propri
utenti. La directory indica solo dove i relay esistono.

## Aggiungere un relay

La guida completa è in [`relays/README.md`](../relays/README.md). In breve:

1. Rendi il relay raggiungibile pubblicamente via HTTPS con un certificato
   valido e `GET /healthz` che risponde `200 ok`.
2. Aggiungi un oggetto a `relays/relays.json` e aggiorna `updated_at`.
3. Esegui `python3 scripts/tests/test_relay_directory.py`.
4. Apri una pull request intitolata `relay: add <hostname>`.

La CI valida il file a ogni push e pull request. Il validatore impone HTTPS,
host univoci, host WebSocket coerenti, date valide e ordinamento delle voci.

## Comportamento del client

Il client Flutter:

- scarica la directory da
  `https://raw.githubusercontent.com/redhunt07/Fedi3/main/relays/relays.json`
  e ripiega sul mirror jsDelivr;
- salva l'ultima copia valida nella directory di supporto dell'applicazione,
  così la schermata iniziale può mostrare i relay anche se GitHub non è
  raggiungibile;
- ripiega su `app/assets/relays.json`, una copia della directory inclusa nella
  build, quando né la rete né la cache sono disponibili;
- misura ogni voce con `GET /healthz` e timeout di 4 s, fino a sei relay in
  parallelo;
- ordina i relay raggiungibili dalla latenza più bassa alla più alta e mantiene
  visibili i relay non raggiungibili in fondo, con il motivo del fallimento
  (host TLS non corrispondente, certificato non valido, timeout, errore HTTP,
  errore di rete);
- mostra posizione, host e latenza in millisecondi nella schermata iniziale,
  nell'onboarding e nelle impostazioni di rete;
- permette di aprire l'onboarding con un relay già selezionato; il token del
  relay va comunque inserito manualmente.

La latenza è misurata dal vivo, dal dispositivo dell'utente, ogni volta che la
lista viene aggiornata. Non viene mai scritta nel file della directory.

## Comportamento del core

Il core scarica la stessa directory durante il sync dei relay
(`relay_sync.rs`) e salva le voci nel proprio registro relay con sorgente
`directory`. L'aggiornamento avviene sull'intervallo regolare del sync relay
(300 s) ed è best-effort.

Override tramite ambiente:

```
FEDI3_RELAY_DIRECTORY_URLS=https://mirror.example.com/relays.json
```

Una lista separata da virgole sostituisce le sorgenti predefinite. Un valore
esplicitamente vuoto disattiva la scoperta dalla directory. Se la variabile non
è impostata vengono usate le sorgenti predefinite del repository.

## Comportamento del relay

Quando `FEDI3_RELAY_SEED_RELAYS` è vuoto, il relay scarica la directory
all'avvio e usa i relay elencati come seed iniziali della mesh HTTP, escludendo
sé stesso. I seed manuali hanno sempre la precedenza:

```
FEDI3_RELAY_SEED_RELAYS=https://relay-a.example.com,https://relay-b.example.com
```

Lo stesso override `FEDI3_RELAY_DIRECTORY_URLS` vale per il relay.

## Privacy e sicurezza

- Il client esegue semplici richieste HTTPS `GET` alla directory e al
  `/healthz` pubblico di ogni relay. Non invia token, nomi utente o contenuti
  dei post.
- Una voce della directory è solo un annuncio. Il client verifica comunque il
  relay con una misura e riporta i fallimenti invece di nasconderli.
- Una voce errata non può rompere l'intera lista: il parser salta gli elementi
  malformati e il validatore rifiuta la pull request in CI.
