//! Bundled SQLite persistence: connection setup, migrations and repositories.

mod db;
mod error;
mod project_repo;

pub use db::open;
pub use error::StorageError;
pub use project_repo::ProjectRepository;
