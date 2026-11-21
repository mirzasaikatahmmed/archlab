use serde::Serialize;
use specta::Type;

use crate::domain::validation::{FieldIssue, ValidationError};
use crate::sim::SimError;
use crate::storage::StorageError;

/// Error returned by every Tauri command, typed on the TypeScript side.
#[derive(Debug, Serialize, Type, thiserror::Error)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AppError {
    #[error("validation failed")]
    Validation { issues: Vec<FieldIssue> },
    #[error("not found: {id}")]
    NotFound { id: String },
    #[error("{message}")]
    Storage { message: String },
    #[error("{message}")]
    Simulation { message: String },
}

impl From<ValidationError> for AppError {
    fn from(err: ValidationError) -> Self {
        Self::Validation { issues: err.issues }
    }
}

impl From<SimError> for AppError {
    fn from(err: SimError) -> Self {
        match err {
            SimError::Invalid(v) => v.into(),
            other => Self::Simulation {
                message: other.to_string(),
            },
        }
    }
}

impl From<StorageError> for AppError {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::NotFound(id) => Self::NotFound { id },
            other => Self::Storage {
                message: other.to_string(),
            },
        }
    }
}
