use serde::{Deserialize, Serialize};
use specta::Type;

use super::validation::{Issues, ValidationError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum OperatingSystem {
    Linux,
    MacOs,
    Windows,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DiskKind {
    Hdd,
    Ssd,
    Nvme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Virtualization {
    BareMetal,
    Vm,
    Container,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiskProfile {
    pub kind: DiskKind,
    pub iops: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NetworkProfile {
    pub bandwidth_mbps: f64,
    pub latency_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerProfile {
    pub os: OperatingSystem,
    pub cpu_cores: u32,
    pub cpu_ghz: f64,
    pub ram_gb: f64,
    pub disk: DiskProfile,
    pub network: NetworkProfile,
    pub virtualization: Virtualization,
    pub fd_limit: u32,
}

impl Default for ServerProfile {
    fn default() -> Self {
        Self {
            os: OperatingSystem::Linux,
            cpu_cores: 4,
            cpu_ghz: 2.5,
            ram_gb: 8.0,
            disk: DiskProfile {
                kind: DiskKind::Ssd,
                iops: 20_000,
            },
            network: NetworkProfile {
                bandwidth_mbps: 1_000.0,
                latency_ms: 0.5,
            },
            virtualization: Virtualization::Vm,
            fd_limit: 65_536,
        }
    }
}

impl ServerProfile {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        issues.range_u32("cpuCores", self.cpu_cores, 1, 1024);
        issues.range_f64("cpuGhz", self.cpu_ghz, 0.1, 10.0);
        issues.range_f64("ramGb", self.ram_gb, 0.25, 16_384.0);
        issues.range_u32("disk.iops", self.disk.iops, 1, 10_000_000);
        issues.range_f64(
            "network.bandwidthMbps",
            self.network.bandwidth_mbps,
            1.0,
            1_000_000.0,
        );
        issues.range_f64("network.latencyMs", self.network.latency_ms, 0.0, 10_000.0);
        issues.range_u32("fdLimit", self.fd_limit, 16, 10_000_000);
        issues.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_valid() {
        assert!(ServerProfile::default().validate().is_ok());
    }

    #[test]
    fn reports_every_invalid_field_with_path() {
        let mut p = ServerProfile {
            cpu_cores: 0,
            cpu_ghz: f64::NAN,
            ..ServerProfile::default()
        };
        p.disk.iops = 0;
        p.network.latency_ms = -1.0;
        let err = p.validate().expect_err("invalid");
        let fields: Vec<_> = err.issues.iter().map(|i| i.field.as_str()).collect();
        assert_eq!(
            fields,
            ["cpuCores", "cpuGhz", "disk.iops", "network.latencyMs"]
        );
    }

    #[test]
    fn boundaries_are_inclusive() {
        let mut p = ServerProfile {
            cpu_cores: 1024,
            fd_limit: 16,
            ..ServerProfile::default()
        };
        assert!(p.validate().is_ok());
        p.cpu_cores = 1025;
        assert!(p.validate().is_err());
    }
}
