use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

use super::*;
use crate::domain::topology::{NodeKind, Position, Topology, TopologyEdge, TopologyNode};
use crate::domain::whatif::{ServerField, Variant};

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

/// Client -> app, one core at the reference speed: about 10 ms per request, so ~100 req/s of capacity.
fn single_server(rps: f64) -> Project {
    let mut p = Project::new("what-if");
    p.server_profile.cpu_cores = 1;
    p.server_profile.cpu_ghz = 3.0;
    p.server_profile.disk.iops = 20_000;
    p.server_profile.network.latency_ms = 0.0;
    p.traffic_profile.requests_per_second = rps;
    p.traffic_profile.read_ratio = 1.0;
    p.topology = Topology {
        nodes: vec![
            node("c", NodeKind::Client),
            node("app", NodeKind::AppServer),
        ],
        edges: vec![TopologyEdge {
            id: "c-app".into(),
            source: "c".into(),
            target: "app".into(),
        }],
    };
    p
}

fn options() -> SimOptions {
    SimOptions {
        seed: 3,
        duration_s: 30.0,
        warmup_s: 5.0,
    }
}

fn quiet() -> impl Fn(u32, u32) + Sync {
    |_, _| {}
}

fn variant(id: &str, name: &str, changes: Vec<Change>) -> Variant {
    Variant {
        id: id.into(),
        name: name.into(),
        changes,
    }
}

#[test]
fn compare_runs_the_baseline_and_every_variant_in_order() {
    let mut p = single_server(150.0);
    p.whatif.variants = vec![
        variant(
            "more",
            "Two replicas",
            vec![Change::Replicas {
                node: "app".into(),
                count: 2,
            }],
        ),
        variant(
            "less",
            "Half the traffic",
            vec![Change::TrafficScale { factor: 0.5 }],
        ),
    ];
    let result = run(
        &p,
        &options(),
        &WhatIfRequest::Compare,
        &AtomicBool::new(false),
        &quiet(),
    )
    .expect("run")
    .expect("finished");
    let WhatIfResult::Compare { report } = result else {
        panic!("not a comparison");
    };
    assert_eq!(report.baseline.name, "Baseline");
    assert_eq!(
        report
            .variants
            .iter()
            .map(|v| v.id.as_str())
            .collect::<Vec<_>>(),
        ["more", "less"]
    );
    // 150 req/s on a ~100 req/s server is overloaded; a second replica or half the traffic fixes it.
    assert!(!report.baseline.outcome.meets_slo);
    assert!(
        report.variants[0].outcome.meets_slo,
        "{:?}",
        report.variants[0].outcome.summary
    );
    assert!(report.variants[1].outcome.meets_slo);
    let delta = report.variants[0].vs_baseline.as_ref().expect("delta");
    assert!(delta.p95_pct.is_some_and(|d| d < -50.0), "{delta:?}");
    assert!(delta.error_rate_points < 0.0);
    assert_eq!(
        report.variants[0].changes,
        ["App server \u{d7}2 replicas".replace("App server", "app")]
    );
}

#[test]
fn compare_is_repeatable() {
    let mut p = single_server(80.0);
    p.whatif.variants = vec![variant(
        "v",
        "Faster CPU",
        vec![Change::Server {
            field: ServerField::CpuGhz,
            value: 6.0,
        }],
    )];
    let go = || {
        run(
            &p,
            &options(),
            &WhatIfRequest::Compare,
            &AtomicBool::new(false),
            &quiet(),
        )
        .expect("run")
        .expect("finished")
    };
    assert_eq!(go(), go());
}

#[test]
fn replicas_multiply_workers_and_capacity() {
    let one = single_server(10.0);
    let mut three = one.clone();
    three.topology.nodes[1].replicas = 3;
    let servers = |p: &Project| crate::sim::run(p, &options()).expect("run").nodes[0].servers;
    assert_eq!(servers(&three), servers(&one) * 3);
}

#[test]
fn sweeping_traffic_shows_latency_rising_and_the_first_miss() {
    let p = single_server(60.0);
    let spec = SweepSpec {
        param: SweepParam::TrafficScale,
        from: 0.5,
        to: 3.0,
        points: 6,
    };
    let result = run(
        &p,
        &options(),
        &WhatIfRequest::Sweep { spec },
        &AtomicBool::new(false),
        &quiet(),
    )
    .expect("run")
    .expect("finished");
    let WhatIfResult::Sweep { report } = result else {
        panic!("not a sweep");
    };
    assert_eq!(report.points.len(), 6);
    assert!(
        (report.points[0].value - 0.5).abs() < 1e-9 && (report.points[5].value - 3.0).abs() < 1e-9
    );
    let p95: Vec<f64> = report
        .points
        .iter()
        .map(|x| x.outcome.summary.latency.p95_ms)
        .collect();
    assert!(p95[5] > p95[0] * 3.0, "{p95:?}");
    assert!(report.points[0].outcome.meets_slo);
    assert!(!report.points[5].outcome.meets_slo);
    let miss = report.first_miss.expect("a miss");
    assert!(miss > 0.5 && miss <= 3.0, "{miss}");
    assert!(report.label.starts_with("Traffic"));
}

