//! JSON, Markdown, SVG and WebM exporters.
//!
//! Text exporters are pure functions from a project (and optionally a report) to a string.
//! [`write_text`] is the one place files are written: it only accepts the extension the
//! exporter produces, so a command cannot be pointed at an arbitrary file type.

pub mod markdown;
pub mod project_file;
pub mod svg;
pub mod video;

use std::path::Path;

use crate::runtime::RuntimeError;

/// Writes `text` to `path`, which must end in `extension` (without the dot).
pub fn write_text(path: &Path, extension: &str, text: &str) -> Result<(), RuntimeError> {
    check_extension(path, extension)?;
    std::fs::write(path, text)
        .map_err(|e| RuntimeError::Failed(format!("could not write {}: {e}", path.display())))
}

pub(crate) fn check_extension(path: &Path, extension: &str) -> Result<(), RuntimeError> {
    let matches = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(extension));
    if matches {
        Ok(())
    } else {
        Err(RuntimeError::Failed(format!(
            "the file name must end in .{extension}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_only_the_expected_extension() {
        let dir = tempfile::tempdir().expect("dir");
        let ok = dir.path().join("report.MD");
        write_text(&ok, "md", "hello").expect("write");
        assert_eq!(std::fs::read_to_string(&ok).expect("read"), "hello");

        let bad = dir.path().join("report.sh");
        assert!(write_text(&bad, "md", "x").is_err());
        assert!(!bad.exists());
        assert!(write_text(&dir.path().join("noext"), "md", "x").is_err());
    }
}
