//! Fault injection and the client's resilience policy.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::topology::{NodeKind, Topology};
use super::validation::{Issues, ValidationError};

pub const MAX_FAULTS: usize = 100;
pub const MAX_FAULT_DURATION_S: f64 = 3_600.0;
pub const SLOWDOWN_RANGE: (f64, f64) = (1.0, 100.0);
pub const LOST_WORKERS_RANGE: (f64, f64) = (0.05, 0.95);
pub const EXTRA_LATENCY_MS_RANGE: (f64, f64) = (0.0, 10_000.0);
pub const LOSS_RANGE: (f64, f64) = (0.0, 1.0);
pub const TIMEOUT_MS_RANGE: (f64, f64) = (50.0, 120_000.0);
pub const MAX_RETRIES_RANGE: (u32, u32) = (0, 5);
pub const BACKOFF_MS_RANGE: (f64, f64) = (0.0, 10_000.0);

/// How clients react to failures: how long they wait and whether they try again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClientPolicy {
    /// A request older than this when it is served, or when it completes, has failed.
    pub timeout_ms: f64,
    /// Extra attempts after the first one fails.
    pub max_retries: u32,
    /// Wait before each retry.
    pub backoff_ms: f64,
}

impl Default for ClientPolicy {
    fn default() -> Self {
        Self {
            timeout_ms: 10_000.0,
            max_retries: 0,
            backoff_ms: 100.0,
        }
    }
}

impl ClientPolicy {
    fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        let (lo, hi) = TIMEOUT_MS_RANGE;
        issues.range_f64("timeoutMs", self.timeout_ms, lo, hi);
        let (lo, hi) = MAX_RETRIES_RANGE;
        issues.range_u32("maxRetries", self.max_retries, lo, hi);
        let (lo, hi) = BACKOFF_MS_RANGE;
        issues.range_f64("backoffMs", self.backoff_ms, lo, hi);
        issues.finish()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FaultKind {
    /// The node refuses and loses everything until it restarts at the end of the fault.
    Crash,
    /// Service time on the node is multiplied by `factor`.
    Slowdown { factor: f64 },
    /// The node loses this share of its workers.
    LostWorkers { fraction: f64 },
    /// Every hop over the edge takes `extra_ms` longer.
    EdgeLatency { extra_ms: f64 },
    /// Each hop over the edge is lost with probability `loss`; 1 is a full partition.
    EdgeLoss { loss: f64 },
}

impl FaultKind {
    /// Whether the fault targets an edge rather than a node.
    pub fn targets_edge(&self) -> bool {
        matches!(self, Self::EdgeLatency { .. } | Self::EdgeLoss { .. })
    }

    /// Checks the severity of the fault against its allowed range.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        check_severity(&mut issues, "fault", self);
        issues.finish()
    }

