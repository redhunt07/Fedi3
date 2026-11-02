# Fedi3 — Piano di rilancio del client (UX/IA)

Data: 2026-11-01
Stato: proposta, da validare prima di implementare.

## 1. Valutazione onesta

Il client attuale **non è ancora appetibile** per un utente che arriva da Mastodon,
Misskey o dai social mainstream. Non per Flutter: per prodotto.

Difetti osservati nella verifica live:

- l'onboarding è un pannello da amministratore (bind, internal token, relay WS,
  dominio) invece di un flusso "scegli un nome e inizia";
- la timeline desktop apre quattro colonne con etichette fuorvianti: la colonna
  "Locale" mostra in realtà il feed dei follow, "Sociale" è vuota senza P2P;
- il profilo mostra attività grezze (`EmojiReact`, `Like`, `Follow`) mescolate alle
  note, senza tab Post/Risposte/Media;
- mancavano liste follower/seguiti (aggiunte ora), paginazione, empty state utili;
- impostazioni presenti ma poco discoverabili, senza ricerca interna né gerarchia
  chiara;
- nessuna identità visiva, nessun onboarding guidato, nessun percorso di recupero
  account spiegato all'utente;
- la chat è tecnicamente E2E ma povera rispetto a Misskey (allega file, react,
  thread, stato di lettura).

Il problema non è "il linguaggio sbagliato": è **architettura dell'informazione,
flussi e finitura**. Riscrivere in un altro linguaggio senza risolvere questi punti
produrrebbe lo stesso prodotto con costi maggiori.

## 2. Cosa copiare da Misskey e cosa no

Misskey è un riferimento utile per **struttura**, non per densità.

Da copiare:

- tassonomia delle impostazioni (Account, Profilo, Privacy, Sicurezza, Moderazione,
  Rete, Aspetto, Notifiche, Lingua, Backup, Informazioni) con sottopagine e ricerca;
- profilo con tab (Post, Risposte, Media, Reazioni) e contatori cliccabili;
- deck/colonne **opzionale**, non layout predefinito;
- composer sempre raggiungibile, con visibilità, allegati, emoji, sondaggi;
- ricerca unificata con filtri e cronologia;
- notifiche raggruppate e filtrabili;
- chat con allegati, reazioni, thread e stato di lettura.

Da non copiare acriticamente:

- la densità visiva e la quantità di opzioni in prima vista: Misskey è per power
  user; un nuovo utente si perde;
- la dipendenza da un server multi-utente centralizzato: Fedi3 è single-user per
  core, quindi "login/registrazione" vanno ridefiniti (vedi §4);
- il modello tutto-web: Fedi3 ha un core locale e un relay; il client desktop è
  un vantaggio, non un limite.

Posizionamento consigliato: **IA ispirata a Misskey, semplicità iniziale da
Mastodon, deck mode per chi la vuole**. Il vantaggio competitivo di Fedi3 resta
P2P/relay: il client deve sembrare normale prima di raccontarlo.

## 3. Linguaggio e piattaforma: proposta

Non riscrivere ora. Motivazioni:

- Flutter copre desktop, mobile e (con limiti) web con un solo codice; il collo di
  bottiglia è il prodotto, non il runtime;
- il core Rust e il protocollo sono l'asset: vanno stabilizzati con un'API client
  documentata, non riscritti;
- un rewrite completo costerebbe 2–3 mesi e congelerebbe le correzioni UX.

Proposta in due binari:

- **Binario A (ora)**: ridisegno di IA e flussi nel client Flutter esistente, a step
  (§6). Ogni step è rilasciabile e testabile.
- **Binario B (dopo)**: se serve la distribuzione web, un client web separato
  (React/TS o Svelte) che consuma la stessa API, condividendo design token e non
  codice. Non è un rewrite del desktop: è un secondo client.

Se in futuro si scegliesse un altro linguaggio, la condizione è avere prima
un'API stabile e documentata: senza quella, ogni client resta accoppiato agli
endpoint interni.

