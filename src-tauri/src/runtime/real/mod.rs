//! Real mode: the topology runs as tokio tasks on localhost sockets with synthetic work,
//! so the simulation's predictions can be compared with measurements from this machine.
//!
//! Every node shares this computer's cores, so a real run does not reproduce a topology
//! spread over many machines. It is a check of the model, not a replacement for it.

mod loadgen;
mod node;
mod protocol;
mod report;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::net::TcpListener;

use super::RuntimeError;
use super::calibrate::cpu_steps_per_second;
use crate::domain::project::Project;
use crate::domain::topology::NodeKind;
use crate::domain::validation::{Issues, ValidationError};
use crate::sim::SimOptions;
use crate::sim::model::Model;
use loadgen::{Collector, Params, generate, sample_queues};
use node::{Cluster, Link, serve_listener};

pub use report::{Delta, RealComparison, RealEvent, RealReport};

pub const MAX_DURATION_S: f64 = 60.0;
pub const MAX_RPS: f64 = 5_000.0;
pub const MAX_NODES: usize = 20;
/// Workers per node are limited to this many per host core; I/O waits cost no core.
const WORKERS_PER_CORE: usize = 4;

pub fn validate(project: &Project, options: &SimOptions) -> Result<(), ValidationError> {
    let mut issues = Issues::default();
    issues.nested("", project.validate());
    issues.nested("", options.validate());
    issues.check(
        options.duration_s <= MAX_DURATION_S,
        "durationS",
        &format!("real runs are limited to {MAX_DURATION_S} seconds"),
    );
    issues.check(
        project.traffic_profile.requests_per_second * peak(project) <= MAX_RPS,
        "trafficProfile.requestsPerSecond",
        &format!("real runs are limited to {MAX_RPS} requests per second, spikes included"),
    );
    let servers = project
        .topology
        .nodes
        .iter()
        .filter(|n| n.kind != NodeKind::Client)
        .count();
    issues.check(
        servers <= MAX_NODES,
        "topology.nodes",
        &format!("real runs are limited to {MAX_NODES} nodes"),
    );
    if let Ok(model) = crate::sim::model::Model::compile(project) {
        let unsupported = model
            .nodes
            .iter()
            .any(|n| n.async_queue || n.rate_limit_rps > 0.0 || n.kind == NodeKind::Cdn);
        issues.check(
            !unsupported,
            "topology.nodes",
            "real runs do not support message queues with workers, CDN nodes or gateway rate limits; use the simulation",
        );
    }
    issues.finish()
}

fn peak(project: &Project) -> f64 {
    use crate::domain::traffic::SpikePattern;
    match project.traffic_profile.spike {
        SpikePattern::None => 1.0,
        SpikePattern::Step { multiplier, .. } | SpikePattern::Periodic { multiplier, .. } => {
            multiplier.max(1.0)
        }
    }
}

