# Real interoperability: Fedi3 ↔ Mastodon ↔ Misskey

This document describes the interoperability test performed against two real and independent
ActivityPub implementations: **Mastodon 4.7.4** and **Misskey 2026.10.1**. The goal is
to verify that a Fedi3 identity with an anchor (`anchor_actor_url`) can be followed,
verified and exchange activities with third-party federated software, both inbound and
outbound.

> This report was produced on 2026-11-07 against Mastodon 4.7.4 and Misskey 2026.10.1. The matrix can be reproduced with `scripts/ap_compat_matrix.py`; the current client is 0.11.9.

> This report was produced on 2026-11-07 against Mastodon 4.7.4 and Misskey 2026.10.1. The matrix can be reproduced with `scripts/ap_compat_matrix.py`; the current client is 0.11.9.

> Result: **test passed in both directions** with both implementations.
> The test also found a real bug in the outbound HTTP signature, fixed in
> `crates/fedi3_core/src/delivery.rs`.

## Topology used

No Docker: the services were run natively on Arch Linux with PostgreSQL 18 and
local Redis/Valkey.

| Host | Role | Internal port |
|------|-------|---------------|
| `https://fedi3.test` | anchored Fedi3 identity + AP relay | core 8788 (actor, webfinger), relay 8787 (inbox, outbox, tunnel) |
| `https://misskey.test` | real Misskey instance | 3000 |
| `https://mastodon.test` | real Mastodon instance | 3001 (web) + Sidekiq |

Nginx acts as a TLS reverse proxy for all three hosts and routes `fedi3.test` so that:

- `/.well-known/webfinger` and `GET /users/alice` are served by the core (anchored actor document);
- everything else (`/users/alice/inbox`, `/users/alice/outbox`, `/_fedi3/tunnel`, …) is served by the relay.

This is the important architectural point that emerged from the test: **Misskey rejects an actor whose
`inbox` is not on the same host as the `id`** (`invalid Actor: inbox has different host`). In
production the anchor host must therefore proxy to the relay for the AP endpoints,
or relay and anchor must share the same hostname. A static anchor on a domain
different from the relay is not compatible with Misskey.

## Why TLS is needed

Mastodon builds the WebFinger URL as `https://<domain>/.well-known/webfinger` (only
exception: `.onion` domains). For the local test a local CA was therefore generated,
installed in the system trust store (`/etc/ca-certificates/trust-source/anchors/` +
`update-ca-trust`), and a certificate with SAN for `fedi3.test`, `misskey.test`, `mastodon.test`.
The core and the relay use `rustls-tls-native-roots`, so they respect the system trust store
(necessary for internal/enterprise CAs; in production normal public CAs are used).

To make the local certificates accepted by the runtimes:

- Node/Misskey: `NODE_EXTRA_CA_CERTS=/path/ca.crt`;
- Ruby/Mastodon: updated system trust store;
- Mastodon: `ALLOWED_PRIVATE_ADDRESSES=127.0.0.0/8,::1/128` to allow resolution of
  loopback hosts;
- Misskey: `allowedPrivateNetworks: ['127.0.0.1/32', '::1/128']`.

## Instance preparation

### Misskey

```bash
# Local PostgreSQL and Redis (ports 5433 and 6380 in the test)
psql -c "CREATE USER misskey WITH PASSWORD 'misskey';" -c "CREATE DATABASE misskey OWNER misskey;"

# build from source (pnpm) and migrations
pnpm install --frozen-lockfile
pnpm build
pnpm migrate

# .config/default.yml: url https://misskey.test/, db on 127.0.0.1:5433, redis on 127.0.0.1:6380
NODE_EXTRA_CA_CERTS=/path/ca.crt pnpm start
```

On first installation Misskey disables registration (`disableRegistration: true`) and
requires a first administrator user: in the test it was enabled via DB
(`UPDATE meta SET "disableRegistration" = false; UPDATE meta SET federation = 'all';`) and
the user was created with `POST /api/signup` (the response already contains a token).

### Mastodon

```bash
# Ruby 3.4 + yarn 4 (corepack) + PostgreSQL 18 + Redis
bundle install
RAILS_ENV=production bundle exec rails db:schema:load
yarn install --immutable
RAILS_ENV=production bundle exec rails assets:precompile
RAILS_ENV=production bundle exec bin/tootctl accounts create feditester \
  --email=feditester@gmail.com --confirmed
RAILS_ENV=production bundle exec rails s -b 127.0.0.1 -p 3001
RAILS_ENV=production bundle exec sidekiq
```

