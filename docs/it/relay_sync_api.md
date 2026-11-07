# Relay Sync API

API incrementali del relay per la convergenza verso backend `relay-authoritative`.

## Auth

Tutti gli endpoint richiedono:

- `Authorization: Bearer <user_token>`
- oppure token admin relay

La query deve includere `username=<local-user>`.

## Endpoints

### `GET /sync/bootstrap`

Restituisce snapshot iniziale e cursori correnti:

- `events`
- `notifications`
- `chat`
- `timeline.home`

Query principali:

- `username`
- `event_limit`
- `notification_limit`
- `chat_limit`
- `timeline_limit`

### `GET /sync/events`

Stream incrementale generico degli eventi relay-first.

Query:

- `username`
- `limit`
- `since_id`
- `cursor_id`

### `GET /sync/stream`

Stream realtime SSE relay-first. Eventi principali:

- `snapshot`
- `events`
- `timeline.home`
- `notifications`
- `chat.envelope`
- `chat.ack`
- `chat.message.deleted`
- `chat.thread.deleted`
- `object.update`
- `object.delete`
- `resync`

Header/query:

- `Authorization: Bearer ...`
- `username`
- `since_id` opzionale
- `Last-Event-ID` opzionale (resume)

### `GET /sync/timeline/home`

Read model server-side della home timeline.

Query:

- `username`
- `limit`
- `since`
- `cursor`

### `GET /sync/notifications`

Read model server-side delle notifiche.

Query:

- `username`
- `limit`
- `since_id`
- `cursor_id`

### `GET /sync/chat`

Replay incrementale degli envelope chat cifrati.

Query:

- `username`
- `limit`
- `since_id`
- `cursor_id`

### `POST /sync/chat/envelope`

Persistenza relay-side di envelope chat E2EE.

Body:

- `username`
- `thread_id`
- `message_id`
- `sender_actor`
- `sender_user` opzionale
- `recipient_users`
- `envelope`
- `created_at_ms` opzionale

### `POST /sync/chat/ack`

Ack per device dei messaggi chat.

Body:

- `username`
- `device_id`
- `message_id`
- `acked_at_ms` opzionale

### `POST /sync/chat/delete`

Soft-delete user scoped di un messaggio chat relay-first.

Body:

- `username`
- `thread_id`
- `message_id`
- `deleted_at_ms` opzionale

### `POST /sync/chat/thread/delete`

Soft-delete user scoped dell'intero thread relay-first.

Body:

- `username`
- `thread_id`
- `deleted_at_ms` opzionale

## Notes

- `event_id` e `cursor_id` sono monotoni per `relay_event_log` e `relay_chat_envelopes`.
- La home timeline usa il read model `relay_legacy_feed` e cursori basati su `inserted_at_ms`.
- Gli eventi ActivityPub in ingresso vengono scritti nel `relay_event_log` e proiettati nelle notifiche e nello stato oggetti prima del consumo client.
- Limite dedicato `/sync/*`: `FEDI3_RELAY_RL_SYNC_PER_MIN` (default `1200`).
