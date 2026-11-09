# FEDI3 Announcements

Official updates from the maintainers. This page is read directly from the Git
repository and the client checks it periodically. The Italian counterpart is
[ANNOUNCEMENTS.it.md](ANNOUNCEMENTS.it.md).

## 2026-11-09 — Fedi3 0.11.9 released

Build 0.11.9 (2012) is available on the [release page](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.9) with the Linux x86_64 AppImage, the Windows x64 ZIP, the Android arm64-v8a and x86_64 APKs and SHA256 checksums. It collects the fixes published since 0.11.1: composer cleanup, single Android task, relayed P2P addresses, migrated outbox normalization, profile banners, the neon logo, continuous navigation particles and a clean Linux startup log. The Windows ZIP is built by CI from the same tag. [Changelog](CHANGELOG.md) · [Screenshots](README.md#screenshots).

## 2026-11-09 — Clean Linux startup output

The Linux client no longer prints the benign ATK critical about a missing plug
ID. Flutter's embedder calls `atk_socket_embed` before ATK can provide a plug
ID; the runner now filters only that message, so other accessibility
diagnostics remain visible. Build 0.11.9 (2012).

## 2026-11-09 — Continuous particle effects in the navigation rail

The selected section's particle field and the brand mark now animate
continuously instead of stopping after a few turns. Flutter mutes tickers while
the app is not visible, so background battery use is unchanged. Build 0.11.8
(2011).

## 2026-11-09 — Composer cleanup, presence layout and single Android task

The composer no longer repeats the file picker and MFM cheatsheet buttons; the
attachment action remains in the media area and toolbar. The presence selector
keeps all four states on one row on narrow phones. Android now uses a single
task (`singleTask`) and the keep-alive notification reuses it, so recent apps
no longer shows a second offline Fedi3 session. Build 0.11.7 (2010).

## 2026-11-09 — Android keep-alive service start fix

The keep-alive foreground service now enters the foreground from `onCreate` and
is started while the app is still in the foreground (`onPause`), so Android's
five second `startForeground` window cannot expire during a slow Flutter engine
teardown. Build 0.11.6 (2009).

## 2026-11-09 — P2P relayed addresses and migrated outbox authorship

Cores now reserve a circuit on the relay's P2P infrastructure and publish the
relayed address in the actor document, so peers behind NAT can dial each other
directly through the relay. Migrated outbox activities are rewritten to the
current actor in the core and in the relay proxy, so classic instances see the
right author. The Alice → RedHunt07 follow is accepted in the current test
accounts, so followers-only posts are delivered too.

## 2026-11-09 — Timelines: federated posts and profile consistency

Public posts from accounts whose follow request is still pending are now polled
into the federated timeline instead of being ignored until acceptance. Profile
pages keep only the posts authored by that profile and merge the public posts
the local core already cached, so a migrated account no longer shows another
identity's posts. The outbox of a migrated account rewrites the authorship of
old activities to the current actor. The Android build is 0.11.5 (2008).
[Docs](docs/app_guide.md).

## 2026-11-09 — Profile banners sit behind the avatar

Profile banners now fill the profile header behind the avatar instead of a
separate card below it. The default gradient is only used when the account has
no banner image. The Android build is 0.11.5 (2008).
[Docs](docs/app_guide.md).

## 2026-11-09 — Fedi3 0.11.4: new logo and app icons

The Fedi3 mark is now the neon retro F over an orbital globe. It replaces the
placeholder hub mark on the welcome screen and in the shell, and it is the
launcher icon for the desktop AppImage, Android, iOS, macOS and Windows. The
Android build is 0.11.4 (2007). Android also gets an adaptive icon and a
monochrome notification icon.
[Docs](docs/app_guide.md).

## 2026-11-09 — Calls: screen share keeps the microphone and camera framing

Sharing the screen no longer silences the operator: the call sends the
microphone and the PC system audio as separate tracks, so the far end hears
both. The system audio comes from the default output on Linux; when no call
output is chosen, the app moves the call to another device for the duration of
the share, so the other side does not hear its own voice through the loopback. Camera frames are now
shown with their native aspect ratio instead of being cropped to the tile, and
one-to-one calls fit the available area. On Android and iOS, video calls have a
button to switch between the front and back camera; the Android build is
0.11.3 (2006). Notification and call sounds now use Discord's message, mention
and call-ring tones. Desktop **Settings > Audio** shows the same routing hint.
[Docs](docs/app_guide.md).

## 2026-11-08 — Calls: automatic TURN and desktop audio settings

Calls could ring, be accepted and hang up correctly while no audio or video
ever flowed. The cause was ICE: configured TURN credentials were ignored unless
a `turn:` URL was typed by hand, so STUN-only calls failed whenever a peer sat
behind a VPN or carrier-grade NAT. The client now derives coturn endpoints from
the relay host, offers and answers carry the candidates gathered in a short
window, and a failed media path triggers up to two automatic ICE restarts plus
a Retry action. Relays can publish the coturn pool to authenticated accounts
(`FEDI3_RELAY_TURN_*`), and late call signals are no longer dropped after one
minute. Desktop **Settings > Audio** selects and tests the microphone and output
used by calls, without changing the system-wide default. Linux playback and routing use the host GStreamer and PulseAudio/PipeWire stack.
[Docs](docs/app_guide.md).

## 2026-11-08 — Android fix release 0.11.1

[0.11.1](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.1) fixes the
Android call and notification failures reported after 0.11.0. The release
resource shrinker had removed the bundled notification sounds, so Android
rejected every alert with `invalid_sound`; calls and messages produced no
notification at all while the app was closed. The keep-alive engine also
registered plugins twice and lost the Keystore-backed configuration, so it
stopped polling. Both are fixed, with an app-private config fallback for the
headless engine. Incoming calls now ring in foreground and background, invite
and accept signals are retried, chat retries run every five seconds, and the
chat composer and call controls stay above the Android navigation bar. Android
calls no longer expose screen sharing, so audio/video calls do not start the
MediaProjection service; desktop calls keep it. Unanswered incoming-call prompts
expire after one minute and close when the caller hangs up.
[Verification and limits](docs/audit/release_0.11.1_2026-11-08.md).

## 2026-11-08 — Linux and Android release 0.11.0

[0.11.0](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.0) combines the updated
Android client with the 0.10.1 AppImage repairs. It includes background
notifications, calls, chat attachments and followed-peer timeline backfill.
Packages are a Linux x86_64 AppImage and Android ARM64/x86_64 APKs with SHA256.
The APKs use the local debug signing key and are intended for sideloading.
[Verification and limits](docs/audit/release_0.11.0_2026-11-08.md).

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


## 2026-11-08 — 0.10.1 released: Linux AppImage startup fix

[Version 0.10.1 is published](https://github.com/redhunt07/Fedi3/releases/tag/v0.10.1)
for Linux x86_64 and Windows x64. It fixes the 0.10.0 AppImage startup failure on
newer desktops, caused by older bundled system libraries shadowing host GLib
requirements (`MOUNT_2_40` / `LIBSYSTEMD_251`). Fontconfig uses the host's font
libraries and configuration together.

The packaged GUI now passes required first-frame startup checks on Ubuntu 22.04
and 24.04 before publication, in addition to the existing tests and audit. Linux
0.10.0 users affected by the failure should download 0.10.1 manually, since their
old client cannot start its updater. Profiles, keys, credentials and messages
remain in their existing data directories; no reset or migration is needed.

[Publication verification](docs/audit/release_0.10.1_2026-11-08.md).

## 2026-11-08 — 0.10.0 released: client updates and compatible security fixes

[Version `0.10.0` is published](https://github.com/redhunt07/Fedi3/releases/tag/v0.10.0) for Linux x86_64 and Windows x64, with SHA256 checksums. Existing
clients need a manual first upgrade to receive the new updater.

- The client can check stable releases from Settings, announce newer versions
  and download packages with SHA256 verification. Linux x86_64 AppImages support
  atomic replacement with a previous-version backup; Windows x64 uses a verified
  ZIP with guided installation. Other platforms keep their installation source.
- RSA operations use AWS-LC and the Rust WebRTC transport uses the current stack.
  Existing profile keys, public keys, DIDs, signatures and encrypted messages
  remain compatible; old/new RSA and WebRTC interoperability was tested.
- Validation: 304 standard Rust tests and 184 Flutter tests passed, alongside
  old/new RSA and WebRTC interoperability checks. GitHub CI and native Linux/Windows
  release builds passed. The dependency audit reports zero vulnerabilities with
  no ignored advisories; five maintenance notices remain.
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
