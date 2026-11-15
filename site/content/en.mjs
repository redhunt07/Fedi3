// English pages. Mirrors content/it.mjs: same slugs by meaning, same facts,
// same schema. Every page points at its Italian alternate for hreflang.

import {
  heroBlock, cta, facts, cardGrid, steps, shots, compareTable, brief, prose,
  toc, qaList, faqGrid, faqVisual, verdict, relayList, splitBlock, appFrame, networkVisual, featureList, base, REPO, RELAY, RELAY_DIRECTORY, DOCS,
  RELEASES, LATEST, VERSION, LICENSE, BUILD,
} from '../lib/render.mjs';

const RELAYS = [
  { url: 'https://fedi3.redhunt07.ovh', name: 'Fedi3 relay (RedHunt07)' },
  { url: 'https://fedi3.palermo.nohost.me', name: 'Fedi3 relay (Panormus)' },
];

const netDiagram = `<div class="net-panel">
  <div class="net-stage">
    <canvas id="fedi3-net" role="img" aria-label="Animated map of the Fedi3 network: a device running the local core at the centre, Fedi3 relays, peers and Mastodon and Misskey instances joined by ActivityPub"></canvas>
    <ul class="net-legend">
      <li><span class="dot core"></span>local core</li>
      <li><span class="dot relay"></span>Fedi3 relay</li>
      <li><span class="dot peer"></span>peer</li>
      <li><span class="dot instance"></span>ActivityPub instance</li>
    </ul>
  </div>
  <div class="net-copy">
    <h3>A client with a local core</h3>
    <p>The device holds the identity and signs every activity. A relay delivers while you are offline and talks to classic instances; the peer-to-peer layer connects two devices without a server between them.</p>
    <p>The drawing maps the network; live numbers from the public relay are on the <a href="${RELAY}/">relay page</a>.</p>
  </div>
</div>`;

// ---------------------------------------------------------------- home

const home = {
  lang: 'en',
  slug: '',
  title: 'Fedi3 — fediverse client on ActivityPub',
  description: 'Fedi3 is a fediverse client on ActivityPub: account and keys on your device, encrypted chat, a P2P layer. It talks to Mastodon and Misskey with no instance to rent.',
  keywords: ['fedi3', 'fediverse', 'activitypub', 'fediverse client', 'mastodon alternative', 'misskey alternative', 'decentralized social network', 'p2p social', 'mastodon', 'misskey', 'bluesky', 'mastodon instance', 'fediverse relay', 'end-to-end encrypted chat', 'open source'],
  subject: 'Fediverse client on ActivityPub, an alternative to Mastodon and Misskey instances',
  ogImage: 'assets/og/fedi3-fediverso.png',
  ogAlt: 'Fedi3, a fediverse client on ActivityPub',
  alt: { it: 'it/', en: '' },
  breadcrumb: [{ label: 'Fedi3', href: '' }],
  hero: heroBlock({
    eyebrow: 'Fediverse client · ActivityPub · peer-to-peer',
    h1: 'Fedi3: the fediverse client on ActivityPub',
    lead: 'Fedi3 speaks ActivityPub with Mastodon and Misskey, while your account, signing keys and encrypted chats stay on your device. A relay is optional; the peer-to-peer layer keeps the network reachable when no server is there.',
    buttons: [
      { href: LATEST, label: 'Download Fedi3', primary: true },
      { href: base('vs/mastodon/'), label: 'Compare with Mastodon' },
    ],
    factList: ['Linux', 'Windows', 'Android', 'AGPL-3.0-only', 'pre-1.0'],
    image: 'assets/img/desktop-timeline.webp',
    imageAlt: 'Fedi3 desktop timeline with federated posts from Mastodon and Misskey instances',
  }),
  sections: [
    {
      id: 'why',
      h2: 'Why Fedi3 is not another fediverse instance',
      sub: 'Fedi3 brings the fediverse network to a client: identity lives on the device, where the keys belong to you.',
      body: splitBlock({
        copy: brief('In short', `<p><strong>Fedi3 is a fediverse client</strong> built on ActivityPub. It is not an instance and not a fork of Mastodon or Misskey: it is a program that speaks the fediverse protocol and keeps account, signing keys and encrypted chats in a local core.</p>`)
          + prose(`<p>A classic instance is a server: someone pays for hosting, updates the software, moderates and owns the database. Your account is a row in that database.</p>
<p>Fedi3 moves that layer onto the device. The local Rust core creates and keeps the identity, signs ActivityPub activities and encrypts chats. The relay stays a convenience for offline delivery and for reaching classic instances; the account itself lives on your device.</p>
<p>Compatibility is real: ActivityPub actors, HTTP signatures and WebFinger. A Mastodon or Misskey instance sees Fedi3 as a normal actor.</p>`),
        visual: appFrame('timeline'),
      }),
    },
    {
      id: 'network',
      h2: 'The Fedi3 network: ActivityPub plus a peer-to-peer layer',
      sub: 'Two routes for the same message: the relay when the recipient is offline, a direct link when it is not.',
      body: splitBlock({
        copy: prose(`<p>The peer-to-peer layer uses libp2p with DHT, relay reservation, DCUtR and mDNS: two devices find each other and talk with no server in between. A P2P-only account works with no relay at all.</p>
<p>A Fedi3 relay delivers activities while the device is offline and bridges to classic instances. Use a public one or install your own with Docker Compose.</p>`)
          + relayList(RELAYS, 'Public relays listed in the project directory. The directory is open: any operator can add their own relay.'),
        visual: networkVisual(),
      }),
    },
    {
      id: 'client',
      h2: 'The Fedi3 client, as it runs today',
      sub: 'Federated timeline, encrypted chat, profile and a composer with an MFM toolbar. The Flutter client is what you see; the Rust core signs, encrypts and delivers.',
      body: shots([
        { src: 'assets/img/desktop-chat.webp', alt: 'Fedi3 end-to-end encrypted chat with messages and an audio call', w: 1280, h: 720 },
        { src: 'assets/img/desktop-profile.webp', alt: 'Fedi3 profile with federated posts, profile fields and followers', w: 1280, h: 720 },
      ]),
    },
    {
      id: 'facts',
      h2: 'What Fedi3 does, in six points',
      sub: 'The functions that matter when you pick a fediverse client.',
      body: splitBlock({
        copy: featureList([
          { title: 'Keys in the client', text: 'Signing and encryption run in the local core; the relay keeps ciphertext and public data.' },
          { title: 'Peer-to-peer client delivery', text: 'Posts and chat travel directly between peers, with no relay at all.' },
          { title: 'ActivityPub compatibility', text: 'WebFinger, HTTP signatures and valid actors: Mastodon and Misskey see a normal actor.' },
          { title: 'Encrypted chat in the client', text: '1:1 and group chat encrypted, audio and video over ICE/TURN, screen sharing on desktop.' },
          { title: 'The client is AGPL-3.0-only', text: 'AGPL-3.0-only, releases with SHA256 checksums and repeatable verification gates.' },
          { title: 'A light Android client', text: 'Background profiles: about 1% of a core idle on tested Android devices.' },
        ]),
        visual: appFrame('composer'),
      }),
    },
    {
      id: 'compare',
      h2: 'Fedi3 against the networks you know',
      sub: 'Comparisons that say what stays the same and what changes, without declaring a winner where there is none.',
      body: splitBlock({
        copy: `<div class="cards">
  <div class="card"><h3>Fedi3 against Mastodon</h3><p>Same ActivityPub. The difference is where the account lives: in the local core, under your keys.</p><p><a href="${base('vs/mastodon/')}">Read the Mastodon comparison</a></p></div>
  <div class="card"><h3>Fedi3 against Misskey</h3><p>Misskey is an instance with MFM and plugins. Fedi3 is a client that speaks MFM and ActivityPub.</p><p><a href="${base('vs/misskey/')}">Read the Misskey comparison</a></p></div>
  <div class="card"><h3>The fediverse, explained</h3><p>What the fediverse is, how ActivityPub works and why a client changes the relationship with the server.</p><p><a href="${base('fediverse/')}">Guide to the fediverse</a></p></div>
</div>`,
        visual: appFrame('profile'),
      }),
    },
    {
      id: 'fit',
      h2: 'When Fedi3 fits, and when it does not',
      sub: 'Fedi3 is pre-1.0 and not for everyone. Here are the cases where it is the right pick and the ones where waiting makes sense.',
      body: splitBlock({
        copy: verdict({
          left: {
            title: 'Choose Fedi3 if',
            items: [
              'You want account and keys to stay on your device.',
              'You need end-to-end encrypted chat, not only public posts.',
              'You want federation without running a server, or with an optional relay.',
              'You care about peer-to-peer delivery.',
            ],
          },
          right: {
            title: 'Stay on Mastodon or Misskey if',
            items: [
              'You want a public instance that is ready now, with a community and clear rules.',
              'You need a published iOS or macOS app.',
              'You want central moderation and an administrator who answers for the server.',
              'You do not want to live with the changes of a pre-1.0 project.',
            ],
          },
        }),
        visual: appFrame('chat'),
      }),
    },
    {
      id: 'faq',
      h2: 'Fedi3 and fediverse questions',
      sub: 'The answers you need before downloading.',
      body: faqVisual() + faqGrid([
        { q: 'Is Fedi3 an alternative to Mastodon?', a: 'It is a fediverse client, so it speaks ActivityPub with Mastodon and Misskey. The difference is not the protocol: it is where the account lives.' },
        { q: 'Do I need to run a server to use Fedi3?', a: 'No. A peer-to-peer account works with no relay. A relay handles federation with classic instances and delivery while the device is offline.' },
        { q: 'What does the Fedi3 relay see?', a: 'Public posts are public. Private chat is encrypted in the client: the relay keeps ciphertext and public data. There is no forward secrecy yet.' },
        { q: 'Which platforms does Fedi3 run on?', a: 'Linux (AppImage and Debian/Ubuntu package), Windows x64 and Android (arm64-v8a and x86_64). iOS and macOS are in the Flutter project but have no published packages.' },
        { q: 'Is Fedi3 ready for everyone?', a: 'It is pre-1.0. Core functions work and are tested, but minor releases can change things. Release notes say what is open.' },
        { q: 'What licence does Fedi3 use?', a: 'AGPL-3.0-only. The code is on GitHub and every release publishes packages with SHA256 checksums.' },
        { q: 'Can I use Fedi3 on iOS?', a: 'The client is Flutter and iOS is planned, but published packages today cover Linux, Windows and Android.' },
        { q: 'How do I update Fedi3?', a: 'Download the latest release from GitHub, check the SHA256 checksum and install it over the previous version.' },
      ]),
    },
    {
      id: 'download',
      h2: 'Bring the fediverse to your device',
      sub: 'Linux, Windows or Android. No instance registration, no server bill.',
      body: cta([
        { href: LATEST, label: 'Download Fedi3', primary: true },
        { href: base('download/'), label: 'Install guide' },
      ]) + facts(['Linux AppImage', 'Debian/Ubuntu', 'Windows x64', 'Android APK', 'SHA256']),
    },
  ],
  faq: [
    { q: 'Is Fedi3 an alternative to Mastodon?', a: 'It is a fediverse client, so it speaks ActivityPub with Mastodon and Misskey. The difference is not the protocol: it is where the account lives. On Mastodon the account sits in the instance; on Fedi3 it sits in the local core on your device.' },
    { q: 'Do I need to run a server to use Fedi3?', a: 'No. A peer-to-peer account works with no relay. A relay handles federation with classic instances and delivery while the device is offline.' },
    { q: 'What does the Fedi3 relay see?', a: 'Public posts are public. Private chat is encrypted in the client: the relay keeps ciphertext and public data. There is no forward secrecy yet, and the limitation is documented.' },
    { q: 'Which platforms does Fedi3 run on?', a: 'Linux (AppImage and Debian/Ubuntu package), Windows x64 and Android (arm64-v8a and x86_64). iOS and macOS are in the Flutter project but have no published packages.' },
    { q: 'Is Fedi3 ready for everyone?', a: 'It is pre-1.0. Core functions work and are tested, but minor releases can change things. Release notes say what is open.' },
    { q: 'What licence does Fedi3 use?', a: 'AGPL-3.0-only. The code is on GitHub and every release publishes packages with SHA256 checksums.' },
  ],
  schemaType: 'WebPage',
  script: 'assets/net.js',
};

