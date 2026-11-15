// Fedi3 public site renderer.
//
// The site is generated, not hand-maintained: one content module per language
// (content/it.mjs, content/en.mjs) describes the pages, this module turns them
// into HTML, and build.mjs writes the result to dist/. Keeping the head, the
// navigation, the schema graph and the footer in one place is what keeps the
// canonical, hreflang and structured-data blocks identical on every page.
//
// The markup follows the same design system as the relay landing page
// (site/assets/brand.css): fixed nav, one screen per section, 12-column splits,
// cards, screenshots, FAQ grid and the closing screen.

export const VERSION = '0.12.4';
export const BUILD = '4020';
export const REPO = 'https://github.com/redhunt07/Fedi3';
export const RELAY = 'https://fedi3.redhunt07.ovh';
export const RELAY_DIRECTORY = `${REPO}/blob/main/relays/relays.json`;
export const DOCS = `${REPO}/tree/main/docs`;
export const RELEASES = `${REPO}/releases`;
export const LATEST = `${REPO}/releases/latest`;
export const LICENSE = 'AGPL-3.0-only';

// Origin and base path are configuration, not copy: a project GitHub Pages site
// lives under /<repo>/, a custom domain lives at the root.
export const ORIGIN = (process.env.SITE_ORIGIN || 'https://redhunt07.github.io').replace(/\/+$/, '');
export const BASE = (process.env.SITE_BASE ?? '/Fedi3').replace(/\/+$/, '');
export const SITE = `${ORIGIN}${BASE}`;

export const abs = (p = '/') => `${SITE}/${p.replace(/^\/+/, '')}`;
export const base = (p = '/') => `${BASE}/${p.replace(/^\/+/, '')}`;

