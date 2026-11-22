//! Blueprints: ready-made architectures for a kind of product, each in sizes by number of users.
//! A blueprint is embedded, versioned JSON; a size ("tier") sets the traffic and how many copies
//! of each node there are. Tests simulate every tier and fail if it misses the blueprint's targets,
//! so "this design handles 1M users" is a checked claim, not a label.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use super::project::Project;
use super::server_profile::ServerProfile;
use super::sizing::peak_rps;
use super::topology::Topology;
use super::traffic::{SpikePattern, TrafficProfile};
use super::validation::{Issues, ValidationError};
use super::whatif::{Slo, WhatIf};

pub const BLUEPRINT_SCHEMA_VERSION: u32 = 1;

/// Add a blueprint by dropping a JSON file in `blueprints/` and listing it here.
const EMBEDDED: &[&str] = &[
    include_str!("../../blueprints/simple-web.json"),
    include_str!("../../blueprints/read-heavy-site.json"),
    include_str!("../../blueprints/async-processing.json"),
    include_str!("../../blueprints/microservices.json"),
    include_str!("../../blueprints/write-heavy-ingestion.json"),
    include_str!("../../blueprints/media-and-files.json"),
    include_str!("../../blueprints/million-users.json"),
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TierDef {
    pub id: String,
    pub label: String,
    pub daily_users: u32,
    /// Copies of each node (by node id); nodes not listed keep one.
    #[serde(default)]
    pub replicas: BTreeMap<String, u32>,
    /// Setting overrides per node id.
    #[serde(default)]
    pub settings: BTreeMap<String, BTreeMap<String, f64>>,
    /// Simulated seconds that fit the simulator's event limit for this size.
    pub run_s: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintDef {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub summary: String,
    pub requests_per_user_per_day: f64,
    pub peak_factor: f64,
    pub read_ratio: f64,
    pub slo: Slo,
    pub server_profile: ServerProfile,
    pub topology: Topology,
    pub tiers: Vec<TierDef>,
    /// Why the design looks like this, one plain sentence each.
    pub why: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TierView {
    pub id: String,
    pub label: String,
    pub daily_users: u32,
    /// Requests per second at the busiest moment.
    pub peak_rps: f64,
    pub run_s: f64,
    /// Number of machines (every copy of every node except clients).
    pub machines: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintView {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub requests_per_user_per_day: f64,
    pub peak_factor: f64,
    pub read_ratio: f64,
    pub slo: Slo,
    pub tiers: Vec<TierView>,
    pub why: Vec<String>,
    pub topology: Topology,
}

impl BlueprintDef {
    fn tier(&self, id: &str) -> Option<&TierDef> {
        self.tiers.iter().find(|t| t.id == id)
    }

    pub(crate) fn peak(&self, tier: &TierDef) -> f64 {
        peak_rps(
            f64::from(tier.daily_users),
            self.requests_per_user_per_day,
            self.peak_factor,
        )
    }

    /// The design at one size, as a new unsaved project.
    pub fn project(&self, tier_id: &str) -> Result<Project, ValidationError> {
        let tier = self.tier(tier_id).ok_or_else(|| {
            let mut issues = Issues::default();
            issues.push("tier", format!("unknown size \"{tier_id}\""));
            issues.finish().expect_err("an issue was added")
        })?;
        let mut topology = self.topology.clone();
        for node in &mut topology.nodes {
            if let Some(count) = tier.replicas.get(&node.id) {
                node.replicas = *count;
            }
            if let Some(settings) = tier.settings.get(&node.id) {
                node.settings.extend(settings.clone());
            }
        }
        let project = Project {
            id: Uuid::new_v4().to_string(),
            name: format!("{} ({})", self.title, tier.label),
            server_profile: self.server_profile.clone(),
            traffic_profile: TrafficProfile {
                concurrent_users: (tier.daily_users / 10).max(1),
                requests_per_second: self.peak(tier),
                read_ratio: self.read_ratio,
                spike: SpikePattern::None,
            },
            topology,
            chaos: Default::default(),
            whatif: WhatIf {
                slo: self.slo.clone(),
                variants: Vec::new(),
            },
            created_at: 0.0,
            updated_at: 0.0,
        };
        project.validate()?;
        Ok(project)
    }

    pub fn view(&self) -> BlueprintView {
        BlueprintView {
            id: self.id.clone(),
            title: self.title.clone(),
            summary: self.summary.clone(),
            requests_per_user_per_day: self.requests_per_user_per_day,
            peak_factor: self.peak_factor,
            read_ratio: self.read_ratio,
            slo: self.slo.clone(),
            tiers: self
                .tiers
                .iter()
                .map(|t| TierView {
                    id: t.id.clone(),
                    label: t.label.clone(),
                    daily_users: t.daily_users,
                    peak_rps: self.peak(t),
                    run_s: t.run_s,
                    machines: self.machines(t),
                })
                .collect(),
            why: self.why.clone(),
            topology: self.topology.clone(),
        }
    }

    fn machines(&self, tier: &TierDef) -> u32 {
        use super::topology::NodeKind;
        self.topology
            .nodes
            .iter()
            .filter(|n| n.kind != NodeKind::Client)
            .map(|n| tier.replicas.get(&n.id).copied().unwrap_or(n.replicas))
            .sum()
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        issues.check(
            self.schema_version == BLUEPRINT_SCHEMA_VERSION,
            "schemaVersion",
            "unsupported blueprint schema version",
        );
        issues.check(!self.id.trim().is_empty(), "id", "must not be empty");
        issues.check(!self.title.trim().is_empty(), "title", "must not be empty");
        issues.check(!self.why.is_empty(), "why", "explain the design");
        issues.check(!self.tiers.is_empty(), "tiers", "needs at least one size");
        issues.range_f64(
            "requestsPerUserPerDay",
            self.requests_per_user_per_day,
            0.01,
            100_000.0,
        );
        issues.range_f64("peakFactor", self.peak_factor, 1.0, 100.0);
        issues.range_f64("readRatio", self.read_ratio, 0.0, 1.0);
        let mut seen = BTreeSet::new();
        for (i, tier) in self.tiers.iter().enumerate() {
            let at = format!("tiers.{i}");
            issues.check(
                seen.insert(tier.id.as_str()),
                &format!("{at}.id"),
                "must be unique",
            );
            issues.range_f64(&format!("{at}.runS"), tier.run_s, 10.0, 3_600.0);
            for key in tier.replicas.keys().chain(tier.settings.keys()) {
                issues.check(
                    self.topology.nodes.iter().any(|n| &n.id == key),
                    &format!("{at}.replicas"),
                    "refers to a node that does not exist",
                );
            }
            issues.nested(&at, self.project(&tier.id).map(|_| ()));
        }
        issues.finish()
    }
}

/// All built-in blueprints, parsed and validated once.
///
/// # Panics
/// Panics if an embedded blueprint is malformed; unit tests guard against that.
pub fn builtin() -> &'static [BlueprintDef] {
    static BLUEPRINTS: OnceLock<Vec<BlueprintDef>> = OnceLock::new();
    BLUEPRINTS.get_or_init(|| {
        let all: Vec<BlueprintDef> = EMBEDDED
            .iter()
            .map(|raw| {
                let b: BlueprintDef =
                    serde_json::from_str(raw).expect("embedded blueprint is valid JSON");
                b.validate().expect("embedded blueprint is valid");
                b
            })
            .collect();
        let ids: BTreeSet<_> = all.iter().map(|b| &b.id).collect();
        assert_eq!(ids.len(), all.len(), "blueprint ids must be unique");
        all
    })
}

pub fn find(id: &str) -> Option<&'static BlueprintDef> {
    builtin().iter().find(|b| b.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{SimOptions, run};
    use crate::whatif::misses;

    fn simulate(b: &BlueprintDef, t: &TierDef) -> crate::sim::SimReport {
        let project = b.project(&t.id).expect("project");
        let options = SimOptions {
            seed: 1,
            duration_s: t.run_s,
            warmup_s: 5.0,
        };
        run(&project, &options).expect("run")
    }

    #[test]
    fn every_embedded_blueprint_is_valid_and_viewable() {
        assert_eq!(builtin().len(), 7);
        for b in builtin() {
            let view = b.view();
            assert!(!view.tiers.is_empty() && !view.why.is_empty(), "{}", b.id);
            assert!(
                view.tiers
                    .iter()
                    .all(|t| t.peak_rps > 0.0 && t.machines >= 1)
            );
            for t in &b.tiers {
                let p = b.project(&t.id).expect("project");
                assert!(p.name.contains(&t.label));
                assert!((p.traffic_profile.requests_per_second - b.peak(t)).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn tiers_apply_copies_and_settings_without_touching_the_blueprint() {
        let b = find("million-users").expect("blueprint");
        let small = b.project("1m").expect("1m");
        let big = b.project("10m").expect("10m");
        let app = |p: &Project| {
            p.topology
                .nodes
                .iter()
                .find(|n| n.id == "app")
                .expect("app")
                .replicas
        };
        assert!(app(&big) > app(&small));
        assert_eq!(
            b.topology
                .nodes
                .iter()
                .find(|n| n.id == "app")
                .expect("app")
                .replicas,
            1
        );
        assert!(b.project("nope").is_err());
        assert_ne!(small.id, big.id);
    }

    #[test]
    fn every_tier_meets_its_targets_with_headroom() {
        for b in builtin() {
            for t in &b.tiers {
                let r = simulate(b, t);
                let label = format!("{} / {}", b.id, t.id);
                assert!(
                    misses(&r.summary, &b.slo).is_empty(),
                    "{label} misses its targets: {:?}",
                    r.summary
                );
                for n in &r.nodes {
                    assert!(
                        n.utilization < 0.85 && n.rejected == 0 && n.timed_out == 0,
                        "{label}: {} is {:.0}% busy ({} rejected)",
                        n.id,
                        n.utilization * 100.0,
                        n.rejected
                    );
                    if let Some(backlog) = &n.backlog {
                        assert!(
                            backlog.left < 200,
                            "{label}: {} messages still waiting at {}",
                            backlog.left,
                            n.id
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_design_cut_to_one_copy_per_node_no_longer_copes_with_a_million_users() {
        let b = find("million-users").expect("blueprint");
        let mut starved = b.clone();
        starved.tiers[0].replicas.clear();
        starved.tiers[0].daily_users = 100_000_000;
        starved.tiers[0].run_s = 10.0;
        let r = simulate(&starved, &starved.tiers[0]);
        assert!(
            !misses(&r.summary, &starved.slo).is_empty()
                || r.nodes.iter().any(|n| n.utilization > 0.85),
            "a single copy of every node should not handle a hundred million users"
        );
    }

    #[test]
    fn the_million_user_design_really_is_for_a_million_users() {
        let b = find("million-users").expect("blueprint");
        let tier = b.tiers.iter().find(|t| t.id == "1m").expect("1m");
        assert_eq!(tier.daily_users, 1_000_000);
        let peak = b.peak(tier);
        assert!((1_000.0..1_300.0).contains(&peak), "{peak}");
    }

    #[test]
    #[ignore = "diagnostic: cargo test print_blueprints -- --ignored --nocapture"]
    fn print_blueprints() {
        for b in builtin() {
            for t in &b.tiers {
                let r = simulate(b, t);
                let meets = misses(&r.summary, &b.slo).is_empty();
                eprintln!(
                    "{} {} rps={:.0} meets={} p95={:.0} err={:.4} bottleneck={:?}",
                    b.id,
                    t.id,
                    b.peak(t),
                    meets,
                    r.summary.latency.p95_ms,
                    r.summary.error_rate,
                    r.bottleneck.as_ref().map(|x| (&x.node_id, &x.reason))
                );
                let busy: Vec<String> = r
                    .nodes
                    .iter()
                    .map(|n| format!("{}={:.2}", n.id, n.utilization))
                    .collect();
                eprintln!("    {}", busy.join(" "));
            }
        }
    }
}
