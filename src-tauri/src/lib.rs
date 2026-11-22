//! ArchLab core: capacity planning and deterministic distributed-system simulation.
//!
//! Tauri commands in [`commands`] stay thin and delegate to the domain modules.

pub mod chaos;
pub mod commands;
pub mod domain;
pub mod error;
pub mod export;
pub mod ipc;
pub mod runtime;
pub mod sim;
pub mod state;
pub mod storage;
pub mod whatif;

use tauri::Manager;

use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = ipc::builder();

    #[cfg(debug_assertions)]
    if let Err(err) = ipc::export_bindings(std::path::Path::new("../src/bindings.ts")) {
        eprintln!("warning: failed to export TypeScript bindings: {err}");
    }

    let typed = builder.invoke_handler();
    let raw: Box<dyn Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync> =
        Box::new(tauri::generate_handler![commands::export::video_frame]);

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(move |invoke| {
            if invoke.message.command() == "video_frame" {
                raw(invoke)
            } else {
                typed(invoke)
            }
        })
        .setup(move |app| {
            builder.mount_events(app);
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db = dir.join("archlab.db");
            app.manage(AppState {
                projects: storage::ProjectRepository::new(storage::open(&db)?),
                calibrations: std::sync::Arc::new(storage::CalibrationRepository::new(
                    storage::open(&db)?,
                )),
                live: sim::live::LiveRuns::default(),
                calibration_job: runtime::jobs::JobSlot::default(),
                real_job: runtime::jobs::JobSlot::default(),
                whatif_job: runtime::jobs::JobSlot::default(),
                video: std::sync::Mutex::new(None),
                scratch_dir: dir.join("scratch"),
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