// ---------------------------------------------------------------- fediverse

const fediverse = {
  lang: 'en',
  slug: 'fediverse/',
  title: 'What the fediverse is: a 2026 guide to ActivityPub',
  description: 'The fediverse is the decentralized social network that speaks ActivityPub. How it works, the limit of Mastodon and Misskey instances, and where Fedi3 fits.',
  keywords: ['fediverse', 'what is the fediverse', 'activitypub', 'decentralized social network', 'mastodon', 'misskey', 'mastodon instance', 'fedi3', 'fediverse client', 'federation', 'decentralized social media'],
  subject: 'Guide to the fediverse and ActivityPub',
  ogImage: 'assets/og/fedi3-fediverso.png',
  ogAlt: 'The fediverse explained: ActivityPub, instances and the Fedi3 client',
  alt: { it: 'it/fediverso/', en: 'fediverse/' },
  breadcrumb: [{ label: 'Fedi3', href: '' }, { label: 'Fediverse', href: 'fediverse/' }],
  hero: heroBlock({
    eyebrow: 'Fediverse guide',
    h1: 'What the fediverse is: a guide to ActivityPub',
    lead: 'The fediverse is the network of independent servers that speak ActivityPub. Fedi3 belongs to that network and changes one part of it: your account is no longer hosted by somebody else.',
    buttons: [
      { href: base('activitypub/'), label: 'How ActivityPub works' },
      { href: LATEST, label: 'Download Fedi3', primary: true },
    ],
    factList: ['ActivityPub', 'WebFinger', 'HTTP signatures', 'AGPL-3.0-only'],
    image: 'assets/img/desktop-profile.webp',
    imageAlt: 'Fedi3 profile with federated posts from the fediverse',
    tocItems: [
      { id: 'definition', label: 'Definition' },
      { id: 'how', label: 'How it works' },
      { id: 'history', label: 'Mastodon and Misskey' },
      { id: 'limit', label: 'The instance limit' },
      { id: 'fedi3', label: 'Where Fedi3 fits' },
      { id: 'start', label: 'How to start' },
    ],
  }),
  sections: [
    {
      id: 'definition',
      h: 2,
      h2: 'Fediverse: the definition that matters',
      sub: 'A short definition that also works when you ask a search engine or an assistant.',
      body: brief('Definition', `<p>The <strong>fediverse</strong> is the set of independent social networks that federate with each other using open protocols, mainly <strong>ActivityPub</strong>. Each server, called an instance, has an administrator, rules and a database; federation lets users on different instances follow, reply to and share with each other.</p><p><strong>Fedi3</strong> is a fediverse client: it speaks ActivityPub, with the account and keys on the device, outside the instance database.</p>`)
        + prose(`<p>Different software shares the fediverse: Mastodon for microblogs, Misskey for notes and reactions, PeerTube for video, Pixelfed for images, Lemmy for forums. No central authority decides who joins: a protocol lets servers recognise each other and exchange activities.</p>
<p>The word appeared in 2017 as a shortening of federated universe and describes a network of people and servers. When someone says they are on the fediverse, they usually mean: I have an account on an instance that speaks ActivityPub and I can interact with the others.</p>`),
    },
    {
      id: 'how',
      h: 2,
      h2: 'How ActivityPub is spoken',
      sub: 'Four parts explain most of it: actors, inboxes, signatures and discovery.',
      body: cardGrid([
        { title: 'Actors', text: 'A user or a server is an ActivityPub actor with a public JSON document: name, public key, inbox and outbox.' },
        { title: 'Inbox and outbox', text: 'Activities arrive in the recipient inbox and leave from the sender outbox. Follow, Create, Announce and Like are standard activities.' },
        { title: 'HTTP signatures', text: 'Every request is signed with the actor private key. The receiving server verifies the signature against the published public key.' },
        { title: 'Discovery', text: 'WebFinger resolves an address such as @user@domain to the actor document. That is why one fediverse address works across different software.' },
      ]) + prose(`<p>A federation relay forwards activities between instances: it is not mandatory, but it helps small servers see each other. Fedi3 uses a relay for offline delivery and for reaching classic instances, and adds a peer-to-peer layer for direct delivery.</p>`),
    },
    {
      id: 'history',
      h: 2,
      h2: 'Mastodon and Misskey: what they taught',
      sub: 'Two projects proved that a social network with no single owner can work. Their limit sits in the instance model.',
      body: prose(`<p>Mastodon made the fediverse reachable for millions of people: an instance, an app, a federated timeline. Misskey added notes, reactions and MFM markup, with its own look and community. Both proved that federation is not a theory.</p>
<p>They also showed the cost of the instance model: someone has to pay for hosting, update the software, moderate and answer for the data. To take part you must pick a server, accept its rules and hand it your identity. When an instance closes, users migrate and rebuild their followers.</p>
<p>Fedi3 does not discard that path: it takes the protocol and moves identity onto the device. Mastodon and Misskey remain peers to talk to.</p>`),
    },
    {
      id: 'limit',
      h: 2,
      h2: 'The instance limit, explained without polemics',
      sub: 'An instance is a server: its uptime is yours and its administration is yours.',
      body: prose(`<p>An instance is a single point: if it falls your account is unreachable, if it changes rules your presence changes, if the admin loses interest you migrate. Federation spreads the conversation while identity stays on the device.</p>
<p>There is a privacy limit too: direct messages on a classic instance pass through and stay on the server, so the administrator can read them. End-to-end encryption is not part of the Mastodon or Misskey model.</p>
<p>The cost shows in small communities: running a server for a few dozen people is ongoing work. The fediverse works better when taking part does not require becoming an administrator.</p>`) + brief('The consequence', `<p>When identity sits on the device, the server becomes a service you can change. You can change relay without changing account, and stay reachable with no relay at all through the peer-to-peer layer.</p>`),
    },
    {
      id: 'fedi3',
      h: 2,
      h2: 'Where Fedi3 sits in the fediverse',
      sub: 'A client with a local core: the missing piece between the open protocol and the person using it.',
      body: prose(`<p>Fedi3 is an application with a local core. The Flutter client shows timeline, profile, chat and composer; the local Rust core handles identity, ActivityPub signatures, encryption and delivery. The public relay is an optional service for federation and offline delivery.</p>
<p>To the rest of the fediverse, Fedi3 is an actor like the others: it publishes on ActivityPub, receives in the inbox and answers HTTP signatures. A Mastodon user can follow a Fedi3 user without installing anything new, and the reverse holds too.</p>
<p>The difference is the point of control: the private key is not in a shared server database, it is in the core on your device. That is what makes Fedi3 a fediverse client rather than one more instance.</p>`) + cardGrid([
        { title: 'Relay', text: 'An optional public service for offline delivery, federation and the relay directory.' },
        { title: 'Local core', text: 'The Rust runtime that creates identity, signs activities and encrypts chats on the device.' },
        { title: 'P2P', text: 'The peer-to-peer layer that connects two devices with no server in between.' },
      ]),
    },
    {
      id: 'start',
      h: 2,
      h2: 'How to join the fediverse with Fedi3',
      sub: 'Three steps, no instance registration.',
      body: steps([
        { title: 'Download the client', text: 'Take the package for Linux, Windows or Android from the releases page and verify its SHA256 checksum.' },
        { title: 'Create the local profile', text: 'On first start the core generates the identity on the device. The profile exists without a network too.' },
        { title: 'Pick a relay or stay P2P', text: 'A relay enables federation and offline delivery; with no relay the account stays reachable over peer-to-peer.' },
      ]) + `<p class="sub gap">The fediverse is not an app: it is a network of people on different software. Fedi3 is your way in without renting somebody else's house. <a href="${base('download/')}">Install guide</a>.</p>`,
    },
    {
      id: 'faq',
      h: 2,
      h2: 'Fediverse questions',
      sub: 'The questions that keep coming back, with short answers.',
      body: faqVisual() + qaList([
        { q: 'Is the fediverse Mastodon?', a: 'No. Mastodon is one of the programs that speak ActivityPub. The fediverse also includes Misskey, PeerTube, Pixelfed, Lemmy and clients such as Fedi3.' },
        { q: 'Do I need an instance to be on the fediverse?', a: 'With Fedi3, no. The client speaks ActivityPub from the device, with an optional relay for federation and offline delivery.' },
        { q: 'Is the fediverse safe?', a: 'Safety depends on the software and its configuration. Fedi3 signs every activity with a local key and encrypts chats in the client; public posts stay public.' },
        { q: 'Can I follow Mastodon users from Fedi3?', a: 'Yes. Federation is ActivityPub: a Mastodon user and a Fedi3 user follow each other as on two different instances.' },
        { q: 'Does Fedi3 replace Mastodon and Misskey?', a: 'No, it stands next to them. Fedi3 is a client that speaks the same protocol with a different identity model.' },
      ]),
    },
    {
      id: 'cta',
      h: 1,
      h2: 'The fediverse is the network. Fedi3 is the client.',
      sub: 'An account you own, with nothing to configure on a server.',
      body: cta([
        { href: LATEST, label: 'Download Fedi3', primary: true },
        { href: base('activitypub/'), label: 'Understand ActivityPub' },
      ]),
    },
  ],
  faq: [
    { q: 'Is the fediverse Mastodon?', a: 'No. Mastodon is one of the programs that speak ActivityPub. The fediverse also includes Misskey, PeerTube, Pixelfed, Lemmy and clients such as Fedi3.' },
    { q: 'Do I need an instance to be on the fediverse?', a: 'With Fedi3, no. The client speaks ActivityPub from the device, with an optional relay for federation and offline delivery.' },
    { q: 'Is the fediverse safe?', a: 'Safety depends on the software and its configuration. Fedi3 signs every activity with a local key and encrypts chats in the client; public posts stay public.' },
    { q: 'Can I follow Mastodon users from Fedi3?', a: 'Yes. Federation is ActivityPub: a Mastodon user and a Fedi3 user follow each other as on two different instances.' },
    { q: 'Does Fedi3 replace Mastodon and Misskey?', a: 'No, it stands next to them. Fedi3 is a client that speaks the same protocol with a different identity model.' },
  ],
  article: true,
  schemaType: 'Article',
  about: ['Fediverse', 'ActivityPub', 'Mastodon', 'Misskey', 'Fedi3'],
  modified: '2026-11-15',
};

