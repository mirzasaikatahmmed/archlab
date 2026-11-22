use crate::domain::blueprint::{self, BlueprintView};
use crate::domain::project::Project;
use crate::error::AppError;

#[tauri::command]
#[specta::specta]
pub fn list_blueprints() -> Vec<BlueprintView> {
    blueprint::builtin()
        .iter()
        .map(blueprint::BlueprintDef::view)
        .collect()
}

/// The blueprint at one size, as a new unsaved project.
#[tauri::command]
#[specta::specta]
pub fn blueprint_project(id: String, tier: String) -> Result<Project, AppError> {
    let def = blueprint::find(&id).ok_or(AppError::NotFound { id })?;
    Ok(def.project(&tier)?)
}
