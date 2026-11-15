// Italian pages. The English mirror lives in en.mjs; the two share slugs by
// meaning (fediverso/fediverse), and every page carries its alternate URL so
// hreflang and the language switch stay correct.

import {
  heroBlock, cta, facts, cardGrid, steps, shots, compareTable, brief, prose,
  toc, qaList, faqGrid, faqVisual, verdict, relayList, splitBlock, appFrame, networkVisual, featureList, base, REPO, RELAY, RELAY_DIRECTORY, DOCS,
  RELEASES, LATEST, VERSION, LICENSE, BUILD,
} from '../lib/render.mjs';

const RELAYS = [
  { url: 'https://fedi3.redhunt07.ovh', name: 'Relay Fedi3 (RedHunt07)' },
  { url: 'https://fedi3.palermo.nohost.me', name: 'Relay Fedi3 (Panormus)' },
];


// ---------------------------------------------------------------- home

const home = {
  lang: 'it',
  slug: 'it/',
  title: 'Fedi3 — client del Fediverso su ActivityPub',
  description: 'Fedi3 è il client del Fediverso su ActivityPub: account e chiavi sul dispositivo, chat cifrata, rete P2P. Parla con Mastodon e Misskey senza istanza da affittare.',
  keywords: ['fedi3', 'fediverso', 'activitypub', 'client del fediverso', 'alternativa a mastodon', 'alternativa a misskey', 'rete sociale decentralizzata', 'social p2p', 'mastodon', 'misskey', 'bluesky', 'istanza mastodon', 'relay fediverso', 'chat cifrata end-to-end', 'software libero'],
  subject: 'Client del Fediverso su ActivityPub, alternativa alle istanze Mastodon e Misskey',
  ogImage: 'assets/og/fedi3-fediverso.png',
  ogAlt: 'Fedi3, client del Fediverso su ActivityPub',
  alt: { it: 'it/', en: '' },
  breadcrumb: [{ label: 'Fedi3', href: 'it/' }],
  hero: heroBlock({
    eyebrow: 'Client del Fediverso · ActivityPub · peer-to-peer',
    h1: 'Fedi3: il client del Fediverso su ActivityPub',
    lead: 'Fedi3 parla ActivityPub con Mastodon e Misskey, ma l\'account, le chiavi di firma e le chat cifrate restano sul tuo dispositivo. Un relay è opzionale; il livello peer-to-peer tiene la rete in piedi anche quando il server non c\'è.',
    buttons: [
      { href: LATEST, label: 'Scarica Fedi3', primary: true },
      { href: base('it/vs/mastodon/'), label: 'Confronta con Mastodon' },
    ],
    factList: ['Linux', 'Windows', 'Android', 'AGPL-3.0-only', 'pre-1.0'],
    image: 'assets/img/desktop-timeline.webp',
    imageAlt: 'Timeline del client Fedi3 con post federati da istanze Mastodon e Misskey',
  }),
  sections: [
    {
      id: 'perche',
      h2: 'Perché Fedi3 non è un\'altra istanza del Fediverso',
      sub: 'Fedi3 porta la rete del Fediverso su un client: l\'identità vive sul dispositivo, non su un server che qualcun altro amministra.',
      body: splitBlock({
        copy: brief('In sintesi', `<p><strong>Fedi3 è un client del Fediverso</strong> basato su ActivityPub. Non è un'istanza e non è un fork di Mastodon o Misskey: è un programma che parla il protocollo del Fediverso e conserva account, chiavi di firma e chat cifrate in un core locale.</p>`)
          + prose(`<p>Un'istanza classica è un server: qualcuno paga l'hosting, aggiorna il software, modera e custodisce il database. Il tuo account è una riga in quel database.</p>
<p>Fedi3 sposta quel livello sul dispositivo. Il core locale in Rust genera e conserva l'identità, firma le attività ActivityPub e cifra le chat. Il relay resta una comodità per la consegna offline e per raggiungere le istanze tradizionali, non il luogo in cui vivi.</p>
<p>La compatibilità è reale: attori ActivityPub, firme HTTP e WebFinger. Un'istanza Mastodon o Misskey vede Fedi3 come un attore normale.</p>`),
        visual: appFrame('timeline'),
      }),
    },
    {
      id: 'rete',
      h2: 'La rete Fedi3: ActivityPub più un livello peer-to-peer',
      sub: 'Due strade per lo stesso messaggio: il relay quando il destinatario è offline, il collegamento diretto quando c\'è.',
      body: splitBlock({
        copy: prose(`<p>Il livello peer-to-peer usa libp2p con DHT, relay reservation, DCUtR e mDNS: due dispositivi si trovano e si parlano senza un server in mezzo. Un account solo P2P funziona anche senza relay.</p>
<p>Il relay Fedi3 consegna le attività mentre il dispositivo è offline e fa da ponte verso le istanze classiche. Puoi usarne uno pubblico o installarne uno tuo con Docker Compose.</p>`)
          + relayList(RELAYS, 'Relay pubblici elencati nella directory del progetto. La directory è aperta: ogni operatore può aggiungere il proprio relay.'),
        visual: networkVisual(),
      }),
    },
    {
      id: 'client',
      h2: 'Il client Fedi3, come funziona oggi',
      sub: 'Timeline federata, chat cifrata, profilo e compositore con toolbar MFM. Il client Flutter è la parte che vedi; il core Rust è la parte che firma, cifra e consegna.',
      body: shots([
        { src: 'assets/img/desktop-chat.webp', alt: 'Chat cifrata end-to-end del client Fedi3 con messaggi e chiamata audio', w: 1280, h: 720 },
        { src: 'assets/img/desktop-profile.webp', alt: 'Profilo Fedi3 con post federati, campi personalizzati e follower', w: 1280, h: 720 },
      ]),
    },
    {
      id: 'fatti',
      h2: 'Cosa fa Fedi3, in sei punti',
      sub: 'Le funzioni che contano quando scegli un client del Fediverso.',
      body: splitBlock({
        copy: featureList([
          { title: 'Chiavi nel client', text: 'Firma e cifratura girano nel core locale; il relay conserva ciphertext e dati pubblici.' },
          { title: 'Consegna P2P fra client', text: 'Post e chat viaggiano direttamente tra peer, senza alcun relay.' },
          { title: 'Compatibilità ActivityPub', text: 'WebFinger, firme HTTP e attori validi: Mastodon e Misskey vedono un attore normale.' },
          { title: 'Chat cifrata nel client', text: 'Chat 1:1 e di gruppo cifrata, audio e video su ICE/TURN, schermo su desktop.' },
          { title: 'Client sotto AGPL-3.0-only', text: 'AGPL-3.0-only, release con checksum SHA256 e gate di verifica ripetibili.' },
          { title: 'Client Android leggero', text: 'Profili in background: circa l\'1% di un core inattivo sui dispositivi testati.' },
        ]),
        visual: appFrame('composer'),
      }),
    },
    {
      id: 'confronti',
      h2: 'Fedi3 contro le reti che conosci',
      sub: 'Confronti con quello che resta uguale e quello che cambia, senza dichiarare vincitori dove non ce ne sono.',
      body: splitBlock({
        copy: `<div class="cards">
  <div class="card"><h3>Fedi3 contro Mastodon</h3><p>Stesso ActivityPub. La differenza è dove vive l'account: nel core locale invece che nell'istanza.</p><p><a href="${base('it/vs/mastodon/')}">Leggi il confronto con Mastodon</a></p></div>
  <div class="card"><h3>Fedi3 contro Misskey</h3><p>Misskey è un'istanza con MFM e plugin. Fedi3 è un client che parla MFM e ActivityPub.</p><p><a href="${base('it/vs/misskey/')}">Leggi il confronto con Misskey</a></p></div>
  <div class="card"><h3>Il Fediverso spiegato</h3><p>Che cos'è il Fediverso, come funziona ActivityPub e perché un client cambia il rapporto con il server.</p><p><a href="${base('it/fediverso/')}">Guida al Fediverso</a></p></div>
</div>`,
        visual: appFrame('profile'),
      }),
    },
    {
      id: 'scelta',
      h2: 'Quando Fedi3 conviene, e quando no',
      sub: 'Fedi3 è pre-1.0 e non è per tutti. Ecco i casi in cui è la scelta giusta e quelli in cui conviene aspettare.',
      body: splitBlock({
        copy: verdict({
          left: {
            title: 'Scegli Fedi3 se',
            items: [
              'Vuoi che account e chiavi restino sul tuo dispositivo.',
              'Ti serve una chat cifrata end-to-end, non solo post pubblici.',
              'Vuoi federarti senza gestire un server, o con un relay opzionale.',
              'Ti interessa il livello peer-to-peer per la consegna diretta.',
            ],
          },
          right: {
            title: 'Resta su Mastodon o Misskey se',
            items: [
              'Vuoi un\'istanza pubblica già pronta, con una community e regole chiare.',
              'Ti serve un\'app iOS o macOS pubblicata.',
              'Vuoi la moderazione centralizzata e un amministratore responsabile.',
              'Non vuoi convivere con i cambiamenti di un progetto pre-1.0.',
            ],
          },
        }),
        visual: appFrame('chat'),
      }),
    },
    {
      id: 'faq',
      h2: 'Domande frequenti su Fedi3 e il Fediverso',
      sub: 'Le risposte che servono prima di scaricare.',
      body: faqVisual() + faqGrid([
        { q: 'Fedi3 è un\'alternativa a Mastodon?', a: 'È un client del Fediverso, quindi parla ActivityPub con Mastodon e Misskey. La differenza non è il protocollo: è dove vive l\'account.' },
        { q: 'Devo gestire un server per usare Fedi3?', a: 'No. Un account solo peer-to-peer funziona senza relay. Il relay serve per la federazione con istanze classiche e per la consegna offline.' },
        { q: 'Che cosa vede il relay di Fedi3?', a: 'I post pubblici sono pubblici. La chat privata è cifrata nel client: il relay conserva ciphertext e dati pubblici. Non c\'è ancora forward secrecy.' },
        { q: 'Su quali piattaforme gira Fedi3?', a: 'Linux (AppImage e pacchetto Debian/Ubuntu), Windows x64 e Android (arm64-v8a e x86_64). iOS e macOS sono nel progetto ma non hanno pacchetti.' },
        { q: 'Fedi3 è pronto per tutti?', a: 'È pre-1.0. Le funzioni principali funzionano e sono testate, ma tra release minori possono cambiare. Le note di rilascio dicono cosa è aperto.' },
        { q: 'Che licenza ha Fedi3?', a: 'AGPL-3.0-only. Il codice è su GitHub e ogni release pubblica i pacchetti con checksum SHA256.' },
        { q: 'Posso usare Fedi3 su iOS?', a: 'Il client è Flutter e il progetto prevede iOS, ma oggi i pacchetti pubblicati coprono Linux, Windows e Android.' },
        { q: 'Come aggiorno Fedi3?', a: 'Scarica l\'ultima release dalla pagina GitHub, verifica il checksum SHA256 e installa sopra la versione precedente.' },
      ]),
    },
    {
      id: 'scarica',
      h2: 'Porta il Fediverso sul tuo dispositivo',
      sub: 'Linux, Windows o Android. Nessuna registrazione su un\'istanza, nessun server da pagare.',
      body: cta([
        { href: LATEST, label: 'Scarica Fedi3', primary: true },
        { href: base('it/download/'), label: 'Guida all\'installazione' },
      ]) + facts(['Linux AppImage', 'Debian/Ubuntu', 'Windows x64', 'Android APK', 'SHA256']),
    },
  ],
  faq: [
    { q: 'Fedi3 è un\'alternativa a Mastodon?', a: 'È un client del Fediverso, quindi parla ActivityPub con Mastodon e Misskey. La differenza non è il protocollo: è dove vive l\'account. Su Mastodon l\'account sta nell\'istanza; su Fedi3 sta nel core locale sul tuo dispositivo.' },
    { q: 'Devo gestire un server per usare Fedi3?', a: 'No. Un account solo peer-to-peer funziona senza relay. Il relay serve per la federazione con istanze classiche e per la consegna mentre il dispositivo è offline.' },
    { q: 'Che cosa vede il relay di Fedi3?', a: 'I post pubblici sono pubblici. La chat privata è cifrata nel client: il relay conserva ciphertext e dati pubblici. Non c\'è ancora forward secrecy, ed è documentato.' },
    { q: 'Su quali piattaforme gira Fedi3?', a: 'Linux (AppImage e pacchetto Debian/Ubuntu), Windows x64 e Android (arm64-v8a e x86_64). iOS e macOS sono nel progetto Flutter ma non hanno pacchetti pubblicati.' },
    { q: 'Fedi3 è pronto per tutti?', a: 'È pre-1.0. Le funzioni principali funzionano e sono testate, ma tra release minori possono cambiare. Le note di rilascio dicono cosa è aperto.' },
    { q: 'Che licenza ha Fedi3?', a: 'AGPL-3.0-only. Il codice è su GitHub e ogni release pubblica i pacchetti con checksum SHA256.' },
  ],
  schemaType: 'WebPage',
  script: 'assets/net.js',
};

