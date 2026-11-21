//! Simulation inputs and results exposed to the UI.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::domain::topology::NodeKind;
use crate::domain::validation::{Issues, ValidationError};

pub const MAX_DURATION_S: f64 = 3_600.0;

/// Label attached to every report; results are model output, not measurements.
pub const ESTIMATE_NOTICE: &str = "Estimate from a simulation model. \
Real performance depends on workload and configuration details this model does not capture.";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SimOptions {
    /// Same seed and inputs always give the same report.
    pub seed: u32,
    /// Simulated time to run, in seconds.
    pub duration_s: f64,
    /// Initial period excluded from the statistics, in seconds.
    pub warmup_s: f64,
}

impl Default for SimOptions {
    fn default() -> Self {
        Self {
            seed: 1,
            duration_s: 60.0,
            warmup_s: 5.0,
        }
    }
}

impl SimOptions {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        issues.range_f64("durationS", self.duration_s, 1.0, MAX_DURATION_S);
        issues.range_f64("warmupS", self.warmup_s, 0.0, MAX_DURATION_S);
        issues.check(
            self.warmup_s < self.duration_s,
            "warmupS",
            "must be shorter than the duration",
        );
        issues.finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Healthy,
    Warning,
    Saturated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Latency {
    pub mean_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub offered: u32,
    pub completed: u32,
    /// Requests dropped because a queue was full or the timeout expired.
    pub errors: u32,
    pub throughput_rps: f64,
    pub error_rate: f64,
    pub latency: Latency,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NodeReport {
    pub id: String,
    pub label: String,
    pub kind: NodeKind,
    /// Parallel workers after scaling to the server profile.
    pub servers: u32,
    /// Requests that may be in the node at once (workers plus waiting room).
    pub capacity: u32,
    pub arrivals: u32,
    pub completed: u32,
    pub rejected: u32,
    pub timed_out: u32,
    /// Fraction of worker time spent busy, 0..=1.
    pub utilization: f64,
    pub mean_queue: f64,
    pub max_queue: u32,
    /// Time spent in the node (waiting plus service).
    pub latency: Latency,
    pub severity: Severity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Bottleneck {
    pub node_id: String,
    pub label: String,
    pub severity: Severity,
    pub reason: String,
}

/// Metrics for one second of simulated time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TimelinePoint {
    pub t_s: u32,
    pub offered_rps: f64,
    pub completed_rps: f64,
    pub error_rps: f64,
    pub p95_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SimReport {
    pub notice: String,
    pub seed: u32,
    pub duration_s: f64,
    pub warmup_s: f64,
    pub summary: Summary,
    pub nodes: Vec<NodeReport>,
    pub bottleneck: Option<Bottleneck>,
    pub timeline: Vec<TimelinePoint>,
    /// Modelling assumptions that affected this run.
    pub assumptions: Vec<String>,
}
