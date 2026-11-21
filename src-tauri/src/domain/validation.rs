use serde::{Deserialize, Serialize};
use specta::Type;

/// One validation problem, addressed by a camelCase field path (e.g. `disk.iops`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldIssue {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, thiserror::Error)]
#[error("validation failed with {} issue(s)", .issues.len())]
#[serde(rename_all = "camelCase")]
pub struct ValidationError {
    pub issues: Vec<FieldIssue>,
}

/// Collects validation issues so every problem is reported at once.
#[derive(Debug, Default)]
pub struct Issues(Vec<FieldIssue>);

impl Issues {
    pub fn push(&mut self, field: impl Into<String>, message: impl Into<String>) {
        self.0.push(FieldIssue {
            field: field.into(),
            message: message.into(),
        });
    }

    pub fn check(&mut self, ok: bool, field: &str, message: &str) {
        if !ok {
            self.push(field, message);
        }
    }

    pub fn range_f64(&mut self, field: &str, value: f64, min: f64, max: f64) {
        if !(value.is_finite() && value >= min && value <= max) {
            self.push(field, format!("must be between {min} and {max}"));
        }
    }

    pub fn range_u32(&mut self, field: &str, value: u32, min: u32, max: u32) {
        if !(min..=max).contains(&value) {
            self.push(field, format!("must be between {min} and {max}"));
        }
    }

    /// Merges nested issues, prefixing their field paths.
    pub fn nested(&mut self, prefix: &str, result: Result<(), ValidationError>) {
        if let Err(err) = result {
            for issue in err.issues {
                self.push(format!("{prefix}.{}", issue.field), issue.message);
            }
        }
    }

    pub fn finish(self) -> Result<(), ValidationError> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(ValidationError { issues: self.0 })
        }
    }
}