// ---------------------------------------------------------------- fediverso

const fediverso = {
  lang: 'it',
  slug: 'it/fediverso/',
  title: 'Che cos’è il Fediverso: guida 2026 ad ActivityPub',
  description: 'Il Fediverso è la rete sociale decentralizzata che parla ActivityPub. Guida a come funziona, al limite delle istanze Mastodon e Misskey e al ruolo di Fedi3.',
  keywords: ['fediverso', 'che cos\'è il fediverso', 'activitypub', 'rete sociale decentralizzata', 'mastodon', 'misskey', 'istanza mastodon', 'fedi3', 'client del fediverso', 'federazione', 'social decentralizzato'],
  subject: 'Guida al Fediverso e ad ActivityPub',
  ogImage: 'assets/og/fedi3-fediverso.png',
  ogAlt: 'Il Fediverso spiegato: ActivityPub, istanze e client Fedi3',
  alt: { it: 'it/fediverso/', en: 'fediverse/' },
  breadcrumb: [{ label: 'Fedi3', href: 'it/' }, { label: 'Fediverso', href: 'it/fediverso/' }],
  hero: heroBlock({
    eyebrow: 'Guida al Fediverso',
    h1: 'Che cos\'è il Fediverso: guida ad ActivityPub',
    lead: 'Il Fediverso è la rete di server indipendenti che si parlano con ActivityPub. Fedi3 appartiene a quella rete e ne cambia un pezzo: l\'account non è più ospitato da qualcun altro.',
    buttons: [
      { href: base('it/activitypub/'), label: 'Leggi come funziona ActivityPub' },
      { href: LATEST, label: 'Scarica Fedi3', primary: true },
    ],
    factList: ['ActivityPub', 'WebFinger', 'HTTP signatures', 'AGPL-3.0-only'],
    image: 'assets/img/desktop-profile.webp',
    imageAlt: 'Profilo Fedi3 con post federati dal Fediverso',
    tocItems: [
      { id: 'definizione', label: 'Definizione' },
      { id: 'come-funziona', label: 'Come funziona' },
      { id: 'storia', label: 'Mastodon e Misskey' },
      { id: 'limite', label: 'Il limite delle istanze' },
      { id: 'fedi3', label: 'Il ruolo di Fedi3' },
      { id: 'iniziare', label: 'Come iniziare' },
    ],
  }),
  sections: [
    {
      id: 'definizione',
      h: 2,
      h2: 'Fediverso: la definizione che conta',
      sub: 'Una definizione asciutta, utile anche quando la chiedi a un motore di ricerca o a un assistente.',
      body: brief('Definizione', `<p>Il <strong>Fediverso</strong> è l'insieme delle reti sociali indipendenti che si federano tra loro usando protocolli aperti, principalmente <strong>ActivityPub</strong>. Ogni server, chiamato istanza, ha un amministratore, delle regole e un database; la federazione permette a utenti di istanze diverse di seguirti, commentare e condividere.</p><p><strong>Fedi3</strong> è un client del Fediverso: parla ActivityPub, ma l'account e le chiavi stanno sul dispositivo invece che nell'istanza.</p>`)
        + prose(`<p>Nel Fediverso convivono software diversi: Mastodon per i microblog, Misskey per le note e le reazioni, PeerTube per i video, Pixelfed per le immagini, Lemmy per i forum. Non esiste un'autorità centrale che decida chi entra: esiste un protocollo che permette ai server di riconoscersi e scambiarsi attività.</p>
<p>La parola è nata nel 2017 come abbreviazione di federated universe, e descrive una rete, non un prodotto. Quando qualcuno dice "sono sul Fediverso" di solito intende: ho un account su un'istanza che parla ActivityPub e posso interagire con le altre.</p>`),
    },
    {
      id: 'come-funziona',
      h: 2,
      h2: 'Come si parla ActivityPub',
      sub: 'Quattro pezzi spiegano quasi tutto: attori, inbox, firme e scoperta.',
      body: cardGrid([
        { title: 'Attori', text: 'Un utente o un server è un attore ActivityPub con un documento JSON pubblico: nome, chiave pubblica, inbox e outbox.' },
        { title: 'Inbox e outbox', text: 'Le attività arrivano nell\'inbox del destinatario e partono dall\'outbox del mittente. Follow, Create, Announce e Like sono attività standard.' },
        { title: 'Firme HTTP', text: 'Ogni richiesta è firmata con la chiave privata dell\'attore. Il server che riceve verifica la firma contro la chiave pubblica pubblicata.' },
        { title: 'Scoperta', text: 'WebFinger risolve un indirizzo come @utente@dominio nel documento dell\'attore. È il motivo per cui un indirizzo del Fediverso funziona tra software diversi.' },
      ]) + prose(`<p>Un relay di federazione è un servizio che inoltra attività tra istanze: non è obbligatorio, ma aiuta i server piccoli a vedersi. Fedi3 usa un relay per la consegna offline e per raggiungere le istanze tradizionali, e aggiunge un livello peer-to-peer per la consegna diretta.</p>`),
    },
    {
      id: 'storia',
      h: 2,
      h2: 'Mastodon e Misskey: quello che hanno insegnato',
      sub: 'Due progetti hanno dimostrato che una rete sociale senza un padrone unico può funzionare. Il loro limite è il modello, non il protocollo.',
      body: splitBlock({ copy: prose(`<p>Mastodon ha reso il Fediverso accessibile a milioni di persone: un'istanza, un'app, una timeline federata. Misskey ha aggiunto le note, le reazioni e il markup MFM, con un'estetica e una community proprie. Entrambi hanno dimostrato che la federazione non è una teoria.</p>
<p>Hanno anche mostrato il costo del modello a istanze: qualcuno deve pagare l'hosting, aggiornare il software, moderare e rispondere dei dati. Chi vuole partecipare deve scegliere un server, accettarne le regole e affidargli la propria identità. Quando un'istanza chiude, gli utenti migrano e i follower vanno ricostruiti.</p>
<p>Fedi3 non rinnega quel percorso: ne prende il protocollo e sposta l'identità sul dispositivo. Mastodon e Misskey restano interlocutori con cui parlare, non avversari da sostituire.</p>`), visual: appFrame('timeline') }),
    },
    {
      id: 'limite',
      h: 2,
      h2: 'Il limite delle istanze, spiegato senza polemica',
      sub: 'Un\'istanza è un server: la sua affidabilità è la tua, e la sua amministrazione è la tua.',
      body: prose(`<p>Un'istanza è un punto unico: se cade, il tuo account non è raggiungibile; se cambia regole, la tua presenza cambia; se l'amministratore perde interesse, devi migrare. La federazione distribuisce la conversazione, non l'identità.</p>
<p>C'è anche un limite di privacy: i messaggi diretti su un'istanza classica passano e restano sul server, quindi l'amministratore può leggerli. La cifratura end-to-end non è parte del modello di Mastodon e Misskey.</p>
<p>Il costo si vede nelle community piccole: un server da mantenere per poche decine di persone è un impegno continuo. Il Fediverso funziona meglio quando partecipare non richiede di diventare amministratori.</p>`) + brief('La conseguenza', `<p>Se l'identità sta sul dispositivo, il server diventa un servizio, non un padrone di casa. Puoi cambiare relay senza cambiare account, e puoi restare raggiungibile anche senza relay grazie al livello peer-to-peer.</p>`),
    },
    {
      id: 'fedi3',
      h: 2,
      h2: 'Dove si colloca Fedi3 nel Fediverso',
      sub: 'Un client con un core locale: il pezzo che mancava tra il protocollo aperto e l\'utente.',
      body: prose(`<p>Fedi3 è un'applicazione, non un\'istanza. Il client Flutter mostra timeline, profilo, chat e compositore; il core Rust locale gestisce identità, firme ActivityPub, cifratura e consegna. Il relay pubblico è un servizio opzionale per la federazione e la consegna offline.</p>
<p>Per il resto del Fediverso, Fedi3 è un attore come gli altri: pubblica su ActivityPub, riceve nell'inbox, risponde alle firme HTTP. Un utente Mastodon può seguire un utente Fedi3 senza installare nulla di nuovo, e viceversa.</p>
<p>La differenza è nel punto di controllo: la chiave privata non è nel database di un server condiviso, è nel core sul tuo dispositivo. È questo che rende Fedi3 un client del Fediverso e non l'ennesima istanza.</p>`) + cardGrid([
        { title: 'Relay', text: 'Un servizio pubblico opzionale per consegna offline, federazione e directory dei relay.' },
        { title: 'Core locale', text: 'Il runtime Rust che genera l\'identità, firma le attività e cifra le chat sul dispositivo.' },
        { title: 'P2P', text: 'Il livello peer-to-peer che collega due dispositivi senza un server in mezzo.' },
      ]),
    },
    {
      id: 'iniziare',
      h: 2,
      h2: 'Come entrare nel Fediverso con Fedi3',
      sub: 'Tre passi, nessuna registrazione su un\'istanza.',
      body: steps([
        { title: 'Scarica il client', text: 'Prendi il pacchetto per Linux, Windows o Android dalla pagina delle release e verifica il checksum SHA256.' },
        { title: 'Crea il profilo locale', text: 'Al primo avvio il core genera l\'identità sul dispositivo. Il profilo esiste anche senza rete.' },
        { title: 'Scegli un relay o resta P2P', text: 'Un relay abilita federazione e consegna offline; senza relay l\'account resta raggiungibile via peer-to-peer.' },
      ]) + `<p class="sub gap">Il Fediverso non è un\'app: è una rete di persone su software diversi. Fedi3 è il tuo modo di entrarci senza affittare una casa altrui. <a href="${base('it/download/')}">Guida all'installazione</a>.</p>` + appFrame('composer'),
    },
    {
      id: 'faq',
      h: 2,
      h2: 'Domande frequenti sul Fediverso',
      sub: 'Le domande che tornano sempre, con risposte brevi.',
      body: faqVisual() + qaList([
        { q: 'Il Fediverso è Mastodon?', a: 'No. Mastodon è uno dei software che parlano ActivityPub. Il Fediverso comprende anche Misskey, PeerTube, Pixelfed, Lemmy e altri, oltre a client come Fedi3.' },
        { q: 'Serve un\'istanza per stare nel Fediverso?', a: 'Con Fedi3 no. Il client parla ActivityPub dal dispositivo, con un relay opzionale per la federazione e la consegna offline.' },
        { q: 'Il Fediverso è sicuro?', a: 'La sicurezza dipende dal software e dalla configurazione. Fedi3 firma ogni attività con una chiave locale e cifra le chat nel client; i post pubblici restano pubblici.' },
        { q: 'Posso seguire utenti Mastodon da Fedi3?', a: 'Sì. La federazione è ActivityPub: un utente Mastodon e un utente Fedi3 si seguono come su due istanze diverse.' },
        { q: 'Fedi3 sostituisce Mastodon e Misskey?', a: 'No: li affianca. Fedi3 è un client che parla lo stesso protocollo, con un modello di identità diverso.' },
      ]),
    },
    {
      id: 'cta',
      h: 1,
      h2: 'Il Fediverso è la rete. Fedi3 è il client.',
      sub: 'Nessuna istanza da scegliere, nessun server da mantenere.',
      body: cta([
        { href: LATEST, label: 'Scarica Fedi3', primary: true },
        { href: base('it/activitypub/'), label: 'Capire ActivityPub' },
      ]),
    },
  ],
  faq: [
    { q: 'Il Fediverso è Mastodon?', a: 'No. Mastodon è uno dei software che parlano ActivityPub. Il Fediverso comprende anche Misskey, PeerTube, Pixelfed, Lemmy e altri, oltre a client come Fedi3.' },
    { q: 'Serve un\'istanza per stare nel Fediverso?', a: 'Con Fedi3 no. Il client parla ActivityPub dal dispositivo, con un relay opzionale per la federazione e la consegna offline.' },
    { q: 'Il Fediverso è sicuro?', a: 'La sicurezza dipende dal software e dalla configurazione. Fedi3 firma ogni attività con una chiave locale e cifra le chat nel client; i post pubblici restano pubblici.' },
    { q: 'Posso seguire utenti Mastodon da Fedi3?', a: 'Sì. La federazione è ActivityPub: un utente Mastodon e un utente Fedi3 si seguono come su due istanze diverse.' },
    { q: 'Fedi3 sostituisce Mastodon e Misskey?', a: 'No: li affianca. Fedi3 è un client che parla lo stesso protocollo, con un modello di identità diverso.' },
  ],
  article: true,
  schemaType: 'Article',
  about: ['Fediverso', 'ActivityPub', 'Mastodon', 'Misskey', 'Fedi3'],
  modified: '2026-11-15',
};

