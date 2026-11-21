use serde::Serialize;
use specta::Type;

/// Static metadata about the running application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

impl AppInfo {
    pub fn current() -> Self {
        Self {
            name: "ArchLab".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_reports_package_version() {
        let info = AppInfo::current();
        assert_eq!(info.name, "ArchLab");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn serializes_as_camel_case() {
        let json = serde_json::to_value(AppInfo::current()).expect("serializes");
        assert!(json.get("name").is_some());
        assert!(json.get("version").is_some());
    }
}
