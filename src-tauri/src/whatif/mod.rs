//! What-if analysis: the same seeded simulation, run for many variations of a project.

mod capacity;
mod parallel;
mod suggest;
mod types;

use std::sync::atomic::AtomicBool;

use crate::domain::project::Project;
use crate::domain::validation::{Issues, ValidationError};
use crate::domain::whatif::{Change, Slo, apply_changes};
use crate::runtime::RuntimeError;
use crate::sim::SimOptions;
use crate::sim::report::{SimReport, Summary};

pub use types::*;

pub const MAX_SWEEP_POINTS: u32 = 24;
const MIN_SWEEP_POINTS: u32 = 2;

/// Progress callback: runs finished, runs expected.
pub type Progress<'a> = &'a (dyn Fn(u32, u32) + Sync);

fn empty_summary() -> Summary {
    use crate::sim::report::Latency;
    let zero = Latency {
        mean_ms: 0.0,
        p50_ms: 0.0,
        p95_ms: 0.0,
        p99_ms: 0.0,
    };
    Summary {
        offered: 0,
        completed: 0,
        errors: 0,
        retries: 0,
        throughput_rps: 0.0,
        error_rate: 0.0,
        latency: zero,
    }
}

pub(crate) fn misses(summary: &Summary, slo: &Slo) -> Vec<String> {
    let mut out = Vec::new();
    if summary.completed == 0 {
        out.push("no requests completed".to_owned());
    }
    if summary.latency.p95_ms > slo.p95_ms {
        out.push(format!("p95 above {} ms", slo.p95_ms));
    }
    if summary.error_rate > slo.max_error_rate {
        out.push(format!("errors above {:.1}%", slo.max_error_rate * 100.0));
    }
    out
}

fn outcome_of(report: &SimReport, slo: &Slo) -> Outcome {
    Outcome {
        meets_slo: misses(&report.summary, slo).is_empty(),
        summary: report.summary.clone(),
        bottleneck: report.bottleneck.clone(),
        nodes: report
            .nodes
            .iter()
            .map(|n| NodeBrief {
                id: n.id.clone(),
                label: n.label.clone(),
                utilization: n.utilization,
                severity: n.severity,
            })
            .collect(),
        error: None,
    }
}

/// Runs one simulation; a run that cannot finish becomes a failed outcome, not an error.
pub(crate) fn evaluate(project: &Project, options: &SimOptions, slo: &Slo) -> Outcome {
    let mut p = project.clone();
    p.whatif.variants.clear();
    match crate::sim::run(&p, options) {
        Ok(report) => outcome_of(&report, slo),
        Err(err) => Outcome {
            summary: empty_summary(),
            bottleneck: None,
            meets_slo: false,
            nodes: Vec::new(),
            error: Some(err.to_string()),
        },
    }
}

fn ratio_pct(base: f64, value: f64) -> Option<f64> {
    (base.abs() > 1e-9).then(|| (value - base) / base * 100.0)
}

pub(crate) fn deltas(baseline: &Outcome, variant: &Outcome) -> Deltas {
    let (b, v) = (&baseline.summary, &variant.summary);
    Deltas {
        throughput_pct: ratio_pct(b.throughput_rps, v.throughput_rps),
        p95_pct: ratio_pct(b.latency.p95_ms, v.latency.p95_ms),
        error_rate_points: (v.error_rate - b.error_rate) * 100.0,
    }
}

fn sweep_label(param: &SweepParam, project: &Project) -> String {
    let node = |id: &str| {
        project
            .topology
            .nodes
            .iter()
            .find(|n| n.id == id)
            .map_or_else(|| id.to_owned(), |n| n.label.clone())
    };
    match param {
        SweepParam::TrafficScale => "Traffic (\u{d7} current)".to_owned(),
        SweepParam::Server { field } => field.label().to_owned(),
        SweepParam::NodeSetting { node: n, key } => format!("{} \u{2013} {key}", node(n)),
        SweepParam::Replicas { node: n } => format!("{} replicas", node(n)),
    }
}

fn change_for(param: &SweepParam, value: f64) -> Change {
    match param {
        SweepParam::TrafficScale => Change::TrafficScale { factor: value },
        SweepParam::Server { field } => Change::Server {
            field: *field,
            value,
        },
        SweepParam::NodeSetting { node, key } => Change::NodeSetting {
            node: node.clone(),
            key: key.clone(),
            value,
        },
        SweepParam::Replicas { node } => Change::Replicas {
            node: node.clone(),
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            count: value.round().clamp(0.0, f64::from(u32::MAX)) as u32,
        },
    }
}

fn is_integer_param(param: &SweepParam) -> bool {
    use crate::domain::whatif::ServerField as F;
    matches!(
        param,
        SweepParam::Replicas { .. }
            | SweepParam::Server {
                field: F::CpuCores | F::DiskIops | F::FdLimit
            }
    )
}

/// The values a sweep tries: geometric for traffic, linear otherwise, whole numbers where needed.
pub fn sweep_values(spec: &SweepSpec) -> Vec<f64> {
    let n = spec.points.max(2);
    let geometric =
        matches!(spec.param, SweepParam::TrafficScale) && spec.from > 0.0 && spec.to > 0.0;
    let mut out: Vec<f64> = (0..n)
        .map(|i| {
            let t = f64::from(i) / f64::from(n - 1);
            let v = if geometric {
                spec.from * (spec.to / spec.from).powf(t)
            } else {
                spec.from + (spec.to - spec.from) * t
            };
            if is_integer_param(&spec.param) {
                v.round()
            } else {
                v
            }
        })
        .collect();
    out.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    out
}