// ---------------------------------------------------------------- activitypub

const activitypub = {
  lang: 'it',
  slug: 'it/activitypub/',
  title: 'ActivityPub: come funziona il protocollo del Fediverso',
  description: 'ActivityPub è il protocollo aperto che collega Mastodon, Misskey e il Fediverso. Attori, inbox, firme HTTP, WebFinger e relay, spiegati con Fedi3.',
  keywords: ['activitypub', 'protocollo activitypub', 'fediverso', 'mastodon', 'misskey', 'webfinger', 'http signatures', 'attore activitypub', 'fedi3', 'relay activitypub'],
  subject: 'ActivityPub, il protocollo del Fediverso',
  script: 'assets/net.js',
  ogImage: 'assets/og/fedi3-activitypub.png',
  ogAlt: 'ActivityPub spiegato: attori, inbox e firme HTTP',
  alt: { it: 'it/activitypub/', en: 'activitypub/' },
  breadcrumb: [{ label: 'Fedi3', href: 'it/' }, { label: 'ActivityPub', href: 'it/activitypub/' }],
  hero: heroBlock({
    eyebrow: 'Protocollo del Fediverso',
    h1: 'ActivityPub, il protocollo che tiene insieme il Fediverso',
    lead: 'ActivityPub è lo standard aperto con cui Mastodon, Misskey e il resto del Fediverso si scambiano post, follow e reazioni. Fedi3 lo implementa in un core locale, sul tuo dispositivo.',
    buttons: [
      { href: base('it/fediverso/'), label: 'Che cos\'è il Fediverso' },
      { href: LATEST, label: 'Scarica Fedi3', primary: true },
    ],
    factList: ['W3C standard', 'JSON-LD', 'HTTP signatures', 'WebFinger'],
    image: 'assets/img/desktop-composer.webp',
    imageAlt: 'Compositore Fedi3 con toolbar MFM e anteprima di un post ActivityPub',
    tocItems: [
      { id: 'cosè', label: 'Che cos\'è' },
      { id: 'attore', label: 'Un attore' },
      { id: 'firme', label: 'Firme e scoperta' },
      { id: 'relay', label: 'Relay' },
      { id: 'fedi3', label: 'Fedi3' },
      { id: 'nonè', label: 'Cosa non è' },
    ],
  }),
  sections: [
    {
      id: 'cosè',
      h: 2,
      h2: 'ActivityPub: che cos\'è e a cosa serve',
      sub: 'Una definizione breve, poi i pezzi che contano davvero.',
      body: brief('Definizione', `<p><strong>ActivityPub</strong> è un protocollo W3C per le reti sociali decentralizzate. Definisce attori, oggetti e attività che i server si scambiano, e permette a software diversi di federarsi: un utente di Mastodon può seguire un utente di Misskey, PeerTube o Fedi3.</p>`)
        + prose(`<p>Il protocollo è aperto e documentato: chiunque può implementarlo senza chiedere permesso. La federazione non dipende da un'autorità centrale; dipende da server che pubblicano documenti JSON e si scambiano attività firmate.</p>
<p>ActivityPub non è un'app né un servizio: è il linguaggio. Il Fediverso è la rete che lo parla. Fedi3 è un client che lo parla dal dispositivo.</p>
<p>In pratica il protocollo definisce un vocabolario ristretto: attori (persone o server), oggetti (note, immagini, video) e attività (Create, Follow, Like, Announce). Un server li pubblica come documenti JSON-LD a URL stabili, e gli altri server li scaricano e li verificano.</p>`),
    },
    {
      id: 'attore',
      h: 2,
      h2: 'Com\'è fatto un attore ActivityPub',
      sub: 'Un attore è un documento JSON pubblico con identità, chiavi e caselle di posta.',
      body: splitBlock({ copy: prose(`<p>Un attore ActivityPub espone un documento con un identificatore, un nome utente, una chiave pubblica e due endpoint: inbox e outbox. L'inbox riceve le attività indirizzate all'attore; l'outbox pubblica quelle che l'attore genera.</p>
<p>Un profilo Fedi3 è un attore come gli altri. La differenza è che il documento è servito dal core locale o dal relay, mentre la chiave privata resta nel core.</p>`) + `<pre><code>{
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
      id: 'firme',
      h: 2,
      h2: 'Firme HTTP e WebFinger: come ci si riconosce',
      sub: 'Due meccanismi rendono la federazione verificabile e gli indirizzi portabili.',
      body: cardGrid([
        { title: 'Firme HTTP', text: 'Ogni richiesta tra server è firmata. Il destinatario verifica la firma contro la chiave pubblica dell\'attore: una richiesta non firmata non entra.' },
        { title: 'WebFinger', text: 'Un indirizzo @utente@dominio viene risolto nel documento dell\'attore. È il motivo per cui lo stesso handle funziona su software diversi.' },
        { title: 'NodeInfo', text: 'Un documento pubblico descrive software e versione del server. Fedi3 pubblica software.name fedi3, così le directory lo classificano correttamente.' },
        { title: 'JSON-LD', text: 'Attività e oggetti sono JSON-LD: leggibili da macchine diverse, con vocabolari condivisi e contesti verificabili.' },
      ]) + prose(`<p>La firma è il punto in cui il modello classico mostra il limite: la chiave privata sta sul server, quindi il server può agire per te. In Fedi3 la chiave sta nel core locale, e il relay inoltra attività già firmate.</p>`),
    },
    {
      id: 'relay',
      h: 2,
      h2: 'Relay e federazione: a cosa serve un relay ActivityPub',
      sub: 'Un relay non è un\'istanza: è un servizio che aiuta la consegna e la scoperta.',
      body: splitBlock({ copy: prose(`<p>Un relay di federazione inoltra attività tra istanze e permette ai server piccoli di vedere contenuti che altrimenti non raggiungerebbero. Non ospita la tua identità e non sostituisce la tua istanza.</p>
<p>Fedi3 usa il relay per due cose: la consegna mentre il dispositivo è offline e la federazione con le istanze tradizionali. Il livello peer-to-peer aggiunge la consegna diretta tra dispositivi, che non passa dal relay.</p>
<p>I relay pubblici sono elencati in una directory aperta nel repository; chiunque può aggiungere il proprio. Il relay pubblica anche metriche aggregate e uno snapshot di trasparenza firmato.</p>`) + relayList(RELAYS, 'Directory dei relay Fedi3: due relay pubblici al 15 novembre 2026.'), visual: networkVisual() }),
    },
    {
      id: 'fedi3',
      h: 2,
      h2: 'Come Fedi3 implementa ActivityPub',
      sub: 'Il protocollo nel core locale, con relay opzionale e livello peer-to-peer.',
      body: prose(`<p>Il core Fedi3 è scritto in Rust e implementa attori, inbox e outbox, firme HTTP e WebFinger. Il client Flutter non parla direttamente con il Fediverso: passa dal core, che firma e consegna.</p>
<p>Il relay è un servizio pubblico separato, con database, media store e peering HTTP tra relay. Il core può usarlo per la federazione o funzionare solo in peer-to-peer.</p>
<p>Il progetto pubblica anche la documentazione OpenAPI, gli endpoint di salute e le metriche del relay: l'infrastruttura è verificabile, non solo dichiarata.</p>`) + cardGrid([
        { title: 'Core locale', text: 'Attori, firme, cifratura e consegna nel runtime Rust sul dispositivo.' },
        { title: 'Relay', text: 'Servizio pubblico opzionale per federazione, consegna offline e directory.' },
        { title: 'Trasparenza', text: 'Metriche aggregate, snapshot firmati e release con checksum nel repository.' },
      ]),
    },
    {
      id: 'nonè',
      h: 2,
      h2: 'Cosa ActivityPub non è',
      sub: 'Tre confusioni frequenti, chiarite.',
      body: `<div class="cards">
  <div class="card"><h3>Non è AT Protocol</h3><p>Bluesky usa AT Protocol, un protocollo diverso. ActivityPub e AT Protocol non sono lo stesso standard, anche se esistono ponti tra le due reti.</p></div>
  <div class="card"><h3>Non è un server</h3><p>ActivityPub è un protocollo. Un server è un'implementazione. Fedi3 implementa il protocollo in un client con un core locale.</p></div>
  <div class="card"><h3>Non è una blockchain</h3><p>Non c'è consenso distribuito né token: ci sono server che si scambiano attività firmate e si riconoscono con chiavi pubbliche.</p></div>
</div>` + prose(`<p>ActivityPub è un protocollo, non un'azienda: nessun fornitore lo possiede e un server può implementarlo senza chiedere permesso. Per questo Mastodon, Misskey, PeerTube e Fedi3 possono federare anche se li costruiscono team diversi.</p>`),
    },
    {
      id: 'faq',
      h: 2,
      h2: 'Domande frequenti su ActivityPub',
      sub: 'Risposte tecniche, senza gergo inutile.',
      body: faqVisual() + qaList([
        { q: 'ActivityPub è lo stesso protocollo di Mastodon?', a: 'Mastodon implementa ActivityPub. Il protocollo è più ampio del software: anche Misskey, PeerTube, Pixelfed, Lemmy e Fedi3 lo implementano.' },
        { q: 'Serve un server per usare ActivityPub?', a: 'Per la federazione classica serve un endpoint raggiungibile, che può essere un relay. Fedi3 aggiunge la consegna peer-to-peer per i casi senza server.' },
        { q: 'ActivityPub è sicuro?', a: 'Il protocollo prevede firme HTTP sulle richieste e chiavi pubbliche verificabili. La riservatezza dei messaggi dipende dall\'implementazione: Fedi3 cifra le chat nel client.' },
        { q: 'Posso usare ActivityPub con Bluesky?', a: 'Bluesky parla AT Protocol. Esistono ponti che traducono tra i due protocolli, ma non sono la stessa cosa.' },
      ]),
    },
    {
      id: 'cta',
      h: 1,
      h2: 'ActivityPub è il linguaggio. Fedi3 è il client.',
      sub: 'Il protocollo resta aperto; l\'identità torna sul tuo dispositivo.',
      body: cta([
        { href: LATEST, label: 'Scarica Fedi3', primary: true },
        { href: base('it/fediverso/'), label: 'Guida al Fediverso' },
      ]),
    },
  ],
  faq: [
    { q: 'ActivityPub è lo stesso protocollo di Mastodon?', a: 'Mastodon implementa ActivityPub. Il protocollo è più ampio del software: anche Misskey, PeerTube, Pixelfed, Lemmy e Fedi3 lo implementano.' },
    { q: 'Serve un server per usare ActivityPub?', a: 'Per la federazione classica serve un endpoint raggiungibile, che può essere un relay. Fedi3 aggiunge la consegna peer-to-peer per i casi senza server.' },
    { q: 'ActivityPub è sicuro?', a: 'Il protocollo prevede firme HTTP sulle richieste e chiavi pubbliche verificabili. La riservatezza dei messaggi dipende dall\'implementazione: Fedi3 cifra le chat nel client.' },
    { q: 'Posso usare ActivityPub con Bluesky?', a: 'Bluesky parla AT Protocol. Esistono ponti che traducono tra i due protocolli, ma non sono la stessa cosa.' },
  ],
  article: true,
  schemaType: 'Article',
  about: ['ActivityPub', 'Fediverso', 'Mastodon', 'Misskey', 'Fedi3'],
  modified: '2026-11-15',
};

