use crate::advisor::{self, Advice, Requirements};
use crate::error::AppError;

/// Designs a system for the requirements and checks it by simulation. Runs on a worker thread:
/// it simulates several candidate designs.
#[tauri::command]
#[specta::specta]
pub async fn advise_design(requirements: Requirements) -> Result<Advice, AppError> {
    let result = tauri::async_runtime::spawn_blocking(move || advisor::advise(&requirements))
        .await
        .map_err(|e| AppError::Runtime {
            message: format!("the advisor task failed: {e}"),
        })?;
    Ok(result?)
}

/// What the advisor assumes when the form is first shown.
#[tauri::command]
#[specta::specta]
pub fn default_requirements() -> Requirements {
    Requirements::default()
}