In `.env.production`: `LOCAL_DOMAIN=mastodon.test`, `WEB_DOMAIN=mastodon.test`,
`LOCAL_HTTPS=true`, `ALLOWED_PRIVATE_ADDRESSES=127.0.0.0/8,::1/128`, plus
`SECRET_KEY_BASE`, `OTP_SECRET`, `ACTIVE_RECORD_ENCRYPTION_*` and `VAPID_*` keys.
The account must be approved (`account.user.approve!`) before it can follow remote accounts.

### Fedi3

```bash
# relay
FEDI3_RELAY_DB=/tmp/fedi3/relay.db \
FEDI3_RELAY_PUBLIC_URL=https://fedi3.test \
FEDI3_RELAY_BASE_DOMAIN=fedi3.test \
FEDI3_RELAY_ALLOW_SELF_REGISTER=true \
FEDI3_RELAY_ALLOW_PRIVATE_ACTOR_FETCH=true \
FEDI3_ALLOW_PRIVATE_REMOTE_FETCH=true \
FEDI3_RELAY_ADMIN_TOKEN=<token 32+ characters> \
./target/debug/fedi3_relay

# user registration
curl -X POST http://127.0.0.1:8787/register \
  -H 'Content-Type: application/json' \
  -d '{"username":"alice","token":"alice-token-0123456789abcdef"}'

# core with anchored identity
XDG_DATA_HOME=/tmp/fedi3/data \
FEDI3_USER=alice FEDI3_DOMAIN=fedi3.test \
FEDI3_BASE_URL=https://fedi3.test \
FEDI3_RELAY_WS=ws://127.0.0.1:8787 \
FEDI3_RELAY_TOKEN=alice-token-0123456789abcdef \
FEDI3_ANCHOR_URL=https://fedi3.test \
FEDI3_BIND=127.0.0.1:8788 \
FEDI3_ALLOW_PRIVATE_ACTOR_FETCH=true \
./target/debug/dev_client
```

## Checks performed

1. **Resolution of the Fedi3 identity from Misskey**: `POST /api/ap/show` with
   `https://fedi3.test/users/alice` → user `alice@fedi3.test` created correctly.
2. **Misskey → Fedi3 follow**: `POST /api/following/create` → the core accepts and delivers
   `Accept`; `GET /users/alice/followers` reports `totalItems: 1` (then `2` with Mastodon).
3. **Fedi3 → Misskey note**: `Create` published by the core, delivered and signed; the note
   appears in the Misskey timeline as `alice@fedi3.test`.
4. **Misskey → Fedi3 note**: after alice follows the Misskey user, a public note is
   delivered to the Fedi3 inbox, verified (202) and saved in `objects`; it appears in the Fedi3 home
   timeline.
5. **Mastodon → Fedi3 follow**: resolution via `https` WebFinger, follow accepted
   (`following: true`), `Accept` delivered and verified by Mastodon.
6. **Fedi3 → Mastodon note**: `Create` delivered to the Mastodon inbox, signature verified,
   visible in the Mastodon home timeline as `alice@fedi3.test`.
7. **Mastodon → Fedi3 note**: note with mention delivered to the Fedi3 inbox, verified (202) and
   saved in `objects`.
8. **Moderation and spool** were not re-tested in this session: covered by the existing
   e2e tests (`e2e_anchor_spool`, `mesh_moderation`).

## Bug found and fixed

During the first follow from Mastodon, the `Accept` was rejected with:

```
401 Unauthorized {"error":"Verification failed for alice@fedi3.test
https://fedi3.test/users/alice https://fedi3.test/users/alice#main-key"}
```

Cause: `Delivery::deliver_json` inserted `Content-Type` into the signed `HeaderMap` and then
added it a second time to the reqwest request. The server received two
`Content-Type` headers, which Rack merges into a single value
(`application/activity+json, application/activity+json`), different from the signed one; the signature
was therefore invalid. The signature was mathematically correct: verified offline with the actor's
public key, but the value of the received header was different.

Fix: removed the duplicated addition in `crates/fedi3_core/src/delivery.rs`; the `Content-Type`
is sent only once, from the signed header set. After the fix Mastodon accepts both
`Accept` and outbound notes.

