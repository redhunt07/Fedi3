# Backup / restore

## Postgres (relay)

```
scripts/backup_relay_pg.sh
scripts/restore_relay_pg.sh <file.sql.gz>
```

## Media (storage locale)

```
scripts/backup_relay_media.sh
scripts/restore_relay_media.sh <file.tar.gz>
```

## Note

Se usi S3/WebDAV, il backup è a carico del provider.

## Migrazioni DB (relay)

```
cat crates/fedi3_relay/sql/postgres_schema.sql | \
  docker compose exec -T postgres psql -U fedi3 -d fedi3_relay
```