## 4. Autenticazione: il punto architetturale da chiarire

Oggi Fedi3 è **single-user per core**: l'identità è una chiave, non un account con
password. L'app e il core girano sulla stessa macchina e condividono un token
interno. Questo non è il modello Misskey (multi-utente, sessioni web).

Tre opzioni, in ordine di costo:

1. **Profilo locale (consigliata ora)**: "registrazione" = creazione identità
   (username + relay, opzioni avanzate nascoste); "login" = import backup oppure
   connessione a un'istanza esistente (URL + token/backup). Il client gestisce più
   profili locali con switcher. Nessuna modifica al core.
2. **Accesso remoto al core**: il core espone un'API client autenticata con bearer
   token su TLS, così un client web può collegarsi a un'istanza remota. Richiede
   CORS, token dedicati, OpenAPI e revoca.
3. **Multi-utente nel core**: il vero modello Misskey, ma è un cambio di backend
   (identità multiple, sessioni, permessi). Rinviabile.

Raccomandazione: partire da (1), progettare l'API per rendere possibile (2), non
promettere (3) finché non c'è domanda reale.

## 5. Architettura dell'informazione target

Route e schermate, in ordine di implementazione:

- `/welcome` — scelta: crea identità, importa backup, collega istanza.
- `/signup` — username, relay consigliato, avanzate collassate, recovery code
  obbligatorio e scaricabile.
- `/login` — import backup, connect URL+token, recovery code.
- `/home` — feed unico con tab Home/Locale/Federata, composer persistente,
  scroll infinito, filtri, media.
- `/explore` — ricerca unificata (post, utenti, hashtag), filtri, cronologia.
- `/notifications` — gruppi per tipo, filtri, segna come letto, real-time.
- `/messages` — lista conversazioni, thread, allegati, reazioni, stato lettura.
- `/@user` — profilo pubblico con tab Post/Risposte/Media, follow, muta, blocca,
  segnala.
- `/me` — profilo personale con tab e modifica, liste follower/seguiti paginate.
- `/settings` — hub con sottosezioni: Account, Profilo, Privacy, Sicurezza,
  Moderazione, Rete, Aspetto, Notifiche, Lingua, Backup, Informazioni, Avanzate.
- `/settings/*` — una pagina per categoria, con salvataggio esplicito e riavvio
  core dove serve.

Regole trasversali: niente token/bind in prima vista; ogni schermata ha empty
state, errore azionabile e stato di caricamento; nessuna attività grezza mostrata
all'utente se non in "Mostra raw".

## 6. Piano a step

Ogni step è un rilascio verificabile; le stime sono indicative (1 sviluppatore).

| Step | Contenuto | Dipendenze | Stima | Criterio di accettazione |
|---|---|---|---|---|
| 0 | Audit UX, design token, skeleton di navigazione e route, gate di avvio | — | 2 gg | route e stati vuoti presenti; nessuna regressione funzionale |
| 1 | Registrazione guidata, recovery code, login (backup/connect), switcher profili | API config attuale | 3 gg | nuovo utente pubblica entro 5 minuti senza toccare token |
| 2 | Home unica con tab, composer persistente, scroll infinito, media | core timeline | 4 gg | feed usabile su desktop e mobile; nessuna colonna fuorviante |
| 3 | Profilo proprio e altrui con tab, liste paginate, modifica profilo | relazioni | 4 gg | tab e liste corrette; follow/unfollow coerenti |
| 4 | Impostazioni con tassonomia completa, ricerca interna, salvataggi sicuri | schermate esistenti | 3 gg | tutte le categorie raggiungibili in ≤2 tap |
| 5 | Ricerca unificata con filtri, cronologia, lookup remoto | API search | 3 gg | risultati post/utenti/hashtag; fallback locale |
| 6 | Notifiche raggruppate, filtri, mark-read, real-time | event stream | 3 gg | gruppi e filtri corretti; badge coerente |
| 7 | Chat completa: allegati, reazioni, thread, read state | chat API | 3 gg | conversazione con media e stato lettura |
| 8 | Finitura: onboarding tour, accessibilità, temi, i18n, scorciatoie | — | 3 gg | audit accessibilità di base superato |

