# Verifica Fedi3 0.12.5 — 16 novembre 2026

Sorgente release: `319bdedcaf07ca742f39db1115cc71402c35004b`.
Client `0.12.5+6022`, APK separati con codici 8022/10022.

Ripristinati cronologia dei profili, risposte, media e paginazione outbox;
esclusi dai suggerimenti gli account già seguiti o con richiesta pendente.
La ricerca combinata tollera copertura parziale senza il precedente 503.
Corretti persistenza dei nomi media, P2P WSS sulla porta HTTPS 443,
validazione DNS/TLS e inizializzazione del contesto DNS nativo Android.

Superato l'intero gate locale: formattazione, Clippy, compilazione, audit,
contratti Python, configurazioni piattaforme, ShellCheck, analisi Flutter e
**285 test Flutter**. Passati **367 test Rust** ordinari e separatamente i
quattro normalmente ignorati (due compatibilità legacy, PostgreSQL isolato,
bootstrap IPFS pubblico): **371 test Rust eseguiti**. Restano gli avvisi di
manutenzione dipendenze già documentati. La pipeline candidata completa
[43928880328](https://github.com/redhunt07/Fedi3/actions/runs/43928880328) è riuscita.

Relay principale, infrastruttura P2P, Flatpak desktop e telefono fisico sono
aggiornati a 0.12.5. Eseguiti backup di database, media e configurazione prima
della sostituzione. Identità, chiavi, peer, account e sei seguiti sono preservati;
integrità SQLite desktop verificata. Tutti e cinque i media persistiti coincidono
byte per byte con gli originali HTTPS dopo il riavvio. Health, readiness e
metriche relay passano.

Sul telefono riavviato sono visibili “Ciao mondo!”, post e risposte di
peppenamir, post/risposte e scheda Media popolata di Panormus. La ricerca `s`
in “Tutti” restituisce risultati senza 503. Le letture P2P TCP/WSS verso il PC
restituiscono oggetto e media identici al relay HTTPS. Il telefono su rete mobile
accetta la prenotazione WSS e restituisce lo stesso oggetto attraverso il circuito.
Il media storico usato come fixture non è nella cache del telefono (404):
la verifica non dimostra il suo trasferimento media da quel dispositivo.
Nessun post, messaggio o cambiamento follow di prova è stato inviato.

Il secondo relay Palermo risponde HTTPS ma SSH 2200 e tunnel VPS 2223 sono
indisponibili. La credenziale root disponibile riceve 401 dall'API amministrativa
YunoHost. Serve un accesso amministratore valido o il riavvio locale del servizio
`fedi3-tunnel`; non si dichiara completato l'aggiornamento di quel server.

Il [rapporto inglese](../../audit/release_0.12.5_2026-11-16.md) contiene dettagli
tecnici e hash delle letture di trasporto.

Installato sul telefono anche l'APK ARM64 della pipeline ufficiale, verificando
firma uguale alla precedente installazione, core 0.12.5, DID e peer invariati,
outbox persistente e nuova prenotazione relay dopo l'avvio.

Tutti i job della [pipeline ufficiale](https://github.com/redhunt07/Fedi3/actions/runs/43932998624)
sono riusciti; pubblicata [v0.12.5](https://github.com/redhunt07/Fedi3/releases/tag/v0.12.5)
con AppImage, DEB, ZIP Windows, APK ARM64/x86_64 e checksum.
Una successiva lettura dei log ha individuato un altro caso: un media caricato da
un altro dispositivo esiste sul relay ma restituisce 404 locale senza riga media.
Serve una patch successiva. Le sonde P2P con identità temporanea possono ricevere
`ConnectionClosed` sulla connessione iniziale: le letture TCP/WSS riuscite sono
provate, non viene dichiarata consegna ininterrotta.

SHA256: 5/5 published packages verified; installed ARM64 APK matches the public asset.