/// Checks a request against the project before any run starts.
pub fn validate(
    project: &Project,
    options: &SimOptions,
    request: &WhatIfRequest,
) -> Result<(), ValidationError> {
    let mut issues = Issues::default();
    issues.nested("", project.validate());
    issues.nested("", options.validate());
    if let WhatIfRequest::Sweep { spec } = request {
        issues.check(
            (MIN_SWEEP_POINTS..=MAX_SWEEP_POINTS).contains(&spec.points),
            "spec.points",
            &format!("must be between {MIN_SWEEP_POINTS} and {MAX_SWEEP_POINTS}"),
        );
        issues.check(
            spec.from.is_finite() && spec.to.is_finite() && (spec.from - spec.to).abs() > 1e-12,
            "spec.to",
            "must differ from the start value",
        );
        if spec.from.is_finite() && spec.to.is_finite() && spec.points <= MAX_SWEEP_POINTS {
            for (i, v) in sweep_values(spec).into_iter().enumerate() {
                if let Err(err) = apply_changes(project, &[change_for(&spec.param, v)]) {
                    issues.nested(&format!("spec.values.{i}"), Err(err));
                    break;
                }
            }
        }
    }
    issues.finish()
}

/// Runs a what-if request. Returns `None` if cancelled.
pub fn run(
    project: &Project,
    options: &SimOptions,
    request: &WhatIfRequest,
    cancel: &AtomicBool,
    progress: Progress<'_>,
) -> Result<Option<WhatIfResult>, RuntimeError> {
    validate(project, options, request)?;
    let slo = project.whatif.slo.clone();
    match request {
        WhatIfRequest::Compare => compare(project, options, &slo, cancel, progress)
            .map(|r| r.map(|report| WhatIfResult::Compare { report })),
        WhatIfRequest::Sweep { spec } => sweep(project, options, &slo, spec, cancel, progress)
            .map(|r| r.map(|report| WhatIfResult::Sweep { report })),
        WhatIfRequest::Capacity => capacity::run(project, options, &slo, cancel, progress)
            .map(|r| r.map(|report| WhatIfResult::Capacity { report })),
        WhatIfRequest::Suggest => suggest::run(project, options, &slo, cancel, progress)
            .map(|r| r.map(|report| WhatIfResult::Suggest { report })),
    }
}

fn compare(
    project: &Project,
    options: &SimOptions,
    slo: &Slo,
    cancel: &AtomicBool,
    progress: Progress<'_>,
) -> Result<Option<CompareReport>, RuntimeError> {
    let mut jobs: Vec<(String, String, Vec<String>, Project)> = vec![(
        "baseline".to_owned(),
        "Baseline".to_owned(),
        Vec::new(),
        project.clone(),
    )];
    for v in &project.whatif.variants {
        let changed = apply_changes(project, &v.changes)?;
        let described = v.changes.iter().map(|c| c.describe(project)).collect();
        jobs.push((v.id.clone(), v.name.trim().to_owned(), described, changed));
    }
    let Some(outcomes) = parallel::run_all(&jobs, cancel, progress, |(_, _, _, p)| {
        evaluate(p, options, slo)
    }) else {
        return Ok(None);
    };
    let mut results = jobs
        .into_iter()
        .zip(outcomes)
        .map(|((id, name, changes, _), outcome)| VariantResult {
            id,
            name,
            changes,
            outcome,
            vs_baseline: None,
        });
    let baseline = results
        .next()
        .ok_or_else(|| RuntimeError::Failed("no baseline".into()))?;
    let variants = results
        .map(|mut v| {
            v.vs_baseline = Some(deltas(&baseline.outcome, &v.outcome));
            v
        })
        .collect();
    Ok(Some(CompareReport {
        slo: slo.clone(),
        baseline,
        variants,
    }))
}

fn sweep(
    project: &Project,
    options: &SimOptions,
    slo: &Slo,
    spec: &SweepSpec,
    cancel: &AtomicBool,
    progress: Progress<'_>,
) -> Result<Option<SweepReport>, RuntimeError> {
    let values = sweep_values(spec);
    let mut jobs = Vec::with_capacity(values.len());
    for &v in &values {
        jobs.push((v, apply_changes(project, &[change_for(&spec.param, v)])?));
    }
    let Some(outcomes) =
        parallel::run_all(&jobs, cancel, progress, |(_, p)| evaluate(p, options, slo))
    else {
        return Ok(None);
    };
    let points: Vec<SweepPoint> = values
        .iter()
        .zip(outcomes)
        .map(|(&value, outcome)| SweepPoint { value, outcome })
        .collect();
    let first_miss = points
        .iter()
        .find(|p| !p.outcome.meets_slo)
        .map(|p| p.value);
    Ok(Some(SweepReport {
        slo: slo.clone(),
        label: sweep_label(&spec.param, project),
        points,
        first_miss,
    }))
}

#[cfg(test)]
mod tests;
