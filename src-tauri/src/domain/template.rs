use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::topology::NodeKind;
use super::validation::{Issues, ValidationError};

/// Version of the embedded template JSON format.
pub const TEMPLATE_SCHEMA_VERSION: u32 = 1;

/// Add a new template by dropping a JSON file in `templates/` and listing it here.
const EMBEDDED: &[&str] = &[
    include_str!("../../templates/api-gateway.json"),
    include_str!("../../templates/cdn.json"),
    include_str!("../../templates/kafka.json"),
    include_str!("../../templates/memcached.json"),
    include_str!("../../templates/mongodb.json"),
    include_str!("../../templates/mysql.json"),
    include_str!("../../templates/nginx.json"),
    include_str!("../../templates/node.json"),
    include_str!("../../templates/postgres.json"),
    include_str!("../../templates/rabbitmq.json"),
    include_str!("../../templates/redis.json"),
    include_str!("../../templates/worker.json"),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TemplateSetting {
    pub key: String,
    pub label: String,
    pub unit: String,
    pub default: f64,
    pub min: f64,
    pub max: f64,
    pub description: String,
}

/// How a node running this template behaves in the simulation.
///
/// Service times are given for a reference machine (see `sim::model`) and
/// scaled by the server profile. All values are estimates, not measurements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SimParams {
    /// Mean service time of a read on the reference machine, in milliseconds.
    pub base_service_ms: f64,
    /// Share of the service time spent on CPU (the rest is I/O), 0..=1.
    pub cpu_fraction: f64,
    /// Lognormal shape: larger values give a heavier latency tail.
    pub sigma: f64,
    /// Service-time factor for writes relative to reads.
    pub write_multiplier: f64,
    /// Setting that gives the number of parallel workers; the core count when absent.
    #[serde(default)]
    pub concurrency_setting: Option<String>,
    /// Workers contributed by each unit of `concurrency_setting`.
    pub concurrency_per_unit: f64,
    /// Setting that gives the waiting-room size; `default_queue` when absent.
    #[serde(default)]
    pub queue_setting: Option<String>,
    pub default_queue: u32,
    /// Setting holding the cache hit ratio (cache nodes only).
    #[serde(default)]
    pub hit_ratio_setting: Option<String>,
    /// Setting holding a gateway's request limit per second; 0 means unlimited (gateways only).
    #[serde(default)]
    pub rate_limit_setting: Option<String>,
    /// Setting holding how many messages a queue may hold (queues with workers only).
    #[serde(default)]
    pub backlog_setting: Option<String>,
}

