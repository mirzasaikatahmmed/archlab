use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;
use sysinfo::{DiskKind as SysDiskKind, Disks, System};

use crate::domain::server_profile::{
    DiskKind, DiskProfile, NetworkProfile, OperatingSystem, ServerProfile, Virtualization,
};

const DEFAULT_CPU_GHZ: f64 = 2.5;
const DEFAULT_FD_LIMIT: u32 = 1_024;

/// A detected profile plus notes about values that were guessed, not measured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DetectedProfile {
    pub profile: ServerProfile,
    pub notes: Vec<String>,
}

/// Raw facts gathered from the host; kept separate so mapping is unit-testable.
#[derive(Debug, Clone, Default)]
pub struct HostSnapshot {
    pub os: String,
    pub logical_cpus: usize,
    pub cpu_mhz: u64,
    pub cpu_brand: String,
    pub total_memory_bytes: u64,
    pub disk_kind: Option<DiskKind>,
    pub fd_limit: Option<u32>,
    pub in_container: bool,
}

pub fn detect() -> DetectedProfile {
    profile_from_snapshot(&gather())
}

fn gather() -> HostSnapshot {
    let mut snap = HostSnapshot {
        os: std::env::consts::OS.to_owned(),
        fd_limit: linux_fd_limit(),
        in_container: in_container(),
        ..HostSnapshot::default()
    };
    if let Ok(mut sys) = System::new() {
        sys.refresh_cpu_all();
        sys.refresh_memory();
        let cpus = sys.cpus();
        snap.logical_cpus = cpus.len();
        snap.cpu_mhz = cpus.iter().map(sysinfo::Cpu::frequency).max().unwrap_or(0);
        snap.cpu_brand = cpus
            .first()
            .map(|c| c.brand().to_owned())
            .unwrap_or_default();
        snap.total_memory_bytes = sys.total_memory();
    }
    if let Ok(disks) = Disks::new_with_refreshed_list() {
        snap.disk_kind = disks
            .iter()
            .filter(|d| !d.is_removable())
            .max_by_key(|d| d.total_space())
            .and_then(|d| match d.kind() {
                SysDiskKind::HDD => Some(DiskKind::Hdd),
                SysDiskKind::SSD => Some(DiskKind::Ssd),
                SysDiskKind::Unknown(_) => None,
            });
    }
    snap
}

fn linux_fd_limit() -> Option<u32> {
    let limits = fs::read_to_string("/proc/self/limits").ok()?;
    let line = limits.lines().find(|l| l.starts_with("Max open files"))?;
    line.split_whitespace().nth(3)?.parse().ok()
}

fn in_container() -> bool {
    Path::new("/.dockerenv").exists()
        || fs::read_to_string("/proc/1/cgroup").is_ok_and(|c| {
            ["docker", "kubepods", "containerd"]
                .iter()
                .any(|k| c.contains(k))
        })
}

fn round(value: f64, decimals: i32) -> f64 {
    let factor = 10f64.powi(decimals);
    (value * factor).round() / factor
}

