use std::collections::BTreeMap;

use crate::domain::project::Project;
use crate::domain::template::TemplateRef;
use crate::domain::topology::{NodeKind, Position, Topology, TopologyEdge, TopologyNode};
use crate::domain::traffic::SpikePattern;

use super::report::Severity;
use super::{SimError, SimOptions, run};

fn node(id: &str, kind: NodeKind) -> TopologyNode {
    TopologyNode {
        id: id.to_owned(),
        kind,
        label: id.to_owned(),
        position: Position { x: 0.0, y: 0.0 },
        template: None,
        settings: BTreeMap::new(),
    }
}

fn topology(nodes: &[(&str, NodeKind)], edges: &[(&str, &str)]) -> Topology {
    Topology {
        nodes: nodes.iter().map(|(id, k)| node(id, *k)).collect(),
        edges: edges
            .iter()
            .map(|(a, b)| TopologyEdge {
                id: format!("{a}-{b}"),
                source: (*a).to_owned(),
                target: (*b).to_owned(),
            })
            .collect(),
    }
}

/// One-core, reference-speed machine with no network latency, so the maths is easy to check.
fn project(topology: Topology, rps: f64, read_ratio: f64) -> Project {
    let mut p = Project::new("t".to_owned());
    p.server_profile.cpu_cores = 1;
    p.server_profile.cpu_ghz = 3.0;
    p.server_profile.disk.iops = 20_000;
    p.server_profile.network.latency_ms = 0.0;
    p.traffic_profile.requests_per_second = rps;
    p.traffic_profile.read_ratio = read_ratio;
    p.topology = topology;
    p
}

fn options(seed: u32, duration_s: f64) -> SimOptions {
    SimOptions {
        seed,
        duration_s,
        warmup_s: 5.0,
    }
}

fn client_app() -> Topology {
    topology(
        &[("c", NodeKind::Client), ("app", NodeKind::AppServer)],
        &[("c", "app")],
    )
}

#[test]
fn same_seed_gives_identical_reports() {
    let p = project(Topology::starter(), 100.0, 0.8);
    let a = run(&p, &options(9, 30.0)).expect("run");
    let b = run(&p, &options(9, 30.0)).expect("run");
    assert_eq!(a, b);
}

#[test]
fn different_seeds_give_different_reports() {
    let p = project(Topology::starter(), 100.0, 0.8);
    let a = run(&p, &options(1, 30.0)).expect("run");
    let b = run(&p, &options(2, 30.0)).expect("run");
    assert_ne!(a.summary, b.summary);
}

/// M/G/1 with lognormal service (Pollaczek-Khinchine): mean latency = E[S] + lambda*E[S^2] / (2(1-rho)).
#[test]
fn single_server_matches_queueing_theory() {
    let p = project(client_app(), 70.0, 1.0);
    let report = run(&p, &options(3, 700.0)).expect("run");

    let mean_s = 0.010; // 10 ms: 6 ms CPU + 4 ms I/O on the reference machine
    let sigma: f64 = 0.6;
    let second_moment = mean_s * mean_s * sigma.mul_add(sigma, 0.0).exp();
    let rho = 70.0 * mean_s;
    let expected_ms = (mean_s + 70.0 * second_moment / (2.0 * (1.0 - rho))) * 1_000.0;

    let app = &report.nodes[0];
    assert!(
        (app.utilization - rho).abs() < 0.03,
        "util {}",
        app.utilization
    );
    let got = report.summary.latency.mean_ms;
    assert!(
        (got - expected_ms).abs() / expected_ms < 0.10,
        "mean latency {got} ms vs theory {expected_ms} ms"
    );
}

#[test]
fn overload_rejects_requests_and_names_the_bottleneck() {
    let p = project(client_app(), 500.0, 1.0);
    let report = run(&p, &options(4, 30.0)).expect("run");
    let app = &report.nodes[0];
    assert!(app.rejected + app.timed_out > 0);
    assert_eq!(app.severity, Severity::Saturated);
    assert!(report.summary.error_rate > 0.3);
    let b = report.bottleneck.expect("bottleneck");
    assert_eq!(b.node_id, "app");
}

#[test]
fn light_load_is_healthy_with_no_bottleneck() {
    let p = project(client_app(), 10.0, 1.0);
    let report = run(&p, &options(5, 30.0)).expect("run");
    assert_eq!(report.summary.errors, 0);
    assert!(report.bottleneck.is_none());
}

