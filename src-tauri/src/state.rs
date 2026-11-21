use std::path::PathBuf;
use std::sync::Arc;

use crate::runtime::jobs::JobSlot;
use crate::sim::live::LiveRuns;
use crate::storage::{CalibrationRepository, ProjectRepository};

/// Shared application state managed by Tauri.
pub struct AppState {
    pub projects: ProjectRepository,
    pub calibrations: Arc<CalibrationRepository>,
    pub live: LiveRuns,
    pub calibration_job: JobSlot,
    pub real_job: JobSlot,
    pub whatif_job: JobSlot,
    /// Scratch folder for the disk benchmark.
    pub scratch_dir: PathBuf,
}
