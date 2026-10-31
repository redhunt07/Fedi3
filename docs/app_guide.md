# Guida all'app FEDI3

FEDI3 è composta da due parti che girano sulla tua macchina:

- l'**app** Flutter, che è l'interfaccia;
- il **core** Rust locale, che gestisce identità, crittografia, cache e federazione.

Il core ascolta su `127.0.0.1` ed è protetto da un token interno generato automaticamente. Il relay è opzionale: senza relay puoi comunque usare P2P e chat, ma non sei raggiungibile dalle istanze del fediverso tradizionale.

## Primo avvio

1. Apri l'app. Se non esiste una configurazione, viene mostrata la schermata di benvenuto.
2. Compila:
   - **Dominio pubblico**: l'hostname con cui vuoi essere raggiungibile (es. `mio.nome`). Deve puntare alla tua istanza o al relay che usi.
   - **Username**: solo lettere, cifre e `_`, da 1 a 64 caratteri. Non può essere cambiato senza migrare l'account.
   - **Bind locale**: lascia `127.0.0.1` a meno che tu non voglia esporre il core in LAN.
   - **Relay**: URL del relay (`https://relay.example.org`) e token ottenuto dalla registrazione. Puoi lasciarlo vuoto per una modalità solo-P2P.
3. Premi **Avvia**. L'app attende che il core sia effettivamente in ascolto prima di mostrare la timeline. Se il core non parte, l'errore viene mostrato nella schermata di setup.

La configurazione viene salvata in modo atomico nella directory dati dell'app, con permessi `600` su POSIX. Se il file è corrotto, l'app prova a recuperarlo dal backup e, se non ci riesce, ti avvisa invece di sovrascrivere silenziosamente.

## Timeline

- Le timeline federate, locali e "following" sono separate.
- I filtri rapidi permettono di vedere solo media, solo risposte o solo boost.
- Le azioni (like, boost, risposta, elimina) sono disponibili dal menu della nota.
- I media si aprono nel viewer integrato: swipe per navigare, tap per il toggle audio.

## Pubblicare

- Il composer accetta testo, media e visibilità (pubblico, non in elenco, follower, diretto).
- Il testo viene sempre convertito in HTML con escaping: i caratteri `<`, `>`, `&`, `"` e `'` non rompono il rendering né aprono injection.
- La sintassi MFM (`**grassetto**`, `:emoji:`) è supportata.
- Le note dirette non compaiono nella outbox pubblica.

## Chat

- Le chat sono 1:1 o di gruppo, con trasporto P2P quando possibile.
- I messaggi sono cifrati con AES-256-GCM e chiavi per messaggio avvolte con RSA-OAEP. Il relay non vede il contenuto.
- Supporta reazioni, file, GIF e stato di lettura.
- Limite noto: non c'è *forward secrecy*. Se una chiave privata viene compromessa, i messaggi passati diventano leggibili.

## Notifiche

- Le notifiche sono locali; non serve un servizio push esterno.
- Su Android serve il permesso `POST_NOTIFICATIONS` (richiesto al primo utilizzo) e il permesso `INTERNET`.
- La lista delle notifiche si aggiorna in testa senza azzerare quelle già lette.

## Impostazioni

- **Sicurezza**: token del relay, rotazione della chiave, visibilità del profilo.
- **Privacy**: consenso alla telemetria (opzionale), cache dei media, blocco per dominio.
- **Moderazione**: liste di attori e domini bloccati.
- **Backup**: esporta e importa configurazione e preferenze in JSON. Il backup **non** contiene la chiave privata: quella vive nella directory dati del core e va salvata separatamente se vuoi poter migrare.
- **Lingua**: italiano e inglese.

Ogni modifica alla configurazione riavvia il core e attende che sia pronto. Se il nuovo valore non funziona, l'app ripristina la configurazione precedente.

## Problemi comuni

| Sintomo | Causa probabile | Rimedio |
|---|---|---|
| "Core non avviato" | porta occupata o username non valido | cambia il bind o correggi lo username |
| Timeline vuota | relay non raggiungibile o token errato | verifica il token nelle impostazioni |
| Nessuna federazione in ingresso | dominio pubblico non risolve verso il relay | controlla DNS e il tunnel |
| Spinner infinito | il core non risponde | l'app rileva il guasto entro pochi secondi e tenta il riavvio |
| Media non visibili | backend media del relay non configurato | controlla `FEDI3_RELAY_MEDIA_BACKEND` |

Per il relay vedi [deploy_relay.md](deploy_relay.md) e [troubleshooting.md](troubleshooting.md). Per compilare l'app da sorgente vedi [build_linux.md](build_linux.md).
