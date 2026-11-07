# Prova tra due client — 4 novembre 2026

Dispositivi: Alice sul PC locale (EndeavourOS) e Peppenamir su Zorin OS 18.1,
con relay pubblico `fedi3.redhunt07.ovh` e Mullvad attivo sul PC locale.

## Risultati verificati

- Alice → Peppenamir: messaggio
  `urn:fedi3:chat:msg:e75bfe5f3b2cfc4fe630870d`, `sent=1`, `queued=0`,
  circa 1700 ms. Identico ID trovato sul core remoto, testo visibile nel client,
  ricevuta `delivered` sul mittente.
- Peppenamir → Alice: messaggio
  `urn:fedi3:chat:msg:984437f68277fbdc1d9bda51`, `sent=1`, `queued=0`,
  circa 1731 ms. Identico ID trovato sul core locale, ricevuta `seen` sul remoto.
- Presenza `busy` propagata in entrambe le direzioni, poi ripristinata `online`.
- Rilevati rifiuti del circuit relay per esaurimento dei limiti. Aggiornato il
  servizio infrastrutturale; osservata la ripresa delle prenotazioni con limiti
  di 16 MiB e 900 secondi.

Queste prove confermano consegna e presenza attraverso i percorsi disponibili;
non dimostrano una connessione diretta senza relay.

## Chiamate: problema ancora aperto

La prova reale ha individuato e fatto correggere l'assenza dell'offerta iniziale
dopo l'accettazione e la perdita dei candidati ICE anticipati. Le prove successive
hanno scambiato SDP e ICE, senza stabilire un flusso audio: contatori RTP a zero.
L'accettazione e la risposta SDP non vengono più mostrate come connessione media
riuscita.

Corrette anche la configurazione ICE del bridge Linux (un URL per elemento) e
la separazione fra TURN supportato dalle chiamate Flutter e quello del backend
Rust. Sono correzioni implementate e coperte da test locali; il successo audio
tra i due PC deve ancora essere verificato.

L'utente ha comunicato lo spegnimento del PC di Peppenamir. I test fra dispositivi
sono quindi sospesi. Il client locale è stato riavviato normalmente senza la
libreria temporanea di tracciamento; la VPN non è stata modificata.

## Validazione e prossima prova

Suite locali: 227 test Rust e 121 Flutter superati. Le AppImage ricostruite
includono core e codice Flutter corrispondenti agli output compilati. I checksum
sono nei rispettivi file `.sha256` in `dist/`.

Quando entrambi i PC saranno disponibili:

1. Ripetere messaggio e ricevuta nelle due direzioni sulle build aggiornate.
2. Accettare una chiamata e verificare `Fedi3 call media` sui due client:
   stato connesso e crescita dei byte RTP inviati e ricevuti per lo stesso ID.
3. Se ICE fallisce, confrontare candidati host, srflx e relay e le risposte alle
   verifiche di connettività, mantenendo la VPN nella configurazione abituale.
4. Ripetere dopo disconnessione e riconnessione di un client.

Non considerare l'audio risolto prima del punto 2; i test simulati non lo provano.
