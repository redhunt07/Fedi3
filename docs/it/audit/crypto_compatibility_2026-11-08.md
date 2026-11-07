# Correzione compatibile RSA e WebRTC — 8 novembre 2026

Le due vulnerabilità crittografiche residue del 7 novembre sono state eliminate
dal grafo delle dipendenze della release. Profili, identità, chiavi e formati dei
messaggi restano compatibili. Non vengono migrati o sovrascritti dati dell’utente.

## Sostituzione del provider RSA

Generazione, importazione, firma, verifica, cifratura e decifratura RSA usano ora
AWS-LC tramite `fedi3_protocol::crypto`. Core e relay non dipendono più da
RustCrypto `rsa`, nemmeno negli helper di test. Questo elimina
[RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html)
sostituendo l’implementazione vulnerabile, senza dichiarare corretta la vecchia crate.

Restano invariati: chiavi private PEM PKCS#8, pubbliche PEM SPKI, firme PKCS#1 v1.5
SHA256, RSA-OAEP con SHA256/MGF1-SHA256 e label vuota, envelope AES-256-GCM,
nomi dei file delle chiavi, calcolo del DID, campi JSON e header ActivityPub.
Le chiavi esistenti vengono lette e validate, non riscritte. Una chiave corrotta
causa un errore, mai la rigenerazione dell’identità. KEM e chiavi della chat non cambiano.

Prove:

- Vettori pubblici di test prodotti con RSA 0.9.10: verifica delle vecchie firme,
  decifratura dei vecchi ciphertext e identità esatta di PEM pubblica e DID.
- Un intero messaggio cifrato v1 prodotto dal vecchio codice viene decifrato nel
  contenuto originale; un ciphertext alterato non viene accettato come plaintext.
- Il vecchio provider RSA decifra nuovi envelope e verifica nuove firme.
- Caricamenti ripetuti del profilo conservano ogni byte della chiave privata;
  una chiave corrotta non viene sostituita con una nuova identità.

## Sostituzione del trasporto WebRTC

Il trasporto opzionale Rust usa WebRTC 0.21.1 con provider AWS-LC. Il vecchio
WebRTC 0.12/DTLS e `crypto-bigint` 0.5.5 non sono più nel grafo della release.
Lo stack firme S3 conserva la versione corretta `crypto-bigint` 0.7.10. È eliminato
[RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html)
senza forzare una dipendenza incompatibile o disattivare WebRTC.

L’adattatore interno conserva SDP, trickle ICE, canale ordinato e affidabile
`fedi3`, frame binari v1 e payload di segnalazione via relay. Il nuovo driver
gestisce DTLS/SCTP e limita il buffer di invio. Shutdown libera reader e callback;
un’attesa di apertura annullata è considerata un errore, non una connessione aperta.

Prove:

- Due peer attuali completano DTLS/SCTP e scambiano 32.000 byte binari.
- La chiusura libera il reader del canale.
- I campi JSON di candidati ICE e SDP coincidono con quelli precedenti.
- Un peer WebRTC 0.12 isolato comunica con il nuovo stack nei ruoli offer e answer
  e restituisce senza modifiche i frame binari Fedi3 v1.

`python3 scripts/verify_legacy_webrtc.py` compila il vecchio peer RSA/WebRTC in un
progetto temporaneo con lockfile proprio, esterno al workspace e alla release.
Usa solo chiavi di test intenzionalmente pubbliche e peer locali. La CI esegue
esplicitamente entrambi i test; la produzione non ripiega sul codice vulnerabile.

## Audit e controlli della release

`cargo audit`: **0 vulnerabilità**, nessuna esclusione e cinque avvisi non bloccanti
di dipendenze non mantenute: paste, pqcrypto-internals, pqcrypto-kyber,
pqcrypto-traits e rustls-pemfile. Il KEM attuale è conservato per compatibilità;
una sua futura modifica richiede un protocollo versionato.

L’audit resta obbligatorio sui tag di release. Un test impedisce di reintrodurre
`rsa`, il vecchio `webrtc-dtls` o `crypto-bigint` vulnerabile. Nessuna segnalazione
è ignorata. Il tag previsto `v0.10.0` coincide con la versione preparata dell’app;
non sono stati pubblicati tag o release durante la verifica.

| Verifica | Esito |
| --- | --- |
| Suite Rust workspace, lockfile bloccato, quattro thread | 303 superati, zero errori; quattro test opt-in eseguiti separatamente |
| Oracle RSA 0.9.10 | Decifrati nuovi envelope OAEP e verificate nuove firme |
| Oracle WebRTC 0.12 | Superati entrambi i ruoli SDP; frame binari v1 identici |
| Quota atomica PostgreSQL, database temporaneo isolato | Superata |
| Bootstrap pubblico IPFS/P2P | Superato |
| Test Rust distinti, inclusi i quattro espliciti | **307 superati** |
| Test Flutter | **184 superati** |
| Clippy rigoroso, tutti i target | Superato |
| Analizzatore Flutter | Nessun problema |
| Libreria core Linux release | Compilata |
| Binario relay Linux release | Compilato |
| Client Flutter Linux release | Compilato |
| Core Android ARM64, NDK 28.2, `cargo check` | Superato |
| Controlli deploy/OpenAPI, directory relay, ricevute, backup e contratto release | Superati |
| Configurazione piattaforme, formattazione Rust e controllo whitespace del diff | Superati |
| Contratto tag con `FEDI3_RELEASE_TAG=v0.10.0` | Superato |
| Audit RustSec | **Exit 0; zero vulnerabilità; nessun advisory ignorato** |

I blocchi di sicurezza RSA/WebRTC sono rimossi e i controlli locali della release
sono verdi. Workflow remoto dei tag e runner nativi Windows/Apple non sono stati
eseguiti in questa sessione; la verifica di piattaforma resta parte della pubblicazione.


## Limiti

Build native release verificate su Linux. Per Android è verificata la compilazione
con l’NDK installato, non una chiamata su dispositivo. Le build native Windows,
macOS, iOS e il workflow remoto dei tag richiedono i rispettivi runner. Profili
reali, servizi di produzione e client installati non sono stati modificati.
Il rapporto storico del 7 novembre rimane consultabile con rimando a questa correzione.
