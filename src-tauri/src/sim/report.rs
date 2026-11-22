//! Simulation inputs and results exposed to the UI.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::control::ControlRecord;
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

/// Status of a node from its utilization and the number of requests it dropped.
pub fn severity_for(utilization: f64, dropped: u32) -> Severity {
    if utilization >= 0.9 || dropped > 0 {
        Severity::Saturated
    } else if utilization >= 0.7 {
        Severity::Warning
    } else {
        Severity::Healthy
    }
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
    /// Extra attempts sent after a failure (see the client policy).
    pub retries: u32,
    pub throughput_rps: f64,
    pub error_rate: f64,
    pub latency: Latency,
}

/// What a message queue did: how much it took in, how much was worked off, and how long
/// messages waited. Messages still queued at the end mean the workers cannot keep up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Backlog {
    /// Messages accepted from producers in the measured window.
    pub enqueued: u32,
    /// Messages that workers finished.
    pub processed: u32,
    /// Messages that failed in a worker (rejected, timed out or lost to a crash).
    pub failed: u32,
    /// Messages still waiting when the run ended.
    pub left: u32,
    /// Time from being queued to a worker picking the message up.
    pub lag: Latency,
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
    /// Requests lost to an injected fault: a crash on this node or packet loss on a link into it.
    pub faulted: u32,
    /// Fraction of worker time spent busy, 0..=1.
    pub utilization: f64,
    pub mean_queue: f64,
    pub max_queue: u32,
    /// Time spent in the node (waiting plus service).
    pub latency: Latency,
    pub severity: Severity,
    /// Messages held, for a queue that has workers after it.
    pub backlog: Option<Backlog>,
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

/// How the system behaved over a stretch of whole seconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WindowStats {
    pub error_rate: f64,
    pub p95_ms: f64,
    pub throughput_rps: f64,
}

/// Effect of one injected fault, compared with equally long windows before and after it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FaultImpact {
    pub fault_id: String,
    pub label: String,
    pub start_s: f64,
    pub end_s: f64,
    pub before: WindowStats,
    pub during: WindowStats,
    pub after: WindowStats,
    /// Seconds after the fault ended until errors and p95 returned to the pre-fault level;
    /// absent if the run ended first or the system never recovered.
    pub recovery_s: Option<f64>,
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
    pub faults: Vec<FaultImpact>,
    /// Commands applied during a live run, in order; empty for a run nobody steered.
    pub controls: Vec<ControlRecord>,
    /// Modelling assumptions that affected this run.
    pub assumptions: Vec<String>,
}

/// Live view of one node, taken at the end of a simulated second.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LiveNodeTick {
    pub id: String,
    /// Busy share of worker time during the last second, 0..=1.
    pub utilization: f64,
    /// Requests waiting right now.
    pub queue: u32,
    /// Workers busy right now.
    pub busy: u32,
    /// Requests dropped since the run started.
    pub dropped: u32,
}

/// Pushed to the UI once per simulated second during a live run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LiveTick {
    pub point: TimelinePoint,
    pub nodes: Vec<LiveNodeTick>,
    /// Share of the run completed, 0..=1.
    pub progress: f64,
    /// Ids of the faults in effect right now.
    pub active_faults: Vec<String>,
}
