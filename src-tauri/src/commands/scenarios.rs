use crate::domain::project::Project;
use crate::domain::scenario::{self, ScenarioSummary, ScenarioView};
use crate::error::AppError;

#[tauri::command]
#[specta::specta]
pub fn list_scenarios() -> Vec<ScenarioSummary> {
    scenario::builtin()
        .iter()
        .map(scenario::ScenarioDef::summary)
        .collect()
}

#[tauri::command]
#[specta::specta]
pub fn get_scenario(id: String) -> Result<ScenarioView, AppError> {
    let def = scenario::find(&id).ok_or(AppError::NotFound { id })?;
    Ok(def.view()?)
}

/// The design as it stands after `step` (counting from 0), as a new unsaved project.
#[tauri::command]
#[specta::specta]
pub fn scenario_step_project(id: String, step: u32) -> Result<Project, AppError> {
    let def = scenario::find(&id).ok_or(AppError::NotFound { id })?;
    Ok(def.project_at(step as usize)?)
}
