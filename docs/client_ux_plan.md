# Fedi3 — Client relaunch plan (UX/IA)

Date: 2026-11-01
Status: proposal, to be validated before implementing.

## 1. Honest assessment

The current client **is not yet appealing** for a user coming from Mastodon,
Misskey or mainstream social networks. Not because of Flutter: because of the product.

Defects observed in the live verification:

- onboarding is an administrator panel (bind, internal token, relay WS,
  domain) instead of a "choose a name and start" flow;
- the desktop timeline opens four columns with misleading labels: the "Local" column
  actually shows the follow feed, "Social" is empty without P2P;
- the profile shows raw activities (`EmojiReact`, `Like`, `Follow`) mixed with
  notes, without Post/Replies/Media tabs;
- follower/following lists were missing (now added), pagination, useful empty states;
- settings present but hard to discover, without internal search or a clear
  hierarchy;
- no visual identity, no guided onboarding, no account recovery path
  explained to the user;
- chat is technically E2E but poor compared to Misskey (attach files, react,
  threads, read status).

The problem is not "the wrong language": it is **information architecture,
flows and finish**. Rewriting in another language without solving these points
would produce the same product at a higher cost.

## 2. What to copy from Misskey and what not to

Misskey is a useful reference for **structure**, not for density.

To copy:

- settings taxonomy (Account, Profile, Privacy, Security, Moderation,
  Network, Appearance, Notifications, Language, Backup, About) with subpages and search;
- profile with tabs (Posts, Replies, Media, Reactions) and clickable counters;
- **optional** deck/columns, not the default layout;
- composer always reachable, with visibility, attachments, emoji, polls;
- unified search with filters and history;
- grouped and filterable notifications;
- chat with attachments, reactions, threads and read status.

Not to copy uncritically:

- visual density and the number of options at first sight: Misskey is for power
  users; a new user gets lost;
- dependence on a centralized multi-user server: Fedi3 is single-user per
  core, so "login/registration" must be redefined (see §4);
- the all-web model: Fedi3 has a local core and a relay; the desktop client is
  an advantage, not a limit.

Recommended positioning: **IA inspired by Misskey, initial simplicity from
Mastodon, deck mode for those who want it**. Fedi3's competitive advantage remains
P2P/relay: the client must look normal before telling that story.

## 3. Language and platform: proposal

Do not rewrite now. Reasons:

- Flutter covers desktop, mobile and (with limits) web with a single codebase; the bottleneck
  is the product, not the runtime;
- the Rust core and the protocol are the asset: they must be stabilized with a documented client
  API, not rewritten;
- a complete rewrite would cost 2–3 months and freeze UX fixes.

Two-track proposal:

- **Track A (now)**: redesign of IA and flows in the existing Flutter client, step by step
  (§6). Every step is releasable and testable.
- **Track B (later)**: if web distribution is needed, a separate web client
  (React/TS or Svelte) that consumes the same API, sharing design tokens and not
  code. It is not a rewrite of the desktop: it is a second client.

If another language were chosen in the future, the condition is to have first
a stable and documented API: without that, every client stays coupled to the internal
endpoints.

## 4. Authentication: the architectural point to clarify

Today Fedi3 is **single-user per core**: the identity is a key, not an account with
a password. The app and the core run on the same machine and share an internal
token. This is not the Misskey model (multi-user, web sessions).

Three options, in order of cost:

1. **Local profile (recommended now)**: "registration" = identity creation
   (username + relay, advanced options hidden); "login" = import backup or
   connect to an existing instance (URL + token/backup). The client manages multiple
   local profiles with a switcher. No changes to the core.
2. **Remote access to the core**: the core exposes a client API authenticated with a bearer
   token over TLS, so a web client can connect to a remote instance. It requires
   CORS, dedicated tokens, OpenAPI and revocation.
3. **Multi-user in the core**: the real Misskey model, but it is a backend change
   (multiple identities, sessions, permissions). Deferrable.

Recommendation: start from (1), design the API to make (2) possible, do not
promise (3) until there is real demand.

## 5. Target information architecture

Routes and screens, in implementation order:

- `/welcome` — choice: create identity, import backup, connect instance.
- `/signup` — username, recommended relay, collapsed advanced options, mandatory and
  downloadable recovery code.
- `/login` — import backup, connect URL+token, recovery code.
- `/home` — single feed with Home/Local/Federated tabs, persistent composer,
  infinite scroll, filters, media.
- `/explore` — unified search (posts, users, hashtags), filters, history.
- `/notifications` — groups by type, filters, mark as read, real-time.
- `/messages` — conversation list, threads, attachments, reactions, read status.
- `/@user` — public profile with Post/Replies/Media tabs, follow, mute, block,
  report.
- `/me` — personal profile with tabs and editing, paginated follower/following lists.
- `/settings` — hub with subsections: Account, Profile, Privacy, Security,
  Moderation, Network, Appearance, Notifications, Language, Backup, About, Advanced.
