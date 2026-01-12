# FAQ

## Il relay è un server social completo?
No. Fornisce compatibilità ActivityPub e routing verso i client.

## Posso usare solo P2P senza relay?
È possibile, ma perdi compatibilità con istanze legacy.

## Telemetria?
È opzionale e attivabile dall’utente. I log locali si possono cancellare.

## Dove sono salvati i media?
Locale, S3 o WebDAV (configurabile sul relay).

## Infrastruttura P2P/ICE avanzata?
Sono disponibili stack opzionali in `deploy/p2p_infra` e `deploy/turn`.
