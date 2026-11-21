use crate::domain::project::Project;
use crate::error::AppError;
use crate::sim::{self, SimOptions, SimReport};

/// Runs the simulation for the (possibly unsaved) project on a worker thread.
#[tauri::command]
#[specta::specta]
pub async fn run_simulation(project: Project, options: SimOptions) -> Result<SimReport, AppError> {
    let result = tauri::async_runtime::spawn_blocking(move || sim::run(&project, &options))
        .await
        .map_err(|e| AppError::Simulation {
            message: format!("simulation task failed: {e}"),
        })?;
    Ok(result?)
}