The fix is covered indirectly by the existing delivery tests; no automatic test against a real
server was added because it would require Mastodon/Misskey in CI. The manual test
is documented here and is reproducible with the steps above.


## AP endpoint proxy: identity separated from the relay

The limit "Misskey rejects an actor with `inbox` on a host different from the `id`" is solved
**without changing the model**: the identity host acts as a reverse proxy for the AP endpoints
(`/users/<u>/inbox`, `/users/<u>/outbox`, `/users/<u>/media/...`, `/_fedi3/tunnel`) toward the
relay, while the relay stays on a separate hostname. Classic instances see a single host
(the identity one) and accept the actor.

Configuration verified in the test:

- identity: `https://fedi3.test` (actor document and WebFinger served by the core);
- relay: `https://relay.fedi3.test` (transport, media, tunnel), publicly reachable;
- nginx on `fedi3.test`: `/.well-known/webfinger` and `GET /users/alice` → core (8788); everything
  else → relay (8787);
- the actor document keeps publishing `id`, `inbox`, `outbox` and `media` on the identity
  host, so the identity/transport split is maintained and the relay host does not appear
  in the actor URLs.

With this configuration, after restarting the relay on `relay.fedi3.test`:

- the resolution of `alice@fedi3.test` from Misskey and Mastodon keeps working;
- follow, `Accept`, notes and media are delivered through the identity host (proxy) and
  verified by the two instances;
- the Misskey and Mastodon home timeline shows the notes published by Fedi3.

Note: media uploaded by the core is mirrored to the relay and the returned URL uses the relay
host (`https://relay.fedi3.test/users/alice/media/...`). The same URLs are also reachable
through the identity host proxy (`https://fedi3.test/users/alice/media/...`), so a
deployment can choose to rewrite the URLs onto the identity host if it wants a single
public host.

## Media: photos, videos and audio

All requested types were tested, in both directions.

### Fedi3 → Misskey / Mastodon

Upload to `POST /users/alice/media` (internal token), then `Create` with `attachment`:

| Type | MIME | Misskey result | Mastodon result |
|------|------|----------------|-----------------|
| Photo | `image/png` | imported and displayed (`image/png`) | imported and displayed (`image`) |
| Video | `video/mp4` | imported and displayed (`video/mp4`) | imported and displayed (`gifv`) |
| Audio | `audio/mpeg` | imported and displayed (`audio/mpeg`) | imported and displayed (`audio`) |

The media URLs returned by the core are served with the correct content-type and the bytes
match the uploaded file (`200`, `image/png` / `video/mp4` / `audio/mpeg`), both via
`relay.fedi3.test` and via the `fedi3.test` proxy.

### Misskey / Mastodon → Fedi3

Upload through the instance APIs and publication with media:

| Origin | Type | Fedi3 result |
|--------|------|--------------|
| Misskey | photo `image/png` | `objects` contains `attachment` with `mediaType=image/png`, `width/height` |
| Misskey | video `video/mp4` | `attachment` with `mediaType=video/mp4` |
| Misskey | audio `audio/mpeg` | `attachment` with `mediaType=audio/mpeg` |
| Mastodon | photo `image/png` | `attachment` with `mediaType=image/png`, `blurhash`, dimensions |
| Mastodon | video `video/mp4` | `attachment` with `mediaType=video/mp4`, `duration` |
| Mastodon | audio `audio/mpeg` | `attachment` with `mediaType=audio/mpeg`, `duration` |

All 9 media URLs present in the Fedi3 timeline (3 Fedi3, 3 Misskey, 3 Mastodon) were
downloaded successfully (`200`) and with the expected content-type.

Deployment note: Mastodon serves media from `/system/` with a static alias in the reverse proxy
(not from Rails). In the test nginx was configured with
`location /system/ { alias .../public/system/; }` and with `include mime.types;`, otherwise the Mastodon media URLs
respond `404` or with the wrong content-type. This does not concern Fedi3 but must be
provided for in the proxy hosting Mastodon.

## Federation consistency

"Seen from the client" consistency check (timelines and threads of the three implementations):

- the Fedi3 federated timeline contains the public notes of Misskey and Mastodon (8 items
  observed), with correct author and content;
- the Fedi3 home timeline contains all 9 notes with media (3 Fedi3, 3 Misskey, 3 Mastodon),
  with correct `mediaType`;
