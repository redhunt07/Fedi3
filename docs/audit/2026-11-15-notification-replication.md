# Sincronizzazione notifiche personali — 15 novembre 2026

Client installati: 0.12.2+4018, Android versionCode 6018, desktop Flatpak. Relay aggiornato con replica negoziata e instradamento fra dispositivi associati.

## Correzioni

- Le notifiche personali accettate vengono replicate come record cifrati dell’account. La cronologia preesistente viene recuperata senza riprodurre avvisi storici; non si replicano come notifiche le reazioni ai post di altre persone.
- Ogni dispositivo conserva una richiesta di avviso atomica e persistente. Replay, riavvio e consumatori concorrenti UI/background non duplicano l’avviso. La lettura della notifica resta separata dalla richiesta di avviso.
- Il cursore delle notifiche nel poll dello spool risveglia Android anche senza messaggi chat. Il cursore viene confermato dopo la sincronizzazione effettiva.
- Il relay conserva i tunnel associati e sceglie un dispositivo ancora connesso quando quello più recente si sospende. La revoca chiude tutti i tunnel dell’account.
- Il supervisore Android sostituisce gli handle nativi rimasti obsoleti dopo che un altro engine Flutter ha fermato il core, anche a configurazione invariata.

Le prime prove reali avevano evidenziato due guasti distinti: perdita della rotta sul relay alla sospensione del telefono e riavvio ignorato per un handle locale obsoleto. Le prove riuscite seguenti sono state eseguite dopo entrambe le correzioni.

## Prove reali PC ↔ Android

Due nuovi Follow firmati da un account temporaneo controllato, diretti esclusivamente all’account del proprietario. In entrambe le prove: app sulla Home, schermo spento e endpoint locale del telefono non disponibile prima dell’invio. Nessun messaggio chat è stato usato per provocare il risveglio.

| Prova | Ricezione PC | Ricezione telefono | Identificatore attività |
| --- | ---: | ---: | --- |
| Primo risveglio | 9,79 s | 13,03 s | identico sui due client |
| Secondo risveglio | 9,78 s | 16,39 s | identico sui due client |

Il processo PC è rimasto invariato durante entrambe le prove. Nella prima Android ha creato la notifica di sistema sul canale attività. Nella seconda il feed è stato verificato; non si deduce un nuovo avviso di sistema dal record già presente. Le notifiche di prova risultano singole, i feed finali hanno gli stessi identificatori e la richiesta locale di avviso verificata sul PC risulta consumata una volta.

Il test riguarda il servizio Android attivo sul telefono collegato. Non dimostra consegna con app arrestata forzatamente, servizio terminato dal sistema, rete assente o sospensione Doze prolungata; i servizi push esterni restano configurabili e non sono stati attivati per questa prova.

## Conservazione e pulizia

Backup cifrati prima dell’aggiornamento, inclusi dati desktop, keyring e identità/stato Android. Installazione APK in place con la medesima firma e aggiornamento Flatpak, senza cancellazione dati. Cinque file di identità desktop e l’export identità Android risultano invariati. Tutti i 176 messaggi originali e tutte le righe originali di post/outbox, follower e seguiti sul PC sono conservati; profilo Android e seguiti invariati. I Follow di prova sono stati annullati e le liste follower su entrambi i dispositivi sono tornate al valore iniziale. L’account temporaneo sul relay e il relativo core sono stati rimossi. Impostazione Android di mantenimento schermo ripristinata e inoltro ADB temporaneo rimosso.

Il relay è stato aggiornato dopo backup PostgreSQL e verifica di un candidato isolato: credenziali account, chiavi di firma, identità, spool e media conservati. Readiness pubblica verificata dopo il deploy. Le modifiche preesistenti dell’utente ai file di deploy/documentazione non sono state incluse.

## Test automatici

351 test Rust superati (quattro ignorati perché richiedono servizi esterni), clippy workspace/all-targets senza avvisi; 255 test Flutter superati e analisi statica senza problemi. Copertura mirata per filtro personale, import storico silenzioso, replay, claim concorrenti persistenti, compatibilità con client precedenti, cursore di risveglio, fallback tra tunnel e recupero degli handle obsoleti.