/// Runs the project for real and compares it with the simulation. Returns `None` if cancelled.
pub fn run(
    project: &Project,
    options: &SimOptions,
    cancel: &AtomicBool,
    sink: &Arc<dyn Fn(RealEvent) -> bool + Send + Sync>,
) -> Result<Option<RealComparison>, RuntimeError> {
    validate(project, options)?;
    let mut model = Model::compile(project)?;
    let simulated = crate::sim::run(project, options)?;

    let cores = std::thread::available_parallelism().map_or(1, usize::from);
    let mut notes = vec![
        format!(
            "All nodes share this computer's {cores} cores, so this is not a multi-machine setup."
        ),
        "Waits shorter than about a millisecond are rounded by the operating system's timers."
            .to_owned(),
    ];
    if cfg!(debug_assertions) {
        notes.push(
            "This is a debug build, which burns CPU far slower than a release build.".to_owned(),
        );
    }
    if !project.chaos.faults.is_empty() || project.chaos.policy.max_retries > 0 {
        notes.push("Injected faults and retries are not applied in real runs.".to_owned());
    }
    let cap = cores * WORKERS_PER_CORE;
    for n in &mut model.nodes {
        if n.servers > cap {
            n.servers = cap;
            n.capacity = n.capacity.max(cap);
            notes.push(format!(
                "{} was limited to {cap} workers on this computer.",
                n.label
            ));
        }
    }

    // How much CPU work fits in a millisecond here, so each node burns about its service time.
    let steps_per_s = cpu_steps_per_second(Duration::from_millis(150), 1, cancel);
    if cancel.load(Ordering::Relaxed) {
        return Ok(None);
    }
    let steps_per_ms = steps_per_s / 1_000.0;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(cores.clamp(2, 8))
        .enable_all()
        .build()
        .map_err(|e| RuntimeError::Failed(format!("could not start the runtime: {e}")))?;
    let duration = Duration::from_secs_f64(options.duration_s);
    let warmup = Duration::from_secs_f64(options.warmup_s);
    let timeout = Duration::from_micros(model.timeout_us);
    let cluster = Arc::new(Cluster::new(
        model,
        u64::from(options.seed),
        warmup,
        steps_per_ms,
    ));
    let collector = Arc::new(Collector::new(duration, warmup));

    let outcome: Result<bool, RuntimeError> = runtime.block_on(async {
        let mut ports = Vec::with_capacity(cluster.model.nodes.len());
        let mut listeners = Vec::new();
        for (i, spec) in cluster.model.nodes.iter().enumerate() {
            if spec.kind == NodeKind::Client {
                ports.push(0);
                continue;
            }
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .map_err(|e| RuntimeError::Failed(format!("could not open a local port: {e}")))?;
            ports.push(
                listener
                    .local_addr()
                    .map_err(|e| RuntimeError::Failed(e.to_string()))?
                    .port(),
            );
            listeners.push((i, listener));
        }
        for (i, listener) in listeners {
            tokio::spawn(serve_listener(Arc::clone(&cluster), i, listener));
        }
        let mut links = std::collections::HashMap::new();
        for &(from, to) in &cluster.model.edges {
            if ports[to] != 0 {
                let link = Link::connect(ports[to])
                    .await
                    .map_err(|e| RuntimeError::Failed(format!("could not connect nodes: {e}")))?;
                links.insert((from, to), link);
            }
        }
        cluster.set_links(links);

        let sampler = tokio::spawn(sample_queues(Arc::clone(&cluster)));
        let progress = tokio::spawn(report_progress(
            Arc::clone(&cluster),
            Arc::clone(&collector),
            Arc::clone(sink),
            options.duration_s,
        ));
        let params = Params {
            seed: u64::from(options.seed),
            timeout,
        };
        generate(&cluster, &collector, &params, cancel).await;
        sampler.abort();
        progress.abort();
        Ok(!cancel.load(Ordering::Relaxed))
    });
    runtime.shutdown_timeout(Duration::from_secs(1));

    if !outcome? {
        return Ok(None);
    }
    let real = report::build(
        &cluster,
        &collector,
        options,
        u32::try_from(cores).unwrap_or(u32::MAX),
        notes,
    );
    let deltas = report::deltas(&simulated, &real);
    Ok(Some(RealComparison {
        real,
        simulated,
        deltas,
    }))
}

