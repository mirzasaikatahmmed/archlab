use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::template::{self, TemplateRef};
use super::validation::{Issues, ValidationError};

pub const MAX_NODES: usize = 200;
pub const MAX_EDGES: usize = 1_000;
pub const MAX_REPLICAS: u32 = 64;

fn one_replica() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum NodeKind {
    Client,
    LoadBalancer,
    AppServer,
    Database,
    Cache,
    Queue,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TopologyNode {
    pub id: String,
    pub kind: NodeKind,
    pub label: String,
    pub position: Position,
    pub template: Option<TemplateRef>,
    /// Per-node overrides of template settings, keyed by setting key.
    pub settings: BTreeMap<String, f64>,
    /// Identical copies of this node behind round-robin; each is a machine of the server profile.
    #[serde(default = "one_replica")]
    pub replicas: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TopologyEdge {
    pub id: String,
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Topology {
    pub nodes: Vec<TopologyNode>,
    pub edges: Vec<TopologyEdge>,
}

impl Topology {
    /// Starter topology for new projects: Client -> AppServer -> Database.
    pub fn starter() -> Self {
        let node = |id: &str, kind, label: &str, x| TopologyNode {
            id: id.to_owned(),
            kind,
            label: label.to_owned(),
            position: Position { x, y: 120.0 },
            template: None,
            settings: BTreeMap::new(),
            replicas: 1,
        };
        let edge = |id: &str, source: &str, target: &str| TopologyEdge {
            id: id.to_owned(),
            source: source.to_owned(),
            target: target.to_owned(),
        };
        Self {
            nodes: vec![
                node("client", NodeKind::Client, "Clients", 0.0),
                node("app", NodeKind::AppServer, "App server", 260.0),
                node("db", NodeKind::Database, "Database", 520.0),
            ],
            edges: vec![
                edge("e-client-app", "client", "app"),
                edge("e-app-db", "app", "db"),
            ],
        }
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        issues.check(
            self.nodes.len() <= MAX_NODES,
            "nodes",
            &format!("at most {MAX_NODES} nodes"),
        );
        issues.check(
            self.edges.len() <= MAX_EDGES,
            "edges",
            &format!("at most {MAX_EDGES} edges"),
        );

        let mut ids = BTreeSet::new();
        for (i, n) in self.nodes.iter().enumerate() {
            let path = format!("nodes.{i}");
            issues.check(
                ids.insert(n.id.as_str()),
                &format!("{path}.id"),
                "duplicate id",
            );
            issues.check(
                !n.id.trim().is_empty(),
                &format!("{path}.id"),
                "must not be empty",
            );
            issues.check(
                !n.label.trim().is_empty() && n.label.chars().count() <= 80,
                &format!("{path}.label"),
                "must be 1-80 characters",
            );
            issues.check(
                n.position.x.is_finite() && n.position.y.is_finite(),
                &format!("{path}.position"),
                "must be finite",
            );
            issues.range_u32(&format!("{path}.replicas"), n.replicas, 1, MAX_REPLICAS);
            validate_node_template(&mut issues, &path, n);
        }

        let mut edge_ids = BTreeSet::new();
        let mut pairs = BTreeSet::new();
        for (i, e) in self.edges.iter().enumerate() {
            let path = format!("edges.{i}");
            issues.check(
                edge_ids.insert(e.id.as_str()),
                &format!("{path}.id"),
                "duplicate id",
            );
            issues.check(
                ids.contains(e.source.as_str()),
                &format!("{path}.source"),
                "unknown node",
            );
            issues.check(
                ids.contains(e.target.as_str()),
                &format!("{path}.target"),
                "unknown node",
            );
            issues.check(e.source != e.target, &format!("{path}.target"), "self-loop");
            issues.check(
                pairs.insert((e.source.as_str(), e.target.as_str())),
                &format!("{path}.target"),
                "duplicate edge",
            );
        }
        issues.finish()
    }
}

fn validate_node_template(issues: &mut Issues, path: &str, node: &TopologyNode) {
    let Some(reference) = &node.template else {
        issues.check(
            node.settings.is_empty(),
            &format!("{path}.settings"),
            "settings require a template",
        );
        return;
    };
    let Some(tpl) = template::find(&reference.id) else {
        issues.push(format!("{path}.template"), "unknown template");
        return;
    };
    issues.check(
        tpl.node_kinds.contains(&node.kind),
        &format!("{path}.template"),
        "template does not support this node kind",
    );
    issues.check(
        reference.version == tpl.version,
        &format!("{path}.template"),
        "template version is outdated",
    );
    issues.nested(path, tpl.validate_overrides(&node.settings, "settings"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_is_valid() {
        assert!(Topology::starter().validate().is_ok());
    }

    #[test]
    fn rejects_duplicate_ids_unknown_refs_self_loops_and_duplicate_edges() {
        let mut t = Topology::starter();
        t.nodes[1].id = "client".to_owned();
        t.edges.push(TopologyEdge {
            id: "x".into(),
            source: "app".into(),
            target: "app".into(),
        });
        t.edges.push(TopologyEdge {
            id: "y".into(),
            source: "ghost".into(),
            target: "db".into(),
        });
        t.edges.push(TopologyEdge {
            id: "z".into(),
            source: "client".into(),
            target: "app".into(),
        });
        let err = t.validate().expect_err("invalid");
        let msgs: Vec<_> = err.issues.iter().map(|i| i.message.as_str()).collect();
        assert!(msgs.contains(&"duplicate id"));
        assert!(msgs.contains(&"self-loop"));
        assert!(msgs.contains(&"unknown node"));
        assert!(msgs.contains(&"duplicate edge"));
    }

    #[test]
    fn template_must_match_kind_and_version() {
        let mut t = Topology::starter();
        t.nodes[2].template = Some(TemplateRef {
            id: "nginx".into(),
            version: 1,
        });
        assert!(t.validate().is_err());
        t.nodes[2].template = Some(TemplateRef {
            id: "postgres".into(),
            version: 99,
        });
        assert!(t.validate().is_err());
        t.nodes[2].template = Some(TemplateRef {
            id: "postgres".into(),
            version: 2,
        });
        t.nodes[2].settings.insert("max_connections".into(), 200.0);
        assert!(t.validate().is_ok());
        t.nodes[2].settings.insert("max_connections".into(), 0.0);
        assert!(t.validate().is_err());
    }

    #[test]
    fn settings_without_template_are_rejected() {
        let mut t = Topology::starter();
        t.nodes[0].settings.insert("x".into(), 1.0);
        assert!(t.validate().is_err());
    }
}
