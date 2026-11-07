# Relay Sync API

Incremental relay APIs for convergence toward `relay-authoritative` backends.

## Auth

All endpoints require:

- `Authorization: Bearer <user_token>`
- or relay admin token

The query must include `username=<local-user>`.

## Endpoints

### `GET /sync/bootstrap`

Returns the initial snapshot and current cursors:

- `events`
- `notifications`
- `chat`
- `timeline.home`

Main query parameters:

- `username`
- `event_limit`
- `notification_limit`
- `chat_limit`
- `timeline_limit`

### `GET /sync/events`

Generic incremental stream of relay-first events.

Query:

- `username`
- `limit`
- `since_id`
- `cursor_id`

### `GET /sync/stream`

Relay-first realtime SSE stream. Main events:

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
- optional `since_id`
- optional `Last-Event-ID` (resume)

### `GET /sync/timeline/home`

Server-side read model of the home timeline.

Query:

- `username`
- `limit`
- `since`
- `cursor`

### `GET /sync/notifications`

Server-side read model of notifications.

Query:

- `username`
- `limit`
- `since_id`
- `cursor_id`

### `GET /sync/chat`

Incremental replay of encrypted chat envelopes.

Query:

- `username`
- `limit`
- `since_id`
- `cursor_id`

### `POST /sync/chat/envelope`

Relay-side persistence of E2EE chat envelopes.

Body:

- `username`
- `thread_id`
- `message_id`
- `sender_actor`
- optional `sender_user`
- `recipient_users`
- `envelope`
- optional `created_at_ms`

### `POST /sync/chat/ack`

Per-device ack for chat messages.

Body:

- `username`
- `device_id`
- `message_id`
- optional `acked_at_ms`

### `POST /sync/chat/delete`

User-scoped soft-delete of a relay-first chat message.

Body:

- `username`
- `thread_id`
- `message_id`
- optional `deleted_at_ms`

### `POST /sync/chat/thread/delete`

User-scoped soft-delete of the entire relay-first thread.

Body:

- `username`
- `thread_id`
- optional `deleted_at_ms`

## Notes

- `event_id` and `cursor_id` are monotonic for `relay_event_log` and `relay_chat_envelopes`.
- The home timeline uses the `relay_legacy_feed` read model and cursors based on `inserted_at_ms`.
- Incoming ActivityPub events are written to `relay_event_log` and projected into notifications and object state before client consumption.
- Dedicated `/sync/*` limit: `FEDI3_RELAY_RL_SYNC_PER_MIN` (default `1200`).