// ---------------------------------------------------------------- vs mastodon

const vsMastodon = {
  lang: 'it',
  slug: 'it/vs/mastodon/',
  title: 'Fedi3 vs Mastodon: dove vive il tuo account',
  description: 'Confronto tra Fedi3 e Mastodon: stesso ActivityPub e stesso Fediverso. Fedi3 tiene account e chiavi sul dispositivo, non richiede un\'istanza e aggiunge chat cifrata e P2P.',
  keywords: ['fedi3 vs mastodon', 'alternativa a mastodon', 'mastodon alternativa', 'confronto mastodon fedi3', 'client fediverso', 'istanza mastodon', 'activitypub', 'chat cifrata mastodon'],
  subject: 'Confronto Fedi3 e Mastodon',
  ogImage: 'assets/og/fedi3-mastodon.png',
  ogAlt: 'Fedi3 contro Mastodon: client del Fediverso o istanza',
  alt: { it: 'it/vs/mastodon/', en: 'vs/mastodon/' },
  breadcrumb: [{ label: 'Fedi3', href: 'it/' }, { label: 'Fedi3 vs Mastodon', href: 'it/vs/mastodon/' }],
  hero: heroBlock({
    eyebrow: 'Confronto',
    h1: 'Fedi3 o Mastodon: due modelli di client',
    lead: 'Entrambi parlano ActivityPub ed entrambi stanno nel Fediverso. Mastodon è un\'istanza che qualcuno amministra; Fedi3 è un client che tiene account e chiavi sul tuo dispositivo.',
    buttons: [
      { href: LATEST, label: 'Scarica Fedi3', primary: true },
      { href: base('it/fediverso/'), label: 'Che cos\'è il Fediverso' },
    ],
    factList: ['ActivityPub', 'Stesso Fediverso', 'AGPL-3.0-only', 'pre-1.0'],
    image: 'assets/img/desktop-timeline.webp',
    imageAlt: 'Timeline Fedi3 con post federati da istanze Mastodon',
    tocItems: [
      { id: 'tabella', label: 'Tabella' },
      { id: 'account', label: 'Il tuo account' },
      { id: 'uguale', label: 'Cosa resta uguale' },
      { id: 'verdetto', label: 'Quando scegliere' },
      { id: 'faq', label: 'Domande' },
    ],
  }),
  sections: [
    {
      id: 'tabella',
      h: 2,
      h2: 'Fedi3 e Mastodon, a confronto',
      sub: 'Le differenze che contano per chi sceglie dove mettere la propria identità.',
      body: compareTable({
        caption: 'Confronto Fedi3-Mastodon, novembre 2026',
        head: ['', 'Mastodon', 'Fedi3'],
        rows: [
          ['Modello', 'Istanza ospitata, amministrata da terzi', 'Client con core locale sul dispositivo'],
          ['Account e chiavi', 'Nel database dell\'istanza', 'Nel core locale; il relay non le possiede'],
          ['Server da gestire', 'Sì, per te o per chi ti ospita', 'No: relay opzionale'],
          ['Chat privata', 'Messaggi diretti sul server', 'Cifrata end-to-end nel client'],
          ['Chiamate', 'Non nel client classico', 'Audio e video su ICE/TURN, schermo su desktop'],
          ['Consegna senza server', 'No', 'Sì, livello peer-to-peer'],
          ['Federazione', 'ActivityPub', 'ActivityPub, relay opzionale'],
          ['Piattaforme', 'Web, iOS, Android', 'Linux, Windows, Android'],
          ['Licenza', 'AGPL-3.0', 'AGPL-3.0-only'],
        ],
      }),
    },
    {
      id: 'account',
      h: 2,
      h2: 'Cosa cambia per il tuo account',
      sub: 'La migrazione tra istanze Mastodon è un\'operazione; su Fedi3 l\'identità è già sul dispositivo.',
      body: prose(`<p>Su Mastodon, cambiare istanza significa spostare l'account, aggiornare i follower e convivere con i limiti del nuovo server. L'indirizzo @utente@dominio cambia, e con esso la storia che gli altri vedono.</p>
<p>Su Fedi3 l'identità nasce e resta sul dispositivo. Il relay è un servizio che puoi cambiare: la chiave privata non si sposta, e il profilo non dipende dal dominio di un hoster.</p>
<p>Questo non rende Fedi3 immune ai problemi: un dispositivo perso è un'identità da recuperare, e per questo il progetto prevede il trasferimento tra dispositivi con QR e spool cifrato.</p>`) + brief('Il punto pratico', `<p>Se il tuo timore è perdere l'account quando un'istanza chiude, Fedi3 risolve il problema alla radice: l'account non è ospitato. Se invece cerchi una community già popolata, Mastodon resta la scelta più diretta.</p>`),
    },
    {
      id: 'uguale',
      h: 2,
      h2: 'Cosa resta uguale: il Fediverso',
      sub: 'La federazione non cambia. Un utente Mastodon e un utente Fedi3 si seguono come su due istanze diverse.',
      body: prose(`<p>Fedi3 non è una rete separata con un ponte: pubblica attività ActivityPub, riceve nell'inbox e risponde alle firme HTTP. Un'istanza Mastodon vede un attore normale, con WebFinger e chiave pubblica.</p>
<p>Questo significa che non devi convincere nessuno a installare Fedi3: i tuoi post arrivano nella timeline federata, e chi è su Mastodon o Misskey ti segue senza cambiare software.</p>
<p>Il Fediverso resta il contesto: Mastodon, Misskey, PeerTube, Pixelfed, Lemmy. Fedi3 è un modo diverso di entrarci, non un universo parallelo.</p>`) + cardGrid([
        { title: 'Stesso protocollo', text: 'ActivityPub, WebFinger e firme HTTP: interoperabilità reale con Mastodon.' },
        { title: 'Stessa rete', text: 'Post, follow, reazioni e media circolano nel Fediverso come su un\'istanza.' },
        { title: 'Nessun ponte', text: 'Non serve un bridge: Fedi3 parla il protocollo nativo del Fediverso.' },
      ]),
    },
    {
      id: 'verdetto',
      h: 1,
      h2: 'Quando Fedi3 conviene, e quando Mastodon conviene',
      sub: 'Due scelte legittime, con criteri diversi.',
      body: verdict({
        left: {
          title: 'Scegli Fedi3 se',
          items: [
            'Vuoi l\'account e le chiavi sul tuo dispositivo.',
            'Ti serve una chat cifrata end-to-end con chiamate.',
            'Vuoi federarti senza gestire un server.',
            'Ti interessa la consegna peer-to-peer.',
            'Accetti un progetto pre-1.0 con release frequenti.',
          ],
        },
        right: {
          title: 'Scegli Mastodon se',
          items: [
            'Vuoi un\'istanza pubblica già pronta con una community.',
            'Ti serve un\'app iOS o macOS pubblicata.',
            'Vuoi la moderazione centralizzata di un amministratore.',
            'Preferisci l\'interfaccia web e il modello a istanze.',
            'Vuoi un progetto con anni di rodaggio alle spalle.',
          ],
        },
      }),
    },
    {
      id: 'faq',
      h: 2,
      h2: 'Domande frequenti su Fedi3 e Mastodon',
      sub: 'Le obiezioni più comuni, con risposte verificabili.',
      body: faqVisual() + qaList([
        { q: 'Fedi3 sostituisce Mastodon?', a: 'No. Fedi3 parla lo stesso protocollo e affianca Mastodon: cambia il modello di identità, non la rete.' },
        { q: 'Posso seguire utenti Mastodon da Fedi3?', a: 'Sì. La federazione è ActivityPub: il tuo attore Fedi3 è raggiungibile e può seguire attori su istanze Mastodon.' },
        { q: 'Devo abbandonare la mia istanza Mastodon?', a: 'No. Puoi tenere entrambi: Fedi3 è un client aggiuntivo, non un fork che richiede di migrare tutto.' },
        { q: 'Perché Fedi3 non ha un\'app iOS?', a: 'Il progetto Flutter include i runner iOS e macOS, ma i pacchetti pubblicati oggi sono Linux, Windows e Android.' },
      ]),
    },
    {
      id: 'cta',
      h: 1,
      h2: 'Prova Fedi3 accanto alla tua istanza',
      sub: 'Stesso Fediverso, identità sul dispositivo.',
      body: cta([
        { href: LATEST, label: 'Scarica Fedi3', primary: true },
        { href: base('it/vs/misskey/'), label: 'Confronta con Misskey' },
      ]),
    },
  ],
  faq: [
    { q: 'Fedi3 sostituisce Mastodon?', a: 'No. Fedi3 parla lo stesso protocollo e affianca Mastodon: cambia il modello di identità, non la rete.' },
    { q: 'Posso seguire utenti Mastodon da Fedi3?', a: 'Sì. La federazione è ActivityPub: il tuo attore Fedi3 è raggiungibile e può seguire attori su istanze Mastodon.' },
    { q: 'Devo abbandonare la mia istanza Mastodon?', a: 'No. Puoi tenere entrambi: Fedi3 è un client aggiuntivo, non un fork che richiede di migrare tutto.' },
    { q: 'Perché Fedi3 non ha un\'app iOS?', a: 'Il progetto Flutter include i runner iOS e macOS, ma i pacchetti pubblicati oggi sono Linux, Windows e Android.' },
  ],
  article: true,
  schemaType: 'Article',
  about: ['Mastodon', 'Fediverso', 'ActivityPub', 'Fedi3'],
  modified: '2026-11-15',
};