    /// A short description of the fault at `place`, such as "Crash of App server".
    pub fn describe_at(&self, place: &str) -> String {
        match self {
            Self::Crash => format!("Crash of {place}"),
            Self::Slowdown { factor } => format!("{factor}\u{d7} slowdown of {place}"),
            Self::LostWorkers { fraction } => {
                format!("{:.0}% of workers lost on {place}", fraction * 100.0)
            }
            Self::EdgeLatency { extra_ms } => format!("+{extra_ms} ms latency on {place}"),
            Self::EdgeLoss { loss } => format!("{:.0}% packet loss on {place}", loss * 100.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Fault {
    pub id: String,
    /// Node id for node faults, edge id for edge faults.
    pub target: String,
    pub kind: FaultKind,
    pub start_s: f64,
    pub duration_s: f64,
}

impl Fault {
    pub fn end_s(&self) -> f64 {
        self.start_s + self.duration_s
    }

    /// A short description using the labels of the node or connection it targets.
    pub fn describe(&self, topology: &Topology) -> String {
        let label = |id: &str| {
            topology
                .nodes
                .iter()
                .find(|n| n.id == id)
                .map_or_else(|| id.to_owned(), |n| n.label.clone())
        };
        let place = if self.kind.targets_edge() {
            topology
                .edges
                .iter()
                .find(|e| e.id == self.target)
                .map_or_else(
                    || self.target.clone(),
                    |e| format!("{} \u{2192} {}", label(&e.source), label(&e.target)),
                )
        } else {
            label(&self.target)
        };
        self.kind.describe_at(&place)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChaosPlan {
    pub faults: Vec<Fault>,
    pub policy: ClientPolicy,
}

impl ChaosPlan {
    pub fn validate(&self, topology: &Topology) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        issues.nested("policy", self.policy.validate());
        issues.check(
            self.faults.len() <= MAX_FAULTS,
            "faults",
            &format!("at most {MAX_FAULTS} faults"),
        );
        let mut ids = BTreeSet::new();
        for (i, fault) in self.faults.iter().enumerate() {
            let at = format!("faults.{i}");
            issues.check(
                !fault.id.trim().is_empty() && ids.insert(fault.id.as_str()),
                &format!("{at}.id"),
                "must be unique and not empty",
            );
            issues.range_f64(&format!("{at}.startS"), fault.start_s, 0.0, 86_400.0);
            issues.range_f64(
                &format!("{at}.durationS"),
                fault.duration_s,
                0.001,
                MAX_FAULT_DURATION_S,
            );
            check_target(&mut issues, &at, fault, topology);
            check_severity(&mut issues, &at, &fault.kind);
        }
        issues.finish()
    }
}

fn check_target(issues: &mut Issues, at: &str, fault: &Fault, topology: &Topology) {
    let field = format!("{at}.target");
    if fault.kind.targets_edge() {
        issues.check(
            topology.edges.iter().any(|e| e.id == fault.target),
            &field,
            "must be an edge of the topology",
        );
    } else {
        let node = topology.nodes.iter().find(|n| n.id == fault.target);
        match node {
            None => issues.push(field, "must be a node of the topology"),
            Some(n) => issues.check(
                n.kind != NodeKind::Client,
                &field,
                "clients cannot be faulted; fault the edge instead",
            ),
        }
    }
}

fn check_severity(issues: &mut Issues, at: &str, kind: &FaultKind) {
    match kind {
        FaultKind::Crash => {}
        FaultKind::Slowdown { factor } => {
            let (lo, hi) = SLOWDOWN_RANGE;
            issues.range_f64(&format!("{at}.kind.factor"), *factor, lo, hi);
        }
        FaultKind::LostWorkers { fraction } => {
            let (lo, hi) = LOST_WORKERS_RANGE;
            issues.range_f64(&format!("{at}.kind.fraction"), *fraction, lo, hi);
        }
        FaultKind::EdgeLatency { extra_ms } => {
            let (lo, hi) = EXTRA_LATENCY_MS_RANGE;
            issues.range_f64(&format!("{at}.kind.extraMs"), *extra_ms, lo, hi);
        }
        FaultKind::EdgeLoss { loss } => {
            let (lo, hi) = LOSS_RANGE;
            issues.range_f64(&format!("{at}.kind.loss"), *loss, lo, hi);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fault(id: &str, target: &str, kind: FaultKind) -> Fault {
        Fault {
            id: id.to_owned(),
            target: target.to_owned(),
            kind,
            start_s: 10.0,
            duration_s: 5.0,
        }
    }

    fn plan(faults: Vec<Fault>) -> ChaosPlan {
        ChaosPlan {
            faults,
            policy: ClientPolicy::default(),
        }
    }

    #[test]
    fn default_plan_is_valid() {
        assert!(ChaosPlan::default().validate(&Topology::starter()).is_ok());
    }

    #[test]
    fn accepts_faults_on_real_nodes_and_edges() {
        let p = plan(vec![
            fault("a", "db", FaultKind::Crash),
            fault("b", "app", FaultKind::Slowdown { factor: 3.0 }),
            fault("c", "e-app-db", FaultKind::EdgeLoss { loss: 0.2 }),
            fault(
                "d",
                "e-client-app",
                FaultKind::EdgeLatency { extra_ms: 50.0 },
            ),
            fault("e", "app", FaultKind::LostWorkers { fraction: 0.5 }),
        ]);
        assert!(p.validate(&Topology::starter()).is_ok());
    }

    #[test]
    fn rejects_unknown_targets_clients_and_wrong_target_type() {
        let p = plan(vec![
            fault("a", "nope", FaultKind::Crash),
            fault("b", "client", FaultKind::Crash),
            fault("c", "app", FaultKind::EdgeLoss { loss: 0.5 }),
            fault("d", "e-app-db", FaultKind::Crash),
        ]);
        let err = p.validate(&Topology::starter()).expect_err("invalid");
        let fields: Vec<_> = err.issues.iter().map(|i| i.field.as_str()).collect();
        assert_eq!(
            fields,
            [
                "faults.0.target",
                "faults.1.target",
                "faults.2.target",
                "faults.3.target"
            ]
        );
    }

    #[test]
    fn rejects_out_of_range_values_and_duplicate_ids() {
        let mut bad = fault("a", "db", FaultKind::Slowdown { factor: 0.5 });
        bad.duration_s = 0.0;
        bad.start_s = -1.0;
        let p = plan(vec![
            bad,
            fault("a", "db", FaultKind::LostWorkers { fraction: 1.0 }),
            fault("c", "e-app-db", FaultKind::EdgeLoss { loss: 1.5 }),
        ]);
        let err = p.validate(&Topology::starter()).expect_err("invalid");
        let fields: Vec<_> = err.issues.iter().map(|i| i.field.as_str()).collect();
        assert!(fields.contains(&"faults.0.startS"));
        assert!(fields.contains(&"faults.0.durationS"));
        assert!(fields.contains(&"faults.0.kind.factor"));
        assert!(fields.contains(&"faults.1.id"));
        assert!(fields.contains(&"faults.1.kind.fraction"));
        assert!(fields.contains(&"faults.2.kind.loss"));
    }

    #[test]
    fn policy_ranges_are_checked() {
        let mut p = ChaosPlan::default();
        p.policy.timeout_ms = 1.0;
        p.policy.max_retries = 9;
        p.policy.backoff_ms = -5.0;
        let err = p.validate(&Topology::starter()).expect_err("invalid");
        assert_eq!(err.issues.len(), 3);
        assert!(err.issues[0].field.starts_with("policy."));
    }

    #[test]
    fn faults_round_trip_through_json() {
        let p = plan(vec![fault("a", "app", FaultKind::Slowdown { factor: 2.5 })]);
        let json = serde_json::to_string(&p).expect("serialize");
        assert!(json.contains(r#""kind":"slowdown""#));
        assert_eq!(serde_json::from_str::<ChaosPlan>(&json).expect("parse"), p);
    }
}
