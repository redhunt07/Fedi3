# FEDI3 Announcements

Official updates from the maintainers. This page is read directly from the Git
repository and the client checks it periodically. The Italian counterpart is
[ANNOUNCEMENTS.it.md](ANNOUNCEMENTS.it.md).

## 2026-11-07 — Relay directory and latency ranking

Relay discovery no longer depends on a single bootstrap relay:

- `relays/relays.json` is a public, Git-hosted list of relays that anyone can
  extend with a pull request; CI validates every entry.
- The client reads the list, probes each relay from the device and ranks them
  by measured latency in milliseconds, with explicit TLS, timeout and network
  failures.
- The Rust core reads the same list during relay sync, and the relay service can
  seed its HTTP mesh from it when no manual seeds are configured.
- Cached directory copies keep the first-run screen usable when GitHub is
  unreachable.

## 2026-11-07 — Security, reliability and repository update

This update hardens federation, the relay and the client without changing the
protocol:

- Federation fetches validate every redirect and cap response bodies; relay
  inbox signatures are bound to the declared actor.
- Relay: bounded database concurrency, streaming media with Range/ETag, graceful
  shutdown, atomic media quota and account deletion that removes blobs.
- Core: 4 MiB P2P frame cap, atomic inbox deduplication, stricter device
  signatures, typed dead-letter classification and bounded legacy sync.
- Client: no UI freeze when the core fails, keyring recovery screen, encrypted
  settings backup, foreground-aware polling, notification taps and preserved
  chat drafts.
- Repository: English documentation with Italian counterparts, green CI (Rust,
  Flutter, scripts and PostgreSQL), cleaned runtime data.

No protocol-breaking change: interoperability with classic ActivityPub
instances is preserved.

## 2026-11-02 — E2E calls, audio and video notes

1:1 and group audio/video calls in a P2P mesh, with screen sharing and PC audio.
Signalling travels in encrypted chat envelopes, while media goes directly
between peers over WebRTC. Recordable audio and video notes were also added to
the composer.

## 2026-11-02 — Global chat search

Search now covers all of the user's conversations, stays local to the core (no
query sent to the network), is limited to threads the user is a member of, and
searches message text only. The client opens the thread directly with the
filter active.

## 2026-11-02 — Chat: pinned messages, full search and invites

Groups can pin a shared message (owner and admins only), search now queries the
full history in the core, and groups have a copyable invite link and a join flow
from the Chat screen.

## 2026-11-02 — Chat: search and forward

Added message search inside a conversation and forwarding of a message, with
attachments, to another chat or group. Search filters only the messages already
loaded; forwarding uses the existing E2E path.

## 2026-11-02 — Telegram-style new chat

Chat creation is now a dedicated screen: pick a mutual contact (sorted by
interactions or alphabetically) and open the conversation immediately, or select
multiple people and create a named group. Groups support owner/admin/member
roles and renaming.

## 2026-11-02 — Discover and grouped notifications

The Discover tab now shows trending hashtags and suggested accounts with one-tap
follow. Repeated notifications from the same author are grouped and can be
expanded.

## 2026-11-02 — Saved posts available offline

Bookmarks now keep a local copy of the post: the new "Saved" screen (Settings or
right rail) shows them even without a network and lets you remove them with a
swipe.

## 2026-11-02 — Tags actually open posts

Fixed the tag path: counts only include tags with existing posts, names are
normalized, and clicking a hashtag opens search with the matching posts instead
of empty results.

## 2026-11-01 — Discover trending hashtags

The right rail now shows the most used hashtags in the fediverse and opens them
directly in search. Also fixed hashtag search, which previously always returned
zero results.

## 2026-11-01 — More usable notifications and composer

Notifications are now compact rows with avatars, presence status and quick
actions. The composer supports drag-and-drop files and native selection with
attachment previews; chat has a rounded Telegram-style composer with a gradient
send button.

## 2026-11-01 — Telegram-style chat and presence

Chats have bubbles with tails, avatars and delivery checks: single, double, and
double with an indicator when every member has read. Online, busy, AFK and
offline status travels between Fedi3 users even across different relays through
encrypted envelopes, expires automatically and is visible on client avatars.

## 2026-11-01 — Client restyling

New visual pass: fixed Fedi3 logo with glowing particles, aurora glow in the
rail, mesh profile banner with overlapping avatar, card-based chat list, composer
highlighted in the sidebar and more compact post actions. Search is reachable
from the timeline bar and empty states are branded.

## 2026-11-01 — Advanced composer and new client

The Fedi3 client introduces a full editor-style composer: toolbar for headings,
fonts, sizes, alignment, lists, quotes, links and media, live preview and
document statistics. Publishing remains fediverse-compatible.

- New rail with particle effects and fixed logo.
- Shortcuts Ctrl+1..7, Ctrl+N, Ctrl+F and Esc.
- Removed the quick composer: the sidebar opens the editor directly.

## 2026-10-30 — OpenAPI spec and interoperability

Published the OpenAPI 3.1 specification for core and relay
(`docs/openapi.yaml`), with 66 documented endpoints. Interoperability tests
against real Mastodon and Misskey instances remain green.

## 2026-10-25 — Security and decentralization

Anchored identity separated from transport, encrypted spool and relay mesh are
in production. The client shows core and tunnel status in real time.

---

For reports and proposals: https://github.com/redhunt07/Fedi3/issues
