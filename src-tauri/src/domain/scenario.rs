//! Guided scenarios: a starting design plus ordered steps, each of which changes the design,
//! explains what happens and says how to play it. Scenarios are embedded, versioned JSON.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use super::chaos::{ChaosPlan, ClientPolicy, Fault};
use super::project::Project;
use super::server_profile::ServerProfile;
use super::topology::Topology;
use super::traffic::TrafficProfile;
use super::validation::{Issues, ValidationError};
use super::whatif::{Change, Slo, WhatIf, apply_changes};

pub const SCENARIO_SCHEMA_VERSION: u32 = 1;

/// Add a scenario by dropping a JSON file in `scenarios/` and listing it here.
const EMBEDDED: &[&str] = &[
    include_str!("../../scenarios/one-machine.json"),
    include_str!("../../scenarios/cache-in-front.json"),
    include_str!("../../scenarios/traffic-spike.json"),
    include_str!("../../scenarios/retry-storm.json"),
    include_str!("../../scenarios/latency-not-capacity.json"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Beginner,
    Intermediate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PlayMode {
    /// Streamed on the Stage as it happens.
    Live,
    /// Run to the end and show the report.
    Instant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Play {
    pub mode: PlayMode,
    /// Simulated seconds to run.
    pub duration_s: f64,
    /// Live speed: simulated seconds per real second; absent runs as fast as possible.
    #[serde(default)]
    pub speed: Option<f64>,
}

/// A change a step makes to the design it inherits from the previous step.
#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StepChange {
    Apply {
        change: Change,
    },
    AddFault {
        fault: Fault,
    },
    SetPolicy {
        policy: ClientPolicy,
    },
    /// Replaces the whole traffic profile, for example to add a spike.
    SetTraffic {
        traffic: TrafficProfile,
    },
}

/// A claim the narration makes, checked by tests so the story stays true.
#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Expectation {
    MeetsTargets {
        value: bool,
    },
    ErrorRateAbove {
        value: f64,
    },
    ErrorRateBelow {
        value: f64,
    },
    P95AboveMs {
        value: f64,
    },
    P95BelowMs {
        value: f64,
    },
    /// The report names this node as the bottleneck; absent means there is none.
    Bottleneck {
        node: Option<String>,
    },
    UtilizationAbove {
        node: String,
        value: f64,
    },
    UtilizationBelow {
        node: String,
        value: f64,
    },
    /// Seconds the first injected fault took to recover from; one that never did counts as infinite.
    RecoveryAboveS {
        value: f64,
    },
    RecoveryBelowS {
        value: f64,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepDef {
    pub title: String,
    pub narration: Vec<String>,
    #[serde(default)]
    pub highlight: Vec<String>,
    /// Replaces the topology from this step on.
    #[serde(default)]
    pub topology: Option<Topology>,
    #[serde(default)]
    pub changes: Vec<StepChange>,
    pub play: Play,
    #[serde(default)]
    pub expect: Vec<Expectation>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioProject {
    pub name: String,
    pub server_profile: ServerProfile,
    pub traffic_profile: TrafficProfile,
    pub topology: Topology,
    #[serde(default)]
    pub chaos: ChaosPlan,
    #[serde(default)]
    pub slo: Slo,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioDef {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub summary: String,
    pub level: Level,
    pub minutes: u32,
    pub project: ScenarioProject,
    pub steps: Vec<StepDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioSummary {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub level: Level,
    pub minutes: u32,
    pub step_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StepView {
    pub title: String,
    pub narration: Vec<String>,
    pub highlight: Vec<String>,
    /// What this step changes, in words.
    pub changes: Vec<String>,
    pub play: Play,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioView {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub level: Level,
    pub minutes: u32,
    pub steps: Vec<StepView>,
}

impl ScenarioDef {
    fn base_project(&self) -> Project {
        let p = &self.project;
        Project {
            id: Uuid::new_v4().to_string(),
            name: p.name.clone(),
            server_profile: p.server_profile.clone(),
            traffic_profile: p.traffic_profile.clone(),
            topology: p.topology.clone(),
            chaos: p.chaos.clone(),
            whatif: WhatIf {
                slo: p.slo.clone(),
                variants: Vec::new(),
            },
            created_at: 0.0,
            updated_at: 0.0,
        }
    }

    fn apply_step(project: &mut Project, step: &StepDef) -> Result<(), ValidationError> {
        if let Some(topology) = &step.topology {
            project.topology = topology.clone();
        }
        for change in &step.changes {
            match change {
                StepChange::Apply { change } => {
                    *project = apply_changes(project, std::slice::from_ref(change))?;
                }
                StepChange::AddFault { fault } => project.chaos.faults.push(fault.clone()),
                StepChange::SetPolicy { policy } => project.chaos.policy = policy.clone(),
                StepChange::SetTraffic { traffic } => project.traffic_profile = traffic.clone(),
            }
        }
        Ok(())
    }

    /// The design as it stands after `step` (counting from 0), with every earlier step applied.
    pub fn project_at(&self, step: usize) -> Result<Project, ValidationError> {
        let mut project = self.base_project();
        let steps = self.steps.get(..=step).ok_or_else(|| {
            let mut issues = Issues::default();
            issues.push("step", "no such step");
            issues.finish().expect_err("an issue was pushed")
        })?;
        for s in steps {
            Self::apply_step(&mut project, s)?;
        }
        project.name = format!("{} (step {})", self.title, step + 1);
        project.validate()?;
        Ok(project)
    }

    fn describe_changes(step: &StepDef, after: &Project) -> Vec<String> {
        let mut out = Vec::new();
        if step.topology.is_some() {
            out.push("Rebuilt the topology".to_owned());
        }
        for change in &step.changes {
            out.push(match change {
                StepChange::Apply { change } => change.describe(after),
                StepChange::AddFault { fault } => fault.describe(&after.topology),
                StepChange::SetPolicy { policy } => format!(
                    "Clients wait {} ms and retry up to {} times after {} ms",
                    policy.timeout_ms, policy.max_retries, policy.backoff_ms
                ),
                StepChange::SetTraffic { traffic } => {
                    format!(
                        "Traffic {} req/s{}",
                        traffic.requests_per_second,
                        spike_note(traffic)
                    )
                }
            });
        }
        out
    }

    pub fn summary(&self) -> ScenarioSummary {
        ScenarioSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            summary: self.summary.clone(),
            level: self.level,
            minutes: self.minutes,
            step_count: u32::try_from(self.steps.len()).unwrap_or(u32::MAX),
        }
    }

    pub fn view(&self) -> Result<ScenarioView, ValidationError> {
        let mut steps = Vec::with_capacity(self.steps.len());
        for (i, s) in self.steps.iter().enumerate() {
            let after = self.project_at(i)?;
            steps.push(StepView {
                title: s.title.clone(),
                narration: s.narration.clone(),
                highlight: s.highlight.clone(),
                changes: Self::describe_changes(s, &after),
                play: s.play.clone(),
            });
        }
        Ok(ScenarioView {
            id: self.id.clone(),
            title: self.title.clone(),
            summary: self.summary.clone(),
            level: self.level,
            minutes: self.minutes,
            steps,
        })
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        issues.check(
            self.schema_version == SCENARIO_SCHEMA_VERSION,
            "schemaVersion",
            "unsupported scenario schema version",
        );
        issues.check(!self.id.trim().is_empty(), "id", "must not be empty");
        issues.check(!self.title.trim().is_empty(), "title", "must not be empty");
        issues.check(!self.steps.is_empty(), "steps", "needs at least one step");
        for (i, s) in self.steps.iter().enumerate() {
            let at = format!("steps.{i}");
            issues.check(
                !s.narration.is_empty() && s.narration.iter().all(|p| !p.trim().is_empty()),
                &format!("{at}.narration"),
                "needs at least one non-empty paragraph",
            );
            issues.range_f64(
                &format!("{at}.play.durationS"),
                s.play.duration_s,
                10.0,
                3_600.0,
            );
            if let Some(speed) = s.play.speed {
                issues.range_f64(&format!("{at}.play.speed"), speed, 0.1, 1_000.0);
            }
            match self.project_at(i) {
                Err(err) => issues.nested(&at, Err(err)),
                Ok(project) => {
                    for node in &s.highlight {
                        issues.check(
                            project.topology.nodes.iter().any(|n| &n.id == node),
                            &format!("{at}.highlight"),
                            "refers to a node that does not exist in this step",
                        );
                    }
                }
            }
        }
        issues.finish()
    }
}

fn spike_note(t: &TrafficProfile) -> String {
    use super::traffic::SpikePattern;
    match &t.spike {
        SpikePattern::None => String::new(),
        SpikePattern::Step {
            at_s,
            multiplier,
            duration_s,
        } => format!(", {multiplier}\u{d7} for {duration_s} s from {at_s} s"),
        SpikePattern::Periodic {
            period_s,
            multiplier,
            duration_s,
        } => format!(", {multiplier}\u{d7} for {duration_s} s every {period_s} s"),
    }
}

/// All built-in scenarios, parsed and validated once.
///
/// # Panics
/// Panics if an embedded scenario is malformed; unit tests guard against that.
pub fn builtin() -> &'static [ScenarioDef] {
    static SCENARIOS: OnceLock<Vec<ScenarioDef>> = OnceLock::new();
    SCENARIOS.get_or_init(|| {
        let all: Vec<ScenarioDef> = EMBEDDED
            .iter()
            .map(|raw| {
                let s: ScenarioDef =
                    serde_json::from_str(raw).expect("embedded scenario is valid JSON");
                s.validate().expect("embedded scenario is valid");
                s
            })
            .collect();
        let ids: BTreeSet<_> = all.iter().map(|s| &s.id).collect();
        assert_eq!(ids.len(), all.len(), "scenario ids must be unique");
        all
    })
}

pub fn find(id: &str) -> Option<&'static ScenarioDef> {
    builtin().iter().find(|s| s.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::SimOptions;
    use crate::sim::report::SimReport;
    use crate::whatif::misses;

    fn options(step: &StepDef) -> SimOptions {
        SimOptions {
            seed: 1,
            duration_s: step.play.duration_s,
            warmup_s: 5.0,
        }
    }

    fn check(e: &Expectation, report: &SimReport, slo: &Slo) -> Option<String> {
        let s = &report.summary;
        let node = |id: &str| report.nodes.iter().find(|n| n.id == id);
        let fail = |what: String| Some(what);
        match e {
            Expectation::MeetsTargets { value } => {
                let meets = misses(s, slo).is_empty();
                (meets != *value).then(|| format!("meets targets: expected {value}, got {meets}"))
            }
            Expectation::ErrorRateAbove { value } => (s.error_rate <= *value)
                .then(|| format!("error rate {:.3} should be above {value}", s.error_rate)),
            Expectation::ErrorRateBelow { value } => (s.error_rate >= *value)
                .then(|| format!("error rate {:.3} should be below {value}", s.error_rate)),
            Expectation::P95AboveMs { value } => (s.latency.p95_ms <= *value)
                .then(|| format!("p95 {:.1} ms should be above {value}", s.latency.p95_ms)),
            Expectation::P95BelowMs { value } => (s.latency.p95_ms >= *value)
                .then(|| format!("p95 {:.1} ms should be below {value}", s.latency.p95_ms)),
            Expectation::Bottleneck { node: want } => {
                let got = report.bottleneck.as_ref().map(|b| b.node_id.clone());
                (got != *want).then(|| format!("bottleneck: expected {want:?}, got {got:?}"))
            }
            Expectation::UtilizationAbove { node: id, value } => match node(id) {
                Some(n) if n.utilization > *value => None,
                Some(n) => fail(format!(
                    "{id} utilization {:.2} should be above {value}",
                    n.utilization
                )),
                None => fail(format!("no node {id}")),
            },
            Expectation::RecoveryAboveS { value } => {
                let got = report.faults.first().and_then(|f| f.recovery_s);
                got.is_some_and(|r| r <= *value)
                    .then(|| format!("recovery {got:?} s should be above {value}"))
            }
            Expectation::RecoveryBelowS { value } => {
                let got = report.faults.first().and_then(|f| f.recovery_s);
                got.is_none_or(|r| r >= *value)
                    .then(|| format!("recovery {got:?} s should be below {value}"))
            }
            Expectation::UtilizationBelow { node: id, value } => match node(id) {
                Some(n) if n.utilization < *value => None,
                Some(n) => fail(format!(
                    "{id} utilization {:.2} should be below {value}",
                    n.utilization
                )),
                None => fail(format!("no node {id}")),
            },
        }
    }

    fn run_step(def: &ScenarioDef, i: usize) -> SimReport {
        let project = def.project_at(i).expect("step project");
        crate::sim::run(&project, &options(&def.steps[i])).expect("run")
    }

    #[test]
    fn every_embedded_scenario_is_valid_and_viewable() {
        assert_eq!(builtin().len(), EMBEDDED.len());
        for def in builtin() {
            let view = def.view().expect("view");
            assert_eq!(view.steps.len(), def.steps.len());
            assert_eq!(def.summary().step_count as usize, def.steps.len());
            assert!(find(&def.id).is_some());
        }
    }

    #[test]
    fn step_views_describe_what_changed() {
        let def = find("one-machine").expect("scenario");
        let view = def.view().expect("view");
        assert!(view.steps[0].changes.is_empty());
        assert_eq!(view.steps[1].changes, ["Traffic 450 req/s"]);
        assert!(
            view.steps[4]
                .changes
                .iter()
                .any(|c| c.starts_with("Crash of"))
        );
        assert!(
            view.steps[5]
                .changes
                .contains(&"Rebuilt the topology".to_owned())
        );
        assert!(view.steps[6].changes[0].contains("retry up to 3 times"));
    }

    #[test]
    fn steps_build_on_each_other_and_projects_are_fresh() {
        let def = find("one-machine").expect("scenario");
        let first = def.project_at(0).expect("step 1");
        let sixth = def.project_at(5).expect("step 6");
        assert_eq!(first.topology.nodes.len(), 2);
        assert_eq!(sixth.topology.nodes.len(), 5);
        // The crash added in step 5 is still in effect in step 6.
        assert_eq!(sixth.chaos.faults.len(), 1);
        assert_ne!(first.id, def.project_at(0).expect("again").id);
        assert!(first.name.ends_with("(step 1)"));
        assert!(def.project_at(99).is_err());
    }

    #[test]
    fn expected_properties_of_the_embedded_set() {
        for def in builtin() {
            assert!((3..=8).contains(&def.steps.len()), "{}", def.id);
            for step in &def.steps {
                assert!(step.narration.len() >= 2, "{}", step.title);
                assert!(
                    !step.expect.is_empty(),
                    "{} has no claims to check",
                    step.title
                );
            }
        }
    }

    #[test]
    fn every_narration_claim_holds_for_every_step() {
        let mut failures = Vec::new();
        for def in builtin() {
            for (i, step) in def.steps.iter().enumerate() {
                let report = run_step(def, i);
                for e in &step.expect {
                    if let Some(why) = check(e, &report, &def.project.slo) {
                        failures.push(format!("{} step {} ({}): {why}", def.id, i + 1, step.title));
                    }
                }
            }
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    /// Prints each step's numbers; run with `cargo test print_scenario_steps -- --ignored --nocapture`.
    #[test]
    #[ignore = "diagnostic output for tuning scenarios"]
    fn print_scenario_steps() {
        for def in builtin() {
            for (i, step) in def.steps.iter().enumerate() {
                let r = run_step(def, i);
                let s = &r.summary;
                let nodes: Vec<String> = r
                    .nodes
                    .iter()
                    .map(|n| format!("{}={:.2}", n.id, n.utilization))
                    .collect();
                for f in &r.faults {
                    println!(
                        "    fault {:<40} during err={:.3} p95={:.0} after err={:.3} recovery={:?}",
                        f.label,
                        f.during.error_rate,
                        f.during.p95_ms,
                        f.after.error_rate,
                        f.recovery_s
                    );
                }
                println!(
                    "{:<22} {} {:<34} meets={} p95={:>7.1} err={:.3} rps={:>6.1} bn={:?} [{}] retries={}",
                    def.id,
                    i + 1,
                    step.title,
                    misses(s, &def.project.slo).is_empty(),
                    s.latency.p95_ms,
                    s.error_rate,
                    s.throughput_rps,
                    r.bottleneck.as_ref().map(|b| b.node_id.as_str()),
                    nodes.join(" "),
                    s.retries,
                );
            }
        }
    }
}
