# API di sync del relay

Questa pagina documenta gli endpoint di sync esposti oggi dal relay e dal core.
Non sono un modello di lettura relay-first: i client leggono timeline, notifiche
e chat tramite l'API del core locale, mentre i relay usano questi endpoint per
scambiare note pubbliche e recuperare attività perse dagli outbox.

## Sync note tra relay peer

`GET /_fedi3/relay/sync/notes`

Endpoint pubblico usato dai relay peer per scaricare le note conservate da un
relay. Non richiede token, ma è limitato per IP client con il limite del forward
(`FEDI3_RELAY_RL_FORWARD_PER_MIN`, predefinito `240`).

Parametri di query:

| Parametro | Tipo | Note |
|---|---|---|
| `limit` | intero | Elementi per pagina, massimo `200`; predefinito `200`. |
| `since` | intero | Restituisce le note create dopo questo timestamp Unix in millisecondi. |
| `cursor` | intero | Restituisce le note create prima di questo timestamp Unix in millisecondi (dalla più recente). |

Risposta:

```json
{
  "items": [
    {
      "note": {"id": "https://relay.example.com/objects/abc", "type": "Note"},
      "created_at_ms": 1790000000000
    }
  ],
  "next": "1789990000000"
}
```

`next` è il cursore per la pagina successiva ed è presente solo quando la pagina
è piena. Se vengono inviati sia `since` sia `cursor`, prevale `since`. La mesh dei
relay usa questo endpoint per sincronizzare le note pubbliche tra relay peer;
vedere [decentralization.md](decentralization.md#3-relay-mesh).

L'endpoint è descritto in [`openapi.yaml`](../openapi.yaml) alla voce
`/_fedi3/relay/sync/notes`.

## Sync legacy degli outbox nel core

`POST /_fedi3/sync/legacy`

Chiede al core locale di scaricare gli outbox degli attori seguiti per recuperare
attività perse. È un endpoint interno: richiede il token interno del core
(`X-Fedi3-Internal`) e risponde subito `202 Accepted`, mentre il sync gira in
background.

Parametri di query:

| Parametro | Tipo | Note |
|---|---|---|
| `pages` | intero | Pagine massime di collection per attore, `1`–`40`; predefinito `6`. |
| `items` | intero | Elementi massimi per attore, `20`–`2000`; predefinito `200`. |

Il client lo chiama dal percorso di aggiornamento della timeline. Il core esegue
lo stesso sync anche ogni 300 secondi in autonomia.

## Sync pubblico degli outbox (P2P)

`GET /.fedi3/sync/outbox`

Endpoint peer-to-peer usato per replicare l'attività pubblica degli outbox.
Restituisce solo attività pubbliche; i messaggi diretti sono esclusi.

Parametri di query:

| Parametro | Tipo | Note |
|---|---|---|
| `limit` | intero | Elementi per pagina, massimo `200`; predefinito `50`. |
| `since` | intero | Restituisce attività dopo questo timestamp Unix in millisecondi; predefinito `0`. |

Risposta:

```json
{
  "items": [{"id": "https://relay.example.com/activities/abc", "type": "Create"}],
  "latest_ms": 1790000000000
}
```

## Note

- Questi endpoint sono additivi e versionati dal percorso; i client devono
  ignorare i campi JSON sconosciuti.
- La semantica dei cursori dipende dall'endpoint: il sync note del
  relay usa `created_at_ms`, l'outbox del core usa `since`/`latest_ms`.
- L'attività ActivityPub in ingresso viene scritta nel database del core prima di
  essere esposta tramite l'API del core; il relay conserva le note dei peer in
  `relay_notes`.
- Nel codice attuale non esistono gli endpoint relay `/sync/bootstrap`,
  `/sync/events` o `/sync/stream` SSE: quei modelli di lettura sono stati
  sostituiti dall'API del core e non fanno parte della superficie relay
  distribuita.