- the Misskey timeline shows the notes of `alice@fedi3.test` with media re-hosted by Misskey;
- the Mastodon home timeline shows the notes of `alice@fedi3.test` with media cached by Mastodon;
- threads: a Misskey reply to a Fedi3 note arrives at Fedi3 with correct `inReplyTo`; a
  Fedi3 reply to a Mastodon note appears in the Mastodon thread with correct `in_reply_to_id` and
  in the Fedi3 thread (`/_fedi3/note/replies`).

No duplications, missing notes or unresolved media were observed in the three clients.

## Flutter client: end-to-end check

The Fedi3 client was started against the real federated stack (relay, core, Misskey,
Mastodon) with the same `alice` identity used by the dev client:

```bash
FEDI3_ALLOW_PRIVATE_ACTOR_FETCH=true \
FEDI3_DATA_DIR=/tmp/fedi3-live4/data/fedi3/dev_client \
DISPLAY=:0 WAYLAND_DISPLAY=wayland-1 \
./app/build/linux/x64/debug/bundle/fedi3
```

Note: `FEDI3_ALLOW_PRIVATE_ACTOR_FETCH=true` is needed only in local tests, where
`fedi3.test`/`misskey.test`/`mastodon.test` resolve to loopback and the core's SSRF
guard would reject the hosts (`resolves to non-public address ::1`). In production
public hosts do not require the variable. The WebSocket tunnel now uses the system
root CAs (`rustls-tls-native-roots`), so `wss://fedi3.test` works even with
a private CA installed in the trust store.

Checks performed by the client:

- **Timeline**: the four views (Home, Local, Social, Federated) open; Home and
  Local show federated notes and interactions, Federated the public notes of
  Misskey/Mastodon. Social stays empty with P2P disabled (expected behavior).
- **Publishing**: a note written by the client appears in the Mastodon timeline
  (`alice@fedi3.test`) and in the Misskey one, without duplications.
- **Nesting**: the thread opens parent and nested reply; a reply sent
  by the client arrives at Mastodon with correct `in_reply_to_id` and appears in the thread.
- **Reactions**: Like, Announce and EmojiReact from the client are persisted and federated;
  the counts update in the UI.
- **Follow/unfollow**: they work both from the follower/following list and from the profile;
  the state is consistent on the Mastodon side (`followed_by`/`following`). The
  follower and following lists are reachable from the profile counters and show
  handle, avatar and follow/unfollow action.
- **Media**: federated images and videos are rendered; the audio player
  (media_kit/mpv) is instantiated for MP3s.
- **Tabs**: Search (posts and users), Chat, Notifications, Relay and Settings
  open without errors; post/user search returns results.

Two fixes that emerged from this check are included in the code:

- search always responded `503 search coverage incomplete` on a standalone relay
  without relay peers or with an empty user index: coverage is now considered
  vacuous in those cases (unit tests in `crates/fedi3_core/src/ap.rs`);
- client-side user search did not find actors because the core index
  contains only activity objects: the client now integrates already seen actors and,
  as a fallback, the local follow/follower network.

## Known limits that emerged

- **Host with port**: Misskey uses `domainToASCII` on the domain and gets an empty string if the
  domain contains a port; for this reason federated hosts must be reachable on the
  standard port (443). In production this is normal; in local tests a TLS reverse proxy is needed.
- **Same host for id and inbox**: solved with the AP endpoint proxy described above.
  The identity host must expose and forward to the relay `inbox`, `outbox`, `media` and the tunnel.
- **HTTPS mandatory**: Mastodon assumes `https` for WebFinger; `http` support exists only
  for `.onion`. The local test uses a dedicated CA.
- **Mastodon media**: the reverse proxy of the Mastodon instance must serve `/system/` as a static
  alias and include the MIME types, otherwise federated media are unreachable.
- **Node 26**: Misskey was run with Node 26.12 (above the recommended version); no
  problem observed in the test, but for production the supported LTS version is preferable.
- **Media URLs on the relay**: the core mirrors media to the relay and returns URLs on the
  relay host; if a single public host is needed, rewrite the host of the URLs or serve the media through
  the identity proxy (both paths were verified).

## Related documents

- [decentralization.md](decentralization.md) — anchored identity and relay mesh.
- [test_plan.md](test_plan.md#12-real-interoperability-manual) — automated coverage and
  the manual interoperability gate.
- [p2p_network.md](p2p_network.md) — transport and discovery behind the client checks.
