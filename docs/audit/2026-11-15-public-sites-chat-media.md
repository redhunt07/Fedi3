# Verifica siti pubblici e allegati chat — 15 novembre 2026

## Ambito e risultato

Verificati i siti pubblici https://redhunt07.github.io/Fedi3/it/ e
https://fedi3.redhunt07.ovh/, i relativi sorgenti e il percorso degli allegati
nelle chat Flutter/core Rust. Le correzioni sono locali: questa verifica non
pubblica il sito, non aggiorna il relay e non distribuisce nuovi pacchetti.

Le pagine pubbliche rispondono HTTP 200. Versione 0.12.3, piattaforme e nomi
dei pacchetti coincidono con la release GitHub v0.12.3. Gli endpoint pubblici
`/healthz`, `/readyz`, `/_fedi3/relay/live` e `/.well-known/nodeinfo` rispondono.
Ispezionate nel browser la home italiana Pages e la landing italiana del
relay, anche nei layout mobile/desktop disponibili; nessun errore console
rilevato in queste visite. Non è una verifica visiva di tutte le pagine.

Un controllo HTTP di 42 URL dei due domini ha trovato un riferimento
inesistente a `/{origin}/` nel relay. Gli altri URL controllati rispondono;
non sono stati verificati tutti i collegamenti esterni a questi due domini.

## Correzioni

- Audio ricevuto: il fallimento del precaricamento sostituiva il play con
  un errore e `_toggle` impediva qualsiasi nuovo tentativo. Il play rimane
  disponibile, il tentativo successivo ricarica la sorgente e preparazione e
  riproduzione sono serializzate. La sorgente viene conservata alla fine
  dell'audio per permetterne il riascolto dall'inizio.
- Videonote: la riproduzione avviata dall'utente nella chat ha l'audio attivo.
  Lo schermo intero apre l'allegato selezionato, anche quando non è il primo.
- Metadati: il core accetta `media_type` e il tipo assente nei payload
  precedenti, continuando a serializzare `mediaType`. Flutter recupera il
  valore legacy anche quando `mediaType` è vuoto. L'inoltro usa il nome
  canonico `mediaType`.
- Relay: corretti i segnaposto dell'alternativa inglese e dell'immagine Open
  Graph; l'alternativa italiana punta a `/it`. Un test controlla gli URL
  effettivamente renderizzati. Tradotti i contatori e le descrizioni italiane
  del pannello live; chiarito che a ripartire da zero sono i contatori del
  traffico, non gli account registrati.
- Pages: tradotti pulsante, descrizione e nome del core nel diagramma
  inglese; l'animazione non riparte quando la scheda è nascosta.
- Entrambi i siti: il percorso diretto per modificare il relay è
  Impostazioni → Account. Le FAQ distinguono gli allegati dai messaggi
  cifrati. Quelle Pages distinguono inoltre i token nel keyring dalle chiavi
  nei file/database del core e spiegano che cambiare dominio può modificare
  gli URL ActivityPub e richiedere la migrazione.

## Limite della cifratura degli allegati

Il composer carica i byte del file tramite `uploadMedia` prima di inviare il
payload cifrato della chat. La cifratura protegge URL e metadati nel payload,
non i byte del file nel backend media. Il download media del relay non
richiede il token del destinatario: chi ottiene l'URL può scaricare il file;
il gestore dello storage può leggerlo. Questo comportamento non è stato
cambiato in questa correzione; FAQ e guide app ora lo dichiarano.

## Verifiche sui sorgenti

- `bash scripts/verify_release.sh`: superato con Rust e Flutter nel PATH.
  Include format, Clippy, build, test Rust, compatibilità WebRTC precedente,
  audit dipendenze, test Python, configurazioni piattaforme, ShellCheck,
  analisi e test Flutter.
- Suite Rust: 359 test superati, 4 ignorati nella suite ordinaria; il gate
  separato verifica anche la compatibilità WebRTC precedente.
- Suite Flutter: 274 test superati. I test di regressione coprono audio
  ricevuto senza MIME, errore iniziale, nuovo tentativo, pausa e riascolto;
  videonote con audio attivo e apertura del secondo allegato.
- `node site/build.mjs`: 14 pagine più la pagina 404; collegamenti e metadati
  richiesti validi. Controllo aggiuntivo di 597 riferimenti locali, incluse
  le ancore: nessun riferimento rotto.
- Controllo sintattico Node dei due script dei diagrammi: superato.

## Verifiche ancora necessarie

I test dei controlli audio usano il mock del plugin: non dimostrano la
decodifica, l'uscita audio o l'accessibilità del file ricevuto sul dispositivo
dell'utente. Serve provare una nuova build su quel dispositivo con un audio e
una videonota realmente ricevuti. Non è stata eseguita una prova completa tra
due dispositivi né una nuova build di tutte le piattaforme.

Il test quota PostgreSQL isolato non è stato eseguito perché manca
`FEDI3_TEST_POSTGRES_URL`. L'audit dipendenze mantiene cinque avvisi ammessi di
pacchetti non mantenuti: paste, pqcrypto-internals, pqcrypto-kyber,
pqcrypto-traits e rustls-pemfile. Il superamento dei gate non elimina questi
limiti e non certifica ogni possibile percorso dell'applicazione.
