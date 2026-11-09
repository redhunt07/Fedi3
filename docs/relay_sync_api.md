# Relay sync API

This page documents the sync endpoints that the relay and the core expose today.
They are not a relay-first read model: clients read timelines, notifications and
chat through the local core API, while relays use these endpoints to exchange
public notes and to pull missed outbox activity.

## Relay peer note sync

`GET /_fedi3/relay/sync/notes`

Public endpoint used by peer relays to pull the notes a relay stores. It requires
no token, but it is rate limited per client IP with the forwarding limit
(`FEDI3_RELAY_RL_FORWARD_PER_MIN`, default `240`).

Query parameters:

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | Items per page, max `200`; default `200`. |
| `since` | integer | Return notes created after this Unix timestamp in milliseconds. |
| `cursor` | integer | Return notes created before this Unix timestamp in milliseconds (newest first). |

Response:

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

`next` is the cursor for the next page and is present only when the page is full.
When both `since` and `cursor` are sent, `since` wins. The relay mesh uses this
endpoint to synchronize public notes between peer relays; see
[decentralization.md](decentralization.md#3-relay-mesh).

The endpoint is described in [`openapi.yaml`](openapi.yaml) under
`/_fedi3/relay/sync/notes`.

## Core legacy outbox sync

`POST /_fedi3/sync/legacy`

Asks the local core to pull the outboxes of followed actors for missed activity.
It is an internal endpoint: it requires the core internal token
(`X-Fedi3-Internal`) and returns `202 Accepted` immediately, while the sync runs
in the background.

Query parameters:

| Parameter | Type | Notes |
|---|---|---|
| `pages` | integer | Maximum collection pages per actor, `1`–`40`; default `6`. |
| `items` | integer | Maximum items per actor, `20`–`2000`; default `200`. |

The client calls it from the timeline refresh path. The core also runs the same
sync every 300 seconds on its own.

## Core public outbox sync (P2P)

`GET /.fedi3/sync/outbox`

Peer-to-peer endpoint used to replicate public outbox activity. It returns only
public activities; direct messages are excluded.

Query parameters:

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | Items per page, max `200`; default `50`. |
| `since` | integer | Return activity after this Unix timestamp in milliseconds; default `0`. |

Response:

```json
{
  "items": [{"id": "https://relay.example.com/activities/abc", "type": "Create"}],
  "latest_ms": 1790000000000
}
```

## Notes

- These endpoints are additive and versioned by their path; clients must ignore
  unknown JSON fields.
- Cursor semantics are endpoint-specific: relay note sync uses
  `created_at_ms`, the core outbox uses `since`/`latest_ms`.
- Incoming ActivityPub activity is written to the core database before it is
  exposed through the core API; the relay stores peer notes in `relay_notes`.
- There is no relay `/sync/bootstrap`, `/sync/events` or `/sync/stream` SSE
  endpoint in the current code base; those read models were superseded by the
  core API and are not part of the deployed relay surface.