async fn report_progress(
    cluster: Arc<Cluster>,
    collector: Arc<Collector>,
    sink: Arc<dyn Fn(RealEvent) -> bool + Send + Sync>,
    total_s: f64,
) {
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tick.tick().await;
        let Some(start) = cluster.start.get() else {
            continue;
        };
        sink(RealEvent::Progress {
            elapsed_s: start.elapsed().as_secs_f64().min(total_s),
            total_s,
            offered: collector.offered.load(Ordering::Relaxed),
            completed: collector.completed.load(Ordering::Relaxed),
            errors: collector.errors.load(Ordering::Relaxed),
        });
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::time::Instant;

    use super::*;
    use crate::domain::topology::{Position, Topology, TopologyEdge, TopologyNode};

    fn node(id: &str, kind: NodeKind) -> TopologyNode {
        TopologyNode {
            id: id.to_owned(),
            kind,
            label: id.to_owned(),
            position: Position { x: 0.0, y: 0.0 },
            template: None,
            settings: BTreeMap::new(),
            replicas: 1,
        }
    }

    fn project(
        nodes: &[(&str, NodeKind)],
        edges: &[(&str, &str)],
        rps: f64,
        reads: f64,
    ) -> Project {
        let mut p = Project::new("real");
        p.server_profile.cpu_cores = 4;
        p.server_profile.cpu_ghz = 3.0;
        p.server_profile.disk.iops = 20_000;
        p.server_profile.network.latency_ms = 0.2;
        p.traffic_profile.requests_per_second = rps;
        p.traffic_profile.read_ratio = reads;
        p.topology = Topology {
            nodes: nodes.iter().map(|(i, k)| node(i, *k)).collect(),
            edges: edges
                .iter()
                .map(|(a, b)| TopologyEdge {
                    id: format!("{a}-{b}"),
                    source: (*a).to_owned(),
                    target: (*b).to_owned(),
                })
                .collect(),
        };
        p
    }

    fn client_app(rps: f64) -> Project {
        project(
            &[("c", NodeKind::Client), ("app", NodeKind::AppServer)],
            &[("c", "app")],
            rps,
            1.0,
        )
    }

    fn options(duration_s: f64) -> SimOptions {
        SimOptions {
            seed: 4,
            duration_s,
            warmup_s: 1.0,
        }
    }

    fn quiet() -> Arc<dyn Fn(RealEvent) -> bool + Send + Sync> {
        Arc::new(|_| true)
    }

    fn real_run(p: &Project, duration_s: f64) -> RealComparison {
        run(p, &options(duration_s), &AtomicBool::new(false), &quiet())
            .expect("run")
            .expect("not cancelled")
    }

    #[test]
    fn rejects_runs_that_are_too_long_too_fast_or_too_big() {
        let ok = client_app(100.0);
        assert!(validate(&ok, &options(10.0)).is_ok());
        assert!(validate(&ok, &options(MAX_DURATION_S + 1.0)).is_err());
        assert!(validate(&client_app(MAX_RPS + 1.0), &options(10.0)).is_err());

        let mut spiky = client_app(1_000.0);
        spiky.traffic_profile.spike = crate::domain::traffic::SpikePattern::Step {
            at_s: 1.0,
            multiplier: 10.0,
            duration_s: 2.0,
        };
        assert!(validate(&spiky, &options(10.0)).is_err());

        let mut big = client_app(10.0);
        for i in 0..=MAX_NODES {
            big.topology
                .nodes
                .push(node(&format!("n{i}"), NodeKind::AppServer));
        }
        assert!(validate(&big, &options(10.0)).is_err());
    }

    #[test]
    fn a_light_load_is_served_at_the_offered_rate_with_no_errors() {
        let c = real_run(&client_app(50.0), 4.0);
        let real = &c.real;
        assert_eq!(real.summary.errors, 0, "{:?}", real.summary);
        let rate = real.summary.throughput_rps;
        assert!((rate - 50.0).abs() / 50.0 < 0.2, "throughput {rate}");
        // Service time is about 10 ms; allow generous slack for timers and a busy machine.
        let p50 = real.summary.latency.p50_ms;
        assert!((5.0..80.0).contains(&p50), "p50 {p50} ms");
        assert_eq!(real.nodes.len(), 1);
        assert!(real.nodes[0].utilization < 0.9);
        assert!(!real.notes.is_empty());
        assert!(c.deltas.iter().any(|d| d.metric.starts_with("Throughput")));
    }

    #[test]
    fn an_overloaded_node_rejects_requests() {
        let c = real_run(&client_app(800.0), 4.0);
        let app = &c.real.nodes[0];
        assert!(app.rejected + app.timed_out > 0, "{app:?}");
        assert!(
            c.real.summary.error_rate > 0.1,
            "{}",
            c.real.summary.error_rate
        );
        assert_eq!(app.severity, crate::sim::report::Severity::Saturated);
    }

    #[test]
    fn caches_take_the_matching_share_of_reads_off_the_database() {
        let p = project(
            &[
                ("c", NodeKind::Client),
                ("app", NodeKind::AppServer),
                ("cache", NodeKind::Cache),
                ("db", NodeKind::Database),
            ],
            &[("c", "app"), ("app", "cache"), ("cache", "db")],
            60.0,
            1.0,
        );
        let c = real_run(&p, 5.0);
        let by_id = |id: &str| c.real.nodes.iter().find(|n| n.id == id).expect("node");
        let misses = f64::from(by_id("db").arrivals) / f64::from(by_id("cache").arrivals);
        assert!((misses - 0.2).abs() < 0.1, "miss ratio {misses}");
    }

    #[test]
    fn cancelling_ends_the_run_quickly() {
        let p = client_app(50.0);
        let cancel = AtomicBool::new(false);
        let started = Instant::now();
        let result = std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(700));
                cancel.store(true, Ordering::Relaxed);
            });
            run(&p, &options(30.0), &cancel, &quiet())
        });
        assert!(result.expect("run").is_none());
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn progress_is_reported_while_running() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink: Arc<dyn Fn(RealEvent) -> bool + Send + Sync> = {
            let seen = Arc::clone(&seen);
            Arc::new(move |e| {
                seen.lock().expect("lock").push(e);
                true
            })
        };
        run(
            &client_app(40.0),
            &options(3.0),
            &AtomicBool::new(false),
            &sink,
        )
        .expect("run")
        .expect("finished");
        let events = seen.lock().expect("lock");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, RealEvent::Progress { total_s, .. } if (*total_s - 3.0).abs() < 1e-9)),
            "{events:?}"
        );
    }
}
