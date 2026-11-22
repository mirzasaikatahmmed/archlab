//! Suggests fixes: tries likely changes to the slowest part and ranks them against the targets.

use std::sync::atomic::AtomicBool;

use super::types::{Outcome, SuggestReport, Suggestion};
use super::{Progress, deltas, evaluate, parallel};
use crate::domain::project::Project;
use crate::domain::template;
use crate::domain::topology::NodeKind;
use crate::domain::whatif::{Change, ServerField, Slo, apply_changes};
use crate::runtime::RuntimeError;
use crate::sim::SimOptions;

const MAX_CANDIDATES: usize = 10;
const MAX_SUGGESTIONS: usize = 6;
const MAX_REPLICAS: u32 = 64;

struct Candidate {
    title: String,
    changes: Vec<Change>,
}

/// The node suggestions aim at: the reported bottleneck, else the busiest server.
fn target_node(baseline: &Outcome) -> Option<String> {
    if let Some(b) = &baseline.bottleneck {
        return Some(b.node_id.clone());
    }
    baseline
        .nodes
        .iter()
        .max_by(|a, b| a.utilization.total_cmp(&b.utilization))
        .map(|n| n.id.clone())
}

fn candidates(project: &Project, node_id: &str) -> Vec<Candidate> {
    let mut out = Vec::new();
    let Some(node) = project.topology.nodes.iter().find(|n| n.id == node_id) else {
        return out;
    };
    let label = &node.label;

    for factor in [2, 4] {
        let count = node.replicas.saturating_mul(factor);
        if count <= MAX_REPLICAS {
            out.push(Candidate {
                title: format!("Run {label} on {count} machines"),
                changes: vec![Change::Replicas {
                    node: node.id.clone(),
                    count,
                }],
            });
        }
    }

    if let Some(t) = node.template.as_ref().and_then(|r| template::find(&r.id)) {
        let sim = t.sim.as_ref();
        let keys = [
            sim.and_then(|s| s.concurrency_setting.as_deref()),
            sim.and_then(|s| s.queue_setting.as_deref()),
        ];
        for key in keys.into_iter().flatten() {
            let Some(setting) = t.settings.iter().find(|s| s.key == key) else {
                continue;
            };
            let current = node.settings.get(key).copied().unwrap_or(setting.default);
            let doubled = (current * 2.0).min(setting.max);
            if doubled > current {
                out.push(Candidate {
                    title: format!("Double {} on {label}", setting.label.to_lowercase()),
                    changes: vec![Change::NodeSetting {
                        node: node.id.clone(),
                        key: key.to_owned(),
                        value: doubled.round(),
                    }],
                });
            }
        }
    }

    for cache in project
        .topology
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Cache)
    {
        let Some(t) = cache.template.as_ref().and_then(|r| template::find(&r.id)) else {
            continue;
        };
        let Some(key) = t.sim.as_ref().and_then(|s| s.hit_ratio_setting.as_deref()) else {
            continue;
        };
        let current = cache
            .settings
            .get(key)
            .copied()
            .or_else(|| t.settings.iter().find(|s| s.key == key).map(|s| s.default))
            .unwrap_or(0.8);
        if current < 0.95 {
            out.push(Candidate {
                title: format!("Raise the hit ratio of {} to 95%", cache.label),
                changes: vec![Change::NodeSetting {
                    node: cache.id.clone(),
                    key: key.to_owned(),
                    value: 0.95,
                }],
            });
        }
    }

    let s = &project.server_profile;
    if s.cpu_ghz * 1.5 <= 10.0 {
        out.push(Candidate {
            title: "Use CPUs that are 50% faster".to_owned(),
            changes: vec![Change::Server {
                field: ServerField::CpuGhz,
                value: (s.cpu_ghz * 1.5 * 100.0).round() / 100.0,
            }],
        });
    }
    if s.cpu_cores.saturating_mul(2) <= 1024 {
        out.push(Candidate {
            title: "Double the CPU cores".to_owned(),
            changes: vec![Change::Server {
                field: ServerField::CpuCores,
                value: f64::from(s.cpu_cores * 2),
            }],
        });
    }
    if node.kind == NodeKind::Database && s.disk.iops.saturating_mul(4) <= 10_000_000 {
        out.push(Candidate {
            title: "Use disks with 4\u{d7} the IOPS".to_owned(),
            changes: vec![Change::Server {
                field: ServerField::DiskIops,
                value: f64::from(s.disk.iops * 4),
            }],
        });
    }
    out.truncate(MAX_CANDIDATES);
    out
}

/// Lower is better: missed targets dominate, then errors, then latency.
fn badness(o: &Outcome, slo: &Slo) -> f64 {
    let missed = super::misses(&o.summary, slo).len() as f64;
    missed * 1e9 + o.summary.error_rate * 1e6 + o.summary.latency.p95_ms
}

fn improves(baseline: &Outcome, candidate: &Outcome, slo: &Slo) -> bool {
    candidate.error.is_none() && badness(candidate, slo) < badness(baseline, slo)
}

pub fn run(
    project: &Project,
    options: &SimOptions,
    slo: &Slo,
    cancel: &AtomicBool,
    progress: Progress<'_>,
) -> Result<Option<SuggestReport>, RuntimeError> {
    let baseline = evaluate(project, options, slo);
    let target = target_node(&baseline);
    let node_id = target.clone().unwrap_or_default();

    let mut jobs = Vec::new();
    for c in candidates(project, &node_id) {
        if let Ok(changed) = apply_changes(project, &c.changes) {
            let described: Vec<String> = c.changes.iter().map(|ch| ch.describe(project)).collect();
            jobs.push((c, described, changed));
        }
    }
    let total = u32::try_from(jobs.len() + 1).unwrap_or(u32::MAX);
    progress(1, total);
    let report_progress = |done: u32, _: u32| progress(done + 1, total);
    let Some(outcomes) = parallel::run_all(&jobs, cancel, &report_progress, |(_, _, p)| {
        evaluate(p, options, slo)
    }) else {
        return Ok(None);
    };

    let mut suggestions: Vec<Suggestion> = jobs
        .into_iter()
        .zip(outcomes)
        .filter(|(_, outcome)| improves(&baseline, outcome, slo))
        .map(|((c, described, _), outcome)| Suggestion {
            vs_baseline: deltas(&baseline, &outcome),
            title: c.title,
            changes: described,
            change_defs: c.changes,
            outcome,
        })
        .collect();
    suggestions.sort_by(|a, b| badness(&a.outcome, slo).total_cmp(&badness(&b.outcome, slo)));
    suggestions.truncate(MAX_SUGGESTIONS);

    let target_label = target.and_then(|id| {
        project
            .topology
            .nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.label.clone())
    });
    Ok(Some(SuggestReport {
        slo: slo.clone(),
        baseline,
        target: target_label,
        suggestions,
    }))
}
