# Fedi3 public website

The public site is generated from this repository and published to GitHub
Pages at <https://redhunt07.github.io/Fedi3/> (Italian: `/it/`). It is the
project's front door for people who search for a fediverse client, and for
answer engines that read `llms.txt`, `ai.txt` and the structured data.

## What it is

- `site/content/it.mjs`, `site/content/en.mjs`: the pages, one module per
  language. Each page carries title, description, keywords, hero, sections,
  FAQ and schema data.
- `site/lib/render.mjs`: the renderer. It owns the head (canonical, hreflang,
  Open Graph, JSON-LD graph), the navigation, the sections and the closing
  footer, so every page carries the same metadata.
- `site/build.mjs`: writes `site/dist/` from the content and assets, checks
  internal links and required head tags, and emits `robots.txt`, `sitemap.xml`,
  `feed.xml`, `llms.txt`, `llms-full.txt`, `ai.txt`, `humans.txt`,
  `site.webmanifest` and the IndexNow payload.
- `site/assets/`: stylesheet, network diagram script, icons, screenshots and
  Open Graph cards. `site/assets/brand.css` is a vendored snapshot of the relay
  landing design system so the site does not change when the relay page does.

## Build

```sh
node site/build.mjs
```

The build reads `SITE_ORIGIN` and `SITE_BASE`:

```sh
SITE_ORIGIN=https://example.org SITE_BASE= node site/build.mjs   # custom domain
node site/build.mjs                                               # project Pages
```

The default is `https://redhunt07.github.io` with base `/Fedi3`.

To preview locally, serve `site/dist` over HTTP (for example
`python3 -m http.server 8099 --directory site/dist`); opening the files with
`file://` breaks the root-relative links.

## Deploy

`.github/workflows/pages.yml` builds the site on every push that touches
`site/**` or the workflow itself, uploads the artifact, deploys to GitHub Pages
and pings IndexNow with the URL list. The workflow needs Pages enabled with
`build_type=workflow` in the repository settings.

## Content rules

- Keep the facts consistent with the release: pre-1.0, AGPL-3.0-only, Linux
  AppImage and Debian, Windows x64, Android arm64-v8a/x86_64, no iOS/macOS
  packages, E2EE chat without forward secrecy, two public relays.
- Every page has an Italian and an English version with `hreflang` and a
  language switch; keep the slugs paired in `it.mjs` and `en.mjs`.
- The site is checked with the design-taste scripts: `copy-lint.mjs` on every
  page (no FAIL) and `consistency.mjs` on the rendered pages (no FAIL at
  1440x900). Run both before a release that changes copy or layout.

## Files to update when the release changes

- `site/lib/render.mjs`: `VERSION`, `BUILD`, `LATEST` and the software schema.
- `site/content/*.mjs`: release notes links, package facts and FAQs.
- `site/assets/og/*.png`: Open Graph cards, if the copy changes.
