//! Independent migration versions for modules sharing the same SQLite file.
use anyhow::{ensure, Result};
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) fn mark_module_version(conn: &mut Connection, table: &str, current: u32) -> Result<()> {
    ensure!(
        matches!(table, "social_schema_version" | "delivery_schema_version"),
        "invalid schema namespace"
    );
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version(version INTEGER NOT NULL);")?;
    let legacy: Option<u32> = conn
        .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
            row.get(0)
        })
        .optional()?;
    // Version 2 was the last shared version. Never overwrite an unknown legacy
    // marker, or copy another module's future version into this namespace.
    ensure!(
        legacy.unwrap_or(2) <= 2,
        "database schema version is newer than this binary"
    );
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {table}(version INTEGER NOT NULL);"
    ))?;
    let found: Option<u32> = conn
        .query_row(&format!("SELECT version FROM {table} LIMIT 1"), [], |row| {
            row.get(0)
        })
        .optional()?;
    ensure!(
        found.unwrap_or(1) <= current,
        "{table} is newer than this binary ({current})"
    );
    let tx = conn.transaction()?;
    tx.execute("INSERT INTO schema_version(version) SELECT 2 WHERE NOT EXISTS(SELECT 1 FROM schema_version)", [])?;
    tx.execute(&format!("DELETE FROM {table}"), [])?;
    tx.execute(
        &format!("INSERT INTO {table}(version) VALUES (?1)"),
        params![current],
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn module_versions_do_not_overwrite_each_other() {
        let mut conn = Connection::open_in_memory().unwrap();
        mark_module_version(&mut conn, "social_schema_version", 3).unwrap();
        mark_module_version(&mut conn, "delivery_schema_version", 2).unwrap();
        let social: u32 = conn
            .query_row("SELECT version FROM social_schema_version", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(social, 3);
        assert!(mark_module_version(&mut conn, "social_schema_version", 2).is_err());
        mark_module_version(&mut conn, "delivery_schema_version", 2).unwrap();
    }
}
