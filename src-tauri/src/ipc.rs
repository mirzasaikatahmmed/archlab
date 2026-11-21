//! Command registry and TypeScript binding generation (tauri-specta).

use specta_typescript::Typescript;
use tauri_specta::{Builder, ErrorHandlingMode, collect_commands};

use crate::commands::{app, detect, project, simulation, templates};

pub fn builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            app::app_info,
            detect::detect_machine,
            templates::list_templates,
            project::list_projects,
            project::get_project,
            project::create_project,
            project::save_project,
            project::delete_project,
            simulation::run_simulation,
        ])
        .error_handling(ErrorHandlingMode::Result)
        .disable_serde_phases()
}

/// Generates the TypeScript bindings into `path`.
pub fn export_bindings(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    builder().export(Typescript::default(), path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BINDINGS: &str = "../src/bindings.ts";

    /// Regenerate with `pnpm bindings`.
    #[test]
    #[ignore = "writes src/bindings.ts; run via `pnpm bindings`"]
    fn write_bindings() {
        export_bindings(std::path::Path::new(BINDINGS)).expect("export");
    }

    #[test]
    fn committed_bindings_are_up_to_date() {
        let dir = tempfile::tempdir().expect("tempdir");
        let generated = dir.path().join("bindings.ts");
        export_bindings(&generated).expect("export");
        let fresh = std::fs::read_to_string(generated).expect("read generated");
        let committed = std::fs::read_to_string(BINDINGS).unwrap_or_default();
        assert_eq!(
            fresh.replace("\r\n", "\n"),
            committed.replace("\r\n", "\n"),
            "src/bindings.ts is stale; run `pnpm bindings`"
        );
    }
}
