use crate::domain::template::{self, ConfigTemplate};

#[tauri::command]
#[specta::specta]
pub fn list_templates() -> Vec<ConfigTemplate> {
    template::builtin().to_vec()
}
