# Promoting Fedi3

How we get Fedi3 in front of more people without turning the fediverse into a
billboard. This document is the working launch kit: what to submit where, what
to post, in which order, and how to tell whether it worked.

## Principles

1. **The fediverse rewards participation, not advertising.** Posts that answer a
   question ("why does my instance cost so much", "why does the phone drain
   battery") travel; launch announcements alone do not.
2. **Every link must land somewhere useful.** Since the relay now serves a
   landing page at `/` (English) and `/it` (Italian), a post can point at a real
   page instead of a 404.
3. **Never overclaim.** Fedi3 is pre-1.0. Say what works, name what is missing
   (no iOS/macOS packages, no forward secrecy in chat, F-Droid pending).

## The funnel we control

| Step | Asset | Where |
| :--- | :--- | :--- |
| Landing page | `/` and `/it` served by the relay binary | any relay |
| Machine-readable identity | `nodeinfo/2.0` with `software.name = fedi3` | any relay |
| Public relay list | [`relays/relays.json`](../relays/relays.json), `/_fedi3/relay/directory` | repo and relays |
| Downloads | GitHub releases with checksums | GitHub |
| Trust | `/transparency`, signed forward log | relay |

Before promoting anything, verify on a live relay:

```sh
curl -s https://<relay>/ | head -5
curl -s https://<relay>/nodeinfo/2.0
curl -s https://<relay>/_fedi3/relay/stats
```

The landing page must return HTML, not 404, and `nodeinfo` must report
`"name": "fedi3"` with the current release version.

## Directory submissions

Directories are where people look for fediverse software. Submit only accurate
metadata and re-check after each release.

| Directory | What it needs | Notes |
| :--- | :--- | :--- |
| [FediDB](https://fedidb.com/) | reachable `nodeinfo/2.0` | crawls servers; software name must be `fedi3` |
| [fediverse.observer](https://fediverse.observer/) | `nodeinfo/2.0`, software name | same metadata; lists software and instances |
| [fediverse.party](https://fediverse.party/) | short description, screenshots, license | curated list of fediverse projects |
| [awesome-selfhosted](https://github.com/awesome-selfhosted/awesome-selfhosted) | one-line description, license, repo | pull request; "Social Networks" section |
| [awesome-fediverse](https://github.com/emilebosch/awesome-fediverse) | repo link, description | pull request |
| YunoHost apps catalog | [`packaging`](../packaging), manifest, screenshots | biggest reach among self-hosters; see [`docs/deploy_yunohost.md`](deploy_yunohost.md) |
| AUR | `packaging/aur/PKGBUILD` + `.SRCINFO` | see [`docs/distribution.md`](distribution.md) |
| Flathub | Flatpak manifest | after the AppImage flow is stable |
| F-Droid | `packaging/fdroiddata` metadata | currently `Disabled` pending provenance review |

Keep one canonical description in [`README.md`](../README.md) and reuse it
verbatim; inconsistent descriptions confuse crawlers and reviewers.

## Presence in the fediverse

The relay is infrastructure, not an audience. Open an official account on an
established Mastodon or Misskey instance (Italian and English posts, or two
accounts) and use it as the project voice.

Cadence that is sustainable and useful:

- **Weekly:** one technical note (what changed, what was measured, what broke).
- **Per release:** a short post linking the landing page and the release notes.
- **Opportunistic:** reply in threads about hosted-instance costs, moderation,
  privacy, battery drain, "is the fediverse only Mastodon?" — with a concrete
  answer, not a link drop.

Hashtags, sparingly: `#FediVerse #ActivityPub #FediDev #OpenSource #Rust`
`#Flutter #P2P`. Italian posts can add `#fediverso #softwarelibero`.

Rules of thumb:

- One link per post, at the end, never in the first line.
- Alt text on every screenshot.
- Quote-boost community replies; do not boost only your own announcements.
- Never post the same text to five communities on the same day.

## Communities and launch windows

Order matters: fix the funnel, seed a few real users, then go wide.

1. **Italian communities first** — [Le Alternative](https://lealternative.org/)
   reviews self-hosted and federated software, and Italian instances
   (mastodon.uno, Poliverso) have active local timelines. A project maintained
   in Italy is a story there.
2. **Fediverse developer channels** — `#FediDev`, the ActivityPub mailing list,
   FediForum, FOSDEM decentralisation tracks. A talk proposal is worth more
   than ten posts.
3. **Technical audiences** — Show HN, Lobsters, r/rust, r/FlutterDev,
   `!fediverse@lemmy.world`, r/fediverse. Lead with the architecture (local
   core, P2P, key handling), not with "new social network".
4. **Package ecosystems** — AUR, Flathub, F-Droid, YunoHost: each one is a
   permanent discovery channel.

Do not launch on a Monday morning; a Tuesday–Thursday window with a maintainer
available to answer for a few hours works better.

## Ready-to-post texts

### Release announcement (English, short)

> Fedi3 0.12.3 is out. A federated social app where the account lives in a local
> Rust core on your device: keys stay local, chat is end-to-end encrypted, and a
> P2P layer keeps you reachable without a server. ActivityPub keeps you
> connected to Mastodon and Misskey. Linux, Windows, Android, checksums
> published. Still pre-1.0, and honest about it.
> https://fedi3.redhunt07.ovh/ — the relay page now shows the network live.
> #FediVerse #ActivityPub #OpenSource

### Release announcement (Italian, short)

> È uscita Fedi3 0.12.2: un'app social federata in cui l'account vive in un core
> Rust locale sul tuo dispositivo. Le chiavi restano in locale, la chat è cifrata
> end-to-end e un livello P2P ti rende raggiungibile senza server. ActivityPub ti
> tiene collegato a Mastodon e Misskey. Linux, Windows e Android con checksum.
> Pre-1.0, e lo diciamo.
> https://fedi3.redhunt07.ovh/it
> #fediverso #ActivityPub #softwarelibero

### Show HN

> Show HN: Fedi3 – federated social with the account on your device
>
> Fedi3 is an ActivityPub client backed by a local Rust core instead of a hosted
> instance. Signing keys and E2E chat keys never leave the device; a libp2p layer
> (DHT, DCUtR, relay reservation) carries posts and chat between peers, and an
> optional relay handles federation with classic instances and offline delivery.
> It is pre-1.0, AGPL-3.0, with Linux/Windows/Android packages. I am happy to
> answer questions about the signature verification path and the relay threat
> model.

### Community reply template (use only when relevant)

> A hosted instance is a server you pay for and trust. Fedi3 moves the account
> into a local core: the relay routes signed traffic but never holds your private
> keys. It still speaks ActivityPub, so it federates with Mastodon instead of
> replacing it.

## First month checklist

- [ ] Landing page verified on both public relays (`/` and `/it`).
- [ ] `nodeinfo` reports `fedi3` and the current version on both relays.
- [ ] Official account opened on an established instance, profile completed,
      pinned post linking the landing page.
- [ ] FediDB and fediverse.observer show Fedi3 (recheck after each release).
- [ ] One pull request to `awesome-selfhosted`, one to `awesome-fediverse`.
- [ ] YunoHost catalog submission prepared with screenshots and manifest.
- [ ] AUR package published; `.SRCINFO` regenerated.
- [ ] One demo video (60 s) and alt text ready for every screenshot.
- [ ] Release notes bilingual and linked from the landing page.
- [ ] Reply rotation: at least one substantive reply per week in fediverse
      threads about hosting costs, moderation or privacy.

## What to measure

Track only what tells you whether the funnel works:

| Signal | Where | Target |
| :--- | :--- | :--- |
| Landing page requests | relay access logs | growth week over week |
| New accounts on public relays | `/_fedi3/relay/stats` (`total_users`) | real users, not signups |
| Referrals from the fediverse | posts linking `/` or `/it` | at least one per release |
| Repository interest | GitHub stars, forks, issues from strangers | issues are worth more than stars |
| Directory presence | FediDB, fediverse.observer | listed with correct software name |
| Package installs | AUR votes, Flathub installs, F-Droid listing | one new channel per month |

Do not optimize for stars. A project with 20 stars and 5 people actually posting
from Fedi3 is healthier than one with 500 stars and no users.

## Things to avoid

- Do not call it "the new Mastodon"; it is a client for the same protocol.
- Do not promise iOS or macOS packages before they exist.
- Do not hide the pre-1.0 status; it builds trust and filters expectations.
- Do not post the same announcement to every community in one day.
- Do not buy ads; the fediverse grows through boosts and word of mouth.
