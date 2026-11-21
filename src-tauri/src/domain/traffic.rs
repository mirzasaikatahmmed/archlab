use serde::{Deserialize, Serialize};
use specta::Type;

use super::validation::{Issues, ValidationError};

const MAX_SECONDS: f64 = 86_400.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SpikePattern {
    None,
    /// One spike: traffic is multiplied from `at_s` for `duration_s`.
    Step {
        at_s: f64,
        multiplier: f64,
        duration_s: f64,
    },
    /// Recurring spike every `period_s`, lasting `duration_s`.
    Periodic {
        period_s: f64,
        multiplier: f64,
        duration_s: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TrafficProfile {
    pub concurrent_users: u32,
    pub requests_per_second: f64,
    /// Fraction of requests that are reads, 0..=1. Writes are `1 - read_ratio`.
    pub read_ratio: f64,
    pub spike: SpikePattern,
}

impl Default for TrafficProfile {
    fn default() -> Self {
        Self {
            concurrent_users: 1_000,
            requests_per_second: 200.0,
            read_ratio: 0.8,
            spike: SpikePattern::None,
        }
    }
}

impl TrafficProfile {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        issues.range_u32("concurrentUsers", self.concurrent_users, 1, 100_000_000);
        issues.range_f64(
            "requestsPerSecond",
            self.requests_per_second,
            0.001,
            10_000_000.0,
        );
        issues.range_f64("readRatio", self.read_ratio, 0.0, 1.0);
        match &self.spike {
            SpikePattern::None => {}
            SpikePattern::Step {
                at_s,
                multiplier,
                duration_s,
            } => {
                issues.range_f64("spike.atS", *at_s, 0.0, MAX_SECONDS);
                issues.range_f64("spike.multiplier", *multiplier, 1.0, 100.0);
                issues.range_f64("spike.durationS", *duration_s, 0.001, MAX_SECONDS);
            }
            SpikePattern::Periodic {
                period_s,
                multiplier,
                duration_s,
            } => {
                issues.range_f64("spike.periodS", *period_s, 0.001, MAX_SECONDS);
                issues.range_f64("spike.multiplier", *multiplier, 1.0, 100.0);
                issues.range_f64("spike.durationS", *duration_s, 0.001, MAX_SECONDS);
                issues.check(
                    duration_s < period_s,
                    "spike.durationS",
                    "must be shorter than the period",
                );
            }
        }
        issues.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_valid() {
        assert!(TrafficProfile::default().validate().is_ok());
    }

    #[test]
    fn rejects_out_of_range_values() {
        let p = TrafficProfile {
            concurrent_users: 0,
            requests_per_second: 0.0,
            read_ratio: 1.5,
            spike: SpikePattern::None,
        };
        let err = p.validate().expect_err("invalid");
        assert_eq!(err.issues.len(), 3);
    }

    #[test]
    fn periodic_spike_duration_must_fit_in_period() {
        let p = TrafficProfile {
            spike: SpikePattern::Periodic {
                period_s: 10.0,
                multiplier: 3.0,
                duration_s: 10.0,
            },
            ..TrafficProfile::default()
        };
        let err = p.validate().expect_err("invalid");
        assert_eq!(err.issues[0].field, "spike.durationS");
    }

    #[test]
    fn spike_serializes_with_kind_tag() {
        let json = serde_json::to_value(SpikePattern::Step {
            at_s: 1.0,
            multiplier: 2.0,
            duration_s: 3.0,
        })
        .expect("serializes");
        assert_eq!(json["kind"], "step");
        assert_eq!(json["durationS"], 3.0);
    }
}