// ---------------------------------------------------------------- activitypub

const activitypub = {
  lang: 'en',
  slug: 'activitypub/',
  title: 'ActivityPub: how the fediverse protocol works',
  description: 'ActivityPub is the open protocol behind Mastodon, Misskey and the fediverse. Actors, inboxes, HTTP signatures, WebFinger and relays, explained with Fedi3.',
  keywords: ['activitypub', 'activitypub protocol', 'fediverse', 'mastodon', 'misskey', 'webfinger', 'http signatures', 'activitypub actor', 'fedi3', 'activitypub relay'],
  subject: 'ActivityPub, the protocol of the fediverse',
  script: 'assets/net.js',
  ogImage: 'assets/og/fedi3-activitypub.png',
  ogAlt: 'ActivityPub explained: actors, inboxes and HTTP signatures',
  alt: { it: 'it/activitypub/', en: 'activitypub/' },
  breadcrumb: [{ label: 'Fedi3', href: '' }, { label: 'ActivityPub', href: 'activitypub/' }],
  hero: heroBlock({
    eyebrow: 'Fediverse protocol',
    h1: 'ActivityPub, the protocol that holds the fediverse together',
    lead: 'ActivityPub is the open standard Mastodon, Misskey and the rest of the fediverse use to exchange posts, follows and reactions. Fedi3 implements it in a local core on your device.',
    buttons: [
      { href: base('fediverse/'), label: 'What the fediverse is' },
      { href: LATEST, label: 'Download Fedi3', primary: true },
    ],
    factList: ['W3C standard', 'JSON-LD', 'HTTP signatures', 'WebFinger'],
    image: 'assets/img/desktop-composer.webp',
    imageAlt: 'Fedi3 composer with an MFM toolbar and a post preview',
    tocItems: [
      { id: 'what', label: 'What it is' },
      { id: 'actor', label: 'An actor' },
      { id: 'signatures', label: 'Signatures and discovery' },
      { id: 'relay', label: 'Relays' },
      { id: 'fedi3', label: 'Fedi3' },
      { id: 'not', label: 'What it is not' },
    ],
  }),
  sections: [
    {
      id: 'what',
      h: 2,
      h2: 'ActivityPub: what it is and what it does',
      sub: 'A short definition, then the parts that matter in practice.',
      body: brief('Definition', `<p><strong>ActivityPub</strong> is a W3C protocol for decentralized social networks. It defines actors, objects and activities that servers exchange, and lets different software federate: a Mastodon user can follow a user on Misskey, PeerTube or Fedi3.</p>`)
        + prose(`<p>The protocol is open and documented: anyone can implement it without asking permission. Federation does not depend on a central authority; it depends on servers that publish JSON documents and exchange signed activities.</p>
<p>ActivityPub is not an app or a service: it is the language. The fediverse is the network that speaks it. Fedi3 is a client that speaks it from the device.</p>
<p>In practice the protocol defines a small vocabulary: actors (people or servers), objects (notes, images, videos) and activities (Create, Follow, Like, Announce). A server publishes them as JSON-LD documents at stable URLs, and other servers fetch and verify them.</p>`),
    },
    {
      id: 'actor',
      h: 2,
      h2: 'What an ActivityPub actor looks like',
      sub: 'An actor is a public JSON document with identity, keys and mailboxes.',
      body: splitBlock({ copy: prose(`<p>An ActivityPub actor exposes a document with an identifier, a username, a public key and two endpoints: inbox and outbox. The inbox receives activities addressed to the actor; the outbox publishes the ones the actor creates.</p>
<p>A Fedi3 profile is an actor like the others. The difference is that the document is served by the local core or the relay, while the private key stays in the core.</p>`) + `<pre><code>{
  "@context": "https://www.w3.org/ns/activitystreams",
  "id": "https://relay.example/users/alice",
  "type": "Person",
  "preferredUsername": "alice",
  "inbox": "https://relay.example/users/alice/inbox",
  "outbox": "https://relay.example/users/alice/outbox",
  "publicKey": {
    "id": "https://relay.example/users/alice#main-key",
    "owner": "https://relay.example/users/alice",
    "publicKeyPem": "-----BEGIN PUBLIC KEY-----..."
  }
}</code></pre>`, visual: appFrame('json') }),
    },
    {
      id: 'signatures',
      h: 2,
      h2: 'HTTP signatures and WebFinger: how servers recognise each other',
      sub: 'Two mechanisms make federation verifiable and addresses portable.',
      body: cardGrid([
        { title: 'HTTP signatures', text: 'Every request between servers is signed. The recipient verifies the signature against the actor public key: an unsigned request does not get in.' },
        { title: 'WebFinger', text: 'An address such as @user@domain resolves to the actor document. That is why the same handle works across different software.' },
        { title: 'NodeInfo', text: 'A public document describes server software and version. Fedi3 publishes software.name fedi3, so directories classify it correctly.' },
        { title: 'JSON-LD', text: 'Activities and objects are JSON-LD: readable by different machines, with shared vocabularies and verifiable contexts.' },
      ]) + prose(`<p>The signature is where the classic model shows its limit: the private key sits on the server, so the server can act for you. In Fedi3 the key sits in the local core, and the relay forwards activities that are already signed.</p>`),
    },
    {
      id: 'relay',
      h: 2,
      h2: 'Relays and federation: what an ActivityPub relay is for',
      sub: 'A relay is not an instance: it is a service that helps delivery and discovery.',
      body: splitBlock({ copy: prose(`<p>A federation relay forwards activities between instances and helps small servers see content they would otherwise miss. It does not host your identity and does not replace your instance.</p>
<p>Fedi3 uses the relay for two things: delivery while the device is offline, and federation with classic instances. The peer-to-peer layer adds direct delivery between devices, which does not pass through the relay.</p>
<p>Public relays are listed in an open directory in the repository; any operator can add their own. The relay also publishes aggregate metrics and a signed transparency snapshot.</p>`) + relayList(RELAYS, 'Fedi3 relay directory: two public relays as of 15 November 2026.'), visual: networkVisual() }),
    },
    {
      id: 'fedi3',
      h: 2,
      h2: 'How Fedi3 implements ActivityPub',
      sub: 'The protocol in the local core, with an optional relay and a peer-to-peer layer.',
      body: prose(`<p>The Fedi3 core is written in Rust and implements actors, inbox and outbox, HTTP signatures and WebFinger. The Flutter client does not talk to the fediverse directly: it goes through the core, which signs and delivers.</p>
<p>The relay is a separate public service with a database, a media store and HTTP peering between relays. The core can use it for federation or run peer-to-peer only.</p>
<p>The project also publishes an OpenAPI document, health endpoints and relay metrics: the infrastructure can be checked, not only described.</p>`) + cardGrid([
        { title: 'Local core', text: 'Actors, signatures, encryption and delivery in the Rust runtime on the device.' },
        { title: 'Relay', text: 'Optional public service for federation, offline delivery and the directory.' },
        { title: 'Transparency', text: 'Aggregate metrics, signed snapshots and releases with checksums in the repository.' },
      ]),
    },
    {
      id: 'not',
      h: 2,
      h2: 'What ActivityPub is not',
      sub: 'Three common confusions, cleared up.',
      body: `<div class="cards">
  <div class="card"><h3>Not AT Protocol</h3><p>Bluesky uses AT Protocol, a different protocol. ActivityPub and AT Protocol are not the same standard, although bridges exist between the two networks.</p></div>
  <div class="card"><h3>Not a server</h3><p>ActivityPub is a protocol. A server is an implementation. Fedi3 implements the protocol in a client with a local core.</p></div>
  <div class="card"><h3>Not a blockchain</h3><p>There is no distributed consensus and no token: there are servers exchanging signed activities and recognising each other with public keys.</p></div>
</div>` + prose(`<p>ActivityPub is a protocol, not a company: no vendor owns it, and a server can implement it without asking permission. That is why Mastodon, Misskey, PeerTube and Fedi3 can federate even when different teams build them.</p>`),
    },
    {
      id: 'faq',
      h: 2,
      h2: 'ActivityPub questions',
      sub: 'Technical answers, without useless jargon.',
      body: faqVisual() + qaList([
        { q: 'Is ActivityPub the same protocol as Mastodon?', a: 'Mastodon implements ActivityPub. The protocol is larger than the software: Misskey, PeerTube, Pixelfed, Lemmy and Fedi3 implement it too.' },
        { q: 'Do I need a server to use ActivityPub?', a: 'Classic federation needs a reachable endpoint, which can be a relay. Fedi3 adds peer-to-peer delivery for the cases without a server.' },
        { q: 'Is ActivityPub secure?', a: 'The protocol provides HTTP signatures on requests and verifiable public keys. Message confidentiality depends on the implementation: Fedi3 encrypts chats in the client.' },
        { q: 'Can I use ActivityPub with Bluesky?', a: 'Bluesky speaks AT Protocol. Bridges translate between the two protocols, but they are not the same thing.' },
      ]),
    },
    {
      id: 'cta',
      h: 1,
      h2: 'ActivityPub is the language. Fedi3 is the client.',
      sub: 'The protocol stays open; identity goes back to your device.',
      body: cta([
        { href: LATEST, label: 'Download Fedi3', primary: true },
        { href: base('fediverse/'), label: 'Fediverse guide' },
      ]),
    },
  ],
  faq: [
    { q: 'Is ActivityPub the same protocol as Mastodon?', a: 'Mastodon implements ActivityPub. The protocol is larger than the software: Misskey, PeerTube, Pixelfed, Lemmy and Fedi3 implement it too.' },
    { q: 'Do I need a server to use ActivityPub?', a: 'Classic federation needs a reachable endpoint, which can be a relay. Fedi3 adds peer-to-peer delivery for the cases without a server.' },
    { q: 'Is ActivityPub secure?', a: 'The protocol provides HTTP signatures on requests and verifiable public keys. Message confidentiality depends on the implementation: Fedi3 encrypts chats in the client.' },
    { q: 'Can I use ActivityPub with Bluesky?', a: 'Bluesky speaks AT Protocol. Bridges translate between the two protocols, but they are not the same thing.' },
  ],
  article: true,
  schemaType: 'Article',
  about: ['ActivityPub', 'Fediverse', 'Mastodon', 'Misskey', 'Fedi3'],
  modified: '2026-11-15',
};

