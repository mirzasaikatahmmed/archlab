use tauri::State;

use crate::domain::project::{Project, ProjectSummary};
use crate::error::AppError;
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub fn list_projects(state: State<'_, AppState>) -> Result<Vec<ProjectSummary>, AppError> {
    Ok(state.projects.list()?)
}

#[tauri::command]
#[specta::specta]
pub fn get_project(state: State<'_, AppState>, id: String) -> Result<Project, AppError> {
    Ok(state.projects.get(&id)?)
}

#[tauri::command]
#[specta::specta]
pub fn create_project(state: State<'_, AppState>, name: String) -> Result<Project, AppError> {
    let project = Project::new(name);
    project.validate()?;
    Ok(state.projects.save(&project)?)
}

#[tauri::command]
#[specta::specta]
pub fn save_project(state: State<'_, AppState>, project: Project) -> Result<Project, AppError> {
    project.validate()?;
    Ok(state.projects.save(&project)?)
}

#[tauri::command]
#[specta::specta]
pub fn delete_project(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    Ok(state.projects.delete(&id)?)
}