#[test]
fn sweeping_a_whole_number_parameter_rounds_and_removes_duplicates() {
    let spec = SweepSpec {
        param: SweepParam::Replicas { node: "app".into() },
        from: 1.0,
        to: 3.0,
        points: 9,
    };
    assert_eq!(sweep_values(&spec), [1.0, 2.0, 3.0]);
    let traffic = SweepSpec {
        param: SweepParam::TrafficScale,
        from: 1.0,
        to: 8.0,
        points: 4,
    };
    let v = sweep_values(&traffic);
    assert!(
        (v[1] - 2.0).abs() < 1e-9 && (v[2] - 4.0).abs() < 1e-9,
        "{v:?}"
    );
}

#[test]
fn sweep_requests_are_validated_before_running() {
    let p = single_server(50.0);
    let bad = |spec: SweepSpec| validate(&p, &options(), &WhatIfRequest::Sweep { spec }).is_err();
    let base = SweepSpec {
        param: SweepParam::TrafficScale,
        from: 1.0,
        to: 2.0,
        points: 5,
    };
    assert!(!bad(base.clone()));
    assert!(bad(SweepSpec {
        points: 1,
        ..base.clone()
    }));
    assert!(bad(SweepSpec {
        points: 100,
        ..base.clone()
    }));
    assert!(bad(SweepSpec {
        to: 1.0,
        ..base.clone()
    }));
    assert!(bad(SweepSpec {
        to: 500.0,
        ..base.clone()
    }));
    assert!(bad(SweepSpec {
        param: SweepParam::Replicas {
            node: "ghost".into()
        },
        from: 1.0,
        to: 2.0,
        points: 2,
    }));
}

/// M/G/1 with ~10 ms service: the p95 target of 200 ms is crossed a little below 100 req/s.
#[test]
fn capacity_search_brackets_the_single_server_limit() {
    let p = single_server(40.0);
    let result = run(
        &p,
        &options(),
        &WhatIfRequest::Capacity,
        &AtomicBool::new(false),
        &quiet(),
    )
    .expect("run")
    .expect("finished");
    let WhatIfResult::Capacity { report } = result else {
        panic!("not capacity");
    };
    assert!(report.current_meets_slo);
    let max = report.max_rps.expect("a limit");
    assert!((70.0..=100.0).contains(&max), "max {max}");
    let headroom = report.headroom.expect("headroom");
    assert!((headroom - max / 40.0).abs() < 1e-6);
    assert!(!report.reached_search_limit);
    let limit = report.limit.expect("limit");
    assert_eq!(limit.node, "app");
    assert!(!limit.missed.is_empty());
    assert!(report.probes.iter().any(|p| !p.meets_slo));
    assert!(report.probes.len() <= 22);
}

#[test]
fn capacity_search_goes_down_when_the_design_already_misses() {
    let p = single_server(300.0);
    let result = run(
        &p,
        &options(),
        &WhatIfRequest::Capacity,
        &AtomicBool::new(false),
        &quiet(),
    )
    .expect("run")
    .expect("finished");
    let WhatIfResult::Capacity { report } = result else {
        panic!("not capacity");
    };
    assert!(!report.current_meets_slo);
    let max = report.max_rps.expect("a limit below today's rate");
    assert!(max < 120.0 && max > 40.0, "max {max}");
    assert!(report.headroom.expect("headroom") < 1.0);
}

#[test]
fn capacity_search_stops_at_its_upper_bound_for_a_huge_design() {
    let mut p = single_server(2.0);
    p.topology.nodes[1].replicas = 64;
    let opts = SimOptions {
        seed: 1,
        duration_s: 10.0,
        warmup_s: 2.0,
    };
    let result = run(
        &p,
        &opts,
        &WhatIfRequest::Capacity,
        &AtomicBool::new(false),
        &quiet(),
    )
    .expect("run")
    .expect("finished");
    let WhatIfResult::Capacity { report } = result else {
        panic!("not capacity");
    };
    assert!(report.reached_search_limit);
    assert_eq!(report.headroom, Some(capacity::MAX_SCALE));
    assert!(report.limit.is_none());
}

#[test]
fn capacity_reports_when_even_a_trickle_misses_the_targets() {
    let mut p = single_server(100.0);
    p.whatif.slo.p95_ms = 1.0; // a 10 ms service can never meet a 1 ms p95
    let result = run(
        &p,
        &options(),
        &WhatIfRequest::Capacity,
        &AtomicBool::new(false),
        &quiet(),
    )
    .expect("run")
    .expect("finished");
    let WhatIfResult::Capacity { report } = result else {
        panic!("not capacity");
    };
    assert!(report.max_rps.is_none() && report.headroom.is_none());
}

