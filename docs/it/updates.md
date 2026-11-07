# Aggiornamenti del client

Configurazioni → Aggiornamenti del client controlla l’ultima release stabile
GitHub di `redhunt07/Fedi3`. Le build release controllano anche dopo l’avvio e
ogni sei ore mentre l’app è in primo piano, con un avviso per versione a
sessione. Le build debug controllano solo aprendo la schermata aggiornamenti.
Le richieste HTTPS sono in sola lettura: non inviano token, log o telemetria.

La versione locale proviene dai metadati dell’app installata. Sono proposte
solo release stabili `vX.Y.Z` strettamente più recenti. Bozze, prerelease,
metadati invalidi, URL esterni e pacchetti per altre architetture sono esclusi.
La prima versione preparata con questo sistema è `0.10.0+1`, successiva alla
release storica `0.9.19`. I vecchi client richiedono un primo aggiornamento manuale.

## Installazione

- **Linux x86_64 AppImage:** scarica, verifica e scegli Installa aggiornamento
  AppImage. Il file indicato da `APPIMAGE` viene sostituito atomicamente sullo
  stesso filesystem. La versione precedente resta in `<percorso>.previous`.
  Chiudi e riapri Fedi3. Per ripristinare, chiudi l’app e rimetti il file
  precedente al nome originale. Errori di download o verifica lasciano intatta
  l’app installata. La cartella dell’AppImage deve essere scrivibile.
- **Windows x64:** scarica lo ZIP verificato e apri la cartella download. Chiudi
  Fedi3, estrai tutto il pacchetto nella cartella dell’applicazione (o in una
  nuova cartella), quindi riaprila. Sostituisci la DLL del core insieme alla UI.
  La release portabile non richiede privilegi amministrativi, compilazione
  locale o esecuzione di script PowerShell remoti.
- **Linux da sorgenti/pacchetti, macOS e mobile:** usa la pagina della release e
  la fonte di installazione, il gestore pacchetti o lo store di provenienza.
  Il client non sovrascrive file gestiti dal sistema né propone pacchetti x64
  ad altre architetture. Questa pipeline non produce ancora asset mobile/macOS.

Configurazione del profilo, credenziali nel portachiavi, chiavi private e database
restano nelle cartelle dati esistenti: non cancellarle durante l’aggiornamento.

## Verifica dei download

I pacchetti sono scaricati in streaming in una cartella temporanea privata,
con limiti di dimensione, timeout e indicatore di avanzamento. Il SHA256 deve
coincidere con il digest `sha256:` dell’asset della release, oppure con l’unica
riga corrispondente in `checksums.txt` per le vecchie release. Download parziali
e falliti vengono rimossi. L’installazione AppImage verifica nuovamente il file
preparato. SHA256 verifica l’integrità rispetto alla release GitHub considerata
attendibile; non costituisce una firma del codice indipendente.

| Piattaforma | Asset |
| --- | --- |
| Linux x86_64 | `Fedi3-linux-x86_64.AppImage` |
| Windows x64 | `Fedi3-windows-x64.zip` |
| Checksum | `checksums.txt` |

Ogni riga di checksum usa `<64 caratteri esadecimali>  <nome esatto dell’asset>`.

## Preparazione di una release

1. Imposta `app/pubspec.yaml` a `X.Y.Z+N`, incrementando versione e build.
2. Esegui test Rust/Flutter e controlli di deploy. Fai commit del codice revisionato.
3. Pubblica il tag `vX.Y.Z` corrispondente al pubspec. Una discordanza blocca il pacchetto.
4. La CI compila core e client insieme per Linux e Windows, calcola i checksum
   di quei pacchetti e crea una release GitHub **in bozza**.
5. Scarica e prova entrambi i pacchetti sui rispettivi sistemi, poi pubblica la
   bozza. Il client vede l’aggiornamento solo dopo la pubblicazione come stabile.

Il token della CI è in sola lettura per impostazione predefinita. Solo il job
che crea la bozza riceve `contents: write`. La CI non fa commit di log o push
di telemetria nel repository.

Per creare i pacchetti localmente:

```sh
NO_BUMP=1 bash scripts/release_full_linux.sh
```

```powershell
./scripts/release_full_windows.ps1 -NoBump
```

Le dipendenze di compilazione servono sulla macchina di build, non su quella
degli utenti. Entrambi i percorsi includono il core nativo e i checksum SHA256.

I tag di release richiedono anche un audit RustSec superato. I blocchi RSA/WebRTC
sono stati rimossi l’8 novembre 2026 con test di compatibilità contro le
implementazioni precedenti. Vedi [SECURITY.md](../../SECURITY.md) e la
[verifica della migrazione](audit/crypto_compatibility_2026-11-08.md).
Il controllo resta obbligatorio: nessun advisory è ignorato.

### Pubblicazione richiesta dal maintainer

Il push di `codex/release/vX.Y.Z` richiede la pubblicazione tramite CI. Il branch
deve puntare al commit corrente di `main` e la versione deve coincidere con il
pubspec. Gli stessi controlli Rust, Flutter, operativi, compatibilità, audit e
piattaforme precedono la creazione dei pacchetti Linux/Windows. Il job verifica
i checksum, carica i pacchetti e pubblica la release come ultima stabile. Le
release già pubblicate non vengono sovrascritte; una bozza esistente deve puntare
allo stesso commit. I normali tag mantengono il percorso bozza/pubblicazione manuale.
