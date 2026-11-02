# Annunci Fedi3

Aggiornamenti ufficiali dai maintainer. Questa pagina è letta direttamente
dal repository Git e il client la controlla periodicamente.

## 2026-11-02 — Scopri e notifiche raggruppate

La scheda Scopri ora mostra hashtag in tendenza e account suggeriti con follow
in un tap. Le notifiche ripetute dello stesso autore vengono raggruppate e
possono essere espanse.

## 2026-11-02 — Post salvati e disponibili offline

I segnalibri ora conservano una copia locale del post: la nuova schermata
«Salvati» (Configurazioni o rail destro) li mostra anche senza rete e permette
di rimuoverli con uno swipe.

## 2026-11-02 — I tag aprono davvero i post

Corretto il percorso dei tag: i conteggi considerano solo tag con post
esistenti, i nomi sono normalizzati e cliccando un hashtag si apre la ricerca
con i post corrispondenti, senza risultati vuoti.

## 2026-11-01 — Scopri gli hashtag in tendenza

Il rail destro mostra ora gli hashtag più usati nel Fediverso e li apre
direttamente nella ricerca. Corretta anche la ricerca hashtag, che prima
restituiva sempre zero risultati.

## 2026-11-01 — Notifiche e composer più usabili

Le notifiche ora sono righe compatte con avatar, stato di presenza e azioni
rapide. Il composer accetta il trascinamento dei file e la selezione nativa,
con anteprima degli allegati; la chat ha un campo di scrittura arrotondato in
stile Telegram con pulsante di invio a gradiente.

## 2026-11-01 — Chat in stile Telegram e presenza

Le chat hanno bolle con coda, avatar e spunte di consegna: singola, doppia e
doppia con indicatore quando tutti i membri hanno letto. Lo stato online,
occupato, AFK e offline viaggia tra utenti Fedi3 anche su relay diversi tramite
buste cifrate, scade automaticamente ed è visibile sui avatar del client.

## 2026-11-01 — Restyling del client

Nuova passata grafica: logo Fedi3 fisso con particelle luminose, bagliore aurora
nel rail, banner profilo con mesh e avatar sovrapposto, lista chat a schede,
composer in evidenza nella sidebar e azioni dei post più compatte. La ricerca
è raggiungibile dalla barra della timeline e gli stati vuoti sono brandizzati.


## 2026-11-01 — Composer avanzato e nuovo client

Il client Fedi3 introduce un composer completo in stile editor: toolbar per
titoli, caratteri, dimensioni, allineamento, liste, citazioni, link e media,
anteprima live e statistiche del documento. La pubblicazione resta compatibile
con il Fediverso.

- Nuovo rail con effetti particellari e logo fisso.
- Scorciatoie Ctrl+1..7, Ctrl+N, Ctrl+F ed Esc.
- Rimozione del composer rapido: la sidebar apre direttamente l'editor.

## 2026-10-30 — Specifica OpenAPI e interoperabilità

Pubblicata la specifica OpenAPI 3.1 di core e relay (`docs/openapi.yaml`), con
66 endpoint documentati. I test di interoperabilità con Mastodon e Misskey
reali restano verdi.

## 2026-10-25 — Sicurezza e decentralizzazione

Identità ancorata separata dal trasporto, spool cifrato e mesh dei relay sono
in produzione. Il client mostra lo stato del core e del tunnel in tempo reale.

---

Per segnalazioni e proposte: https://github.com/redhunt07/Fedi3/issues