// ---------------------------------------------------------------- vs mastodon

const vsMastodon = {
  lang: 'en',
  slug: 'vs/mastodon/',
  title: 'Fedi3 vs Mastodon: where your account lives',
  description: 'Fedi3 and Mastodon compared: same ActivityPub, same fediverse. Fedi3 keeps account and keys on your device, needs no instance and adds encrypted chat and P2P.',
  keywords: ['fedi3 vs mastodon', 'mastodon alternative', 'mastodon comparison', 'fediverse client', 'mastodon instance', 'activitypub', 'encrypted chat mastodon', 'decentralized social network'],
  subject: 'Fedi3 and Mastodon compared',
  ogImage: 'assets/og/fedi3-mastodon.png',
  ogAlt: 'Fedi3 against Mastodon: fediverse client or instance',
  alt: { it: 'it/vs/mastodon/', en: 'vs/mastodon/' },
  breadcrumb: [{ label: 'Fedi3', href: '' }, { label: 'Fedi3 vs Mastodon', href: 'vs/mastodon/' }],
  hero: heroBlock({
    eyebrow: 'Comparison',
    h1: 'Fedi3 or Mastodon: two models of a client',
    lead: 'Both speak ActivityPub and both sit in the fediverse. Mastodon is an instance somebody runs; Fedi3 is a client that keeps account and keys on your device.',
    buttons: [
      { href: LATEST, label: 'Download Fedi3', primary: true },
      { href: base('fediverse/'), label: 'What the fediverse is' },
    ],
    factList: ['ActivityPub', 'Same fediverse', 'AGPL-3.0-only', 'pre-1.0'],
    image: 'assets/img/desktop-timeline.webp',
    imageAlt: 'Fedi3 timeline with federated posts from Mastodon instances',
    tocItems: [
      { id: 'table', label: 'Table' },
      { id: 'account', label: 'Your account' },
      { id: 'same', label: 'What stays the same' },
      { id: 'verdict', label: 'When to choose' },
      { id: 'faq', label: 'Questions' },
    ],
  }),
  sections: [
    {
      id: 'table',
      h: 2,
      h2: 'Fedi3 and Mastodon, side by side',
      sub: 'The differences that matter when you choose where to put your identity.',
      body: compareTable({
        caption: 'Fedi3 and Mastodon compared, November 2026',
        head: ['', 'Mastodon', 'Fedi3'],
        rows: [
          ['Model', 'Hosted instance, run by others', 'Client with a local core on the device'],
          ['Account and keys', 'In the instance database', 'In the local core; the relay does not own them'],
          ['Server to run', 'Yes, for you or for your host', 'No: optional relay'],
          ['Private chat', 'Direct messages on the server', 'End-to-end encrypted in the client'],
          ['Calls', 'Not in the classic client', 'Audio and video over ICE/TURN, screen sharing on desktop'],
          ['Delivery with no server', 'No', 'Yes, peer-to-peer layer'],
          ['Federation', 'ActivityPub', 'ActivityPub, optional relay'],
          ['Platforms', 'Web, iOS, Android', 'Linux, Windows, Android'],
          ['Licence', 'AGPL-3.0', 'AGPL-3.0-only'],
        ],
      }),
    },
    {
      id: 'account',
      h: 2,
      h2: 'What changes for your account',
      sub: 'Migrating between Mastodon instances is an operation; on Fedi3 the identity is already on the device.',
      body: prose(`<p>On Mastodon, changing instance means moving the account, updating followers and living with the limits of the new server. The @user@domain address changes, and so does the history others see.</p>
<p>On Fedi3 the identity is created and stays on the device. The relay is a service you can change: the private key does not move, and the profile does not depend on a hoster domain.</p>
<p>That does not make Fedi3 immune to problems: a lost device is an identity to recover, which is why the project supports transfer between devices with a QR code and an encrypted spool.</p>`) + brief('The practical point', `<p>If your worry is losing an account when an instance closes, Fedi3 solves it at the root: the account is not hosted. If you want a community that is already busy, Mastodon remains the direct choice.</p>`),
    },
    {
      id: 'same',
      h: 2,
      h2: 'What stays the same: the fediverse',
      sub: 'Federation does not change. A Mastodon user and a Fedi3 user follow each other as on two different instances.',
      body: prose(`<p>Fedi3 is not a separate network with a bridge: it publishes ActivityPub activities, receives in the inbox and answers HTTP signatures. A Mastodon instance sees a normal actor with WebFinger and a public key.</p>
<p>That means nobody has to install Fedi3 to see your posts: they land in the federated timeline, and people on Mastodon or Misskey follow you without changing software.</p>
<p>The fediverse stays the context: Mastodon, Misskey, PeerTube, Pixelfed, Lemmy. Fedi3 is a different way into the same network.</p>`) + cardGrid([
        { title: 'Same protocol', text: 'ActivityPub, WebFinger and HTTP signatures: real interoperability with Mastodon.' },
        { title: 'Same network', text: 'Posts, follows, reactions and media circulate in the fediverse as on an instance.' },
        { title: 'No bridge', text: 'No bridge is needed: Fedi3 speaks the native protocol of the fediverse.' },
      ]),
    },
    {
      id: 'verdict',
      h: 1,
      h2: 'When Fedi3 fits, and when Mastodon fits',
      sub: 'Two legitimate choices, with different criteria.',
      body: verdict({
        left: {
          title: 'Choose Fedi3 if',
          items: [
            'You want the account and keys on your device.',
            'You need end-to-end encrypted chat with calls.',
            'You want federation without running a server.',
            'You care about peer-to-peer delivery.',
            'You accept a pre-1.0 project with frequent releases.',
          ],
        },
        right: {
          title: 'Choose Mastodon if',
          items: [
            'You want a public instance that is ready now, with a community.',
            'You need a published iOS or macOS app.',
            'You want central moderation by an administrator.',
            'You prefer the web interface and the instance model.',
            'You want a project with years of running behind it.',
          ],
        },
      }),
    },
    {
      id: 'faq',
      h: 2,
      h2: 'Fedi3 and Mastodon questions',
      sub: 'The common objections, with answers you can check.',
      body: faqVisual() + qaList([
        { q: 'Does Fedi3 replace Mastodon?', a: 'No. Fedi3 speaks the same protocol and stands next to Mastodon: it changes the identity model while staying on the same network.' },
        { q: 'Can I follow Mastodon users from Fedi3?', a: 'Yes. Federation is ActivityPub: your Fedi3 actor is reachable and can follow actors on Mastodon instances.' },
        { q: 'Do I have to leave my Mastodon instance?', a: 'No. You can keep both: Fedi3 is an additional client that needs no migration.' },
        { q: 'Why is there no Fedi3 iOS app?', a: 'The Flutter project includes iOS and macOS runners, but the published packages today are Linux, Windows and Android.' },
      ]),
    },
    {
      id: 'cta',
      h: 1,
      h2: 'Try Fedi3 next to your instance',
      sub: 'Same fediverse, identity on the device.',
      body: cta([
        { href: LATEST, label: 'Download Fedi3', primary: true },
        { href: base('vs/misskey/'), label: 'Compare with Misskey' },
      ]),
    },
  ],
  faq: [
    { q: 'Does Fedi3 replace Mastodon?', a: 'No. Fedi3 speaks the same protocol and stands next to Mastodon: it changes the identity model while staying on the same network.' },
    { q: 'Can I follow Mastodon users from Fedi3?', a: 'Yes. Federation is ActivityPub: your Fedi3 actor is reachable and can follow actors on Mastodon instances.' },
    { q: 'Do I have to leave my Mastodon instance?', a: 'No. You can keep both: Fedi3 is an additional client that needs no migration.' },
    { q: 'Why is there no Fedi3 iOS app?', a: 'The Flutter project includes iOS and macOS runners, but the published packages today are Linux, Windows and Android.' },
  ],
  article: true,
  schemaType: 'Article',
  about: ['Mastodon', 'Fediverse', 'ActivityPub', 'Fedi3'],
  modified: '2026-11-15',
};

