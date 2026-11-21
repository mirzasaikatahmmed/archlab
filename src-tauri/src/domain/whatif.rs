//! What-if definitions: targets (SLO) and named variants, each a list of typed changes to a
//! project. Applying changes never touches the original project.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::project::Project;
use super::validation::{Issues, ValidationError};

pub const MAX_VARIANTS: usize = 20;
pub const MAX_CHANGES: usize = 10;
pub const MAX_NAME_CHARS: usize = 60;
pub const SLO_P95_MS_RANGE: (f64, f64) = (1.0, 60_000.0);
pub const SLO_ERROR_RATE_RANGE: (f64, f64) = (0.0, 1.0);
pub const TRAFFIC_SCALE_RANGE: (f64, f64) = (0.01, 100.0);

/// The targets a design has to meet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Slo {
    /// 95th percentile latency limit, in milliseconds.
    pub p95_ms: f64,
    /// Highest acceptable share of failed requests, 0..=1.
    pub max_error_rate: f64,
}

impl Default for Slo {
    fn default() -> Self {
        Self {
            p95_ms: 200.0,
            max_error_rate: 0.01,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ServerField {
    CpuCores,
    CpuGhz,
    RamGb,
    DiskIops,
    NetworkBandwidthMbps,
    NetworkLatencyMs,
    FdLimit,
}

impl ServerField {
    pub fn label(self) -> &'static str {
        match self {
            Self::CpuCores => "CPU cores",
            Self::CpuGhz => "CPU clock (GHz)",
            Self::RamGb => "RAM (GB)",
            Self::DiskIops => "Disk IOPS",
            Self::NetworkBandwidthMbps => "Network bandwidth (Mbps)",
            Self::NetworkLatencyMs => "Network latency (ms)",
            Self::FdLimit => "File descriptor limit",
        }
    }

    fn is_integer(self) -> bool {
        matches!(self, Self::CpuCores | Self::DiskIops | Self::FdLimit)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Change {
    /// Multiply the request rate.
    TrafficScale {
        factor: f64,
    },
    /// Set the request rate.
    TrafficRate {
        rps: f64,
    },
    Server {
        field: ServerField,
        value: f64,
    },
    NodeSetting {
        node: String,
        key: String,
        value: f64,
    },
    /// Run this many copies of a node behind round-robin.
    Replicas {
        node: String,
        count: u32,
    },
    /// Remove every injected fault.
    ChaosOff,
}

impl Change {
    /// The node this change refers to, if any.
    pub fn node(&self) -> Option<&str> {
        match self {
            Self::NodeSetting { node, .. } | Self::Replicas { node, .. } => Some(node),
            _ => None,
        }
    }

    pub fn describe(&self, project: &Project) -> String {
        let name = |id: &str| {
            project
                .topology
                .nodes
                .iter()
                .find(|n| n.id == id)
                .map_or_else(|| id.to_owned(), |n| n.label.clone())
        };
        match self {
            Self::TrafficScale { factor } => format!("Traffic \u{d7}{factor}"),
            Self::TrafficRate { rps } => format!("Traffic {rps} req/s"),
            Self::Server { field, value } => format!("{} = {value}", field.label()),
            Self::NodeSetting { node, key, value } => format!("{} {key} = {value}", name(node)),
            Self::Replicas { node, count } => format!("{} \u{d7}{count} replicas", name(node)),
            Self::ChaosOff => "No injected faults".to_owned(),
        }
    }

    fn apply(&self, p: &mut Project) -> Result<(), String> {
        match self {
            Self::TrafficScale { factor } => {
                let (lo, hi) = TRAFFIC_SCALE_RANGE;
                if !(factor.is_finite() && *factor >= lo && *factor <= hi) {
                    return Err(format!("traffic scale must be between {lo} and {hi}"));
                }
                p.traffic_profile.requests_per_second *= factor;
            }
            Self::TrafficRate { rps } => p.traffic_profile.requests_per_second = *rps,
            Self::Server { field, value } => set_server_field(p, *field, *value)?,
            Self::NodeSetting { node, key, value } => {
                let n = p
                    .topology
                    .nodes
                    .iter_mut()
                    .find(|n| &n.id == node)
                    .ok_or_else(|| format!("unknown node {node}"))?;
                n.settings.insert(key.clone(), *value);
            }
            Self::Replicas { node, count } => {
                let n = p
                    .topology
                    .nodes
                    .iter_mut()
                    .find(|n| &n.id == node)
                    .ok_or_else(|| format!("unknown node {node}"))?;
                n.replicas = *count;
            }
            Self::ChaosOff => p.chaos.faults.clear(),
        }
        Ok(())
    }
}

fn set_server_field(p: &mut Project, field: ServerField, value: f64) -> Result<(), String> {
    if !value.is_finite() {
        return Err(format!("{} must be a number", field.label()));
    }
    let whole = || -> Result<u32, String> {
        if value.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&value) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Ok(value as u32)
        } else {
            Err(format!("{} must be a whole number", field.label()))
        }
    };
    let s = &mut p.server_profile;
    match field {
        ServerField::CpuCores => s.cpu_cores = whole()?,
        ServerField::DiskIops => s.disk.iops = whole()?,
        ServerField::FdLimit => s.fd_limit = whole()?,
        ServerField::CpuGhz => s.cpu_ghz = value,
        ServerField::RamGb => s.ram_gb = value,
        ServerField::NetworkBandwidthMbps => s.network.bandwidth_mbps = value,
        ServerField::NetworkLatencyMs => s.network.latency_ms = value,
    }
    debug_assert!(!field.is_integer() || value.fract() == 0.0);
    Ok(())
}

/// Applies `changes` to a copy of `base` and checks that the result is a valid project.
pub fn apply_changes(base: &Project, changes: &[Change]) -> Result<Project, ValidationError> {
    let mut issues = Issues::default();
    let mut project = base.clone();
    for (i, change) in changes.iter().enumerate() {
        if let Err(message) = change.apply(&mut project) {
            issues.push(format!("changes.{i}"), message);
        }
    }
    issues.nested("project", project.validate_core());
    issues.finish().map(|()| project)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
    pub id: String,
    pub name: String,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WhatIf {
    pub slo: Slo,
    pub variants: Vec<Variant>,
}

impl WhatIf {
    pub fn validate(&self, project: &Project) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        let (lo, hi) = SLO_P95_MS_RANGE;
        issues.range_f64("slo.p95Ms", self.slo.p95_ms, lo, hi);
        let (lo, hi) = SLO_ERROR_RATE_RANGE;
        issues.range_f64("slo.maxErrorRate", self.slo.max_error_rate, lo, hi);
        issues.check(
            self.variants.len() <= MAX_VARIANTS,
            "variants",
            &format!("at most {MAX_VARIANTS} variants"),
        );
        let mut ids = BTreeSet::new();
        for (i, v) in self.variants.iter().enumerate() {
            let at = format!("variants.{i}");
            issues.check(
                !v.id.trim().is_empty() && ids.insert(v.id.as_str()),
                &format!("{at}.id"),
                "must be unique and not empty",
            );
            issues.check(
                (1..=MAX_NAME_CHARS).contains(&v.name.trim().chars().count()),
                &format!("{at}.name"),
                &format!("must be 1-{MAX_NAME_CHARS} characters"),
            );
            issues.check(
                v.changes.len() <= MAX_CHANGES,
                &format!("{at}.changes"),
                &format!("at most {MAX_CHANGES} changes"),
            );
            if let Err(err) = apply_changes(project, &v.changes) {
                issues.nested(&at, Err(err));
            }
        }
        issues.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::template;

    fn project() -> Project {
        Project::new("What-if")
    }

    #[test]
    fn default_whatif_is_valid() {
        assert!(WhatIf::default().validate(&project()).is_ok());
    }

    #[test]
    fn applies_every_kind_of_change_without_touching_the_original() {
        let base = project();
        let changes = [
            Change::TrafficScale { factor: 2.0 },
            Change::Server {
                field: ServerField::CpuCores,
                value: 16.0,
            },
            Change::Server {
                field: ServerField::NetworkLatencyMs,
                value: 2.0,
            },
            Change::Replicas {
                node: "app".into(),
                count: 3,
            },
            Change::ChaosOff,
        ];
        let out = apply_changes(&base, &changes).expect("valid");
        assert_eq!(
            out.traffic_profile.requests_per_second,
            base.traffic_profile.requests_per_second * 2.0
        );
        assert_eq!(out.server_profile.cpu_cores, 16);
        assert_eq!(out.server_profile.network.latency_ms, 2.0);
        assert_eq!(out.topology.nodes[1].replicas, 3);
        assert_eq!(
            base.server_profile.cpu_cores,
            project().server_profile.cpu_cores
        );
        assert_eq!(base.topology.nodes[1].replicas, 1);
    }

    #[test]
    fn node_settings_need_a_template_that_knows_the_key() {
        let mut base = project();
        let t = template::find("postgres").expect("postgres");
        base.topology.nodes[2].template = Some(template::TemplateRef {
            id: t.id.clone(),
            version: t.version,
        });
        let ok = Change::NodeSetting {
            node: "db".into(),
            key: "max_connections".into(),
            value: 400.0,
        };
        assert!(apply_changes(&base, &[ok]).is_ok());
        let bad_key = Change::NodeSetting {
            node: "db".into(),
            key: "nope".into(),
            value: 1.0,
        };
        assert!(apply_changes(&base, &[bad_key]).is_err());
        let out_of_range = Change::NodeSetting {
            node: "db".into(),
            key: "max_connections".into(),
            value: 0.0,
        };
        assert!(apply_changes(&base, &[out_of_range]).is_err());
    }

    #[test]
    fn rejects_bad_values_unknown_nodes_and_fractional_counts() {
        let base = project();
        let bad = [
            Change::TrafficScale { factor: 0.0 },
            Change::Server {
                field: ServerField::CpuCores,
                value: 2.5,
            },
            Change::Server {
                field: ServerField::CpuGhz,
                value: 99.0,
            },
            Change::Replicas {
                node: "ghost".into(),
                count: 2,
            },
            Change::Replicas {
                node: "app".into(),
                count: 0,
            },
        ];
        for change in bad {
            assert!(
                apply_changes(&base, std::slice::from_ref(&change)).is_err(),
                "{change:?}"
            );
        }
    }

    #[test]
    fn validation_reports_the_variant_that_is_wrong() {
        let mut w = WhatIf::default();
        w.slo.max_error_rate = 2.0;
        w.variants.push(Variant {
            id: "a".into(),
            name: "  ".into(),
            changes: vec![Change::TrafficScale { factor: -1.0 }],
        });
        w.variants.push(Variant {
            id: "a".into(),
            name: "Duplicate id".into(),
            changes: Vec::new(),
        });
        let err = w.validate(&project()).expect_err("invalid");
        let fields: Vec<_> = err.issues.iter().map(|i| i.field.as_str()).collect();
        assert!(fields.contains(&"slo.maxErrorRate"));
        assert!(fields.contains(&"variants.0.name"));
        assert!(fields.contains(&"variants.1.id"));
        assert!(fields.iter().any(|f| f.starts_with("variants.0.changes.0")));
    }

    #[test]
    fn descriptions_use_node_labels() {
        let p = project();
        let c = Change::Replicas {
            node: "app".into(),
            count: 4,
        };
        assert_eq!(c.describe(&p), "App server \u{d7}4 replicas");
        assert_eq!(c.node(), Some("app"));
        assert_eq!(Change::ChaosOff.node(), None);
    }

    #[test]
    fn changes_round_trip_through_json() {
        let c = Change::NodeSetting {
            node: "db".into(),
            key: "k".into(),
            value: 1.5,
        };
        let json = serde_json::to_string(&c).expect("serialize");
        assert!(json.contains(r#""kind":"nodeSetting""#));
        assert_eq!(serde_json::from_str::<Change>(&json).expect("parse"), c);
    }
}
