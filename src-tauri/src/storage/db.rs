use std::path::Path;
use std::sync::LazyLock;

use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

use super::StorageError;

static MIGRATIONS: LazyLock<Migrations<'static>> =
    LazyLock::new(|| Migrations::new(vec![M::up(include_str!("migrations/001_init.sql"))]));

fn configure(conn: &mut Connection) -> Result<(), StorageError> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    MIGRATIONS.to_latest(conn)?;
    Ok(())
}

/// Opens (creating if needed) the database at `path` and applies migrations.
pub fn open(path: &Path) -> Result<Connection, StorageError> {
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    configure(&mut conn)?;
    Ok(conn)
}

#[cfg(test)]
pub fn open_in_memory() -> Result<Connection, StorageError> {
    let mut conn = Connection::open_in_memory()?;
    configure(&mut conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid() {
        MIGRATIONS.validate().expect("migrations validate");
    }

    #[test]
    fn open_creates_schema_and_is_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("t.db");
        open(&path).expect("first open");
        let conn = open(&path).expect("second open");
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 'projects'",
                [],
                |r| r.get(0),
            )
            .expect("query");
        assert_eq!(n, 1);
    }
}
