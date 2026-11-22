//! Results of a real run and its comparison with the simulation.

use std::sync::atomic::Ordering;
use std::sync::{Arc, PoisonError};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::loadgen::Collector;
use super::node::Cluster;
use crate::domain::topology::NodeKind;
use crate::sim::engine::{latency, percentile_sorted, ratio};
use crate::sim::report::{NodeReport, SimOptions, SimReport, Summary, TimelinePoint, severity_for};

pub const REAL_NOTICE: &str = "Measured on this computer with real sockets and real CPU work. \
Results vary from run to run and depend on what else the computer is doing.";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RealReport {
    pub notice: String,
    pub duration_s: f64,
    pub warmup_s: f64,
    pub host_cores: u32,
    pub summary: Summary,
    pub nodes: Vec<NodeReport>,
    pub timeline: Vec<TimelinePoint>,
    /// Caveats that affect how far these numbers can be trusted.
    pub notes: Vec<String>,
}

/// One metric measured both ways.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Delta {
    pub metric: String,
    pub simulated: f64,
    pub real: f64,
    /// `(real - simulated) / simulated`, absent when the simulated value is zero.
    pub difference_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RealComparison {
    pub real: RealReport,
    pub simulated: SimReport,
    pub deltas: Vec<Delta>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RealEvent {
    Progress {
        elapsed_s: f64,
        total_s: f64,
        offered: u32,
        completed: u32,
        errors: u32,
    },
    Finished {
        comparison: Box<RealComparison>,
    },
    Cancelled,
    Failed {
        message: String,
    },
}

fn delta(metric: &str, simulated: f64, real: f64) -> Delta {
    Delta {
        metric: metric.to_owned(),
        simulated,
        real,
        difference_pct: (simulated.abs() > 1e-9).then(|| (real - simulated) / simulated * 100.0),
    }
}

pub fn deltas(simulated: &SimReport, real: &RealReport) -> Vec<Delta> {
    let (s, r) = (&simulated.summary, &real.summary);
    let mut out = vec![
        delta("Throughput (req/s)", s.throughput_rps, r.throughput_rps),
        delta("Error rate", s.error_rate, r.error_rate),
        delta("Latency p50 (ms)", s.latency.p50_ms, r.latency.p50_ms),
        delta("Latency p95 (ms)", s.latency.p95_ms, r.latency.p95_ms),
        delta("Latency p99 (ms)", s.latency.p99_ms, r.latency.p99_ms),
    ];
    for node in &real.nodes {
        if let Some(sim) = simulated.nodes.iter().find(|n| n.id == node.id) {
            out.push(delta(
                &format!("{} utilization", node.label),
                sim.utilization,
                node.utilization,
            ));
        }
    }
    out
}

pub fn build(
    cluster: &Arc<Cluster>,
    collector: &Collector,
    options: &SimOptions,
    host_cores: u32,
    notes: Vec<String>,
) -> RealReport {
    let window_s = options.duration_s - options.warmup_s;
    let window_us = window_s * 1e6;

    let mut nodes = Vec::new();
    for (spec, runtime) in cluster.model.nodes.iter().zip(&cluster.nodes) {
        if spec.kind == NodeKind::Client {
            continue;
        }
        let stats = &runtime.stats;
        let mut samples = std::mem::take(
            &mut *stats
                .latencies
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        #[allow(clippy::cast_precision_loss)]
        let utilization = (stats.busy_us.load(Ordering::Relaxed) as f64
            / (spec.servers as f64 * window_us))
            .clamp(0.0, 1.0);
        let rejected = stats.rejected.load(Ordering::Relaxed);
        let timed_out = stats.timed_out.load(Ordering::Relaxed);
        #[allow(clippy::cast_precision_loss)]
        let samples_taken = stats.queue_samples.load(Ordering::Relaxed).max(1) as f64;
        #[allow(clippy::cast_precision_loss)]
        let mean_queue = stats.queue_sum.load(Ordering::Relaxed) as f64 / samples_taken;
        nodes.push(NodeReport {
            id: spec.id.clone(),
            label: spec.label.clone(),
            kind: spec.kind,
            servers: u32::try_from(spec.servers).unwrap_or(u32::MAX),
            capacity: u32::try_from(spec.capacity).unwrap_or(u32::MAX),
            arrivals: stats.arrivals.load(Ordering::Relaxed),
            completed: stats.completed.load(Ordering::Relaxed),
            rejected,
            timed_out,
            faulted: 0,
            utilization,
            mean_queue,
            max_queue: u32::try_from(stats.max_queue.load(Ordering::Relaxed)).unwrap_or(u32::MAX),
            latency: latency(&mut samples),
            severity: severity_for(utilization, rejected + timed_out),
            backlog: None,
        });
    }

    let offered = collector.offered.load(Ordering::Relaxed);
    let completed = collector.completed.load(Ordering::Relaxed);
    let errors = collector.errors.load(Ordering::Relaxed);
    let mut all = std::mem::take(
        &mut *collector
            .latencies
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
    );
    let summary = Summary {
        offered,
        completed,
        errors,
        retries: 0,
        throughput_rps: f64::from(completed) / window_s,
        error_rate: ratio(errors, offered),
        latency: latency(&mut all),
    };

    let mut buckets = collector
        .buckets
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let seconds = options.duration_s.ceil() as usize;
    let timeline = buckets
        .iter_mut()
        .take(seconds)
        .enumerate()
        .map(|(i, b)| {
            #[allow(clippy::cast_precision_loss)]
            let width = (options.duration_s - i as f64).min(1.0);
            b.latencies.sort_unstable();
            TimelinePoint {
                t_s: u32::try_from(i).unwrap_or(u32::MAX),
                offered_rps: f64::from(b.offered) / width,
                completed_rps: f64::from(b.completed) / width,
                error_rps: f64::from(b.errors) / width,
                p95_ms: percentile_sorted(&b.latencies, 0.95),
            }
        })
        .collect();

    RealReport {
        notice: REAL_NOTICE.to_owned(),
        duration_s: options.duration_s,
        warmup_s: options.warmup_s,
        host_cores,
        summary,
        nodes,
        timeline,
        notes,
    }
}
