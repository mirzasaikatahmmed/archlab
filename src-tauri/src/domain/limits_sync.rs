//! Keeps Rust validation ranges in sync with the zod schemas in the UI.
//! Both sides test against the shared fixture `src/schemas/limits.json`.

use serde::Deserialize;
use serde_json::{Value, json};

use super::chaos::{ChaosPlan, ClientPolicy, Fault, FaultKind};
use super::project::Project;
use super::server_profile::ServerProfile;
use super::topology::Topology;
use super::traffic::TrafficProfile;
use super::whatif::{Change, Slo, Variant, WhatIf};

#[derive(Deserialize)]
struct Limit {
    path: String,
    min: f64,
    max: f64,
    below: f64,
    above: f64,
}

#[derive(Deserialize)]
struct FaultLimit {
    kind: String,
    #[serde(flatten)]
    limit: Limit,
}

#[derive(Deserialize)]
struct ChangeLimit {
    kind: String,
    #[serde(flatten)]
    limit: Limit,
}

#[derive(Deserialize)]
struct Limits {
    slo: Vec<Limit>,
    #[serde(rename = "changeParams")]
    change_params: Vec<ChangeLimit>,
    #[serde(rename = "sweepPoints")]
    sweep_points: Vec<Limit>,
    #[serde(rename = "clientPolicy")]
    client_policy: Vec<Limit>,
    #[serde(rename = "faultTiming")]
    fault_timing: Vec<Limit>,
    #[serde(rename = "faultParams")]
    fault_params: Vec<FaultLimit>,
    #[serde(rename = "serverProfile")]
    server_profile: Vec<Limit>,
    #[serde(rename = "trafficProfile")]
    traffic_profile: Vec<Limit>,
}

fn load() -> Limits {
    let raw = include_str!("../../../src/schemas/limits.json");
    serde_json::from_str(raw).expect("limits.json parses")
}

fn set_path(root: &mut Value, path: &str, value: f64) {
    let mut cur = root;
    let mut parts = path.split('.').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            cur[part] = if value.fract() == 0.0 {
                json!(value as i64)
            } else {
                json!(value)
            };
        } else {
            cur = &mut cur[part];
        }
    }
}

fn is_valid<T>(base: &T, path: &str, value: f64, validate: impl Fn(&T) -> bool) -> bool
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let mut v = serde_json::to_value(base).expect("serialize");
    set_path(&mut v, path, value);
    serde_json::from_value::<T>(v).is_ok_and(|t| validate(&t))
}

fn check<T>(base: &T, limits: &[Limit], validate: impl Fn(&T) -> bool + Copy)
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    for l in limits {
        assert!(is_valid(base, &l.path, l.min, validate), "{} min", l.path);
        assert!(is_valid(base, &l.path, l.max, validate), "{} max", l.path);
        // Out-of-range integers may fail to deserialize (e.g. negative u32) — both are rejections.
        assert!(
            !is_valid(base, &l.path, l.below, validate),
            "{} below",
            l.path
        );
        assert!(
            !is_valid(base, &l.path, l.above, validate),
            "{} above",
            l.path
        );
        assert!(!l.path.is_empty() && l.min <= l.max);
    }
}

#[test]
fn server_profile_ranges_match_shared_fixture() {
    check(&ServerProfile::default(), &load().server_profile, |p| {
        p.validate().is_ok()
    });
}

#[test]
fn traffic_profile_ranges_match_shared_fixture() {
    check(&TrafficProfile::default(), &load().traffic_profile, |p| {
        p.validate().is_ok()
    });
}

fn plan_is_valid(faults: Vec<Fault>, policy: ClientPolicy) -> bool {
    ChaosPlan { faults, policy }
        .validate(&Topology::starter())
        .is_ok()
}

fn base_fault(kind: FaultKind, target: &str) -> Fault {
    Fault {
        id: "f".to_owned(),
        target: target.to_owned(),
        kind,
        start_s: 10.0,
        duration_s: 5.0,
    }
}

#[test]
fn client_policy_ranges_match_shared_fixture() {
    check(&ClientPolicy::default(), &load().client_policy, |p| {
        plan_is_valid(Vec::new(), p.clone())
    });
}

#[test]
fn fault_timing_ranges_match_shared_fixture() {
    check(
        &base_fault(FaultKind::Crash, "db"),
        &load().fault_timing,
        |f| plan_is_valid(vec![f.clone()], ClientPolicy::default()),
    );
}

#[test]
fn fault_parameter_ranges_match_shared_fixture() {
    for row in load().fault_params {
        let (kind, target) = match row.kind.as_str() {
            "slowdown" => (FaultKind::Slowdown { factor: 2.0 }, "db"),
            "lostWorkers" => (FaultKind::LostWorkers { fraction: 0.5 }, "db"),
            "edgeLatency" => (FaultKind::EdgeLatency { extra_ms: 10.0 }, "e-app-db"),
            "edgeLoss" => (FaultKind::EdgeLoss { loss: 0.5 }, "e-app-db"),
            other => panic!("unknown fault kind in fixture: {other}"),
        };
        check(
            &base_fault(kind, target),
            std::slice::from_ref(&row.limit),
            |f| plan_is_valid(vec![f.clone()], ClientPolicy::default()),
        );
    }
}

#[test]
fn slo_ranges_match_shared_fixture() {
    let project = Project::new("limits");
    check(&Slo::default(), &load().slo, |s| {
        WhatIf {
            slo: s.clone(),
            variants: Vec::new(),
        }
        .validate(&project)
        .is_ok()
    });
}

#[test]
fn change_parameter_ranges_match_shared_fixture() {
    let project = Project::new("limits");
    for row in load().change_params {
        let change = match row.kind.as_str() {
            "trafficScale" => Change::TrafficScale { factor: 2.0 },
            "replicas" => Change::Replicas {
                node: "app".to_owned(),
                count: 2,
            },
            other => panic!("unknown change kind in fixture: {other}"),
        };
        check(&change, std::slice::from_ref(&row.limit), |c| {
            WhatIf {
                slo: Slo::default(),
                variants: vec![Variant {
                    id: "v".to_owned(),
                    name: "Variant".to_owned(),
                    changes: vec![c.clone()],
                }],
            }
            .validate(&project)
            .is_ok()
        });
    }
}

#[test]
fn sweep_point_limits_match_shared_fixture() {
    use crate::sim::SimOptions;
    use crate::whatif::{SweepParam, SweepSpec, WhatIfRequest, validate};
    let project = Project::new("limits");
    let spec = SweepSpec {
        param: SweepParam::TrafficScale,
        from: 1.0,
        to: 2.0,
        points: 5,
    };
    check(&spec, &load().sweep_points, |s| {
        validate(
            &project,
            &SimOptions::default(),
            &WhatIfRequest::Sweep { spec: s.clone() },
        )
        .is_ok()
    });
}