Totale ≈ 28 giorni; i primi tre step (0–2) danno già un client presentabile.

## 7. Lavoro backend necessario in parallelo

- **API client stabile e documentata** (OpenAPI): endpoint, errori tipizzati,
  paginazione a cursore, versioning. È il prerequisito per qualsiasi client futuro.
- **Auth per accesso remoto**: bearer token dedicato, CORS, revoca, rotazione.
- **Paginazione** per follower/seguiti e notifiche (oggi il client carica la prima
  pagina).
- **Feed profilo filtrato** lato API o client: solo Note, con tab.
- **Notifiche raggruppate** e mark-read lato server.
- **Chat**: allegati e stato di lettura se non già presenti.
- **Moderazione**: mute/block/report esposti in modo uniforme.

## 8. Rischi e metriche

Rischi: rifare l'IA senza test utente; introdurre multi-utente troppo presto;
duplicare client (desktop + web) senza API condivisa; regressioni nelle funzioni
P2P durante il restyling.

Metriche da misurare: tempo al primo post, successo dei task principali
(post, follow, risposta, ricerca), crash-free sessions, retention D1/D7,
tempo medio per trovare un'impostazione.

## 9. Proposta immediata

Partire dallo **Step 0 + Step 1** (skeleton di navigazione + registrazione/login
guidati), perché sono il cancello d'ingresso e il difetto più visibile. In
parallelo, scrivere la specifica OpenAPI minima dell'API client: senza quella,
lo Step 2 rischia di consolidare endpoint interni.

Se il vincolo è "copiare Misskey", la sequenza resta valida: si copia la
tassonomia e i flussi, non la densità, e si tiene Flutter finché l'IA non è
stabile.

## 10. Stato di avanzamento (2026-11-01)

| Step | Stato | Note |
|---|---|---|
| 0 — Audit, design token, skeleton, gate di avvio | completato | tema Aurora, rail custom, route esistenti, empty state; manca un vero router dichiarativo |
| 1 — Registrazione/login guidati, recovery | completato | onboarding con identità/relay in primo piano, login rapido da URL+token, avanzate collassate e dialog di recovery con copia dei codici |
| 2 — Home unica con tab | completato (base) | layout a tab predefinito su desktop, tab Home/Seguiti/Scopri/Federata; composer full-screen da sidebar e FAB mobile |
| 3 — Profilo proprio/altrui | completato (base) | banner, contatori cliccabili, tab Post/Risposte/Media, griglia media, paginazione outbox; mancano filtri media avanzati |
| 4 — Impostazioni | completato (visual) | hub raggruppato con icone e sezioni; manca ricerca interna |
| 5 — Ricerca | completato (base) | ricerca post/utenti/hashtag, filtro fonte (tutti/locale/relay), cronologia persistente con chip e cancellazione; mancano filtri temporali |
| 6 — Notifiche | parziale | filtri, mark-all-read e raggruppamento per data; manca il raggruppamento per attore/tipo |
| 7 — Chat | completato (base) | thread, allegati con anteprima, reazioni, read state, badge E2E; mancano chiamate; ricerca completa, inoltro, messaggi fissati e link invito completati |
| 8 — Finitura | parziale | scorciatoie Ctrl+1..7/Ctrl+N/Ctrl+F/Esc, rail accessibile con hint, composer avanzato, logo fisso con particelle, annunci dai maintainer con alert periodico, banner profilo mesh, chat a schede e bolle stile Telegram con spunte, notifiche compatte, post salvati offline, composer con drag&drop, presenza online/occupato/AFK/offline; restano audit contrasto e navigazione da tastiera completa |

Prossimo passo consigliato: client web sulla specifica OpenAPI, poi audit di contrasto/accessibilità completo e test end-to-end della chat con due identità reali.
