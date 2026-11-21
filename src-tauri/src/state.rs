use crate::sim::live::LiveRuns;
use crate::storage::ProjectRepository;

/// Shared application state managed by Tauri.
pub struct AppState {
    pub projects: ProjectRepository,
    pub live: LiveRuns,
}
