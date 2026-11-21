use tauri::State;
use tauri::ipc::Channel;

use crate::domain::project::Project;
use crate::error::AppError;
use crate::sim::live::{LiveCommand, LiveEvent};
use crate::sim::{self, SimOptions, SimReport};
use crate::state::AppState;

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

/// Starts a live run that streams one tick per simulated second through `on_event`.
/// `speed` is simulated seconds per real second; `None` runs as fast as possible.
#[tauri::command]
#[specta::specta]
pub fn start_live_run(
    state: State<'_, AppState>,
    project: Project,
    options: SimOptions,
    speed: Option<f64>,
    on_event: Channel<LiveEvent>,
) -> Result<(), AppError> {
    state.live.start(&project, &options, speed, move |event| {
        on_event.send(event).is_ok()
    })?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn control_live_run(state: State<'_, AppState>, command: LiveCommand) -> Result<(), AppError> {
    state.live.control(command)?;
    Ok(())
}
