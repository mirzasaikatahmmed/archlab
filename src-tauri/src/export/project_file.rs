//! The `.archlab.json` project file: a project and, optionally, the report of its last run.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::project::Project;
use crate::runtime::RuntimeError;
use crate::sim::report::SimReport;

pub const FORMAT: &str = "archlab-project";
pub const SCHEMA_VERSION: u32 = 1;
/// Larger files are rejected before parsing.
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectFile {
    format: String,
    schema_version: u32,
    project: Project,
    #[serde(default)]
    report: Option<SimReport>,
}

pub fn to_json(project: &Project, report: Option<&SimReport>) -> Result<String, RuntimeError> {
    let file = ProjectFile {
        format: FORMAT.to_owned(),
        schema_version: SCHEMA_VERSION,
        project: project.clone(),
        report: report.cloned(),
    };
    serde_json::to_string_pretty(&file)
        .map_err(|e| RuntimeError::Failed(format!("could not encode the project: {e}")))
}

/// Parses and validates a project file. The project gets a new id, so importing never
/// overwrites an existing project.
pub fn from_json(text: &str) -> Result<Project, RuntimeError> {
    let file: ProjectFile = serde_json::from_str(text)
        .map_err(|e| RuntimeError::Failed(format!("this is not a valid ArchLab file: {e}")))?;
    if file.format != FORMAT {
        return Err(RuntimeError::Failed(
            "this file is not an ArchLab project".to_owned(),
        ));
    }
    if file.schema_version == 0 || file.schema_version > SCHEMA_VERSION {
        return Err(RuntimeError::Failed(format!(
            "this file uses format version {}, but this app reads up to {SCHEMA_VERSION}; update ArchLab",
            file.schema_version
        )));
    }
    let mut project = file.project;
    project.id = Uuid::new_v4().to_string();
    project.validate()?;
    Ok(project)
}

/// Reads and parses the file at `path`.
pub fn read(path: &std::path::Path) -> Result<Project, RuntimeError> {
    super::check_extension(path, "json")?;
    let size = std::fs::metadata(path)
        .map_err(|e| RuntimeError::Failed(format!("could not read {}: {e}", path.display())))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(RuntimeError::Failed("this file is too large".to_owned()));
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| RuntimeError::Failed(format!("could not read {}: {e}", path.display())))?;
    from_json(&text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::Project;

    fn project() -> Project {
        Project::new("Demo")
    }

    #[test]
    fn round_trips_with_a_fresh_id() {
        let original = project();
        let text = to_json(&original, None).expect("encode");
        let back = from_json(&text).expect("decode");
        assert_ne!(back.id, original.id);
        assert_eq!(back.name, original.name);
        assert_eq!(back.topology, original.topology);
        assert_eq!(back.server_profile, original.server_profile);
    }

    #[test]
    fn rejects_other_formats_and_newer_versions() {
        let text = to_json(&project(), None).expect("encode");
        let other = text.replace(FORMAT, "something-else");
        assert!(from_json(&other).is_err());
        let newer = text.replace("\"schemaVersion\": 1", "\"schemaVersion\": 99");
        let err = from_json(&newer).expect_err("newer");
        assert!(err.to_string().contains("update ArchLab"));
        assert!(from_json("not json").is_err());
    }

    #[test]
    fn rejects_an_invalid_project() {
        let mut bad = project();
        bad.server_profile.cpu_cores = 0;
        let text = to_json(&bad, None).expect("encode");
        assert!(matches!(from_json(&text), Err(RuntimeError::Invalid(_))));
    }

    #[test]
    fn read_checks_the_extension() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("p.txt");
        std::fs::write(&path, to_json(&project(), None).expect("encode")).expect("write");
        assert!(read(&path).is_err());
        let json = dir.path().join("p.archlab.json");
        std::fs::write(&json, to_json(&project(), None).expect("encode")).expect("write");
        assert!(read(&json).is_ok());
    }
}