#[test]
fn cancelling_stops_every_kind_of_request() {
    let p = single_server(50.0);
    let spec = SweepSpec {
        param: SweepParam::TrafficScale,
        from: 1.0,
        to: 2.0,
        points: 4,
    };
    for request in [
        WhatIfRequest::Compare,
        WhatIfRequest::Sweep { spec },
        WhatIfRequest::Capacity,
    ] {
        let out = run(&p, &options(), &request, &AtomicBool::new(true), &quiet()).expect("run");
        assert!(out.is_none(), "{request:?}");
    }
}

#[test]
fn progress_reaches_the_total() {
    let mut p = single_server(50.0);
    p.whatif.variants = vec![variant(
        "a",
        "A",
        vec![Change::TrafficScale { factor: 2.0 }],
    )];
    let last = std::sync::atomic::AtomicU32::new(0);
    let progress = |done: u32, total: u32| {
        assert_eq!(total, 2);
        last.fetch_max(done, Ordering::Relaxed);
    };
    run(
        &p,
        &options(),
        &WhatIfRequest::Compare,
        &AtomicBool::new(false),
        &progress,
    )
    .expect("run")
    .expect("finished");
    assert_eq!(last.load(Ordering::Relaxed), 2);
}

#[test]
fn runs_that_cannot_finish_become_failed_outcomes() {
    let mut p = single_server(100_000.0);
    p.traffic_profile.requests_per_second = 1_000_000.0;
    let big = SimOptions {
        seed: 1,
        duration_s: 60.0,
        warmup_s: 5.0,
    };
    let outcome = evaluate(&p, &big, &Slo::default());
    assert!(!outcome.meets_slo);
    assert!(outcome.error.is_some_and(|e| e.contains("events")));
}

#[test]
fn suggestions_rank_fixes_for_an_overloaded_server() {
    let p = single_server(150.0);
    let result = run(
        &p,
        &options(),
        &WhatIfRequest::Suggest,
        &AtomicBool::new(false),
        &quiet(),
    )
    .expect("run")
    .expect("finished");
    let WhatIfResult::Suggest { report } = result else {
        panic!("not suggestions");
    };
    assert!(!report.baseline.meets_slo);
    assert_eq!(report.target.as_deref(), Some("app"));
    assert!(!report.suggestions.is_empty());
    let best = &report.suggestions[0];
    assert!(best.outcome.meets_slo, "{best:?}");
    assert!(best.vs_baseline.p95_pct.is_some_and(|d| d < -50.0));
    // Sorted best first: once one misses the targets, none after it meets them.
    let met: Vec<bool> = report
        .suggestions
        .iter()
        .map(|s| s.outcome.meets_slo)
        .collect();
    assert!(met.windows(2).all(|w| w[0] || !w[1]), "{met:?}");
    assert!(
        report
            .suggestions
            .iter()
            .any(|s| matches!(s.change_defs[0], Change::Replicas { .. }))
    );
    assert!(
        report
            .suggestions
            .iter()
            .all(|s| s.changes.len() == s.change_defs.len())
    );
}

#[test]
fn suggested_changes_can_be_saved_as_a_variant() {
    let p = single_server(150.0);
    let WhatIfResult::Suggest { report } = run(
        &p,
        &options(),
        &WhatIfRequest::Suggest,
        &AtomicBool::new(false),
        &quiet(),
    )
    .expect("run")
    .expect("finished") else {
        panic!("not suggestions");
    };
    let mut with_variant = p.clone();
    with_variant.whatif.variants = vec![variant(
        "s",
        "Suggestion",
        report.suggestions[0].change_defs.clone(),
    )];
    assert!(with_variant.validate().is_ok());
}

#[test]
fn a_healthy_design_gets_few_or_no_suggestions() {
    let p = single_server(20.0);
    let WhatIfResult::Suggest { report } = run(
        &p,
        &options(),
        &WhatIfRequest::Suggest,
        &AtomicBool::new(false),
        &quiet(),
    )
    .expect("run")
    .expect("finished") else {
        panic!("not suggestions");
    };
    assert!(report.baseline.meets_slo);
    assert!(
        report
            .suggestions
            .iter()
            .all(|s| s.vs_baseline.p95_pct.is_some_and(|d| d < 0.0)
                || s.vs_baseline.error_rate_points < 0.0)
    );
}

#[test]
fn suggestions_report_progress_and_can_be_cancelled() {
    let p = single_server(150.0);
    let last = std::sync::atomic::AtomicU32::new(0);
    let progress = |done: u32, total: u32| {
        assert!(done <= total);
        last.fetch_max(done, Ordering::Relaxed);
    };
    run(
        &p,
        &options(),
        &WhatIfRequest::Suggest,
        &AtomicBool::new(false),
        &progress,
    )
    .expect("run")
    .expect("finished");
    assert!(last.load(Ordering::Relaxed) >= 3);
    assert!(
        run(
            &p,
            &options(),
            &WhatIfRequest::Suggest,
            &AtomicBool::new(true),
            &quiet()
        )
        .expect("run")
        .is_none()
    );
}
