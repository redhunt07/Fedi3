# Verifica client 0.11.1 — 2026-11-08

Release: [v0.11.1](https://github.com/redhunt07/Fedi3/releases/tag/v0.11.1).
Versione Flutter: `0.11.1+4`.

## Guasti segnalati

Due client Android sul relay pubblico non riuscivano a squillare. Nessuno dei
due riceveva una notifica di chiamata, la consegna delle chat sembrava lenta e
il composer della chat finiva sotto la barra di navigazione Android.

## Cause individuate

1. **Le notifiche venivano rifiutate prima di essere pubblicate.** Il resource
   shrinking della release rimuoveva i suoni in `res/raw`, referenziati solo per
   nome a runtime. `flutter_local_notifications` lanciava
   `PlatformException(invalid_sound, The resource fedi3_call could not be
   found...)` e nessuna notifica di chiamata o chat compariva.
   `res/raw/keep.xml` ora conserva i tre suoni.
2. **Il motore keep-alive headless non aveva configurazione.** Il servizio
   registrava i plugin generati una seconda volta; la registrazione duplicata
   sostituiva le istanze e faceva fallire `flutter_secure_storage` con `Could
   not decrypt key`. Il motore caricava quindi configurazione nulla e usciva
   prima di interrogare le chiamate. La registrazione duplicata è stata
   rimossa e il motore ora usa una cache di configurazione privata dell'app
   (`background_config.json`) quando il Keystore non è disponibile.
3. **Il signaling delle chiamate era troppo fragile.** La shell interrogava le
   chiamate con un backoff che poteva ritardare un invito fino a 15 s, e un
   singolo invito o accept perso lasciava entrambi i peer in attesa. Inviti e
   accept vengono ora ritentati ogni 3 s durante lo squillo e la shell
   interroga ogni 3 s.
4. **Il recupero delle chat in coda era lento.** Il worker di retry girava ogni
   20 s con backoff iniziale di 20 s. Ora gira ogni 5 s, con backoff iniziale
   di 5 s e tetto di un minuto, abbreviando il recupero di messaggi e segnali
   di chiamata.
5. **Inset Android.** Composer della chat e controlli di chiamata sono ora
   dentro una `SafeArea` inferiore, sopra la barra di navigazione e la gesture
   pill.
6. **Hero duplicati.** Le pagine tenute montate nella shell esponevano più tag
   Hero predefiniti dei floating action button; `HeroMode` ora limita la
   raccolta degli Hero alla pagina visibile.
7. **Condivisione schermo rimossa da Android.** L'interfaccia di chiamata
   Android non mostra più il controllo di condivisione schermo, quindi le
   chiamate audio/video non avviano il servizio MediaProjection. Le chiamate
   desktop mantengono la condivisione.
8. **Finestre di chiamata in arrivo obsolete.** Una finestra senza risposta
   restava sullo schermo dopo che il chiamante smetteva di squillare. Ora scade
   dopo 60 s e un `leave` per la chiamata in squillo chiude la finestra aperta.
   Anche il chiamante termina la chiamata dopo la finestra di retry di un
   minuto.

## Verifiche eseguite

- `flutter analyze`: nessun problema.
- Suite Flutter: 201 test superati.
- Rust: `cargo test -p fedi3_core --test semantics --test db_queue --test
  p2p_chat` superato, inclusi chat cifrata, ricevute, presenza e signaling di
  chiamata tra due core reali.
- Emulatore Android 13 (`aliceemu`) ed emulatore Android 11 (`bobemu`) contro
  `https://fedi3.redhunt07.ovh`:
  - Una chiamata avviata da bobemu ha mostrato la finestra di chiamata in
    arrivo su aliceemu; accettandola entrambi i lati si sono connessi e il
    contatore di durata è avanzato.
  - Con bobemu in background, il servizio keep-alive ha pubblicato la notifica
    id 1004 nel canale `fedi3_call_v2` (importanza 5, categoria call); la
    notifica heads-up mostrava "Incoming call — aliceemu" e toccandola si è
    aperta la finestra di chiamata. Accettando la chiamata si è connessa.
  - Un messaggio inviato da bobemu è comparso su aliceemu entro l'intervallo di
    polling, marcato "just now".
  - Composer della chat e controlli di chiamata restano sopra la barra di
    navigazione di sistema su entrambe le versioni Android.
- L'APK di release contiene le copie `res/raw` dei tre suoni di notifica dopo la
  correzione `keep.xml`.
- Xiaomi 2201123G (Android 15, TIM 5G, account `redhunt07` su
  `https://fedi3.redhunt07.ovh`) contro l'emulatore Android 11 (`bobemu`):
  - Un messaggio inviato dal telefono su 5G è arrivato nel core di bobemu e ha
    prodotto la notifica chat id 1001 in `fedi3_chat_v2`.
  - Una chiamata audio dal telefono su 5G ha squillato su bobemu con l'app
    chiusa: notifica id 1004 in `fedi3_call_v2`, heads-up "Incoming call —
    redhunt07". Toccandola e accettando, entrambi i lati si sono connessi e il
    contatore è avanzato fino a 00:08.
  - L'interfaccia di chiamata del telefono non mostra più la condivisione
    schermo.
  - È stata provata anche la direzione inversa: con il telefono sul launcher
    (app chiusa), una chiamata da bobemu ha pubblicato la notifica id 1004 in
    `fedi3_call_v2`; la heads-up mostrava "Incoming call — bobemu", il motore
    background ha registrato lo squillo e accettando entrambi i lati si sono
    connessi (durata 00:10). Android 15/MIUI ha riportato
    `FSI_REQUESTED_BUT_DENIED` per il full-screen intent, ma la heads-up è
    comunque comparsa.
- Scadenza chiamata senza risposta: una chiamata da aliceemu a bobemu lasciata
  senza risposta si è chiusa da entrambi i lati dopo 60 s; il chiamante è
  tornato alla chat e la finestra del chiamato è sparita, senza prompt
  obsoleto.

## Limiti

- Telefono ed emulatore usano lo stesso relay pubblico. Il percorso cross-relay
  (`fedi3.redhunt07.ovh` ↔ `fedi3.palermo.nohost.me`) usa lo stesso forwarding
  HTTP del relay e i nuovi retry; una chiamata dal telefono a `panormus` ha
  squillato ma non è stata risposta in questa passata.
- L'audio non è stato giudicato a orecchio sugli emulatori; sono stati verificati
  il canale di notifica e lo stato di connessione della chiamata.
- Il telefono reale è Android 15 con MIUI. La richiesta di full-screen intent è
  stata negata dal sistema, quindi la chiamata in arrivo si è affidata alla
  heads-up; la chiamata si è comunque connessa.
- Gli APK usano ancora la chiave di debug locale perché `android/key.properties`
  non è configurato. Profili e dati del keyring esistenti sono preservati.
