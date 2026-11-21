//! Requests and results of what-if analyses.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::domain::whatif::{ServerField, Slo};
use crate::sim::report::{Bottleneck, Severity, Summary};

/// Which parameter a sweep varies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SweepParam {
    TrafficScale,
    Server { field: ServerField },
    NodeSetting { node: String, key: String },
    Replicas { node: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SweepSpec {
    pub param: SweepParam,
    pub from: f64,
    pub to: f64,
    /// Number of values to try, evenly spaced (traffic is spaced geometrically).
    pub points: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WhatIfRequest {
    /// Baseline against the project's saved variants.
    Compare,
    Sweep {
        spec: SweepSpec,
    },
    /// Highest traffic that still meets the targets.
    Capacity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NodeBrief {
    pub id: String,
    pub label: String,
    pub utilization: f64,
    pub severity: Severity,
}

/// What one simulation run came to, judged against the targets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub summary: Summary,
    pub bottleneck: Option<Bottleneck>,
    pub meets_slo: bool,
    pub nodes: Vec<NodeBrief>,
    /// Why the run could not finish, for example when it needed too many events.
    pub error: Option<String>,
}

/// How a variant differs from the baseline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Deltas {
    pub throughput_pct: Option<f64>,
    pub p95_pct: Option<f64>,
    /// Change in error rate, in percentage points.
    pub error_rate_points: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VariantResult {
    pub id: String,
    pub name: String,
    /// The changes, in words.
    pub changes: Vec<String>,
    pub outcome: Outcome,
    pub vs_baseline: Option<Deltas>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CompareReport {
    pub slo: Slo,
    pub baseline: VariantResult,
    pub variants: Vec<VariantResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SweepPoint {
    pub value: f64,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SweepReport {
    pub slo: Slo,
    /// Name of the varied parameter.
    pub label: String,
    pub points: Vec<SweepPoint>,
    /// First value, in sweep order, at which the targets were missed.
    pub first_miss: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub rps: f64,
    pub p95_ms: f64,
    pub error_rate: f64,
    pub meets_slo: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CapacityLimit {
    pub node: String,
    pub reason: String,
    /// Which targets were missed just above the limit.
    pub missed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CapacityReport {
    pub slo: Slo,
    pub current_rps: f64,
    pub current_meets_slo: bool,
    /// Highest request rate found that meets the targets.
    pub max_rps: Option<f64>,
    /// `max_rps / current_rps`; below 1 means the design is already over its limit.
    pub headroom: Option<f64>,
    /// The search stopped at its upper bound without finding a limit.
    pub reached_search_limit: bool,
    pub limit: Option<CapacityLimit>,
    pub probes: Vec<Probe>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WhatIfResult {
    Compare { report: CompareReport },
    Sweep { report: SweepReport },
    Capacity { report: CapacityReport },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WhatIfEvent {
    Progress { done: u32, total: u32 },
    Finished { result: Box<WhatIfResult> },
    Cancelled,
    Failed { message: String },
}
