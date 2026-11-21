#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),
    #[error("stored data is corrupt: {0}")]
    Json(#[from] serde_json::Error),
    #[error("project not found: {0}")]
    NotFound(String),
    #[error("project was written by a newer version of ArchLab (schema {0})")]
    NewerSchema(u32),
    #[error("database lock poisoned")]
    Poisoned,
}
