use tauri::{AppHandle, Manager};

use crate::error::AppError;
use crate::runtime::RuntimeError;
use crate::uninstall::{self, UninstallPlan};

fn data_dir(app: &AppHandle) -> std::path::PathBuf {
    app.path().app_data_dir().unwrap_or_default()
}

/// What uninstalling would do on this computer, for the confirmation. Changes nothing.
#[tauri::command]
#[specta::specta]
pub fn uninstall_plan(app: AppHandle) -> UninstallPlan {
    let ctx = uninstall::detect();
    uninstall::describe(&uninstall::choose(&ctx), &data_dir(&app))
}

/// Uninstalls ArchLab, then quits. The plan is worked out again here: the window is not trusted
/// to say what to delete. `delete_data` also removes the person's projects and settings.
#[tauri::command]
#[specta::specta]
pub async fn uninstall_app(app: AppHandle, delete_data: bool) -> Result<(), AppError> {
    let ctx = uninstall::detect();
    let action = uninstall::choose(&ctx);
    let dir = data_dir(&app);
    let home = ctx.home.clone();
    tauri::async_runtime::spawn_blocking(move || {
        uninstall::execute(&action, delete_data, &dir, home.as_deref())
    })
    .await
    .map_err(|e| RuntimeError::Failed(format!("the uninstall task failed: {e}")))??;
    app.exit(0);
    Ok(())
}
