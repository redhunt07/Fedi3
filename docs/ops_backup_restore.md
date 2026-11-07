# Backup and restore

All scripts read the configuration from `.env` (including the PostgreSQL user and database), verify archive integrity and apply a retention (`BACKUP_KEEP`, default 7 copies). They do not require the user or the database to be named `fedi3`.

The `.env` file is read as data, without `source` or `eval`: literal
`KEY=VALUE` assignments are supported, including with outer quotes. Shell
commands and `$(...)` substitutions are not executed. Variables already defined
in the environment take precedence. Docker Compose keeps reading and interpolating
its own `.env`; the scripts' parser does not export literal values that
could override that interpolation.

## PostgreSQL (relay)

```bash
scripts/backup_relay_pg.sh [destination_dir]
scripts/restore_relay_pg.sh <file.sql.gz> [--yes]
```

The backup uses `pg_dump -U "$FEDI3_RELAY_PG_USER" -d "$FEDI3_RELAY_PG_DB" --no-owner --no-privileges`, checks that the file is not empty and that the gzip archive is intact.

The restore:

1. verifies the archive;
2. asks for confirmation (unless `--yes`) because it overwrites schema and data;
3. stops the relay to avoid concurrent writes;
4. runs `psql -v ON_ERROR_STOP=1`;
5. restarts the relay only if the restore succeeded.

If `psql` fails, the relay stays stopped: this is intentional, so it does not restart on a half-restored database.

## Media (local storage)

```bash
scripts/backup_relay_media.sh [destination_dir]
scripts/restore_relay_media.sh <file.tar.gz> [--yes]
```

The archive contains paths relative to the media directory (`media/...`), so the restore does not create the typical nesting `/data/media/data/media/...`. The restore also recognizes legacy archives with absolute paths (`/data/media/...`) and extracts them to the correct root.

## Verifying a backup

```bash
gzip -t backups/relay_pg_<ts>.sql.gz
gzip -t backups/relay_media_<ts>.tar.gz
tar -tzf backups/relay_media_<ts>.tar.gz | head
```

An unverified backup is not a backup.

## Schema migrations

The schema is applied automatically by the relay at startup, with numbered and idempotent migrations. To apply it by hand (usually not needed):

```bash
cat crates/fedi3_relay/sql/postgres_schema.sql | \
  docker compose exec -T postgres psql -U "$FEDI3_RELAY_PG_USER" -d "$FEDI3_RELAY_PG_DB"
```

## App backup (client)

The app exports only configuration and preferences in JSON. It does **not** contain the private key: if you want to be able to migrate the account, also save the core data directory (`FEDI3_DATA_DIR`).

## If you use S3/WebDAV

With `FEDI3_RELAY_MEDIA_BACKEND=s3` or `webdav`, media backup is the provider's responsibility: `backup_relay_media.sh` archives only the local directory. Verify that bucket retention and replication are configured.


## Client settings backup (v2)

The Backup screen now exports settings encrypted with AES-GCM and a
passphrase of at least 12 characters. The passphrase is not saved in the file:
keep it separately. A modified file or a wrong password prevent
import. Old v1 JSON files are still importable and must be treated
as plaintext secrets.

This export does not contain RSA/libp2p keys, message database or media.
To migrate the same identity between computers, stop the client and also keep
an encrypted copy of the core data directory. The config JSON alone is not enough:
the tokens of the current configuration reside in the system keyring and the
file contains only a reference. The encrypted export from the app
includes the necessary settings values for the transfer.