pub fn profile_from_snapshot(snap: &HostSnapshot) -> DetectedProfile {
    let mut notes = Vec::new();
    let os = match snap.os.as_str() {
        "linux" => OperatingSystem::Linux,
        "macos" => OperatingSystem::MacOs,
        "windows" => OperatingSystem::Windows,
        _ => OperatingSystem::Other,
    };
    let mut profile = ServerProfile {
        os,
        ..ServerProfile::default()
    };

    let cores = u32::try_from(snap.logical_cpus)
        .unwrap_or(u32::MAX)
        .clamp(1, 1024);
    profile.cpu_cores = cores;
    if snap.logical_cpus == 0 {
        notes.push("CPU count unavailable; defaulted to 1 core.".to_owned());
    }

    if snap.cpu_mhz > 0 {
        profile.cpu_ghz = round(snap.cpu_mhz as f64 / 1000.0, 1).clamp(0.1, 10.0);
    } else {
        profile.cpu_ghz = DEFAULT_CPU_GHZ;
        notes.push(format!(
            "CPU frequency unavailable; defaulted to {DEFAULT_CPU_GHZ} GHz."
        ));
    }

    if snap.total_memory_bytes > 0 {
        let gb = snap.total_memory_bytes as f64 / 1024f64.powi(3);
        profile.ram_gb = round(gb, 2).clamp(0.25, 16_384.0);
    } else {
        notes.push("RAM size unavailable; kept default.".to_owned());
    }

    let (kind, iops) = match snap.disk_kind {
        Some(DiskKind::Hdd) => (DiskKind::Hdd, 150),
        Some(DiskKind::Nvme) => (DiskKind::Nvme, 100_000),
        Some(DiskKind::Ssd) | None => (DiskKind::Ssd, 20_000),
    };
    profile.disk = DiskProfile { kind, iops };
    if snap.disk_kind.is_none() {
        notes.push("Disk type not detected; assumed SSD.".to_owned());
    }
    notes.push(
        "Disk IOPS is a typical value for the disk type, not measured. Run the calibration \
         benchmark for real numbers."
            .to_owned(),
    );

    profile.network = NetworkProfile {
        bandwidth_mbps: 1_000.0,
        latency_ms: 0.5,
    };
    notes.push("Network bandwidth and latency are defaults; adjust manually.".to_owned());

    let brand = snap.cpu_brand.to_lowercase();
    profile.virtualization = if snap.in_container {
        Virtualization::Container
    } else if brand.contains("virtual") || brand.contains("qemu") || brand.contains("kvm") {
        Virtualization::Vm
    } else {
        notes.push("Virtualization cannot be detected reliably; verify manually.".to_owned());
        Virtualization::BareMetal
    };

    match snap.fd_limit {
        Some(limit) => profile.fd_limit = limit.clamp(16, 10_000_000),
        None => {
            profile.fd_limit = DEFAULT_FD_LIMIT;
            notes.push(format!(
                "File descriptor limit not detected; defaulted to {DEFAULT_FD_LIMIT}."
            ));
        }
    }

    DetectedProfile { profile, notes }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> HostSnapshot {
        HostSnapshot {
            os: "linux".into(),
            logical_cpus: 8,
            cpu_mhz: 3_400,
            cpu_brand: "Intel Xeon".into(),
            total_memory_bytes: 16 * 1024 * 1024 * 1024,
            disk_kind: Some(DiskKind::Ssd),
            fd_limit: Some(1_048_576),
            in_container: false,
        }
    }

    #[test]
    fn maps_measured_values() {
        let d = profile_from_snapshot(&snapshot());
        assert_eq!(d.profile.os, OperatingSystem::Linux);
        assert_eq!(d.profile.cpu_cores, 8);
        assert!((d.profile.cpu_ghz - 3.4).abs() < f64::EPSILON);
        assert!((d.profile.ram_gb - 16.0).abs() < f64::EPSILON);
        assert_eq!(d.profile.fd_limit, 1_048_576);
        assert!(d.profile.validate().is_ok());
    }

    #[test]
    fn falls_back_and_notes_missing_data() {
        let d = profile_from_snapshot(&HostSnapshot {
            os: "plan9".into(),
            ..HostSnapshot::default()
        });
        assert_eq!(d.profile.os, OperatingSystem::Other);
        assert_eq!(d.profile.cpu_cores, 1);
        assert!((d.profile.cpu_ghz - DEFAULT_CPU_GHZ).abs() < f64::EPSILON);
        assert_eq!(d.profile.fd_limit, DEFAULT_FD_LIMIT);
        assert!(d.notes.len() >= 5);
        assert!(d.profile.validate().is_ok());
    }

    #[test]
    fn container_and_vm_detection() {
        let mut s = snapshot();
        s.in_container = true;
        assert_eq!(
            profile_from_snapshot(&s).profile.virtualization,
            Virtualization::Container
        );
        s.in_container = false;
        s.cpu_brand = "QEMU Virtual CPU".into();
        assert_eq!(
            profile_from_snapshot(&s).profile.virtualization,
            Virtualization::Vm
        );
    }

    #[test]
    fn detect_on_this_host_yields_valid_profile() {
        assert!(detect().profile.validate().is_ok());
    }
}
