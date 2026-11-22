use crate::domain::app_info::AppInfo;

#[tauri::command]
#[specta::specta]
pub fn app_info() -> AppInfo {
    AppInfo::current()
}

/// Quits and starts the app again, to finish an update.
#[tauri::command]
#[specta::specta]
pub fn restart_app(app: tauri::AppHandle) {
    app.restart();
}
