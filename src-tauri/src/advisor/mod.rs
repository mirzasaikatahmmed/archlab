//! Design advisor: from a description of the product and its needs, picks a blueprint, sizes it
//! and checks the result by simulation.
//!
//! No AI service and no network: the choice of blueprint is rule-based, and the sizing is a loop
//! of "simulate, find the nodes that are too busy, add copies". The answer is therefore as
//! trustworthy as the simulator, and the text says what it assumed.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::domain::blueprint::{self, BlueprintDef};
use crate::domain::project::Project;
use crate::domain::server_profile::{
    DiskKind, DiskProfile, NetworkProfile, OperatingSystem, ServerProfile, Virtualization,
};
use crate::domain::sizing::peak_rps;
use crate::domain::topology::NodeKind;
use crate::domain::validation::{Issues, ValidationError};
use crate::runtime::RuntimeError;
use crate::sim::{SimOptions, SimReport, run};
use crate::whatif::misses;

/// Nodes busier than this at peak get more copies.
const TARGET_UTIL: f64 = 0.55;
const MAX_ROUNDS: usize = 10;
const MAX_COPIES_PER_NODE: u32 = 200;
const MAX_MACHINES: u32 = 600;
/// Rough number of simulator events one request costs, for choosing a run length.
const EVENTS_PER_REQUEST: f64 = 16.0;
/// Event budget for one advisor simulation, well under the engine's hard limit.
const EVENT_BUDGET: f64 = 3_000_000.0;
/// Largest peak the advisor will simulate.
pub const MAX_PEAK_RPS: f64 = 25_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Shape {
    /// Pages and APIs for people (a website, an app backend).
    Web,
    /// Large files and media (images, video, downloads).
    Files,
    /// Work that can finish later (orders, emails, processing).
    Jobs,
    /// Very many small writes (telemetry, events, logs).
    Events,
    /// Many independent services, each with its own data.
    Services,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Availability {
    /// Planned downtime and rare outages are fine.
    Standard,
    /// No single point of failure in the stateless layers (about 99.9%).
    High,
    /// Survive losing any one machine anywhere (about 99.99%).
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Machine {
    Small,
    Medium,
    Large,
}

impl Machine {
    fn profile(self) -> ServerProfile {
        let (cores, ghz, ram, mbps) = match self {
            Self::Small => (2, 2.5, 8.0, 1_000.0),
            Self::Medium => (4, 2.5, 16.0, 1_000.0),
            Self::Large => (8, 3.0, 32.0, 10_000.0),
        };
        ServerProfile {
            os: OperatingSystem::Linux,
            cpu_cores: cores,
            cpu_ghz: ghz,
            ram_gb: ram,
            disk: DiskProfile {
                kind: DiskKind::Ssd,
                iops: 20_000,
            },
            network: NetworkProfile {
                bandwidth_mbps: mbps,
                latency_ms: 0.5,
            },
            virtualization: Virtualization::Vm,
            fd_limit: 65_536,
        }
    }
}

/// What the person tells the advisor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Requirements {
    pub daily_users: u32,
    pub requests_per_user_per_day: f64,
    /// Share of requests that only read, 0..=1.
    pub read_ratio: f64,
    /// Busiest moment compared with the daily average.
    pub peak_factor: f64,
    pub shape: Shape,
    pub availability: Availability,
    pub machine: Machine,
    pub p95_ms: f64,
    pub max_error_rate: f64,
}

impl Default for Requirements {
    fn default() -> Self {
        Self {
            daily_users: 1_000_000,
            requests_per_user_per_day: 20.0,
            read_ratio: 0.9,
            peak_factor: 5.0,
            shape: Shape::Web,
            availability: Availability::High,
            machine: Machine::Medium,
            p95_ms: 300.0,
            max_error_rate: 0.01,
        }
    }
}

impl Requirements {
    pub fn peak_rps(&self) -> f64 {
        peak_rps(
            f64::from(self.daily_users),
            self.requests_per_user_per_day,
            self.peak_factor,
        )
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        issues.range_u32("dailyUsers", self.daily_users, 100, 1_000_000_000);
        issues.range_f64(
            "requestsPerUserPerDay",
            self.requests_per_user_per_day,
            0.1,
            10_000.0,
        );
        issues.range_f64("readRatio", self.read_ratio, 0.0, 1.0);
        issues.range_f64("peakFactor", self.peak_factor, 1.0, 50.0);
        issues.range_f64("p95Ms", self.p95_ms, 1.0, 60_000.0);
        issues.range_f64("maxErrorRate", self.max_error_rate, 0.0, 1.0);
        let peak = self.peak_rps();
        issues.check(
            peak <= MAX_PEAK_RPS,
            "dailyUsers",
            &format!(
                "this is about {peak:.0} requests per second at peak; the advisor simulates up to {MAX_PEAK_RPS:.0}. Lower the users, requests per user or peak factor"
            ),
        );
        issues.finish()
    }
}

/// How one node of the design ended up.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NodeChoice {
    pub id: String,
    pub label: String,
    pub kind: NodeKind,
    pub copies: u32,
    /// How busy it is at peak.
    pub utilization: f64,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Advice {
    pub blueprint_id: String,
    pub blueprint_title: String,
    pub peak_rps: f64,
    pub machines: u32,
    pub meets_targets: bool,
    /// Rounds of "simulate, then add copies" it took.
    pub rounds: u32,
    pub nodes: Vec<NodeChoice>,
    /// Plain-language reasons, in reading order.
    pub rationale: Vec<String>,
    /// What could not be satisfied, and what to try.
    pub warnings: Vec<String>,
    pub assumptions: Vec<String>,
    pub project: Project,
    /// The simulation of the final design.
    pub report: SimReport,
}

fn blueprint_for(r: &Requirements) -> (&'static BlueprintDef, String) {
    let pick = |id: &str| blueprint::find(id).expect("embedded blueprint");
    match r.shape {
        Shape::Files => (
            pick("media-and-files"),
            "you serve large files, so a CDN in front of object storage keeps most traffic off the origin".to_owned(),
        ),
        Shape::Jobs => (
            pick("async-processing"),
            "your work can finish later, so a queue with workers lets users get an answer at once and smooths out bursts".to_owned(),
        ),
        Shape::Events => (
            pick("write-heavy-ingestion"),
            "you collect very many small writes, so a log-based broker takes the burst and consumers write at a steady pace".to_owned(),
        ),
        Shape::Services => (
            pick("microservices"),
            "you run independent services, so a gateway routes to each service and each owns its data".to_owned(),
        ),
        Shape::Web => {
            if r.daily_users >= 500_000 {
                (
                    pick("million-users"),
                    "at this size you need every layer: CDN, gateway, load balancers, app servers, cache and a sharded database".to_owned(),
                )
            } else if r.read_ratio >= 0.85 && r.daily_users >= 50_000 {
                (
                    pick("read-heavy-site"),
                    "most of your traffic is reading, so a CDN and a cache keep it away from the database".to_owned(),
                )
            } else {
                (
                    pick("simple-web"),
                    "at this size a load balancer, app servers and one database are enough; anything more adds cost and moving parts".to_owned(),
                )
            }
        }
    }
}

/// Fewest copies a node should have for the availability you asked for.
fn minimum_copies(kind: NodeKind, availability: Availability) -> u32 {
    let stateless = matches!(
        kind,
        NodeKind::LoadBalancer
            | NodeKind::ApiGateway
            | NodeKind::AppServer
            | NodeKind::Cdn
            | NodeKind::Worker
    );
    match (availability, stateless) {
        (Availability::Standard, _) => 1,
        (Availability::High, true) | (Availability::Critical, false) => 2,
        (Availability::Critical, true) => 3,
        (Availability::High, false) => 1,
    }
}

fn run_options(peak: f64) -> SimOptions {
    let budget_s = EVENT_BUDGET / (peak.max(1.0) * EVENTS_PER_REQUEST);
    let duration_s = budget_s.clamp(10.0, 60.0).floor();
    SimOptions {
        seed: 1,
        duration_s,
        warmup_s: (duration_s / 4.0).floor().clamp(2.0, 5.0),
    }
}

fn build(base: &Project, copies: &BTreeMap<String, u32>) -> Project {
    let mut project = base.clone();
    for node in &mut project.topology.nodes {
        if let Some(n) = copies.get(&node.id) {
            node.replicas = *n;
        }
    }
    project
}

fn machines(project: &Project) -> u32 {
    project
        .topology
        .nodes
        .iter()
        .filter(|n| n.kind != NodeKind::Client)
        .map(|n| n.replicas)
        .sum()
}

fn is_hot(n: &crate::sim::report::NodeReport) -> bool {
    n.utilization > TARGET_UTIL
        || n.rejected > 0
        || n.timed_out > 0
        || n.backlog.as_ref().is_some_and(|b| b.left > 100)
}

/// Designs a system for `requirements`.
pub fn advise(requirements: &Requirements) -> Result<Advice, RuntimeError> {
    requirements.validate()?;
    let peak = requirements.peak_rps();
    let (blueprint, why_shape) = blueprint_for(requirements);

    // The blueprint's smallest size gives the structure and settings; the requirements give the
    // traffic, the targets and the machines.
    let mut base = blueprint.project(&blueprint.tiers[0].id)?;
    base.name = format!(
        "{} for {} daily users",
        blueprint.title, requirements.daily_users
    );
    base.server_profile = requirements.machine.profile();
    base.traffic_profile.requests_per_second = peak;
    base.traffic_profile.read_ratio = requirements.read_ratio;
    base.traffic_profile.concurrent_users = (requirements.daily_users / 10).max(1);
    base.whatif.slo.p95_ms = requirements.p95_ms;
    base.whatif.slo.max_error_rate = requirements.max_error_rate;
    for node in &mut base.topology.nodes {
        node.replicas = 1;
    }

    let mut copies: BTreeMap<String, u32> = base
        .topology
        .nodes
        .iter()
        .filter(|n| n.kind != NodeKind::Client)
        .map(|n| {
            (
                n.id.clone(),
                minimum_copies(n.kind, requirements.availability),
            )
        })
        .collect();

    let options = run_options(peak);
    let mut rounds = 0u32;
    let mut warnings = Vec::new();
    let (project, report) = loop {
        rounds += 1;
        let project = build(&base, &copies);
        let report = run(&project, &options).map_err(RuntimeError::Sim)?;
        let hot: Vec<_> = report.nodes.iter().filter(|n| is_hot(n)).collect();
        if hot.is_empty() || rounds as usize >= MAX_ROUNDS {
            if !hot.is_empty() {
                warnings.push(format!(
                    "Stopped after {MAX_ROUNDS} rounds with some nodes still busy; try larger machines."
                ));
            }
            break (project, report);
        }
        let mut grew = false;
        for n in hot {
            let current = copies.get(&n.id).copied().unwrap_or(1);
            let wanted = (f64::from(current) * n.utilization.max(0.7) / TARGET_UTIL).ceil();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let next = (wanted as u32).max(current + 1).min(MAX_COPIES_PER_NODE);
            if next > current {
                copies.insert(n.id.clone(), next);
                grew = true;
            }
        }
        if !grew || machines(&build(&base, &copies)) > MAX_MACHINES {
            warnings.push(format!(
                "The design would need more than {MAX_MACHINES} machines or a node reached its limit; try larger machines or a lower load."
            ));
            break (project, report);
        }
    };

    let meets_targets = misses(&report.summary, &project.whatif.slo).is_empty();
    if !meets_targets {
        warnings.push(
            "The design does not meet your targets. If no node is busy, the target is lower than these machines can deliver: allow a higher p95 or choose a larger machine."
                .to_owned(),
        );
    }
    let nodes: Vec<NodeChoice> = report
        .nodes
        .iter()
        .map(|n| NodeChoice {
            id: n.id.clone(),
            label: n.label.clone(),
            kind: n.kind,
            copies: copies.get(&n.id).copied().unwrap_or(1),
            utilization: n.utilization,
        })
        .collect();

    let mut rationale = vec![format!(
        "Your peak is about {} requests per second: {} daily users, {} requests each per day, a busiest moment {}\u{d7} the average.",
        peak.round(),
        requirements.daily_users,
        requirements.requests_per_user_per_day,
        requirements.peak_factor
    )];
    rationale.push(format!("Design: {}, because {why_shape}.", blueprint.title));
    rationale.extend(blueprint.why.iter().cloned());
    rationale.push(match requirements.availability {
        Availability::Standard => "Availability: standard, so a layer has one copy unless the load needs more.".to_owned(),
        Availability::High => "Availability: high, so every stateless layer (load balancers, gateways, app servers, workers, CDN) has at least two copies and one can fail without an outage.".to_owned(),
        Availability::Critical => "Availability: critical, so stateless layers have at least three copies and stateful ones at least two. In this model extra database copies share load like shards; real failover also needs replication and leader election.".to_owned(),
    });
    for n in &nodes {
        rationale.push(format!(
            "{}: {} {} at about {:.0}% busy at peak.",
            n.label,
            n.copies,
            if n.copies == 1 { "copy" } else { "copies" },
            n.utilization * 100.0
        ));
    }
    rationale.push(format!(
        "Simulated for {} s at peak load: p95 {:.0} ms (target {:.0} ms), errors {:.2}% (target {:.2}%).",
        options.duration_s,
        report.summary.latency.p95_ms,
        requirements.p95_ms,
        report.summary.error_rate * 100.0,
        requirements.max_error_rate * 100.0
    ));

    Ok(Advice {
        blueprint_id: blueprint.id.clone(),
        blueprint_title: blueprint.title.clone(),
        peak_rps: peak,
        machines: machines(&project),
        meets_targets,
        rounds,
        nodes,
        rationale,
        warnings,
        assumptions: vec![
            "Every machine is the same size; real fleets mix sizes.".to_owned(),
            "Traffic is steady at the peak rate; real traffic has bursts and daily cycles."
                .to_owned(),
            "Cost, regions, data consistency and operations effort are not considered.".to_owned(),
            "Results are estimates from a simulation model, not measurements.".to_owned(),
        ],
        project,
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(f: impl FnOnce(&mut Requirements)) -> Requirements {
        let mut r = Requirements::default();
        f(&mut r);
        r
    }

    fn copies_of(a: &Advice, id: &str) -> u32 {
        a.nodes.iter().find(|n| n.id == id).expect("node").copies
    }

    #[test]
    fn the_shape_and_size_choose_the_blueprint() {
        let pick = |r: Requirements| blueprint_for(&r).0.id.clone();
        assert_eq!(pick(ask(|r| r.daily_users = 10_000)), "simple-web");
        assert_eq!(
            pick(ask(|r| {
                r.daily_users = 200_000;
                r.read_ratio = 0.95;
            })),
            "read-heavy-site"
        );
        assert_eq!(
            pick(ask(|r| {
                r.daily_users = 200_000;
                r.read_ratio = 0.5;
            })),
            "simple-web"
        );
        assert_eq!(pick(Requirements::default()), "million-users");
        assert_eq!(pick(ask(|r| r.shape = Shape::Files)), "media-and-files");
        assert_eq!(pick(ask(|r| r.shape = Shape::Jobs)), "async-processing");
        assert_eq!(
            pick(ask(|r| r.shape = Shape::Events)),
            "write-heavy-ingestion"
        );
        assert_eq!(pick(ask(|r| r.shape = Shape::Services)), "microservices");
    }

    #[test]
    fn a_million_users_get_a_verified_design_with_reasons() {
        let a = advise(&Requirements::default()).expect("advice");
        assert_eq!(a.blueprint_id, "million-users");
        assert!(a.meets_targets, "{:?}", a.warnings);
        assert!(a.warnings.is_empty());
        assert!((a.peak_rps - 1_157.4).abs() < 0.1);
        for n in &a.nodes {
            assert!(
                n.utilization <= 0.6,
                "{} is {:.0}% busy",
                n.label,
                n.utilization * 100.0
            );
            assert!(n.copies >= 1);
        }
        assert_eq!(a.machines, a.nodes.iter().map(|n| n.copies).sum::<u32>());
        assert!(
            a.rationale
                .iter()
                .any(|l| l.contains("1157 requests per second"))
        );
        assert!(
            a.rationale
                .iter()
                .any(|l| l.starts_with("Availability: high"))
        );
        assert!(
            a.rationale
                .last()
                .is_some_and(|l| l.starts_with("Simulated for"))
        );
        assert_eq!(a.project.traffic_profile.requests_per_second, a.peak_rps);
        // The project really is the design that was simulated.
        assert_eq!(
            a.project
                .topology
                .nodes
                .iter()
                .find(|n| n.id == "app")
                .map(|n| n.replicas),
            Some(copies_of(&a, "app"))
        );
    }

    #[test]
    fn availability_sets_the_least_number_of_copies() {
        let small = |availability| {
            advise(&ask(|r| {
                r.daily_users = 10_000;
                r.availability = availability;
            }))
            .expect("advice")
        };
        let standard = small(Availability::Standard);
        assert_eq!(standard.blueprint_id, "simple-web");
        assert!(
            standard.nodes.iter().all(|n| n.copies == 1),
            "{:?}",
            standard.nodes
        );
        let high = small(Availability::High);
        assert_eq!(copies_of(&high, "app"), 2);
        assert_eq!(copies_of(&high, "lb"), 2);
        assert_eq!(copies_of(&high, "db"), 1);
        let critical = small(Availability::Critical);
        assert_eq!(copies_of(&critical, "app"), 3);
        assert_eq!(copies_of(&critical, "db"), 2);
    }

    #[test]
    fn it_adds_copies_where_the_load_needs_them_and_stops_when_it_is_enough() {
        let light = advise(&ask(|r| {
            r.daily_users = 200_000;
            r.availability = Availability::Standard;
        }))
        .expect("light");
        let heavy = advise(&ask(|r| {
            r.daily_users = 8_000_000;
            r.availability = Availability::Standard;
        }))
        .expect("heavy");
        assert!(heavy.machines > light.machines);
        assert!(heavy.meets_targets && light.meets_targets);
        assert!(heavy.rounds >= 1 && heavy.rounds <= MAX_ROUNDS as u32);
    }

    #[test]
    fn each_shape_produces_a_design_that_meets_its_targets() {
        for shape in [
            Shape::Web,
            Shape::Files,
            Shape::Jobs,
            Shape::Events,
            Shape::Services,
        ] {
            let a = advise(&ask(|r| {
                r.shape = shape;
                r.daily_users = 300_000;
                r.read_ratio = match shape {
                    Shape::Files => 0.98,
                    Shape::Jobs => 0.0,
                    Shape::Events => 0.02,
                    _ => 0.9,
                };
            }))
            .unwrap_or_else(|e| panic!("{shape:?}: {e}"));
            assert!(a.meets_targets, "{shape:?}: {:?}", a.warnings);
            assert!(
                a.nodes.iter().all(|n| n.utilization <= 0.6),
                "{shape:?}: {:?}",
                a.nodes
            );
        }
    }

    #[test]
    fn an_impossible_latency_target_is_reported_not_hidden() {
        let a = advise(&ask(|r| {
            r.daily_users = 10_000;
            r.p95_ms = 1.0;
        }))
        .expect("advice");
        assert!(!a.meets_targets);
        assert!(
            a.warnings
                .iter()
                .any(|w| w.contains("does not meet your targets"))
        );
    }

    #[test]
    fn nonsense_and_too_large_requests_are_refused_with_reasons() {
        for bad in [
            ask(|r| r.daily_users = 5),
            ask(|r| r.read_ratio = 1.5),
            ask(|r| r.peak_factor = 0.2),
            ask(|r| r.requests_per_user_per_day = 0.0),
        ] {
            assert!(matches!(advise(&bad), Err(RuntimeError::Invalid(_))));
        }
        let huge = ask(|r| r.daily_users = 1_000_000_000);
        let Err(RuntimeError::Invalid(err)) = advise(&huge) else {
            panic!("expected a validation error");
        };
        assert!(
            err.issues
                .iter()
                .any(|i| i.message.contains("requests per second at peak")),
            "{err:?}"
        );
    }

    #[test]
    fn the_same_question_always_gets_the_same_answer() {
        let r = ask(|r| r.daily_users = 2_000_000);
        let (a, b) = (advise(&r).expect("a"), advise(&r).expect("b"));
        assert_eq!(a.nodes, b.nodes);
        assert_eq!(a.report, b.report);
        assert_eq!(a.rationale, b.rationale);
    }
}
