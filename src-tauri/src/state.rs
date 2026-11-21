use crate::storage::ProjectRepository;

/// Shared application state managed by Tauri.
pub struct AppState {
    pub projects: ProjectRepository,
}