- `/settings/*` — one page per category, with explicit save and core restart
  where needed.

Cross-cutting rules: no token/bind at first sight; every screen has an empty
state, an actionable error and a loading state; no raw activity shown
to the user except in "Show raw".

## 6. Step plan

Every step is a verifiable release; estimates are indicative (1 developer).

| Step | Content | Dependencies | Estimate | Acceptance criterion |
|---|---|---|---|---|
| 0 | UX audit, design tokens, navigation and route skeleton, startup gate | — | 2 d | routes and empty states present; no functional regression |
| 1 | Guided registration, recovery code, login (backup/connect), profile switcher | current config API | 3 d | new user publishes within 5 minutes without touching tokens |
| 2 | Single home with tabs, persistent composer, infinite scroll, media | core timeline | 4 d | usable feed on desktop and mobile; no misleading column |
| 3 | Own and other users' profiles with tabs, paginated lists, profile editing | relationships | 4 d | correct tabs and lists; consistent follow/unfollow |
| 4 | Settings with complete taxonomy, internal search, safe saves | existing screens | 3 d | all categories reachable in ≤2 taps |
| 5 | Unified search with filters, history, remote lookup | search API | 3 d | post/user/hashtag results; local fallback |
| 6 | Grouped notifications, filters, mark-read, real-time | event stream | 3 d | correct groups and filters; consistent badge |
| 7 | Complete chat: attachments, reactions, threads, read state | chat API | 3 d | conversation with media and read status |
| 8 | Finish: onboarding tour, accessibility, themes, i18n, shortcuts | — | 3 d | basic accessibility audit passed |

Total ≈ 28 days; the first three steps (0–2) already give a presentable client.

## 7. Backend work needed in parallel

- **Stable and documented client API** (OpenAPI): endpoints, typed errors,
  cursor pagination, versioning. It is the prerequisite for any future client.
- **Auth for remote access**: dedicated bearer token, CORS, revocation, rotation.
- **Pagination** for followers/following and notifications (today the client loads the first
  page).
- **Filtered profile feed** on the API or client side: only Notes, with tabs.
- **Grouped notifications** and server-side mark-read.
- **Chat**: attachments and read status if not already present.
- **Moderation**: mute/block/report exposed uniformly.

## 8. Risks and metrics

Risks: redoing the IA without user testing; introducing multi-user too early;
duplicating clients (desktop + web) without a shared API; regressions in P2P
functions during the restyling.

Metrics to measure: time to first post, success of the main tasks
(post, follow, reply, search), crash-free sessions, D1/D7 retention,
average time to find a setting.

## 9. Immediate proposal

Start with **Step 0 + Step 1** (navigation skeleton + guided registration/login),
because they are the entry gate and the most visible defect. In
parallel, write the minimal OpenAPI specification of the client API: without it,
Step 2 risks consolidating internal endpoints.

If the constraint is "copy Misskey", the sequence remains valid: copy the
taxonomy and the flows, not the density, and keep Flutter until the IA is
stable.

## 10. Progress status (2026-11-01)

| Step | Status | Notes |
|---|---|---|
| 0 — Audit, design tokens, skeleton, startup gate | completed | Aurora theme, custom rail, existing routes, empty states; a real declarative router is missing |
| 1 — Guided registration/login, recovery | completed | onboarding with identity/relay in the foreground, quick login from URL+token, collapsed advanced options and recovery dialog with code copying |
| 2 — Single home with tabs | completed (base) | default tab layout on desktop, Home/Following/Discover/Federated tabs; full-screen composer from the sidebar and mobile FAB |
| 3 — Own/other profiles | completed (base) | banner, clickable counters, Post/Replies/Media tabs, media grid, outbox pagination; advanced media filters are missing |
| 4 — Settings | completed (visual) | grouped hub with icons and sections; internal search is missing |
| 5 — Search | completed (base) | post/user/hashtag search, source filter (all/local/relay), persistent history with chips and clearing; time filters are missing |
| 6 — Notifications | partial | filters, mark-all-read and grouping by date; grouping by actor/type is missing |
| 7 — Chat | completed (base) | threads, attachments with preview, reactions, read state, E2E badge; E2E calls, audio/video notes, complete and global search, forwarding, pinned messages and invite links completed |
| 8 — Finish | partial | Ctrl+1..7/Ctrl+N/Ctrl+F/Esc shortcuts, accessible rail with hints, advanced composer, fixed logo with particles, announcements from maintainers with periodic alert, mesh profile banner, tabbed chat and Telegram-style bubbles with check marks, compact notifications, posts saved offline, composer with drag&drop, online/busy/AFK/offline presence; contrast audit and full keyboard navigation remain |

Recommended next step: web client on the OpenAPI specification, then a full contrast/accessibility audit and end-to-end chat testing with two real identities.