const esc = (s) => String(s)
  .replace(/&/g, '&amp;')
  .replace(/</g, '&lt;')
  .replace(/>/g, '&gt;')
  .replace(/"/g, '&quot;');

const attr = (s) => esc(s).replace(/'/g, '&#39;');

// ---------------------------------------------------------------- components

export const cta = (items) => `<div class="cta">${items.map((i) =>
  `<a class="btn${i.primary ? ' primary' : ''}" href="${attr(i.href)}"${i.rel ? ` rel="${attr(i.rel)}"` : ''}>${esc(i.label)}</a>`).join('')}</div>`;

export const facts = (items) => `<p class="facts">${items.map((i) => `<span>${esc(i)}</span>`).join('')}</p>`;

export const cardGrid = (items) => `<div class="cards">${items.map((i) =>
  `<div class="card"><h3>${esc(i.title)}</h3><p>${i.text}</p></div>`).join('')}</div>`;

const STEP_ICONS = [
  '<path d="M12 4v10m0 0l-4-4m4 4l4-4"/><path d="M4 19h16"/>',
  '<circle cx="8.5" cy="12" r="3.5"/><path d="M12 12h8m-3 0v3m-3-3v2"/>',
  '<circle cx="12" cy="12" r="2.5"/><circle cx="5" cy="6" r="2"/><circle cx="19" cy="6" r="2"/><circle cx="5" cy="18" r="2"/><circle cx="19" cy="18" r="2"/><path d="M7 7.5l3.2 3M17 7.5l-3.2 3M7 16.5l3.2-3M17 16.5l-3.2-3"/>',
];

export const steps = (items) => `<div class="steps">${items.map((i, n) =>
  `<div class="step"><span class="ic"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${STEP_ICONS[n % STEP_ICONS.length]}</svg></span><h3>${esc(i.title)}</h3><p>${i.text}</p></div>`).join('')}</div>`;

export const shots = (items) => `<div class="shots">${items.map((i) =>
  `<figure class="shot${i.wide ? ' wide' : ''}"><img src="${attr(base(i.src))}" alt="${attr(i.alt)}" loading="lazy" width="${i.w}" height="${i.h}"></figure>`).join('')}</div>`;

export const compareTable = ({ head, rows, caption }) => `<div class="table-scroll" data-ui><table class="compare">
${caption ? `<caption>${esc(caption)}</caption>` : ''}
<thead><tr>${head.map((h) => `<th scope="col">${esc(h)}</th>`).join('')}</tr></thead>
<tbody>${rows.map((r) => `<tr>${r.map((c, i) => i === 0 ? `<th scope="row">${esc(c)}</th>` : `<td>${c}</td>`).join('')}</tr>`).join('')}</tbody>
</table></div>`;

export const brief = (title, html) => `<aside class="brief"><p class="brief-k">${esc(title)}</p>${html}</aside>`;

export const prose = (html) => `<div class="doc">${html}</div>`;

export const toc = (items) => `<nav class="toc" aria-label="Indice"><ul>${items.map((i) =>
  `<li><a href="#${attr(i.id)}">${esc(i.label)}</a></li>`).join('')}</ul></nav>`;

export const qaList = (items) => `<div class="qa">${items.map((f) =>
  `<div class="qa-item"><h3>${esc(f.q)}</h3><p>${f.a}</p></div>`).join('')}</div>`;

export const faqGrid = (items) => `<div class="faq-grid">${items.map((f) =>
  `<div class="faq-item"><h3>${esc(f.q)}</h3><p>${f.a}</p></div>`).join('')}</div>`;

const FAQ_ICONS = [
  ['ActivityPub', '<circle cx="12" cy="12" r="2.6"/><circle cx="4.5" cy="5" r="2"/><circle cx="19.5" cy="5" r="2"/><circle cx="4.5" cy="19" r="2"/><circle cx="19.5" cy="19" r="2"/><path d="M6.2 6.4l3.4 3.4M17.8 6.4l-3.4 3.4M6.2 17.6l3.4-3.4M17.8 17.6l-3.4-3.4"/>'],
  ['Peer-to-peer', '<circle cx="6" cy="12" r="3"/><circle cx="18" cy="12" r="3"/><path d="M9 12h6"/>'],
  ['Chat cifrata', '<rect x="4" y="5" width="16" height="12" rx="3"/><path d="M8 17l-2 3"/><path d="M9.5 9.5l2 2 3-3"/>'],
  ['Relay', '<circle cx="12" cy="12" r="8"/><path d="M4 12h16M12 4c3 3 3 13 0 16M12 4c-3 3-3 13 0 16"/>'],
];
export const faqVisual = () => `<ul class="faq-icons" aria-hidden="true">${FAQ_ICONS.map(([label, svg]) =>
  `<li><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">${svg}</svg><span>${label}</span></li>`).join('')}</ul>`;

export const verdict = ({ left, right }) => `<div class="verdict" data-ui>
<div class="verdict-col yes"><h3>${esc(left.title)}</h3><ul>${left.items.map((i) => `<li>${i}</li>`).join('')}</ul></div>
<div class="verdict-col no"><h3>${esc(right.title)}</h3><ul>${right.items.map((i) => `<li>${i}</li>`).join('')}</ul></div>
</div>`;

export const relayList = (relays, label) => `<ul class="relay-list">${relays.map((r) =>
  `<li><a href="${attr(r.url)}" rel="noopener">${esc(r.url.replace(/^https?:\/\//, ''))}</a><span>${esc(r.name || r.operator || '')}</span></li>`).join('')}</ul>
<p class="relay-note">${label}</p>`;

export const featureList = (items) => `<div class="feature-list">${items.map((i) =>
  `<div class="feature"><h3>${esc(i.title)}</h3><p>${i.text}</p></div>`).join('')}</div>`;

// Two columns: words on the left, the product's own screen on the right. This
// is the shape every section of the home page uses, so the page shows the
// client instead of describing it.
export const splitBlock = ({ copy, visual }) => `<div class="split">
<div class="split-copy">${copy}</div>
<div class="split-visual">${visual}</div>
</div>`;

// ---------------------------------------------------------------- app frames
// Live HTML fragments of the Fedi3 client: a reader sees a real screen, not a
// drawing. Their strings are the product's UI, not the page copy.

const frameHead = (title, badge) => `<div class="af-head"><span class="af-title">${title}</span><span class="af-badge">${badge}</span></div>`;

export const appFrame = (kind) => {
  if (kind === 'chat') {
    return `<div class="app-frame app-chat" data-ui data-visual>
${frameHead('Chat', 'cifrata end-to-end')}
<div class="af-thread">
  <div class="af-msg in"><span class="af-who">Panormus · 10:24</span><p>Ci sei per una chiamata?</p></div>
  <div class="af-msg out"><span class="af-who">tu · 10:25</span><p>Sì, ti chiamo adesso dal client.</p></div>
  <div class="af-msg in"><span class="af-who">Panormus · 10:26</span><p>La chiave resta sul dispositivo.</p></div>
</div>
<div class="af-compose"><span>Scrivi un messaggio…</span><span class="af-lock">🔒</span></div>
</div>`;
  }
  if (kind === 'composer') {
    return `<div class="app-frame app-composer" data-ui data-visual>
${frameHead('Nuovo post', 'Pubblico · Federato')}
<p class="af-text">Oggi provo Fedi3: account sul dispositivo, federazione con il Fediverso, nessuna istanza da affittare.</p>
<div class="af-toolbar"><span>B</span><span>I</span><span>MFM</span><span>Emoji</span><span>Media</span></div>
<div class="af-actions"><span class="af-count">280</span><span class="af-post-btn">Pubblica</span></div>
</div>`;
  }
  if (kind === 'profile') {
    return `<div class="app-frame app-profile" data-ui data-visual>
<div class="af-banner"></div>
<div class="af-profile">
  <span class="af-avatar big">R</span>
  <p class="af-name">RedHunt07 <span>@redhunt07@fedi3.redhunt07.ovh</span></p>
  <p class="af-text">Client del Fediverso su ActivityPub. Account e chiavi sul dispositivo.</p>
  <p class="af-meta">128 follower · 42 post · relay fedi3.redhunt07.ovh</p>
</div>
</div>`;
  }
  if (kind === 'json') {
    return `<div class="app-frame app-json" data-ui data-visual>
${frameHead('Attore ActivityPub', 'JSON-LD')}
<pre><code>{
  "@context": "https://www.w3.org/ns/activitystreams",
  "type": "Person",
  "preferredUsername": "alice",
  "inbox": "/users/alice/inbox",
  "outbox": "/users/alice/outbox",
  "publicKey": { "id": "#main-key" }
}</code></pre>
</div>`;
  }
  return `<div class="app-frame app-timeline" data-ui data-visual>
${frameHead('Timeline', 'Federated')}
<div class="af-post">
  <span class="af-avatar">R</span>
  <div>
    <p class="af-name">RedHunt07 <span>@redhunt07 · 14 ore</span></p>
    <p class="af-text">Ciao mondo! Primo post federato dal core locale.</p>
    <p class="af-meta">1 reazione · ActivityPub · relay online</p>
  </div>
</div>
<div class="af-post">
  <span class="af-avatar alt">P</span>
  <div>
    <p class="af-name">Panormus <span>@panormus@fedi3.palermo.nohost.me</span></p>
    <p class="af-text">Test con immagine dal Fediverso.</p>
    <div class="af-media"></div>
  </div>
</div>
</div>`;
};

export const networkVisual = (lang = 'it') => {
  const en = lang === 'en';
  const pause = en ? 'Pause animation' : 'Pausa animazione';
  const resume = en ? 'Resume animation' : 'Riprendi animazione';
  return `<div class="net-stage" data-visual>
<canvas id="fedi3-net" data-core-label="${en ? 'LOCAL CORE' : 'CORE LOCALE'}" data-relays="${attr(base('assets/relays.json'))}" role="img" aria-label="${en ? 'Animated Fedi3 network diagram: local core, Fedi3 relays, peers and ActivityPub instances' : 'Schema animato della rete Fedi3: core locale, relay Fedi3, peer e istanze ActivityPub'}"></canvas>
<button class="net-pause" type="button" aria-pressed="false" data-pause-label="${pause}" data-resume-label="${resume}">${pause}</button>
</div>`;
};

const CLOSING_SVG = `<svg viewBox="0 0 480 340" role="img" aria-label="Mappa della rete Fedi3: un core locale collegato a relay e istanze del Fediverso">
<g stroke="#33506c" stroke-width="1.4" fill="none">
  <line x1="240" y1="170" x2="96" y2="84"/><line x1="240" y1="170" x2="386" y2="72"/>
  <line x1="240" y1="170" x2="84" y2="262"/><line x1="240" y1="170" x2="392" y2="268"/>
  <line x1="240" y1="170" x2="240" y2="302"/>
</g>
<g stroke="#7dd3fc" stroke-width="1.4" stroke-dasharray="4 5" fill="none" opacity=".8">
  <line x1="240" y1="170" x2="392" y2="268"/>
</g>
<g>
  <circle cx="240" cy="170" r="22" fill="rgba(196,161,255,.16)"/><circle cx="240" cy="170" r="10" fill="#c4a1ff"/>
  <circle cx="96" cy="84" r="7" fill="#c4a1ff"/><circle cx="386" cy="72" r="7" fill="#c4a1ff"/>
  <circle cx="84" cy="262" r="7" fill="#c4a1ff"/><circle cx="392" cy="268" r="7" fill="#7dd3fc"/>
  <circle cx="240" cy="302" r="7" fill="#c4a1ff"/>
</g>
<g fill="#7dd3fc" opacity=".9">
  <circle cx="168" cy="127" r="2.6"/><circle cx="316" cy="120" r="2.6"/>
  <circle cx="162" cy="216" r="2.6"/><circle cx="240" cy="236" r="2.6"/>
</g>
</svg>`;

export const closingVisual = () => `<div class="closing-visual" aria-hidden="true">${CLOSING_SVG}</div>`;

// ---------------------------------------------------------------- schema

function orgNode() {
  return {
    '@type': 'Organization',
    '@id': `${SITE}/#org`,
    name: 'Fedi3',
    alternateName: ['FEDI3', 'Fedi3 Fediverso'],
    url: SITE,
    logo: { '@type': 'ImageObject', url: abs('assets/logo.png'), width: 512, height: 512 },
    description: 'Fedi3 è un client del Fediverso su ActivityPub: account, chiavi e chat cifrate restano sul dispositivo.',
    sameAs: [REPO, RELAY, DOCS],
    knowsAbout: [
      'Fediverso', 'ActivityPub', 'Mastodon', 'Misskey', 'Bluesky', 'AT Protocol',
      'rete sociale decentralizzata', 'social network peer-to-peer', 'cifratura end-to-end',
      'software libero', 'AGPL-3.0-only', 'relay di federazione', 'identità digitale sovrana',
    ],
  };
}

function websiteNode() {
  return {
    '@type': 'WebSite',
    '@id': `${SITE}/#website`,
    url: SITE,
    name: 'Fedi3',
    inLanguage: ['it', 'en'],
    publisher: { '@id': `${SITE}/#org` },
  };
}

function softwareNode(lang) {
  return {
    '@type': 'SoftwareApplication',
    '@id': `${SITE}/#app`,
    name: 'Fedi3',
    applicationCategory: 'SocialNetworkingApplication',
    applicationSubCategory: 'Fediverse client',
    operatingSystem: 'Linux, Windows, Android',
    softwareVersion: VERSION,
    softwareRequirements: 'Windows x64, Linux x86_64, Android arm64-v8a/x86_64',
    downloadUrl: LATEST,
    installUrl: abs(lang === 'it' ? 'it/download/' : 'download/'),
    license: 'https://www.gnu.org/licenses/agpl-3.0.html',
    isAccessibleForFree: true,
    offers: { '@type': 'Offer', price: '0', priceCurrency: 'EUR' },
    publisher: { '@id': `${SITE}/#org` },
    author: { '@id': `${SITE}/#org` },
    screenshot: [abs('assets/img/desktop-timeline.webp'), abs('assets/img/desktop-chat.webp')],
    featureList: [
      'Client del Fediverso su ActivityPub',
      'Account e chiavi di firma sul dispositivo',
      'Chat cifrata end-to-end con chiamate audio e video',
      'Consegna peer-to-peer senza server',
      'Federazione con istanze Mastodon e Misskey',
      'Relay pubblico opzionale per la consegna offline',
      'Software libero AGPL-3.0-only',
      'Client Flutter con core locale in Rust',
    ],
    sameAs: [REPO, RELAY],
  };
}

function breadcrumbNode(page) {
  if (!page.breadcrumb?.length) return null;
  return {
    '@type': 'BreadcrumbList',
    '@id': `${abs(page.slug)}#breadcrumb`,
    itemListElement: page.breadcrumb.map((b, i) => ({
      '@type': 'ListItem',
      position: i + 1,
      name: b.label,
      item: abs(b.href),
    })),
  };
}

function faqNode(page) {
  if (!page.faq?.length) return null;
  return {
    '@type': 'FAQPage',
    '@id': `${abs(page.slug)}#faq`,
    mainEntity: page.faq.map((f) => ({
      '@type': 'Question',
      name: f.q,
      acceptedAnswer: { '@type': 'Answer', text: f.plain || stripTags(f.a) },
    })),
  };
}

function articleNode(page) {
  if (!page.article) return null;
  return {
    '@type': 'Article',
    '@id': `${abs(page.slug)}#article`,
    headline: page.h1,
    description: page.description,
    inLanguage: page.lang,
    datePublished: '2026-11-15',
    dateModified: page.modified || '2026-11-15',
    author: { '@id': `${SITE}/#org` },
    publisher: { '@id': `${SITE}/#org` },
    mainEntityOfPage: abs(page.slug),
    image: abs(page.ogImage || 'assets/og/fedi3-fediverso.png'),
    about: page.about || ['Fediverso', 'ActivityPub', 'Fedi3'],
  };
}

function howToNode(page) {
  if (!page.howTo) return null;
  return {
    '@type': 'HowTo',
    '@id': `${abs(page.slug)}#howto`,
    name: page.howTo.name,
    description: page.howTo.description,
    totalTime: 'PT10M',
    step: page.howTo.steps.map((s, i) => ({
      '@type': 'HowToStep',
      position: i + 1,
      name: s.name,
      text: s.text,
      url: `${abs(page.slug)}#passo-${i + 1}`,
    })),
  };
}

const stripTags = (s) => String(s).replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim();

function pageNode(page) {
  return {
    '@type': page.schemaType || 'WebPage',
    '@id': `${abs(page.slug)}#page`,
    url: abs(page.slug),
    name: page.title,
    description: page.description,
    inLanguage: page.lang,
    isPartOf: { '@id': `${SITE}/#website` },
    about: { '@id': `${SITE}/#app` },
    primaryImageOfPage: abs(page.ogImage || 'assets/og/fedi3-fediverso.png'),
    speakable: {
      '@type': 'SpeakableSpecification',
      cssSelector: ['h1', '.brief p', '.lead'],
    },
    breadcrumb: page.breadcrumb?.length ? { '@id': `${abs(page.slug)}#breadcrumb` } : undefined,
  };
}

export function schemaFor(page) {
  const graph = [orgNode(), websiteNode(), softwareNode(page.lang), pageNode(page)];
  const extra = [breadcrumbNode(page), faqNode(page), articleNode(page), howToNode(page)].filter(Boolean);
  return { '@context': 'https://schema.org', '@graph': [...graph, ...extra] };
}

// ---------------------------------------------------------------- head

export function head(page) {
  const canonical = abs(page.slug);
  const og = abs(page.ogImage || 'assets/og/fedi3-fediverso.png');
  const alternates = page.alt ? [
    `<link rel="alternate" hreflang="it" href="${attr(abs(page.alt.it))}">`,
    `<link rel="alternate" hreflang="en" href="${attr(abs(page.alt.en))}">`,
    `<link rel="alternate" hreflang="x-default" href="${attr(abs(page.alt.en))}">`,
  ].join('\n') : '';
  return `<!doctype html>
<html lang="${attr(page.lang)}" class="m-enter" data-type="software" data-enter>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<meta name="page-type" content="software">
<meta name="site-kind" content="software">
<meta name="theme-color" content="#070b12">
<title>${esc(page.title)}</title>
<meta name="description" content="${attr(page.description)}">
<meta name="keywords" content="${attr(page.keywords.join(', '))}">
<meta name="robots" content="index, follow, max-snippet:-1, max-image-preview:large, max-video-preview:-1">
<meta name="googlebot" content="index, follow, max-snippet:-1, max-image-preview:large">
<meta name="author" content="RedHunt07">
<meta name="application-name" content="Fedi3">
<meta name="subject" content="${attr(page.subject || 'Fediverso, ActivityPub, client decentralizzato')}">
<meta name="category" content="Social network, Fediverso, ActivityPub">
<meta name="rating" content="general">
<link rel="canonical" href="${attr(canonical)}">
${alternates}
<meta property="og:type" content="website">
<meta property="og:site_name" content="Fedi3">
<meta property="og:locale" content="${page.lang === 'it' ? 'it_IT' : 'en_US'}">
<meta property="og:title" content="${attr(page.title)}">
<meta property="og:description" content="${attr(page.description)}">
<meta property="og:url" content="${attr(canonical)}">
<meta property="og:image" content="${attr(og)}">
<meta property="og:image:width" content="1200">
<meta property="og:image:height" content="630">
<meta property="og:image:alt" content="${attr(page.ogAlt || page.h1)}">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="${attr(page.title)}">
<meta name="twitter:description" content="${attr(page.description)}">
<meta name="twitter:image" content="${attr(og)}">
<link rel="icon" href="${attr(base('assets/favicon.svg'))}" type="image/svg+xml">
<link rel="icon" href="${attr(base('assets/favicon.png'))}" type="image/png" sizes="64x64">
<link rel="apple-touch-icon" href="${attr(base('assets/apple-touch-icon.png'))}">
<link rel="manifest" href="${attr(base('site.webmanifest'))}">
<link rel="alternate" type="application/atom+xml" title="Fedi3 — novità del progetto" href="${attr(base('feed.xml'))}">
<link rel="alternate" type="text/plain" title="Fedi3 — descrizione per assistenti AI" href="${attr(base('llms.txt'))}">
<link rel="stylesheet" href="${attr(base('assets/style.css'))}">
<script type="application/ld+json">
${JSON.stringify(schemaFor(page), null, 1)}
</script>
</head>`;
}

// ---------------------------------------------------------------- chrome

function navLinks(lang) {
  return lang === 'it'
    ? [
        { href: 'it/fediverso/', label: 'Fediverso' },
        { href: 'it/activitypub/', label: 'ActivityPub' },
        { href: 'it/vs/mastodon/', label: 'Confronti' },
        { href: 'it/download/', label: 'Scarica Fedi3' },
        { href: 'it/faq/', label: 'FAQ' },
      ]
    : [
        { href: 'fediverse/', label: 'Fediverse' },
        { href: 'activitypub/', label: 'ActivityPub' },
        { href: 'vs/mastodon/', label: 'Compare' },
        { href: 'download/', label: 'Download Fedi3' },
        { href: 'faq/', label: 'FAQ' },
      ];
}

export function header(page) {
  const l = page.lang;
  const other = l === 'it' ? 'en' : 'it';
  const otherHref = page.alt ? page.alt[other] : (other === 'it' ? 'it/' : '');
  const linkHtml = navLinks(l).map((n) => `<a href="${attr(base(n.href))}">${esc(n.label)}</a>`).join('\n      ');
  return `<header class="nav">
  <div class="wrap nav__bar">
    <a class="skip-link" href="#main">${l === 'it' ? 'Vai al contenuto' : 'Skip to content'}</a>
    <a class="brand" href="${attr(base(l === 'it' ? 'it/' : ''))}">FEDI3 <small>${l === 'it' ? 'client del Fediverso' : 'fediverse client'}</small></a>
    <nav class="nav__links" aria-label="${l === 'it' ? 'Navigazione principale' : 'Main navigation'}">
      ${linkHtml}
      <a href="${REPO}" rel="noopener">${l === 'it' ? 'Sorgente' : 'Source'}</a>
      <a href="${attr(base(otherHref))}" hreflang="${other}" lang="${other}">${other === 'it' ? 'Italiano' : 'English'}</a>
    </nav>
    <details class="nav__sheet">
      <summary>Menu</summary>
      <nav aria-label="${l === 'it' ? 'Navigazione principale' : 'Main navigation'}">
        ${linkHtml}
        <a href="${REPO}" rel="noopener">${l === 'it' ? 'Sorgente' : 'Source'}</a>
        <a href="${attr(base(otherHref))}" hreflang="${other}" lang="${other}">${other === 'it' ? 'Italiano' : 'English'}</a>
      </nav>
    </details>
  </div>
</header>`;
}

export function crumbs(page) {
  if (!page.breadcrumb || page.breadcrumb.length < 2) return '';
  return `<nav class="crumbs" aria-label="Percorso"><ol>${page.breadcrumb.map((b, i) =>
    `<li>${i === page.breadcrumb.length - 1 ? `<span aria-current="page">${esc(b.label)}</span>` : `<a href="${attr(base(b.href))}">${esc(b.label)}</a>`}</li>`).join('')}</ol></nav>`;
}

export function footer(page) {
  const l = page.lang;
  const t = l === 'it'
    ? {
        explore: 'Esplora il Fediverso',
        links: [
          ['it/fediverso/', 'Che cos’è il Fediverso'],
          ['it/activitypub/', 'ActivityPub, spiegato'],
          ['it/vs/mastodon/', 'Fedi3 contro Mastodon'],
          ['it/vs/misskey/', 'Fedi3 contro Misskey'],
          ['it/download/', 'Scarica Fedi3'],
          ['it/faq/', 'Domande frequenti'],
        ],
        projectLinks: [
          [REPO, 'Codice su GitHub'],
          [RELEASES, 'Release e checksum'],
          [DOCS, 'Documentazione'],
          [RELAY, 'Relay pubblico'],
        ],
        line: `Fedi3 ${VERSION} · pre-1.0 · ${LICENSE} · progetto di RedHunt07`,
        credit: 'Screenshot del client Fedi3, nessuna foto di repertorio.',
      }
    : {
        explore: 'Explore the fediverse',
        links: [
          ['fediverse/', 'What the fediverse is'],
          ['activitypub/', 'ActivityPub, explained'],
          ['vs/mastodon/', 'Fedi3 vs Mastodon'],
          ['vs/misskey/', 'Fedi3 vs Misskey'],
          ['download/', 'Download Fedi3'],
          ['faq/', 'Frequently asked questions'],
        ],
        projectLinks: [
          [REPO, 'Source on GitHub'],
          [RELEASES, 'Releases and checksums'],
          [DOCS, 'Documentation'],
          [RELAY, 'Public relay'],
        ],
        line: `Fedi3 ${VERSION} · pre-1.0 · ${LICENSE} · a RedHunt07 project`,
        credit: 'Screenshots of the Fedi3 client, no stock photography.',
      };
  const closing = page.closing || {
    h2: l === 'it' ? 'Porta il Fediverso sul tuo dispositivo' : 'Bring the fediverse to your device',
    sub: l === 'it' ? 'Linux, Windows o Android. Nessuna istanza da affittare.' : 'Linux, Windows or Android. No instance to rent.',
    buttons: [{ href: LATEST, label: `${l === 'it' ? 'Scarica' : 'Download'} Fedi3`, primary: true }],
    facts: ['Linux', 'Windows', 'Android', 'SHA256'],
  };
  return `<footer class="closing">
  <div class="wrap">
    <div class="closing-split">
      <div class="closing-copy">
        <h2>${esc(closing.h2)}</h2>
        <p class="sub">${esc(closing.sub)}</p>
        ${closing.body || cta(closing.buttons || [{ href: LATEST, label: `${l === 'it' ? 'Scarica' : 'Download'} Fedi3`, primary: true }]) + (closing.facts?.length ? facts(closing.facts) : '')}
        <nav class="closing-links" aria-label="${esc(t.explore)}">
          ${t.links.map(([h, label]) => `<a href="${attr(base(h))}">${esc(label)}</a>`).join('')}
        </nav>
      </div>
      ${closingVisual()}
    </div>
    <div class="closing-legal">
      <span>${esc(t.line)}</span>
      <span>${t.projectLinks.map(([h, label]) => `<a href="${attr(h)}" rel="noopener">${esc(label)}</a>`).join('')}<span class="legal-credit">${esc(t.credit)}</span></span>
    </div>
  </div>
</footer>`;
}

// ---------------------------------------------------------------- page shell

export function page({ lang, slug, sections, ...rest }) {
  const p = { lang, slug, sections, ...rest };
  const body = sections.map((s) => `<section id="${attr(s.id)}"${s.statement ? ' data-statement' : ''}>
<div class="wrap">
${s.h2 ? `<h2>${s.h2}</h2>` : ''}
${s.sub ? `<p class="sub">${s.sub}</p>` : ''}
${s.body || ''}
</div>
</section>`).join('\n');
  let hero = p.hero || '';
  const cr = crumbs(p);
  if (cr && hero.includes('<div class="hero-copy">')) {
    hero = hero.replace('<div class="hero-copy">', `<div class="hero-copy">${cr}`);
  }
  return `${head(p)}
<body>
${header(p)}
<main id="main">
${hero}
${body}
</main>
${footer(p)}
${p.script ? `<script src="${attr(base(p.script))}" defer></script>` : ''}
</body>
</html>
`;
}

export function heroBlock({ eyebrow, h1, lead, buttons, factList, image, imageAlt, tocItems }) {
  return `<section class="hero" data-enter>
  <div class="wrap">
    <div class="hero-split hero-text">
    <div class="hero-copy">
      <p class="eyebrow">${esc(eyebrow)}</p>
      <h1>${h1}</h1>
      <p class="lead">${lead}</p>
      ${buttons?.length ? cta(buttons) : ''}
      ${factList?.length ? facts(factList) : ''}
      ${tocItems?.length ? toc(tocItems) : ''}
    </div>
    ${image ? `<figure class="hero-visual"><img src="${attr(base(image))}" alt="${attr(imageAlt)}" width="1280" height="720" fetchpriority="high"></figure>` : ''}
    </div>
  </div>
</section>`;
}
