//! Changes a person can make to a live run while it plays.
//!
//! A command takes effect at the current whole simulated second, between two ticks, and is
//! logged in the report. A run with the same seed and the same logged commands is reproducible
//! (see `engine::run_scripted`).

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::domain::chaos::{FaultKind, MAX_FAULT_DURATION_S};
use crate::domain::validation::{Issues, ValidationError};

/// Range of the live traffic multiplier (1 = the profile's own rate).
pub const TRAFFIC_RANGE: (f64, f64) = (0.1, 10.0);
/// Most commands a replay script may hold.
pub const MAX_SCRIPT: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ControlCommand {
    /// Multiplies the traffic profile's rate (spikes included) from now on.
    SetTraffic { multiplier: f64 },
    /// Starts a fault on a server node now. Only node faults can be injected live.
    InjectFault {
        target: String,
        fault: FaultKind,
        duration_s: f64,
    },
    /// Ends a fault that is in effect, whether it came from the project or was injected.
    EndFault { fault_id: String },
}

impl ControlCommand {
    /// Checks everything that does not depend on the running simulation.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        match self {
            Self::SetTraffic { multiplier } => {
                let (lo, hi) = TRAFFIC_RANGE;
                issues.range_f64("multiplier", *multiplier, lo, hi);
            }
            Self::InjectFault {
                target,
                fault,
                duration_s,
            } => {
                issues.check(!target.trim().is_empty(), "target", "must not be empty");
                issues.check(
                    !fault.targets_edge(),
                    "fault",
                    "live faults target a node, not a connection",
                );
                issues.nested("", fault.validate());
                issues.range_f64("durationS", *duration_s, 0.001, MAX_FAULT_DURATION_S);
            }
            Self::EndFault { fault_id } => {
                issues.check(!fault_id.trim().is_empty(), "faultId", "must not be empty");
            }
        }
        issues.finish()
    }
}

/// One applied command, as logged in the report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ControlRecord {
    /// Whole simulated second at which it took effect.
    pub t_s: u32,
    pub command: ControlCommand,
    /// Plain-language description, such as "Traffic ×3".
    pub summary: String,
    /// The fault it started (`live-1`, `live-2`, ...) or ended.
    pub fault_id: Option<String>,
}
