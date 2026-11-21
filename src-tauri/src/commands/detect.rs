use crate::runtime::detect::{self, DetectedProfile};

#[tauri::command]
#[specta::specta]
pub fn detect_machine() -> DetectedProfile {
    detect::detect()
}