// ---------------------------------------------------------------- vs misskey

const vsMisskey = {
  lang: 'it',
  slug: 'it/vs/misskey/',
  title: 'Fedi3 vs Misskey: stesso Fediverso, account diverso',
  description: 'Fedi3 e Misskey parlano ActivityPub e condividono il Fediverso. Misskey è un\'istanza con MFM; Fedi3 è un client con account locale, chat cifrata e livello P2P.',
  keywords: ['fedi3 vs misskey', 'alternativa a misskey', 'misskey alternativa', 'confronto misskey fedi3', 'mfm', 'client fediverso', 'istanza misskey', 'activitypub'],
  subject: 'Confronto Fedi3 e Misskey',
  ogImage: 'assets/og/fedi3-misskey.png',
  ogAlt: 'Fedi3 contro Misskey: client del Fediverso o istanza',
  alt: { it: 'it/vs/misskey/', en: 'vs/misskey/' },
  breadcrumb: [{ label: 'Fedi3', href: 'it/' }, { label: 'Fedi3 vs Misskey', href: 'it/vs/misskey/' }],
  hero: heroBlock({
    eyebrow: 'Confronto',
    h1: 'Fedi3 o Misskey: due modelli di client',
    lead: 'Misskey ha portato note, reazioni e MFM nel Fediverso. Fedi3 prende lo stesso protocollo e sposta l\'identità sul dispositivo, con chat cifrata e consegna peer-to-peer.',
    buttons: [
      { href: LATEST, label: 'Scarica Fedi3', primary: true },
      { href: base('it/fediverso/'), label: 'Guida al Fediverso' },
    ],
    factList: ['ActivityPub', 'MFM', 'AGPL-3.0-only', 'pre-1.0'],
    image: 'assets/img/desktop-composer.webp',
    imageAlt: 'Compositore Fedi3 con toolbar MFM',
    tocItems: [
      { id: 'tabella', label: 'Tabella' },
      { id: 'mfm', label: 'MFM e note' },
      { id: 'uguale', label: 'Cosa resta uguale' },
      { id: 'verdetto', label: 'Quando scegliere' },
      { id: 'faq', label: 'Domande' },
    ],
  }),
  sections: [
    {
      id: 'tabella',
      h: 2,
      h2: 'Fedi3 e Misskey, a confronto',
      sub: 'Due modi di stare nel Fediverso, con un modello di identità diverso.',
      body: compareTable({
        caption: 'Confronto Fedi3-Misskey, novembre 2026',
        head: ['', 'Misskey', 'Fedi3'],
        rows: [
          ['Modello', 'Istanza ospitata, amministrata da terzi', 'Client con core locale sul dispositivo'],
          ['Account e chiavi', 'Nel database dell\'istanza', 'Nel core locale; il relay non le possiede'],
          ['Server da gestire', 'Sì, per te o per chi ti ospita', 'No: relay opzionale'],
          ['Chat privata', 'Sul server', 'Cifrata end-to-end nel client'],
          ['Chiamate', 'Non nel client classico', 'Audio e video su ICE/TURN, schermo su desktop'],
          ['Consegna senza server', 'No', 'Sì, livello peer-to-peer'],
          ['Personalizzazione', 'Plugin, temi e widget dell\'istanza', 'Interfaccia del client, tema coerente'],
          ['Piattaforme', 'Web, iOS, Android', 'Linux, Windows, Android'],
          ['Licenza', 'AGPL-3.0', 'AGPL-3.0-only'],
        ],
      }) + prose(`<p>Misskey è un prodotto ricco: reazioni, note, MFM, plugin. Fedi3 non cerca di replicare tutto: punta su identità locale, chat cifrata e federazione senza server, con un compositore che parla MFM.</p>`),
    },
    {
      id: 'mfm',
      h: 2,
      h2: 'MFM e note: cosa resta di Misskey in Fedi3',
      sub: 'Il markup MFM è parte del Fediverso che Misskey ha reso popolare; Fedi3 lo supporta nel compositore.',
      body: prose(`<p>MFM è il linguaggio di markup che Misskey usa per animare e formattare le note. È diventato un tratto riconoscibile del Fediverso, e i client che lo ignorano perdono parte dei contenuti.</p>
<p>Il compositore Fedi3 include una toolbar MFM e mostra le note formattate nella timeline. Non è un clone di Misskey: è un client che parla lo stesso vocabolario.</p>
<p>Le reazioni, le note e i media circolano tra le due implementazioni tramite ActivityPub, come tra due istanze qualsiasi.</p>`) + cardGrid([
        { title: 'Toolbar MFM', text: 'Il compositore Fedi3 offre i comandi MFM più usati, con anteprima nella timeline.' },
        { title: 'Reazioni', text: 'Le reazioni alle note arrivano come attività ActivityPub standard.' },
        { title: 'Media', text: 'Immagini e video federati sono visibili nel client, con cache locale controllata.' },
      ]),
    },
    {
      id: 'uguale',
      h: 2,
      h2: 'Cosa resta uguale: il Fediverso',
      sub: 'La federazione non cambia: Misskey e Fedi3 si parlano tramite ActivityPub.',
      body: prose(`<p>Un utente Misskey può seguire un utente Fedi3 e viceversa, senza ponti e senza installare altro. La differenza è dove sta la chiave privata e chi amministra il server.</p>
<p>Il Fediverso resta lo stesso: Mastodon, Misskey, PeerTube, Pixelfed, Lemmy. Fedi3 è un modo diverso di entrarci, con un\'identità che non è ospitata da terzi.</p>`) + brief('La differenza in una frase', `<p>Misskey ti dà un\'istanza ricca e una community; Fedi3 ti dà un client con l\'account sul dispositivo e la federazione senza server.</p>`) + cardGrid([
        { title: 'Stesso protocollo', text: 'Misskey e Fedi3 si scambiano attività ActivityPub; un utente Misskey segue un utente Fedi3 senza ponti.' },
        { title: 'Stessa rete', text: 'Fedi3 pubblica nella timeline federata: nessuno deve installarlo per leggere i tuoi post.' },
        { title: 'Chiave diversa', text: 'La chiave privata resta nel core locale, non nel database dell\'istanza Misskey.' },
      ]),
    },
    {
      id: 'verdetto',
      h: 1,
      h2: 'Quando Fedi3 conviene, e quando Misskey conviene',
      sub: 'Nessuna delle due scelte è sbagliata: dipende da cosa vuoi controllare.',
      body: verdict({
        left: {
          title: 'Scegli Fedi3 se',
          items: [
            'Vuoi account e chiavi sul dispositivo.',
            'Ti serve una chat cifrata end-to-end.',
            'Vuoi federarti senza gestire un server.',
            'Ti interessa la consegna peer-to-peer.',
            'Vuoi un client leggero con MFM e ActivityPub.',
          ],
        },
        right: {
          title: 'Scegli Misskey se',
          items: [
            'Vuoi un\'istanza con plugin, temi e widget.',
            'Cerchi una community già attiva su un server.',
            'Ti serve un\'app iOS o macOS pubblicata.',
            'Vuoi la moderazione centralizzata.',
            'Preferisci l\'interfaccia e le reazioni di Misskey.',
          ],
        },
      }),
    },
    {
      id: 'faq',
      h: 2,
      h2: 'Domande frequenti su Fedi3 e Misskey',
      sub: 'Le differenze pratiche, senza gergo.',
      body: faqVisual() + qaList([
        { q: 'Fedi3 è un fork di Misskey?', a: 'No. Fedi3 è un client indipendente, scritto in Flutter con un core Rust, che implementa ActivityPub e MFM.' },
        { q: 'Posso seguire utenti Misskey da Fedi3?', a: 'Sì. La federazione ActivityPub rende gli attori Misskey e Fedi3 interoperabili.' },
        { q: 'Fedi3 supporta le reazioni di Misskey?', a: 'Le reazioni arrivano come attività ActivityPub; il client le mostra nella timeline.' },
        { q: 'Devo lasciare Misskey per usare Fedi3?', a: 'No. Puoi usare entrambi: Fedi3 è un client aggiuntivo con un modello di identità diverso.' },
      ]),
    },
    {
      id: 'cta',
      h: 1,
      h2: 'Porta Misskey e Fedi3 nella stessa timeline',
      sub: 'Stesso Fediverso, identità sul dispositivo.',
      body: cta([
        { href: LATEST, label: 'Scarica Fedi3', primary: true },
        { href: base('it/vs/mastodon/'), label: 'Confronta con Mastodon' },
      ]),
    },
  ],
  faq: [
    { q: 'Fedi3 è un fork di Misskey?', a: 'No. Fedi3 è un client indipendente, scritto in Flutter con un core Rust, che implementa ActivityPub e MFM.' },
    { q: 'Posso seguire utenti Misskey da Fedi3?', a: 'Sì. La federazione ActivityPub rende gli attori Misskey e Fedi3 interoperabili.' },
    { q: 'Fedi3 supporta le reazioni di Misskey?', a: 'Le reazioni arrivano come attività ActivityPub; il client le mostra nella timeline.' },
    { q: 'Devo lasciare Misskey per usare Fedi3?', a: 'No. Puoi usare entrambi: Fedi3 è un client aggiuntivo con un modello di identità diverso.' },
  ],
  article: true,
  schemaType: 'Article',
  about: ['Misskey', 'Fediverso', 'ActivityPub', 'Fedi3'],
  modified: '2026-11-15',
};

