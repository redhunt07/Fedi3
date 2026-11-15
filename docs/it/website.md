# Sito pubblico di Fedi3

Il sito pubblico è generato da questo repository e pubblicato su GitHub Pages
all'indirizzo <https://redhunt07.github.io/Fedi3/> (italiano: `/it/`). È la
porta d'ingresso del progetto per chi cerca un client del Fediverso e per i
motori di risposta che leggono `llms.txt`, `ai.txt` e i dati strutturati.

## Com'è fatto

- `site/content/it.mjs`, `site/content/en.mjs`: le pagine, un modulo per lingua.
  Ogni pagina porta titolo, descrizione, keyword, hero, sezioni, FAQ e dati
  schema.
- `site/lib/render.mjs`: il renderer. Gestisce head (canonical, hreflang, Open
  Graph, grafo JSON-LD), navigazione, sezioni e footer di chiusura, così ogni
  pagina ha gli stessi metadati.
- `site/build.mjs`: scrive `site/dist/` da contenuti e asset, controlla i link
  interni e i tag head obbligatori, e genera `robots.txt`, `sitemap.xml`,
  `feed.xml`, `llms.txt`, `llms-full.txt`, `ai.txt`, `humans.txt`,
  `site.webmanifest` e il payload IndexNow.
- `site/assets/`: foglio di stile, script del diagramma di rete, icone,
  screenshot e card Open Graph. `site/assets/brand.css` è una copia del design
  system della landing del relay, così il sito non cambia quando cambia la
  pagina del relay.

## Build

```sh
node site/build.mjs
```

La build legge `SITE_ORIGIN` e `SITE_BASE`:

```sh
SITE_ORIGIN=https://example.org SITE_BASE= node site/build.mjs   # dominio proprio
node site/build.mjs                                               # Pages di progetto
```

Il default è `https://redhunt07.github.io` con base `/Fedi3`.

Per l'anteprima locale servi `site/dist` via HTTP (ad esempio
`python3 -m http.server 8099 --directory site/dist`); aprendo i file con
`file://` i link assoluti si rompono.

## Deploy

`.github/workflows/pages.yml` compila il sito a ogni push che tocca `site/**`
o il workflow, carica l'artifact, pubblica su GitHub Pages e invia a IndexNow
l'elenco degli URL. Serve Pages abilitato con `build_type=workflow` nelle
impostazioni del repository.

## Regole sui contenuti

- Tieni i fatti allineati alla release: pre-1.0, AGPL-3.0-only, AppImage Linux
  e Debian, Windows x64, Android arm64-v8a/x86_64, nessun pacchetto iOS/macOS,
  chat E2EE senza forward secrecy, due relay pubblici.
- Ogni pagina ha una versione italiana e una inglese con `hreflang` e cambio
  lingua; mantieni le slug accoppiate in `it.mjs` e `en.mjs`.
- Il sito si controlla con gli script design-taste: `copy-lint.mjs` su ogni
  pagina (nessun FAIL) e `consistency.mjs` sulle pagine renderizzate (nessun
  FAIL a 1440x900). Esegui entrambi prima di una release che cambia copy o
  layout.

## File da aggiornare quando cambia la release

- `site/lib/render.mjs`: `VERSION`, `BUILD`, `LATEST` e lo schema software.
- `site/content/*.mjs`: link alle note di rilascio, fatti sui pacchetti, FAQ.
- `site/assets/og/*.png`: card Open Graph, se cambia il copy.
