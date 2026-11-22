use tauri::State;
use tauri::ipc::Channel;

use crate::domain::project::Project;
use crate::error::AppError;
use crate::sim::control::ControlRecord;
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
/// `speed` is simulated seconds per real second; `None` runs as fast as possible. `script` is a list
/// of logged commands to re-apply at their seconds (empty for a normal run).
#[tauri::command]
#[specta::specta]
pub fn start_live_run(
    state: State<'_, AppState>,
    project: Project,
    options: SimOptions,
    speed: Option<f64>,
    script: Vec<ControlRecord>,
    on_event: Channel<LiveEvent>,
) -> Result<(), AppError> {
    state
        .live
        .start(&project, &options, speed, &script, move |event| {
            on_event.send(event).is_ok()
        })?;
    Ok(())
}

/// Pauses, resumes, stops or steers the live run. Returns the log entry for a steering command.
/// Async because a steering command waits for the run to apply it.
#[tauri::command]
#[specta::specta]
pub async fn control_live_run(
    state: State<'_, AppState>,
    command: LiveCommand,
) -> Result<Option<ControlRecord>, AppError> {
    Ok(state.live.control(command)?)
}