// ---------------------------------------------------------------- download

const download = {
  lang: 'it',
  slug: 'it/download/',
  title: 'Scarica Fedi3 per Linux, Windows e Android',
  description: 'Pacchetti Fedi3 0.12.3 per Linux AppImage e Debian/Ubuntu, Windows x64 e Android. Checksum SHA256, licenza AGPL-3.0-only, installazione in tre passi.',
  keywords: ['scaricare fedi3', 'fedi3 download', 'fedi3 linux', 'fedi3 windows', 'fedi3 android', 'appimage fedi3', 'apk fedi3', 'client fediverso', 'fedi3 0.12.3'],
  subject: 'Download e installazione di Fedi3',
  ogImage: 'assets/og/fedi3-download.png',
  ogAlt: 'Scarica Fedi3 per Linux, Windows e Android',
  alt: { it: 'it/download/', en: 'download/' },
  breadcrumb: [{ label: 'Fedi3', href: 'it/' }, { label: 'Scarica', href: 'it/download/' }],
  hero: heroBlock({
    eyebrow: 'Download',
    h1: 'Scarica Fedi3 e porta il Fediverso sul tuo dispositivo',
    lead: 'Linux, Windows e Android. Il pacchetto contiene il client e il core locale: nessuna registrazione su un\'istanza, nessun server da pagare.',
    buttons: [
      { href: LATEST, label: 'Scarica Fedi3', primary: true },
      { href: RELEASES, label: 'Tutte le release' },
    ],
    factList: ['Linux AppImage', 'Debian/Ubuntu', 'Windows x64', 'Android APK', 'SHA256'],
    image: 'assets/img/desktop-timeline.webp',
    imageAlt: 'Client Fedi3 su Linux con timeline federata',
    tocItems: [
      { id: 'pacchetti', label: 'Pacchetti' },
      { id: 'installazione', label: 'Installazione' },
      { id: 'primo-avvio', label: 'Primo avvio' },
      { id: 'relay', label: 'Relay o P2P' },
      { id: 'requisiti', label: 'Requisiti' },
      { id: 'faq', label: 'Domande' },
    ],
  }),
  sections: [
    {
      id: 'pacchetti',
      h: 1,
      h2: 'Pacchetti di Fedi3',
      sub: 'Ogni release pubblica i pacchetti con checksum SHA256 e note di rilascio bilingui.',
      body: `<div class="cards">
  <div class="card"><h3>Linux</h3><p>AppImage x86_64 e pacchetto Debian/Ubuntu amd64. Il core locale è incluso nel pacchetto.</p><p><a href="${LATEST}">Scarica per Linux</a></p></div>
  <div class="card"><h3>Windows</h3><p>Pacchetto x64 con core locale incluso. Firma e cifratura girano sulla macchina.</p><p><a href="${LATEST}">Scarica per Windows</a></p></div>
  <div class="card"><h3>Android</h3><p>APK arm64-v8a e x86_64 con firma di release. Aggiornamenti con lo stesso certificato.</p><p><a href="${LATEST}">Scarica per Android</a></p></div>
</div>` + `<p class="sub gap">iOS e macOS sono nel progetto Flutter ma non hanno pacchetti pubblicati in questa release. La versione è <strong>${VERSION}</strong> (build ${BUILD}), licenza ${LICENSE}.</p>`,
    },
    {
      id: 'installazione',
      h: 2,
      h2: 'Installazione in quattro passi',
      sub: 'Dal download al primo post federato.',
      body: steps([
        { title: 'Scarica il pacchetto', text: 'Apri la pagina delle release e prendi il file per il tuo sistema. Controlla la versione e il nome del file nella tabella degli asset.' },
        { title: 'Verifica il checksum', text: 'Confronta l\'SHA256 del file con quello pubblicato nella release. Su Linux: sha256sum Fedi3-*.AppImage.' },
        { title: 'Avvia il client', text: 'Installa o esegui il pacchetto. Al primo avvio il core locale genera l\'identità sul dispositivo e mostra l\'onboarding.' },
        { title: 'Configura la rete', text: 'In Impostazioni, Rete, inserisci un relay e un token di almeno 16 caratteri, oppure lascia il profilo in modalità peer-to-peer.' },
      ]) + `<p class="sub gap">Il relay pubblico del progetto è <a href="${RELAY}">${RELAY.replace(/^https?:\/\//, '')}</a>. Chi preferisce può installarne uno proprio con Docker Compose seguendo la <a href="${DOCS}/deploy_relay.md">guida al deploy</a>.</p>`,
    },
    {
      id: 'primo-avvio',
      h: 1,
      h2: 'Fedi3 al primo avvio: identità sul dispositivo',
      sub: 'Nessun modulo di registrazione su un\'istanza: il profilo è tuo dal primo secondo.',
      body: prose(`<p>Il core genera la coppia di chiavi, costruisce l'identità e prepara l'attore ActivityPub. Il profilo esiste anche senza rete: puoi scrivere, cifrare e conservare i contenuti prima di configurare un relay.</p>
<p>Quando aggiungi un relay, il core pubblica l'attore e inizia la federazione. Se cambi relay, l'identità resta la stessa: cambia solo il servizio che consegna le attività.</p>`) + brief('Cosa non serve', `<p>Non serve un account su un\'istanza, non serve un dominio, non serve pagare un hosting. Un relay è opzionale, e il livello peer-to-peer copre i casi senza server.</p>`),
    },
    {
      id: 'relay',
      h: 1,
      h2: 'Fedi3: relay o solo peer-to-peer',
      sub: 'Due configurazioni legittime, con scopi diversi.',
      body: `<div class="cards">
  <div class="card"><h3>Con relay</h3><p>Federazione con istanze Mastodon e Misskey, consegna offline, media e directory. Il relay non possiede le tue chiavi.</p></div>
  <div class="card"><h3>Solo peer-to-peer</h3><p>Nessun server, nessun relay: i dispositivi si trovano e si parlano direttamente. Adatto a coppie di dispositivi e reti locali.</p></div>
</div>` + relayList(RELAYS, 'Relay pubblici elencati nella directory del progetto al 15 novembre 2026.'),
    },
    {
      id: 'requisiti',
      h: 1,
      h2: 'Requisiti e verifica dei pacchetti Fedi3',
      sub: 'Cosa serve per eseguire Fedi3 e come controllare che il pacchetto sia integro.',
      body: prose(`<p>Su Linux servono le librerie desktop di Flutter e, per le credenziali nel keyring, libsecret e un Secret Service attivo. Su Windows il pacchetto x64 è autonomo. Su Android serve una versione recente del sistema e lo spazio per il core e la cache.</p>
<p>Ogni release pubblica i checksum SHA256: verifica il file prima di eseguirlo. Il progetto pubblica anche i gate di verifica e i report di release readiness nel repository.</p>`) + `<pre><code># verifica del pacchetto Linux
sha256sum Fedi3-${VERSION}-linux-x86_64.AppImage

# confronta l'hash con quello nella pagina della release</code></pre>`,
    },
    {
      id: 'faq',
      h: 2,
      h2: 'Domande frequenti sul download',
      sub: 'Dubbi pratici prima di installare.',
      body: faqVisual() + qaList([
        { q: 'Quale pacchetto devo scegliere?', a: 'Su Linux l\'AppImage è la via più diretta; il pacchetto Debian/Ubuntu è per chi preferisce il gestore di pacchetti. Su Windows serve il pacchetto x64. Su Android l\'APK arm64-v8a copre la maggior parte dei dispositivi.' },
        { q: 'Fedi3 è gratis?', a: 'Sì. Il client è software libero AGPL-3.0-only e non richiede abbonamenti. Un relay è opzionale e può essere pubblico o auto-ospitato.' },
        { q: 'Serve un account per iniziare?', a: 'No. Il primo avvio crea l\'identità sul dispositivo; il relay si aggiunge dopo, se serve.' },
        { q: 'Come aggiorno Fedi3?', a: 'Scarica la nuova release e verifica il checksum. Su Android l\'aggiornamento richiede lo stesso certificato di firma della versione installata.' },
      ]),
    },
    {
      id: 'cta',
      h: 1,
      h2: 'Il Fediverso ti aspetta sul tuo dispositivo',
      sub: 'Scarica, verifica, avvia. Il resto è federazione.',
      body: cta([
        { href: LATEST, label: 'Scarica Fedi3', primary: true },
        { href: base('it/faq/'), label: 'Leggi le FAQ' },
      ]),
    },
  ],
  faq: [
    { q: 'Quale pacchetto devo scegliere?', a: 'Su Linux l\'AppImage è la via più diretta; il pacchetto Debian/Ubuntu è per chi preferisce il gestore di pacchetti. Su Windows serve il pacchetto x64. Su Android l\'APK arm64-v8a copre la maggior parte dei dispositivi.' },
    { q: 'Fedi3 è gratis?', a: 'Sì. Il client è software libero AGPL-3.0-only e non richiede abbonamenti. Un relay è opzionale e può essere pubblico o auto-ospitato.' },
    { q: 'Serve un account per iniziare?', a: 'No. Il primo avvio crea l\'identità sul dispositivo; il relay si aggiunge dopo, se serve.' },
    { q: 'Come aggiorno Fedi3?', a: 'Scarica la nuova release e verifica il checksum. Su Android l\'aggiornamento richiede lo stesso certificato di firma della versione installata.' },
  ],
  schemaType: 'WebPage',
  howTo: {
    name: 'Installare Fedi3 su Linux, Windows o Android',
    description: 'Quattro passi per scaricare, verificare e avviare Fedi3 con un profilo locale.',
    steps: [
      { name: 'Scarica il pacchetto', text: 'Apri la pagina delle release e prendi il file per il tuo sistema.' },
      { name: 'Verifica il checksum', text: 'Confronta l\'SHA256 del file con quello pubblicato nella release.' },
      { name: 'Avvia il client', text: 'Installa o esegui il pacchetto; al primo avvio il core genera l\'identità locale.' },
      { name: 'Configura la rete', text: 'Inserisci un relay e un token, oppure resta in modalità peer-to-peer.' },
    ],
  },
};

