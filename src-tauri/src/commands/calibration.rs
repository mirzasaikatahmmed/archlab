use tauri::State;
use tauri::ipc::Channel;

use crate::domain::calibration::{Calibration, apply_to_profile};
use crate::domain::server_profile::ServerProfile;
use crate::error::AppError;
use crate::runtime::calibrate::{self, CalibrationEvent};
use crate::state::AppState;

/// Benchmarks this machine in the background, reporting each step through `on_event`.
#[tauri::command]
#[specta::specta]
pub fn start_calibration(
    state: State<'_, AppState>,
    on_event: Channel<CalibrationEvent>,
) -> Result<(), AppError> {
    let repo = std::sync::Arc::clone(&state.calibrations);
    let scratch = state.scratch_dir.clone();
    state.calibration_job.spawn("calibration", move |cancel| {
        let send = |event: CalibrationEvent| on_event.send(event).is_ok();
        let outcome = match calibrate::run(&scratch, cancel, &send) {
            Ok(Some(c)) => match repo.save(&c) {
                Ok(()) => CalibrationEvent::Finished { calibration: c },
                Err(e) => CalibrationEvent::Failed {
                    message: e.to_string(),
                },
            },
            Ok(None) => CalibrationEvent::Cancelled,
            Err(e) => CalibrationEvent::Failed {
                message: e.to_string(),
            },
        };
        send(outcome);
    })?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn cancel_calibration(state: State<'_, AppState>) -> Result<(), AppError> {
    state.calibration_job.cancel("calibration")?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn latest_calibration(state: State<'_, AppState>) -> Result<Option<Calibration>, AppError> {
    Ok(state.calibrations.latest()?)
}

/// Returns `profile` with the newest calibration's measurements filled in.
#[tauri::command]
#[specta::specta]
pub fn apply_calibration(
    state: State<'_, AppState>,
    profile: ServerProfile,
) -> Result<ServerProfile, AppError> {
    let calibration = state
        .calibrations
        .latest()?
        .ok_or_else(|| AppError::Runtime {
            message: "Calibrate this machine first.".to_owned(),
        })?;
    Ok(apply_to_profile(&calibration, &profile))
}
