use std::sync::Arc;

use tauri::State;
use tauri::ipc::Channel;

use crate::domain::project::Project;
use crate::error::AppError;
use crate::sim::SimOptions;
use crate::state::AppState;
use crate::whatif::{self, WhatIfEvent, WhatIfRequest};

/// Runs a what-if analysis in the background, reporting progress and then the result.
#[tauri::command]
#[specta::specta]
pub fn start_what_if(
    state: State<'_, AppState>,
    project: Project,
    options: SimOptions,
    request: WhatIfRequest,
    on_event: Channel<WhatIfEvent>,
) -> Result<(), AppError> {
    whatif::validate(&project, &options, &request)?;
    state.whatif_job.spawn("what-if analysis", move |cancel| {
        let sink: Arc<dyn Fn(WhatIfEvent) -> bool + Send + Sync> =
            Arc::new(move |event| on_event.send(event).is_ok());
        let progress = {
            let sink = Arc::clone(&sink);
            move |done: u32, total: u32| {
                sink(WhatIfEvent::Progress { done, total });
            }
        };
        let outcome = match whatif::run(&project, &options, &request, cancel, &progress) {
            Ok(Some(result)) => WhatIfEvent::Finished {
                result: Box::new(result),
            },
            Ok(None) => WhatIfEvent::Cancelled,
            Err(e) => WhatIfEvent::Failed {
                message: e.to_string(),
            },
        };
        sink(outcome);
    })?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn cancel_what_if(state: State<'_, AppState>) -> Result<(), AppError> {
    state.whatif_job.cancel("what-if analysis")?;
    Ok(())
}