impl SimParams {
    fn validate(&self, settings: &[TemplateSetting]) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        issues.range_f64("baseServiceMs", self.base_service_ms, 0.001, 100_000.0);
        issues.range_f64("cpuFraction", self.cpu_fraction, 0.01, 1.0);
        issues.range_f64("sigma", self.sigma, 0.0, 3.0);
        issues.range_f64("writeMultiplier", self.write_multiplier, 0.1, 100.0);
        issues.range_f64(
            "concurrencyPerUnit",
            self.concurrency_per_unit,
            0.1,
            10_000.0,
        );
        issues.range_u32("defaultQueue", self.default_queue, 0, 10_000_000);
        for (field, key) in [
            ("concurrencySetting", &self.concurrency_setting),
            ("queueSetting", &self.queue_setting),
            ("hitRatioSetting", &self.hit_ratio_setting),
            ("rateLimitSetting", &self.rate_limit_setting),
            ("backlogSetting", &self.backlog_setting),
        ] {
            if let Some(key) = key {
                issues.check(
                    settings.iter().any(|s| &s.key == key),
                    field,
                    "refers to an unknown setting",
                );
            }
        }
        issues.finish()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfigTemplate {
    pub schema_version: u32,
    pub id: String,
    /// Bumped when the template's settings change; nodes record the version they used.
    pub version: u32,
    pub name: String,
    pub description: String,
    pub node_kinds: Vec<NodeKind>,
    pub settings: Vec<TemplateSetting>,
    /// Simulation behaviour; nodes without it fall back to per-kind defaults.
    #[serde(default)]
    pub sim: Option<SimParams>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TemplateRef {
    pub id: String,
    pub version: u32,
}

impl ConfigTemplate {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        issues.check(
            self.schema_version == TEMPLATE_SCHEMA_VERSION,
            "schemaVersion",
            "unsupported template schema version",
        );
        issues.check(!self.id.trim().is_empty(), "id", "must not be empty");
        issues.check(self.version >= 1, "version", "must be at least 1");
        issues.check(
            !self.node_kinds.is_empty(),
            "nodeKinds",
            "must not be empty",
        );
        let mut seen = std::collections::BTreeSet::new();
        for (i, s) in self.settings.iter().enumerate() {
            let path = format!("settings.{i}");
            issues.check(seen.insert(&s.key), &format!("{path}.key"), "duplicate key");
            issues.check(
                s.min.is_finite() && s.max.is_finite() && s.min <= s.max,
                &format!("{path}.min"),
                "min must be <= max",
            );
            issues.check(
                s.default >= s.min && s.default <= s.max,
                &format!("{path}.default"),
                "default must be within min..max",
            );
        }
        if let Some(sim) = &self.sim {
            issues.nested("sim", sim.validate(&self.settings));
        }
        issues.finish()
    }

    /// Checks node-level overrides against this template's settings.
    pub fn validate_overrides(
        &self,
        overrides: &BTreeMap<String, f64>,
        field: &str,
    ) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        for (key, value) in overrides {
            match self.settings.iter().find(|s| &s.key == key) {
                None => issues.push(
                    format!("{field}.{key}"),
                    "unknown setting for this template",
                ),
                Some(s) => issues.range_f64(&format!("{field}.{key}"), *value, s.min, s.max),
            }
        }
        issues.finish()
    }
}

/// All built-in templates, parsed and validated once.
///
/// # Panics
/// Panics if an embedded template is malformed; unit tests guard against that.
pub fn builtin() -> &'static [ConfigTemplate] {
    static TEMPLATES: OnceLock<Vec<ConfigTemplate>> = OnceLock::new();
    TEMPLATES.get_or_init(|| {
        EMBEDDED
            .iter()
            .map(|raw| {
                let t: ConfigTemplate =
                    serde_json::from_str(raw).expect("embedded template is valid JSON");
                t.validate().expect("embedded template is valid");
                t
            })
            .collect()
    })
}

pub fn find(id: &str) -> Option<&'static ConfigTemplate> {
    builtin().iter().find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_embedded_templates_parse_and_validate() {
        let ids: Vec<_> = builtin().iter().map(|t| t.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "api-gateway",
                "cdn",
                "kafka",
                "memcached",
                "mongodb",
                "mysql",
                "nginx",
                "node",
                "postgres",
                "rabbitmq",
                "redis",
                "worker"
            ]
        );
    }

    #[test]
    fn every_server_node_type_has_a_template_to_configure_it() {
        use super::NodeKind::*;
        for kind in [
            LoadBalancer,
            AppServer,
            Database,
            Cache,
            Queue,
            ApiGateway,
            Cdn,
            Worker,
        ] {
            assert!(
                builtin().iter().any(|t| t.node_kinds.contains(&kind)),
                "no template for {kind:?}"
            );
        }
    }

    #[test]
    fn template_ids_are_unique() {
        let mut ids: Vec<_> = builtin().iter().map(|t| &t.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), builtin().len());
    }

    #[test]
    fn overrides_are_checked_against_ranges_and_keys() {
        let t = find("nginx").expect("nginx exists");
        let ok = BTreeMap::from([("worker_processes".to_owned(), 8.0)]);
        assert!(t.validate_overrides(&ok, "settings").is_ok());
        let bad = BTreeMap::from([
            ("worker_processes".to_owned(), 0.0),
            ("nope".to_owned(), 1.0),
        ]);
        let err = t.validate_overrides(&bad, "settings").expect_err("invalid");
        assert_eq!(err.issues.len(), 2);
    }

    #[test]
    fn rejects_default_outside_range() {
        let mut t = find("redis").expect("redis exists").clone();
        t.settings[0].default = t.settings[0].max + 1.0;
        assert!(t.validate().is_err());
    }
}