// ---------------------------------------------------------------- vs misskey

const vsMisskey = {
  lang: 'en',
  slug: 'vs/misskey/',
  title: 'Fedi3 vs Misskey: same fediverse, different account',
  description: 'Fedi3 and Misskey speak ActivityPub and share the fediverse. Misskey is an instance with MFM; Fedi3 is a client with a local account, encrypted chat and a P2P layer.',
  keywords: ['fedi3 vs misskey', 'misskey alternative', 'misskey comparison', 'mfm', 'fediverse client', 'misskey instance', 'activitypub', 'decentralized social network'],
  subject: 'Fedi3 and Misskey compared',
  ogImage: 'assets/og/fedi3-misskey.png',
  ogAlt: 'Fedi3 against Misskey: fediverse client or instance',
  alt: { it: 'it/vs/misskey/', en: 'vs/misskey/' },
  breadcrumb: [{ label: 'Fedi3', href: '' }, { label: 'Fedi3 vs Misskey', href: 'vs/misskey/' }],
  hero: heroBlock({
    eyebrow: 'Comparison',
    h1: 'Fedi3 or Misskey: two models of a client',
    lead: 'Misskey brought notes, reactions and MFM into the fediverse. Fedi3 takes the same protocol and moves identity onto the device, with encrypted chat and peer-to-peer delivery.',
    buttons: [
      { href: LATEST, label: 'Download Fedi3', primary: true },
      { href: base('fediverse/'), label: 'Fediverse guide' },
    ],
    factList: ['ActivityPub', 'MFM', 'AGPL-3.0-only', 'pre-1.0'],
    image: 'assets/img/desktop-composer.webp',
    imageAlt: 'Fedi3 composer with an MFM toolbar',
    tocItems: [
      { id: 'table', label: 'Table' },
      { id: 'mfm', label: 'MFM and notes' },
      { id: 'same', label: 'What stays the same' },
      { id: 'verdict', label: 'When to choose' },
      { id: 'faq', label: 'Questions' },
    ],
  }),
  sections: [
    {
      id: 'table',
      h: 2,
      h2: 'Fedi3 and Misskey, side by side',
      sub: 'Two ways to be in the fediverse, with a different identity model.',
      body: compareTable({
        caption: 'Fedi3 and Misskey compared, November 2026',
        head: ['', 'Misskey', 'Fedi3'],
        rows: [
          ['Model', 'Hosted instance, run by others', 'Client with a local core on the device'],
          ['Account and keys', 'In the instance database', 'In the local core; the relay does not own them'],
          ['Server to run', 'Yes, for you or for your host', 'No: optional relay'],
          ['Private chat', 'On the server', 'End-to-end encrypted in the client'],
          ['Calls', 'Not in the classic client', 'Audio and video over ICE/TURN, screen sharing on desktop'],
          ['Delivery with no server', 'No', 'Yes, peer-to-peer layer'],
          ['Customisation', 'Instance plugins, themes and widgets', 'Client interface, consistent theme'],
          ['Platforms', 'Web, iOS, Android', 'Linux, Windows, Android'],
          ['Licence', 'AGPL-3.0', 'AGPL-3.0-only'],
        ],
      }) + prose(`<p>Misskey is a rich product: reactions, notes, MFM, plugins. Fedi3 does not try to replicate everything: it focuses on local identity, encrypted chat and federation without a server, with a composer that speaks MFM.</p>`),
    },
    {
      id: 'mfm',
      h: 2,
      h2: 'MFM and notes: what Misskey left in Fedi3',
      sub: 'MFM markup is part of the fediverse Misskey made popular; Fedi3 supports it in the composer.',
      body: prose(`<p>MFM is the markup language Misskey uses to animate and format notes. It became a recognisable part of the fediverse, and clients that ignore it lose part of the content.</p>
<p>The Fedi3 composer includes an MFM toolbar and renders formatted notes in the timeline. It is not a Misskey clone: it is a client that speaks the same vocabulary.</p>
<p>Reactions, notes and media circulate between the two implementations over ActivityPub, as between any two instances.</p>`) + cardGrid([
        { title: 'MFM toolbar', text: 'The Fedi3 composer offers the most used MFM commands, with a preview in the timeline.' },
        { title: 'Reactions', text: 'Reactions to notes arrive as standard ActivityPub activities.' },
        { title: 'Media', text: 'Federated images and video are visible in the client, with a bounded local cache.' },
      ]),
    },
    {
      id: 'same',
      h: 2,
      h2: 'What stays the same: the fediverse',
      sub: 'Federation does not change: Misskey and Fedi3 talk over ActivityPub.',
      body: prose(`<p>A Misskey user can follow a Fedi3 user and the reverse, with no bridges and nothing extra to install. The difference is where the private key sits and who runs the server.</p>
<p>The fediverse stays the same: Mastodon, Misskey, PeerTube, Pixelfed, Lemmy. Fedi3 is a different way in, with an identity that is not hosted by others.</p>`) + brief('The difference in one sentence', `<p>Misskey gives you a rich instance and a community; Fedi3 gives you a client with the account on the device and federation without a server.</p>`) + cardGrid([
        { title: 'Same protocol', text: 'Misskey and Fedi3 exchange ActivityPub activities; a Misskey user follows a Fedi3 user with no bridge.' },
        { title: 'Same network', text: 'Fedi3 publishes into the federated timeline: nobody has to install it to read your posts.' },
        { title: 'Different key', text: 'The private key stays in the local core, not in the Misskey instance database.' },
      ]),
    },
    {
      id: 'verdict',
      h: 1,
      h2: 'When Fedi3 fits, and when Misskey fits',
      sub: 'Neither choice is wrong: it depends on what you want to control.',
      body: verdict({
        left: {
          title: 'Choose Fedi3 if',
          items: [
            'You want account and keys on the device.',
            'You need end-to-end encrypted chat.',
            'You want federation without running a server.',
            'You care about peer-to-peer delivery.',
            'You want a light client with MFM and ActivityPub.',
          ],
        },
        right: {
          title: 'Choose Misskey if',
          items: [
            'You want an instance with plugins, themes and widgets.',
            'You are looking for a community that is already active on a server.',
            'You need a published iOS or macOS app.',
            'You want central moderation.',
            'You prefer the Misskey interface and reactions.',
          ],
        },
      }),
    },
    {
      id: 'faq',
      h: 2,
      h2: 'Fedi3 and Misskey questions',
      sub: 'The practical differences, without jargon.',
      body: faqVisual() + qaList([
        { q: 'Is Fedi3 a Misskey fork?', a: 'No. Fedi3 is an independent client, written in Flutter with a Rust core, that implements ActivityPub and MFM.' },
        { q: 'Can I follow Misskey users from Fedi3?', a: 'Yes. ActivityPub federation makes Misskey and Fedi3 actors interoperable.' },
        { q: 'Does Fedi3 support Misskey reactions?', a: 'Reactions arrive as ActivityPub activities; the client shows them in the timeline.' },
        { q: 'Do I have to leave Misskey to use Fedi3?', a: 'No. You can use both: Fedi3 is an additional client with a different identity model.' },
      ]),
    },
    {
      id: 'cta',
      h: 1,
      h2: 'Put Misskey and Fedi3 in the same timeline',
      sub: 'Same fediverse, identity on the device.',
      body: cta([
        { href: LATEST, label: 'Download Fedi3', primary: true },
        { href: base('vs/mastodon/'), label: 'Compare with Mastodon' },
      ]),
    },
  ],
  faq: [
    { q: 'Is Fedi3 a Misskey fork?', a: 'No. Fedi3 is an independent client, written in Flutter with a Rust core, that implements ActivityPub and MFM.' },
    { q: 'Can I follow Misskey users from Fedi3?', a: 'Yes. ActivityPub federation makes Misskey and Fedi3 actors interoperable.' },
    { q: 'Does Fedi3 support Misskey reactions?', a: 'Reactions arrive as ActivityPub activities; the client shows them in the timeline.' },
    { q: 'Do I have to leave Misskey to use Fedi3?', a: 'No. You can use both: Fedi3 is an additional client with a different identity model.' },
  ],
  article: true,
  schemaType: 'Article',
  about: ['Misskey', 'Fediverse', 'ActivityPub', 'Fedi3'],
  modified: '2026-11-15',
};