#[test]
fn cache_absorbs_reads_before_the_database() {
    let t = topology(
        &[
            ("c", NodeKind::Client),
            ("app", NodeKind::AppServer),
            ("cache", NodeKind::Cache),
            ("db", NodeKind::Database),
        ],
        &[("c", "app"), ("app", "cache"), ("cache", "db")],
    );
    let report = run(&project(t, 40.0, 1.0), &options(6, 200.0)).expect("run");
    let by_id = |id: &str| report.nodes.iter().find(|n| n.id == id).expect("node");
    let misses = f64::from(by_id("db").arrivals) / f64::from(by_id("cache").arrivals);
    assert!((misses - 0.2).abs() < 0.03, "miss ratio {misses}");
}

#[test]
fn writes_bypass_the_cache() {
    let t = topology(
        &[
            ("c", NodeKind::Client),
            ("app", NodeKind::AppServer),
            ("cache", NodeKind::Cache),
            ("db", NodeKind::Database),
        ],
        &[("c", "app"), ("app", "cache"), ("app", "db")],
    );
    let report = run(&project(t, 20.0, 0.0), &options(7, 60.0)).expect("run");
    let by_id = |id: &str| report.nodes.iter().find(|n| n.id == id).expect("node");
    assert_eq!(by_id("cache").arrivals, 0);
    assert!(by_id("db").arrivals > 0);
}

#[test]
fn a_cache_miss_falls_back_to_the_app_servers_database() {
    let t = topology(
        &[
            ("c", NodeKind::Client),
            ("app", NodeKind::AppServer),
            ("cache", NodeKind::Cache),
            ("db", NodeKind::Database),
        ],
        &[("c", "app"), ("app", "cache"), ("app", "db")],
    );
    let report = run(&project(t, 30.0, 1.0), &options(8, 120.0)).expect("run");
    let by_id = |id: &str| report.nodes.iter().find(|n| n.id == id).expect("node");
    let ratio = f64::from(by_id("db").arrivals) / f64::from(by_id("cache").arrivals);
    assert!((ratio - 0.2).abs() < 0.04, "miss ratio {ratio}");
}

#[test]
fn template_settings_change_capacity() {
    let mut t = topology(
        &[("c", NodeKind::Client), ("lb", NodeKind::LoadBalancer)],
        &[("c", "lb")],
    );
    t.nodes[1].template = Some(TemplateRef {
        id: "nginx".to_owned(),
        version: 2,
    });
    t.nodes[1]
        .settings
        .insert("worker_processes".to_owned(), 1.0);
    let mut p = project(t, 10.0, 1.0);
    p.server_profile.cpu_cores = 8;
    let one = run(&p, &options(1, 20.0)).expect("run").nodes[0].servers;
    p.topology.nodes[1]
        .settings
        .insert("worker_processes".to_owned(), 4.0);
    let four = run(&p, &options(1, 20.0)).expect("run").nodes[0].servers;
    assert_eq!((one, four), (1, 4));
}

#[test]
fn step_spike_raises_the_offered_rate() {
    let mut p = project(client_app(), 50.0, 1.0);
    p.traffic_profile.spike = SpikePattern::Step {
        at_s: 20.0,
        multiplier: 3.0,
        duration_s: 10.0,
    };
    let report = run(&p, &options(2, 40.0)).expect("run");
    let rate = |from: usize, to: usize| {
        report.timeline[from..to]
            .iter()
            .map(|t| t.offered_rps)
            .sum::<f64>()
            / (to - from) as f64
    };
    assert!(rate(20, 30) > 2.0 * rate(8, 18));
}

#[test]
fn loops_and_missing_clients_are_rejected() {
    let looped = topology(
        &[
            ("c", NodeKind::Client),
            ("a", NodeKind::AppServer),
            ("b", NodeKind::AppServer),
        ],
        &[("c", "a"), ("a", "b"), ("b", "a")],
    );
    let err = run(&project(looped, 10.0, 1.0), &options(1, 10.0)).expect_err("loop");
    assert!(matches!(err, SimError::Model(m) if m.contains("loop")));

    let none = topology(&[("a", NodeKind::AppServer)], &[]);
    let err = run(&project(none, 10.0, 1.0), &options(1, 10.0)).expect_err("no client");
    assert!(matches!(err, SimError::Model(_)));
}

#[test]
fn runaway_scenarios_hit_the_event_limit() {
    let mut p = project(client_app(), 100_000.0, 1.0);
    p.traffic_profile.requests_per_second = 100_000.0;
    let err = run(&p, &options(1, 600.0)).expect_err("limit");
    assert!(matches!(err, SimError::EventLimit { .. }));
}

#[test]
fn invalid_options_are_reported() {
    let p = project(client_app(), 10.0, 1.0);
    let bad = SimOptions {
        seed: 1,
        duration_s: 10.0,
        warmup_s: 10.0,
    };
    assert!(matches!(run(&p, &bad), Err(SimError::Invalid(_))));
}
