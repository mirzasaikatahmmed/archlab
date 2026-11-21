use crate::domain::app_info::AppInfo;

#[tauri::command]
#[specta::specta]
pub fn app_info() -> AppInfo {
    AppInfo::current()
}