// ---------------------------------------------------------------- faq

const faq = {
  lang: 'it',
  slug: 'it/faq/',
  title: 'Domande frequenti su Fedi3 e il Fediverso',
  description: 'Risposte su Fedi3, ActivityPub e il Fediverso: differenze con Mastodon e Misskey, relay, sicurezza, piattaforme, licenza e limiti dichiarati.',
  keywords: ['fedi3 faq', 'domande fedi3', 'fedi3 mastodon differenza', 'fedi3 misskey differenza', 'relay fedi3', 'sicurezza fedi3', 'fedi3 ios', 'fedi3 licenza', 'fediverso domande'],
  subject: 'Domande frequenti su Fedi3, ActivityPub e il Fediverso',
  ogImage: 'assets/og/fedi3-fediverso.png',
  ogAlt: 'Domande frequenti su Fedi3 e il Fediverso',
  alt: { it: 'it/faq/', en: 'faq/' },
  breadcrumb: [{ label: 'Fedi3', href: 'it/' }, { label: 'FAQ', href: 'it/faq/' }],
  hero: heroBlock({
    eyebrow: 'Domande frequenti',
    h1: 'Domande frequenti su Fedi3 e il Fediverso',
    lead: 'Le risposte che servono prima di scegliere un client del Fediverso: differenze con Mastodon e Misskey, relay, sicurezza, piattaforme e limiti dichiarati.',
    buttons: [
      { href: LATEST, label: 'Scarica Fedi3', primary: true },
      { href: base('it/fediverso/'), label: 'Guida al Fediverso' },
    ],
    factList: ['pre-1.0', 'AGPL-3.0-only', 'Linux', 'Windows', 'Android'],
    image: 'assets/img/desktop-profile.webp',
    imageAlt: 'Profilo Fedi3 con post federati',
    tocItems: [
      { id: 'faq-fediverso', label: 'Fedi3 e il Fediverso' },
      { id: 'faq-sicurezza', label: 'Sicurezza' },
      { id: 'faq-rete', label: 'Relay e rete Fedi3' },
      { id: 'faq-progetto', label: 'Piattaforme e progetto' },
    ],
  }),
  sections: [
    {
      id: 'faq-fediverso',
      h: 2,
      h2: 'Fedi3 e il Fediverso',
      sub: 'Che rapporto c\'è tra il client e le istanze Mastodon e Misskey.',
      body: faqVisual() + qaList([
        { q: 'Fedi3 è un\'alternativa a Mastodon e Misskey?', a: 'È un client del Fediverso: parla ActivityPub e affianca Mastodon e Misskey invece di sostituirli. La differenza è dove vive l\'account, non il protocollo.' },
        { q: 'Fedi3 è un\'istanza?', a: 'No. Un\'istanza è un server con un database di utenti; Fedi3 è un\'applicazione con un core locale. Il relay è un servizio opzionale, non un\'istanza sociale.' },
        { q: 'Posso seguire utenti Mastodon e Misskey?', a: 'Sì. Gli attori ActivityPub di Fedi3 sono raggiungibili dalle istanze classiche e possono seguire attori su Mastodon, Misskey, PeerTube, Pixelfed e Lemmy.' },
        { q: 'Devo abbandonare la mia istanza attuale?', a: 'No. Fedi3 si aggiunge: puoi tenere l\'istanza che hai e usare Fedi3 come client con identità locale.' },
        { q: 'Il Fediverso è solo Mastodon?', a: 'No. Mastodon è uno dei software; il Fediverso comprende Misskey, PeerTube, Pixelfed, Lemmy e client come Fedi3.' },
      ]),
    },
    {
      id: 'faq-sicurezza',
      h: 2,
      h2: 'Sicurezza e privacy',
      sub: 'Cosa è cifrato, cosa è pubblico e cosa il relay può vedere.',
      body: faqVisual() + qaList([
        { q: 'Le chat sono cifrate end-to-end?', a: 'Sì, la chat privata è cifrata nel client. Il relay conserva ciphertext e dati pubblici, non le chiavi private. Non c\'è ancora forward secrecy, ed è documentato.' },
        { q: 'I post pubblici sono privati?', a: 'No. Un post pubblico è pubblico per definizione: lo vedono il relay, le istanze federate e chiunque abbia accesso alla timeline.' },
        { q: 'Dove sono le mie chiavi private?', a: 'Nel core locale sul dispositivo, nel keyring del sistema. Non vengono caricate sul relay.' },
        { q: 'Cosa succede se perdo il dispositivo?', a: 'L\'identità è sul dispositivo: il progetto prevede il trasferimento tra dispositivi con QR e spool cifrato. Senza backup, l\'identità va ricreata.' },
        { q: 'Il relay può censurare i miei post?', a: 'Il relay può rifiutarsi di consegnare, ma le attività sono firmate dal client: il relay non può alterarle senza invalidare la firma.' },
      ]),
    },
    {
      id: 'faq-rete',
      h: 2,
      h2: 'Relay e rete Fedi3',
      sub: 'Quando serve un relay, come si sceglie e cosa fa il livello peer-to-peer.',
      body: faqVisual() + qaList([
        { q: 'Serve un relay per usare Fedi3?', a: 'No. Un account solo peer-to-peer funziona senza relay. Il relay serve per la federazione con istanze classiche e per la consegna mentre il dispositivo è offline.' },
        { q: 'Come scelgo un relay?', a: 'La directory del progetto elenca i relay pubblici con operatore e contatto. Puoi usarne uno pubblico o installarne uno tuo con Docker Compose.' },
        { q: 'Posso cambiare relay senza perdere l\'account?', a: 'Sì. L\'identità sta nel core locale: cambiare relay cambia il servizio di consegna, non l\'account.' },
        { q: 'Come funziona il livello peer-to-peer?', a: 'Usa libp2p con DHT, relay reservation, DCUtR e mDNS per trovare e collegare due dispositivi. La consegna diretta non passa dal relay.' },
        { q: 'Il relay raccoglie telemetria?', a: 'Pubblica metriche aggregate e uno snapshot di trasparenza firmato. I contatori sono aggregati, non profili individuali.' },
      ]),
    },
    {
      id: 'faq-progetto',
      h: 2,
      h2: 'Piattaforme, licenza e stato di Fedi3',
      sub: 'Cosa è pubblicato oggi e cosa manca.',
      body: faqVisual() + qaList([
        { q: 'Su quali piattaforme gira Fedi3?', a: 'Linux (AppImage, Debian/Ubuntu), Windows x64 e Android (arm64-v8a, x86_64). iOS e macOS sono nel progetto Flutter ma non hanno pacchetti pubblicati.' },
        { q: 'Fedi3 è pronto per tutti?', a: 'È pre-1.0: le funzioni principali funzionano e sono testate, ma tra release minori possono cambiare. Le note di rilascio dicono cosa è aperto.' },
        { q: 'Che licenza ha Fedi3?', a: 'AGPL-3.0-only. Il codice è su GitHub, ogni release pubblica i checksum SHA256 e i gate di verifica sono ripetibili.' },
        { q: 'Chi sviluppa Fedi3?', a: 'Il progetto è mantenuto da RedHunt07, con contributi aperti su GitHub. Il repository contiene client Flutter, core Rust, relay e infrastruttura P2P.' },
        { q: 'Fedi3 sarà su F-Droid?', a: 'La metadata F-Droid è nel repository ed è attualmente in attesa di revisione della provenienza. Le release APK sono su GitHub.' },
      ]),
    },
    {
      id: 'cta',
      h: 1,
      h2: 'Altre domande? Il repository è aperto.',
      sub: 'Issue, discussioni e documentazione sono pubblici.',
      body: cta([
        { href: `${REPO}/issues`, label: 'Apri una issue su GitHub', primary: true },
      ]),
    },
  ],
  faq: [
    { q: 'Fedi3 è un\'alternativa a Mastodon e Misskey?', a: 'È un client del Fediverso: parla ActivityPub e affianca Mastodon e Misskey invece di sostituirli. La differenza è dove vive l\'account, non il protocollo.' },
    { q: 'Le chat sono cifrate end-to-end?', a: 'Sì, la chat privata è cifrata nel client. Il relay conserva ciphertext e dati pubblici, non le chiavi private. Non c\'è ancora forward secrecy, ed è documentato.' },
    { q: 'Serve un relay per usare Fedi3?', a: 'No. Un account solo peer-to-peer funziona senza relay. Il relay serve per la federazione con istanze classiche e per la consegna mentre il dispositivo è offline.' },
    { q: 'Su quali piattaforme gira Fedi3?', a: 'Linux (AppImage, Debian/Ubuntu), Windows x64 e Android (arm64-v8a, x86_64). iOS e macOS sono nel progetto Flutter ma non hanno pacchetti pubblicati.' },
    { q: 'Che licenza ha Fedi3?', a: 'AGPL-3.0-only. Il codice è su GitHub, ogni release pubblica i checksum SHA256 e i gate di verifica sono ripetibili.' },
  ],
  schemaType: 'WebPage',
};

export const pages = [home, fediverso, activitypub, vsMastodon, vsMisskey, download, faq];
