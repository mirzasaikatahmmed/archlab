use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use super::chaos::ChaosPlan;
use super::server_profile::ServerProfile;
use super::topology::Topology;
use super::traffic::TrafficProfile;
use super::validation::{Issues, ValidationError};
use super::whatif::WhatIf;

pub const MAX_NAME_CHARS: usize = 100;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub server_profile: ServerProfile,
    pub traffic_profile: TrafficProfile,
    pub topology: Topology,
    pub chaos: ChaosPlan,
    pub whatif: WhatIf,
    /// Unix epoch milliseconds. Set by the storage layer on save.
    pub created_at: f64,
    pub updated_at: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub node_count: u32,
    pub updated_at: f64,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            server_profile: ServerProfile::default(),
            traffic_profile: TrafficProfile::default(),
            topology: Topology::starter(),
            chaos: ChaosPlan::default(),
            whatif: WhatIf::default(),
            created_at: 0.0,
            updated_at: 0.0,
        }
    }

    /// Everything except the what-if variants, which are checked by applying them to this.
    pub fn validate_core(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        issues.check(
            Uuid::parse_str(&self.id).is_ok(),
            "id",
            "must be a valid UUID",
        );
        let name_len = self.name.trim().chars().count();
        issues.check(
            (1..=MAX_NAME_CHARS).contains(&name_len),
            "name",
            &format!("must be 1-{MAX_NAME_CHARS} characters"),
        );
        issues.nested("serverProfile", self.server_profile.validate());
        issues.nested("trafficProfile", self.traffic_profile.validate());
        issues.nested("topology", self.topology.validate());
        issues.nested("chaos", self.chaos.validate(&self.topology));
        issues.finish()
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        issues.nested("", self.validate_core());
        issues.nested("whatif", self.whatif.validate(self));
        issues.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_project_is_valid() {
        assert!(Project::new("Demo").validate().is_ok());
    }

    #[test]
    fn nested_issues_are_prefixed() {
        let mut p = Project::new("  ");
        p.server_profile.cpu_cores = 0;
        p.topology.nodes[0].label.clear();
        let err = p.validate().expect_err("invalid");
        let fields: Vec<_> = err.issues.iter().map(|i| i.field.as_str()).collect();
        assert_eq!(
            fields,
            ["name", "serverProfile.cpuCores", "topology.nodes.0.label"]
        );
    }

    #[test]
    fn chaos_issues_are_prefixed() {
        let mut p = Project::new("Demo");
        p.chaos.policy.max_retries = 99;
        let err = p.validate().expect_err("invalid");
        assert_eq!(err.issues[0].field, "chaos.policy.maxRetries");
    }

    #[test]
    fn overlong_name_is_rejected() {
        let p = Project::new("x".repeat(MAX_NAME_CHARS + 1));
        assert!(p.validate().is_err());
    }
}