// ---------------------------------------------------------------- download

const download = {
  lang: 'en',
  slug: 'download/',
  title: 'Download Fedi3 for Linux, Windows and Android',
  description: 'Fedi3 0.12.3 packages for Linux AppImage and Debian/Ubuntu, Windows x64 and Android. SHA256 checksums, AGPL-3.0-only licence, a three-step install.',
  keywords: ['download fedi3', 'fedi3 linux', 'fedi3 windows', 'fedi3 android', 'fedi3 appimage', 'fedi3 apk', 'fediverse client', 'fedi3 0.12.3'],
  subject: 'Downloading and installing Fedi3',
  ogImage: 'assets/og/fedi3-download.png',
  ogAlt: 'Download Fedi3 for Linux, Windows and Android',
  alt: { it: 'it/download/', en: 'download/' },
  breadcrumb: [{ label: 'Fedi3', href: '' }, { label: 'Download', href: 'download/' }],
  hero: heroBlock({
    eyebrow: 'Download',
    h1: 'Download Fedi3 and bring the fediverse to your device',
    lead: 'Linux, Windows and Android. The package contains the client and the local core: no instance registration, no server bill.',
    buttons: [
      { href: LATEST, label: 'Download Fedi3', primary: true },
      { href: RELEASES, label: 'All releases' },
    ],
    factList: ['Linux AppImage', 'Debian/Ubuntu', 'Windows x64', 'Android APK', 'SHA256'],
    image: 'assets/img/desktop-timeline.webp',
    imageAlt: 'Fedi3 client on Linux with a federated timeline',
    tocItems: [
      { id: 'packages', label: 'Packages' },
      { id: 'install', label: 'Install' },
      { id: 'first-run', label: 'First run' },
      { id: 'relay', label: 'Relay or P2P' },
      { id: 'requirements', label: 'Requirements' },
      { id: 'faq', label: 'Questions' },
    ],
  }),
  sections: [
    {
      id: 'packages',
      h: 1,
      h2: 'Packages for Fedi3',
      sub: 'Every release publishes packages with SHA256 checksums and bilingual release notes.',
      body: `<div class="cards">
  <div class="card"><h3>Linux</h3><p>x86_64 AppImage and Debian/Ubuntu amd64 package. The local core is included in the package.</p><p><a href="${LATEST}">Download for Linux</a></p></div>
  <div class="card"><h3>Windows</h3><p>x64 package with the local core included. Signing and encryption run on the machine.</p><p><a href="${LATEST}">Download for Windows</a></p></div>
  <div class="card"><h3>Android</h3><p>arm64-v8a and x86_64 APKs with a release signing identity. Updates require the same certificate.</p><p><a href="${LATEST}">Download for Android</a></p></div>
</div>` + `<p class="sub gap">iOS and macOS are in the Flutter project but have no published packages in this release. The version is <strong>${VERSION}</strong> (build ${BUILD}), licence ${LICENSE}.</p>`,
    },
    {
      id: 'install',
      h: 2,
      h2: 'Install in four steps',
      sub: 'From download to a first federated post.',
      body: steps([
        { title: 'Download the package', text: 'Open the releases page and take the file for your system. Check the version and file name in the asset table.' },
        { title: 'Verify the checksum', text: 'Compare the file SHA256 with the one published in the release. On Linux: sha256sum Fedi3-*.AppImage.' },
        { title: 'Start the client', text: 'Install or run the package. On first start the local core creates the identity on the device and shows onboarding.' },
        { title: 'Set up the network', text: 'In Settings, Networking, enter a relay and a token of at least 16 characters, or keep the profile peer-to-peer only.' },
      ]) + `<p class="sub gap">The project public relay is <a href="${RELAY}">${RELAY.replace(/^https?:\/\//, '')}</a>. You can also install your own with Docker Compose following the <a href="${DOCS}/deploy_relay.md">deployment guide</a>.</p>`,
    },
    {
      id: 'first-run',
      h: 1,
      h2: 'Fedi3 first run: identity on the device',
      sub: 'The profile is yours from the first second, with no registration form on an instance.',
      body: prose(`<p>The core generates the key pair, builds the identity and prepares the ActivityPub actor. The profile exists without a network: you can write, encrypt and keep content before configuring a relay.</p>
<p>When you add a relay, the core publishes the actor and federation begins. If you change relay, the identity stays the same: only the service that delivers activities changes.</p>`) + brief('What you do not need', `<p>You skip the account on an instance, the domain and the hosting bill. The relay is optional, and the peer-to-peer layer covers the cases without a server.</p>`),
    },
    {
      id: 'relay',
      h: 1,
      h2: 'Fedi3 relay or peer-to-peer',
      sub: 'Two legitimate setups with different purposes.',
      body: `<div class="cards">
  <div class="card"><h3>With a relay</h3><p>Federation with Mastodon and Misskey instances, offline delivery, media and the directory. The relay does not own your keys.</p></div>
  <div class="card"><h3>Peer-to-peer only</h3><p>The devices find each other and talk directly; nothing sits in between. Suited to device pairs and local networks.</p></div>
</div>` + relayList(RELAYS, 'Public relays listed in the project directory as of 15 November 2026.'),
    },
    {
      id: 'requirements',
      h: 1,
      h2: 'Fedi3 requirements and package verification',
      sub: 'What Fedi3 needs to run and how to check that the package is intact.',
      body: prose(`<p>On Linux you need the Flutter desktop libraries and, for credentials in the keyring, libsecret and a running Secret Service. On Windows the x64 package is self-contained. On Android you need a recent system version and space for the core and cache.</p>
<p>Every release publishes SHA256 checksums: verify the file before running it. The project also publishes verification gates and release readiness reports in the repository.</p>`) + `<pre><code># verify the Linux package
sha256sum Fedi3-${VERSION}-linux-x86_64.AppImage

# compare the hash with the one on the release page</code></pre>`,
    },
    {
      id: 'faq',
      h: 2,
      h2: 'Download questions',
      sub: 'Practical doubts before installing.',
      body: faqVisual() + qaList([
        { q: 'Which package should I pick?', a: 'On Linux the AppImage is the direct route; the Debian/Ubuntu package is for people who prefer the package manager. On Windows you need the x64 package. On Android the arm64-v8a APK covers most devices.' },
        { q: 'Is Fedi3 free?', a: 'Yes. The client is AGPL-3.0-only free software and needs no subscription. A relay is optional and can be public or self-hosted.' },
        { q: 'Do I need an account to start?', a: 'No. The first start creates the identity on the device; a relay is added later if needed.' },
        { q: 'How do I update Fedi3?', a: 'Download the new release and verify the checksum. On Android an update requires the same signing certificate as the installed version.' },
      ]),
    },
    {
      id: 'cta',
      h: 1,
      h2: 'The fediverse is waiting on your device',
      sub: 'Download, verify, start. The rest is federation.',
      body: cta([
        { href: LATEST, label: 'Download Fedi3', primary: true },
        { href: base('faq/'), label: 'Read the FAQ' },
      ]),
    },
  ],
  faq: [
    { q: 'Which package should I pick?', a: 'On Linux the AppImage is the direct route; the Debian/Ubuntu package is for people who prefer the package manager. On Windows you need the x64 package. On Android the arm64-v8a APK covers most devices.' },
    { q: 'Is Fedi3 free?', a: 'Yes. The client is AGPL-3.0-only free software and needs no subscription. A relay is optional and can be public or self-hosted.' },
    { q: 'Do I need an account to start?', a: 'No. The first start creates the identity on the device; a relay is added later if needed.' },
    { q: 'How do I update Fedi3?', a: 'Download the new release and verify the checksum. On Android an update requires the same signing certificate as the installed version.' },
  ],
  schemaType: 'WebPage',
  howTo: {
    name: 'Install Fedi3 on Linux, Windows or Android',
    description: 'Four steps to download, verify and start Fedi3 with a local profile.',
    steps: [
      { name: 'Download the package', text: 'Open the releases page and take the file for your system.' },
      { name: 'Verify the checksum', text: 'Compare the file SHA256 with the one published in the release.' },
      { name: 'Start the client', text: 'Install or run the package; on first start the core creates the local identity.' },
      { name: 'Set up the network', text: 'Enter a relay and a token, or stay peer-to-peer only.' },
    ],
  },
};

