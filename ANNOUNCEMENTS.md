# FEDI3 Announcements

Official updates from the maintainers. This page is read directly from the Git
repository and the client checks it periodically. The Italian counterpart is
[ANNOUNCEMENTS.it.md](ANNOUNCEMENTS.it.md).

## 2026-11-08 — Android chat media, notifications and relay update

The Android client was hardened end to end on two real devices against the
public relay.

- Chat accepts photos, files, voice notes and video messages, including messages
  that carry only an attachment. Empty recordings are discarded instead of
  uploading an empty body, and media uploads no longer hit the small inbox rate
  limit.
- Video messages record real files and show a poster frame; group member rows
  keep names readable.
- Notification rows no longer duplicate, and activity, chat and call alerts keep
  their three distinct sounds while the app is closed.
- Followed Fedi3 peers are polled and backfilled into the home timeline, so posts
  from people you follow appear without a manual refresh.
- The public relay was redeployed from the current main branch; it now exposes
  `/_fedi3/relay/push-config`. UnifiedPush stays optional until an operator
  configures a gateway; the foreground service covers notifications meanwhile.
- Validation: 51 Rust tests and 189 Flutter tests passed; idle CPU stays under
  1% of a core and memory stays flat around 200 MB.

## 2026-11-08 — Preparing 0.10.0: client updates and compatible security fixes

Version `0.10.0` is being prepared and has not been published yet. Existing
clients need a manual first upgrade to receive the new updater.

- The client can check stable releases from Settings, announce newer versions
  and download packages with SHA256 verification. Linux x86_64 AppImages support
  atomic replacement with a previous-version backup; Windows x64 uses a verified
  ZIP with guided installation. Other platforms keep their installation source.
- RSA operations use AWS-LC and the Rust WebRTC transport uses the current stack.
  Existing profile keys, public keys, DIDs, signatures and encrypted messages
  remain compatible; old/new RSA and WebRTC interoperability was tested.
- Local validation: 307 Rust tests and 184 Flutter tests passed, Linux release
  builds succeeded and the dependency audit reported zero vulnerabilities with
  no ignored advisories. Five maintenance notices remain; native Windows/Apple
  runners and the remote release workflow are still part of publication.
- Client diagnostics stay on the device, with bounded logs and manual export.
  There is no automatic telemetry upload or Git push.
- Relay directory fallback, core authentication diagnostics and submillisecond
  RTT reporting were corrected.

Details: [updates](docs/updates.md), [compatibility verification](docs/audit/crypto_compatibility_2026-11-08.md),
[security policy](SECURITY.md) and [changelog](CHANGELOG.md).

## 2026-11-07 — YunoHost relay installation

`scripts/install_yunohost.sh` installs a full Fedi3 node on YunoHost without
conflicting with the existing nginx on ports 80/443. It runs the server
installer with an external TLS proxy, writes a YunoHost nginx location with
WebSocket support and SSO bypass, opens 4001/tcp and 4001/udp in the YunoHost
firewall, and verifies the public HTTPS endpoint. A full node contributes the
relay to the HTTP relay mesh and runs the libp2p mailbox/circuit-relay
infrastructure. See `docs/deploy_yunohost.md`.

## 2026-11-07 — One-command server installer

A new installer sets up a Fedi3 server on Debian/Ubuntu derivatives and Arch
derivatives. It offers three modes: relay + P2P infrastructure, relay only, or
P2P only, with optional TURN and Meilisearch. The script installs Docker,
checks DNS/ports, writes `.env` and `relay-secrets.txt` with random secrets,
selects the Compose profile, starts the stack and verifies the health checks.
See `docs/deploy_server.md`.

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
- Repository: English documentation with Italian counterparts, CI checks (Rust,
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
