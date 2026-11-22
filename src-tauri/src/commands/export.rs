use std::path::Path;
use std::sync::PoisonError;

use serde::Deserialize;
use specta::Type;
use tauri::ipc::{InvokeBody, Request};
use tauri::{State, WebviewWindow};

use crate::domain::project::Project;
use crate::error::AppError;
use crate::export::video::{self, Encoder, VideoSpec};
use crate::export::{self, markdown, project_file, svg};
use crate::runtime::RuntimeError;
use crate::sim::report::SimReport;
use crate::state::AppState;

/// What a Save dialog is for; decides the file filter and the extension.
#[derive(Debug, Clone, Copy, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExportKind {
    Json,
    Markdown,
    Svg,
    Video,
}

impl ExportKind {
    fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Markdown => "md",
            Self::Svg => "svg",
            Self::Video => "webm",
        }
    }

    fn filter_name(self) -> &'static str {
        match self {
            Self::Json => "ArchLab project",
            Self::Markdown => "Markdown",
            Self::Svg => "SVG image",
            Self::Video => "WebM video",
        }
    }
}

/// Asks where to save. `None` means the person cancelled.
#[tauri::command]
#[specta::specta]
pub async fn pick_save_path(
    window: WebviewWindow,
    kind: ExportKind,
    file_name: String,
) -> Result<Option<String>, AppError> {
    let ext = kind.extension();
    let file = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .set_file_name(file_name)
        .add_filter(kind.filter_name(), &[ext])
        .save_file()
        .await;
    Ok(file.map(|f| {
        let mut path = f.path().to_path_buf();
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_none_or(|e| !e.eq_ignore_ascii_case(ext))
        {
            let mut name = path.into_os_string();
            name.push(format!(".{ext}"));
            path = name.into();
        }
        path.to_string_lossy().into_owned()
    }))
}

/// Asks which project file to open. `None` means the person cancelled.
#[tauri::command]
#[specta::specta]
pub async fn pick_open_path(window: WebviewWindow) -> Result<Option<String>, AppError> {
    let file = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .add_filter(ExportKind::Json.filter_name(), &["json"])
        .pick_file()
        .await;
    Ok(file.map(|f| f.path().to_string_lossy().into_owned()))
}

/// Saves the project, and the report of its last run if given, as an `.archlab.json` file.
#[tauri::command]
#[specta::specta]
pub fn export_project_json(
    path: String,
    project: Project,
    report: Option<SimReport>,
) -> Result<(), AppError> {
    let text = project_file::to_json(&project, report.as_ref())?;
    export::write_text(Path::new(&path), "json", &text)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn export_markdown(
    path: String,
    project: Project,
    report: Option<SimReport>,
) -> Result<(), AppError> {
    let text = markdown::render(&project, report.as_ref());
    export::write_text(Path::new(&path), "md", &text)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn export_svg(
    path: String,
    project: Project,
    report: Option<SimReport>,
) -> Result<(), AppError> {
    let text = svg::render(&project.name, &project.topology, report.as_ref());
    export::write_text(Path::new(&path), "svg", &text)?;
    Ok(())
}

/// Reads an `.archlab.json` file as a new, unsaved project.
#[tauri::command]
#[specta::specta]
pub fn import_project_file(path: String) -> Result<Project, AppError> {
    Ok(project_file::read(Path::new(&path))?)
}

/// Whether an ffmpeg program was found, so the UI can explain a missing one up front.
#[tauri::command]
#[specta::specta]
pub fn video_available() -> bool {
    video::locate().is_some()
}

/// Opens an encoder for `path`. Frames follow through [`video_frame`].
#[tauri::command]
#[specta::specta]
pub fn video_start(
    state: State<'_, AppState>,
    path: String,
    width: u32,
    height: u32,
    fps: u32,
) -> Result<(), AppError> {
    let program = video::locate().ok_or_else(|| RuntimeError::Failed(NO_FFMPEG.to_owned()))?;
    let mut slot = state.video.lock().unwrap_or_else(PoisonError::into_inner);
    if slot.is_some() {
        return Err(RuntimeError::Busy("video export").into());
    }
    let spec = VideoSpec { width, height, fps };
    *slot = Some(Encoder::start(&program, spec, Path::new(&path))?);
    Ok(())
}

/// One raw RGBA frame as the request body. Async so a slow encoder never blocks the UI thread.
///
/// Not in the typed registry: a raw `Request` has no generated TypeScript signature, so the
/// UI calls it by name (see `lib.rs` and `src/ipc/index.ts`).
#[tauri::command]
pub async fn video_frame(state: State<'_, AppState>, request: Request<'_>) -> Result<(), AppError> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(RuntimeError::Failed("a frame must be sent as raw bytes".to_owned()).into());
    };
    let mut slot = state.video.lock().unwrap_or_else(PoisonError::into_inner);
    let encoder = slot
        .as_mut()
        .ok_or(RuntimeError::NothingToCancel("video export"))?;
    encoder.write_frame(bytes)?;
    Ok(())
}

/// Closes the video and returns the number of frames in it.
#[tauri::command]
#[specta::specta]
pub async fn video_finish(state: State<'_, AppState>) -> Result<u32, AppError> {
    let encoder = state
        .video
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
        .ok_or(RuntimeError::NothingToCancel("video export"))?;
    Ok(encoder.finish()?)
}

/// Stops the video and removes the partial file. Does nothing if none is open.
#[tauri::command]
#[specta::specta]
pub async fn video_cancel(state: State<'_, AppState>) -> Result<(), AppError> {
    let encoder = state
        .video
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    if let Some(encoder) = encoder {
        encoder.abort();
    }
    Ok(())
}

const NO_FFMPEG: &str =
    "ffmpeg was not found. Install it (or set ARCHLAB_FFMPEG to its path) to export video.";
