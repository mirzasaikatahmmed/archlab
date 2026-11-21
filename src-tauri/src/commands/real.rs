use std::sync::Arc;

use tauri::State;
use tauri::ipc::Channel;

use crate::domain::project::Project;
use crate::error::AppError;
use crate::runtime::real::{self, RealEvent};
use crate::sim::SimOptions;
use crate::state::AppState;

/// Runs the project on real localhost sockets and streams progress, then the comparison
/// with the simulation, through `on_event`.
#[tauri::command]
#[specta::specta]
pub fn start_real_run(
    state: State<'_, AppState>,
    project: Project,
    options: SimOptions,
    on_event: Channel<RealEvent>,
) -> Result<(), AppError> {
    real::validate(&project, &options)?;
    state.real_job.spawn("real run", move |cancel| {
        let sink: Arc<dyn Fn(RealEvent) -> bool + Send + Sync> =
            Arc::new(move |event| on_event.send(event).is_ok());
        let outcome = match real::run(&project, &options, cancel, &sink) {
            Ok(Some(comparison)) => RealEvent::Finished {
                comparison: Box::new(comparison),
            },
            Ok(None) => RealEvent::Cancelled,
            Err(e) => RealEvent::Failed {
                message: e.to_string(),
            },
        };
        sink(outcome);
    })?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn cancel_real_run(state: State<'_, AppState>) -> Result<(), AppError> {
    state.real_job.cancel("real run")?;
    Ok(())
}
