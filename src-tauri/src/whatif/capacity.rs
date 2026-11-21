//! Finds the highest traffic a design can take while still meeting its targets.
//!
//! The search doubles the traffic until the targets are missed, then bisects between the last
//! passing and first failing rate. Every probe is one seeded simulation at a traffic scale.

use std::sync::atomic::{AtomicBool, Ordering};

use super::types::{CapacityLimit, CapacityReport, Outcome, Probe};
use super::{Progress, evaluate, misses};
use crate::domain::project::Project;
use crate::domain::whatif::{Change, Slo, apply_changes};
use crate::runtime::RuntimeError;
use crate::sim::SimOptions;

/// Highest traffic scale the search tries, relative to the project's current rate.
pub const MAX_SCALE: f64 = 32.0;
/// Lowest scale tried when the design already misses its targets.
const MIN_SCALE: f64 = 0.02;
const BISECTIONS_UP: u32 = 8;
const BISECTIONS_DOWN: u32 = 10;
/// Upper estimate of runs, for the progress bar.
const EXPECTED_RUNS: u32 = 22;

struct Search<'a> {
    project: &'a Project,
    options: &'a SimOptions,
    slo: &'a Slo,
    cancel: &'a AtomicBool,
    progress: Progress<'a>,
    probes: Vec<Probe>,
}

impl Search<'_> {
    /// One run at `scale`; `None` if the search was cancelled first.
    fn probe(&mut self, scale: f64) -> Result<Option<Outcome>, RuntimeError> {
        if self.cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let changed = apply_changes(self.project, &[Change::TrafficScale { factor: scale }])?;
        let outcome = evaluate(&changed, self.options, self.slo);
        self.probes.push(Probe {
            rps: changed.traffic_profile.requests_per_second,
            p95_ms: outcome.summary.latency.p95_ms,
            error_rate: outcome.summary.error_rate,
            meets_slo: outcome.meets_slo,
            error: outcome.error.clone(),
        });
        let done = u32::try_from(self.probes.len()).unwrap_or(u32::MAX);
        (self.progress)(done.min(EXPECTED_RUNS), EXPECTED_RUNS);
        Ok(Some(outcome))
    }

    /// Narrows `[pass, fail]` to the highest passing scale; returns it with the failing outcome.
    fn bisect(
        &mut self,
        mut pass: f64,
        mut fail: f64,
        mut failing: Outcome,
        rounds: u32,
    ) -> Result<Option<(f64, Outcome)>, RuntimeError> {
        for _ in 0..rounds {
            let mid = (pass + fail) / 2.0;
            let Some(outcome) = self.probe(mid)? else {
                return Ok(None);
            };
            if outcome.meets_slo {
                pass = mid;
            } else {
                fail = mid;
                failing = outcome;
            }
        }
        Ok(Some((pass, failing)))
    }
}

fn limit_of(outcome: &Outcome, slo: &Slo) -> Option<CapacityLimit> {
    let b = outcome.bottleneck.as_ref()?;
    Some(CapacityLimit {
        node: b.label.clone(),
        reason: b.reason.clone(),
        missed: misses(&outcome.summary, slo),
    })
}

pub fn run(
    project: &Project,
    options: &SimOptions,
    slo: &Slo,
    cancel: &AtomicBool,
    progress: Progress<'_>,
) -> Result<Option<CapacityReport>, RuntimeError> {
    let current = project.traffic_profile.requests_per_second;
    let mut s = Search {
        project,
        options,
        slo,
        cancel,
        progress,
        probes: Vec::new(),
    };
    let Some(now) = s.probe(1.0)? else {
        return Ok(None);
    };
    let current_meets_slo = now.meets_slo;
    let limit_now = limit_of(&now, slo);

    // (highest passing scale, outcome just above it)
    let found: Option<(f64, Option<Outcome>)> = if now.meets_slo {
        let (mut pass, mut fail) = (1.0, None);
        let mut scale = 2.0;
        while scale <= MAX_SCALE {
            let Some(o) = s.probe(scale)? else {
                return Ok(None);
            };
            if o.meets_slo {
                pass = scale;
                scale *= 2.0;
            } else {
                fail = Some((scale, o));
                break;
            }
        }
        match fail {
            None => Some((pass, None)),
            Some((fail_scale, failing)) => {
                let Some((best, failing)) = s.bisect(pass, fail_scale, failing, BISECTIONS_UP)?
                else {
                    return Ok(None);
                };
                Some((best, Some(failing)))
            }
        }
    } else {
        let Some(floor) = s.probe(MIN_SCALE)? else {
            return Ok(None);
        };
        if floor.meets_slo {
            let Some((best, failing)) = s.bisect(MIN_SCALE, 1.0, now, BISECTIONS_DOWN)? else {
                return Ok(None);
            };
            Some((best, Some(failing)))
        } else {
            None
        }
    };

    let (max_rps, headroom, reached_search_limit, limit) = match found {
        Some((scale, failing)) => (
            Some(current * scale),
            Some(scale),
            failing.is_none(),
            failing.as_ref().and_then(|o| limit_of(o, slo)),
        ),
        None => (None, None, false, limit_now),
    };
    Ok(Some(CapacityReport {
        slo: slo.clone(),
        current_rps: current,
        current_meets_slo,
        max_rps,
        headroom,
        reached_search_limit,
        limit,
        probes: s.probes,
    }))
}
