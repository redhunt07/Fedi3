# Backup e ripristino

Tutti gli script leggono la configurazione da `.env` (utente e database PostgreSQL inclusi), verificano l'integrità dell'archivio e applicano una retention (`BACKUP_KEEP`, default 7 copie). Non richiedono che l'utente o il database si chiamino `fedi3`.

## PostgreSQL (relay)

```bash
scripts/backup_relay_pg.sh [dir_destinazione]
scripts/restore_relay_pg.sh <file.sql.gz> [--yes]
```

Il backup usa `pg_dump -U "$FEDI3_RELAY_PG_USER" -d "$FEDI3_RELAY_PG_DB" --no-owner --no-privileges`, controlla che il file non sia vuoto e che l'archivio gzip sia integro.

Il restore:

1. verifica l'archivio;
2. chiede conferma (salvo `--yes`) perché sovrascrive schema e dati;
3. ferma il relay per evitare scritture concorrenti;
4. esegue `psql -v ON_ERROR_STOP=1`;
5. riavvia il relay solo se il restore è andato a buon fine.

Se `psql` fallisce, il relay resta fermo: è intenzionale, così non riparte su un database a metà.

## Media (storage locale)

```bash
scripts/backup_relay_media.sh [dir_destinazione]
scripts/restore_relay_media.sh <file.tar.gz> [--yes]
```

L'archivio contiene i percorsi relativi alla directory media (`media/...`), quindi il restore non crea il tipico annidamento `/data/media/data/media/...`. Il restore riconosce anche gli archivi legacy con percorsi assoluti (`/data/media/...`) e li estrae alla radice corretta.

## Verifica di un backup

```bash
gzip -t backups/relay_pg_<ts>.sql.gz
gzip -t backups/relay_media_<ts>.tar.gz
tar -tzf backups/relay_media_<ts>.tar.gz | head
```

Un backup non verificato non è un backup.

## Migrazioni dello schema

Lo schema viene applicato automaticamente dal relay all'avvio, con migrazioni numerate e idempotenti. Per applicarlo a mano (di solito non serve):

```bash
cat crates/fedi3_relay/sql/postgres_schema.sql | \
  docker compose exec -T postgres psql -U "$FEDI3_RELAY_PG_USER" -d "$FEDI3_RELAY_PG_DB"
```

## Backup dell'app (client)

L'app esporta solo configurazione e preferenze in JSON. **Non** contiene la chiave privata: se vuoi poter migrare l'account, salva anche la directory dati del core (`FEDI3_DATA_DIR`).

## Se usi S3/WebDAV

Con `FEDI3_RELAY_MEDIA_BACKEND=s3` o `webdav`, il backup dei media è a carico del provider: `backup_relay_media.sh` archivia solo la directory locale. Verifica che la retention e la replica del bucket siano configurate.