// ---------------------------------------------------------------- faq

const faq = {
  lang: 'en',
  slug: 'faq/',
  title: 'Fedi3 and fediverse: frequently asked questions',
  description: 'Answers about Fedi3, ActivityPub and the fediverse: differences from Mastodon and Misskey, relays, security, platforms, licence and stated limits.',
  keywords: ['fedi3 faq', 'fedi3 questions', 'fedi3 vs mastodon', 'fedi3 vs misskey', 'fedi3 relay', 'fedi3 security', 'fedi3 ios', 'fedi3 licence', 'fediverse questions'],
  subject: 'Frequently asked questions about Fedi3, ActivityPub and the fediverse',
  ogImage: 'assets/og/fedi3-fediverso.png',
  ogAlt: 'Frequently asked questions about Fedi3 and the fediverse',
  alt: { it: 'it/faq/', en: 'faq/' },
  breadcrumb: [{ label: 'Fedi3', href: '' }, { label: 'FAQ', href: 'faq/' }],
  hero: heroBlock({
    eyebrow: 'Frequently asked questions',
    h1: 'Fedi3 and fediverse: frequently asked questions',
    lead: 'The answers you need before choosing a fediverse client: differences from Mastodon and Misskey, relays, security, platforms and stated limits.',
    buttons: [
      { href: LATEST, label: 'Download Fedi3', primary: true },
      { href: base('fediverse/'), label: 'Fediverse guide' },
    ],
    factList: ['pre-1.0', 'AGPL-3.0-only', 'Linux', 'Windows', 'Android'],
    image: 'assets/img/desktop-profile.webp',
    imageAlt: 'Fedi3 profile with federated posts',
    tocItems: [
      { id: 'faq-fediverse', label: 'Fedi3 and the fediverse' },
      { id: 'faq-security', label: 'Security' },
      { id: 'faq-network', label: 'Fedi3 relays and network' },
      { id: 'faq-project', label: 'Platforms and project' },
    ],
  }),
  sections: [
    {
      id: 'faq-fediverse',
      h: 2,
      h2: 'Fedi3 and the fediverse',
      sub: 'How the client relates to Mastodon and Misskey instances.',
      body: faqVisual() + qaList([
        { q: 'Is Fedi3 an alternative to Mastodon and Misskey?', a: 'It is a fediverse client: it speaks ActivityPub and stands next to Mastodon and Misskey. The difference is where the account lives; the protocol stays ActivityPub.' },
        { q: 'Is Fedi3 an instance?', a: 'No. An instance is a server with a user database; Fedi3 is an application with a local core. The relay is an optional service.' },
        { q: 'Can I follow Mastodon and Misskey users?', a: 'Yes. Fedi3 ActivityPub actors are reachable from classic instances and can follow actors on Mastodon, Misskey, PeerTube, Pixelfed and Lemmy.' },
        { q: 'Do I have to leave my current instance?', a: 'No. Fedi3 adds to it: keep the instance you have and use Fedi3 as a client with a local identity.' },
        { q: 'Is the fediverse only Mastodon?', a: 'No. Mastodon is one of the programs; the fediverse includes Misskey, PeerTube, Pixelfed, Lemmy and clients such as Fedi3.' },
      ]),
    },
    {
      id: 'faq-security',
      h: 2,
      h2: 'Security and privacy',
      sub: 'What is encrypted, what is public and what the relay can see.',
      body: faqVisual() + qaList([
        { q: 'Is chat end-to-end encrypted?', a: 'Yes, private chat is encrypted in the client. The relay keeps ciphertext and public data; private keys stay on the device. There is no forward secrecy yet, and the limitation is documented.' },
        { q: 'Are public posts private?', a: 'No. A public post is public by definition: the relay, federated instances and anyone with timeline access can see it.' },
        { q: 'Where are my private keys?', a: 'In the local core on the device, in the system keyring. They are not uploaded to the relay.' },
        { q: 'What happens if I lose the device?', a: 'The identity is on the device: the project supports transfer between devices with a QR code and an encrypted spool. Without a backup the identity has to be recreated.' },
        { q: 'Can the relay censor my posts?', a: 'The relay can refuse to deliver, but activities are signed by the client: the relay cannot alter them without breaking the signature.' },
      ]),
    },
    {
      id: 'faq-network',
      h: 2,
      h2: 'Fedi3 relays and network',
      sub: 'When a relay is needed, how to pick one and what the peer-to-peer layer does.',
      body: faqVisual() + qaList([
        { q: 'Do I need a relay to use Fedi3?', a: 'No. A peer-to-peer account works with no relay. A relay handles federation with classic instances and delivery while the device is offline.' },
        { q: 'How do I choose a relay?', a: 'The project directory lists public relays with operator and contact. Use a public one or install your own with Docker Compose.' },
        { q: 'Can I change relay without losing the account?', a: 'Yes. The identity sits in the local core: changing relay changes the delivery service; the account stays.' },
        { q: 'How does the peer-to-peer layer work?', a: 'It uses libp2p with DHT, relay reservation, DCUtR and mDNS to find and connect two devices. Direct delivery does not pass through the relay.' },
        { q: 'Does the relay collect telemetry?', a: 'It publishes aggregate metrics and a signed transparency snapshot. Counters are aggregate, not individual profiles.' },
      ]),
    },
    {
      id: 'faq-project',
      h: 2,
      h2: 'Fedi3 platforms, licence and project status',
      sub: 'What is published today and what is missing.',
      body: faqVisual() + qaList([
        { q: 'Which platforms does Fedi3 run on?', a: 'Linux (AppImage, Debian/Ubuntu), Windows x64 and Android (arm64-v8a, x86_64). iOS and macOS are in the Flutter project but have no published packages.' },
        { q: 'Is Fedi3 ready for everyone?', a: 'It is pre-1.0: core functions work and are tested, but minor releases can change things. Release notes say what is open.' },
        { q: 'What licence does Fedi3 use?', a: 'AGPL-3.0-only. The code is on GitHub, every release publishes SHA256 checksums and the verification gates are repeatable.' },
        { q: 'Who develops Fedi3?', a: 'The project is maintained by RedHunt07, with open contributions on GitHub. The repository holds the Flutter client, the Rust core, the relay and the P2P infrastructure.' },
        { q: 'Will Fedi3 be on F-Droid?', a: 'The F-Droid metadata is in the repository and is currently waiting for provenance review. APK releases are on GitHub.' },
      ]),
    },
    {
      id: 'cta',
      h: 1,
      h2: 'More questions? The repository is open.',
      sub: 'Issues, discussions and documentation are public.',
      body: cta([
        { href: `${REPO}/issues`, label: 'Open an issue on GitHub', primary: true },
      ]),
    },
  ],
  faq: [
    { q: 'Is Fedi3 an alternative to Mastodon and Misskey?', a: 'It is a fediverse client: it speaks ActivityPub and stands next to Mastodon and Misskey. The difference is where the account lives; the protocol stays ActivityPub.' },
    { q: 'Is chat end-to-end encrypted?', a: 'Yes, private chat is encrypted in the client. The relay keeps ciphertext and public data; private keys stay on the device. There is no forward secrecy yet, and the limitation is documented.' },
    { q: 'Do I need a relay to use Fedi3?', a: 'No. A peer-to-peer account works with no relay. A relay handles federation with classic instances and delivery while the device is offline.' },
    { q: 'Which platforms does Fedi3 run on?', a: 'Linux (AppImage, Debian/Ubuntu), Windows x64 and Android (arm64-v8a, x86_64). iOS and macOS are in the Flutter project but have no published packages.' },
    { q: 'What licence does Fedi3 use?', a: 'AGPL-3.0-only. The code is on GitHub, every release publishes SHA256 checksums and the verification gates are repeatable.' },
  ],
  schemaType: 'WebPage',
};

export const pages = [home, fediverse, activitypub, vsMastodon, vsMisskey, download, faq];
