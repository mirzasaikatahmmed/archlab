//! Measurements of the machine ArchLab runs on.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::server_profile::ServerProfile;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Calibration {
    /// Unix epoch milliseconds when the measurement finished.
    pub measured_at: f64,
    /// Clock speed of a core that would run a chain of dependent integer operations as fast as
    /// this one, assuming 4 cycles per step. A relative speed score, not the nameplate clock.
    pub equivalent_ghz: f64,
    /// Synced 4 KiB writes per second. Usually lower than a drive's rated random IOPS.
    pub disk_iops: f64,
    /// Half the median round trip of a TCP message over localhost, in milliseconds.
    pub loopback_latency_ms: f64,
    /// Measured by a debug build, which runs far slower than a release build.
    pub debug_build: bool,
    pub notes: Vec<String>,
}

/// Copies the measured values into a server profile, kept inside the profile's valid ranges.
pub fn apply_to_profile(calibration: &Calibration, profile: &ServerProfile) -> ServerProfile {
    let mut out = profile.clone();
    out.cpu_ghz = round(calibration.equivalent_ghz.clamp(0.1, 10.0), 2);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        out.disk.iops = calibration.disk_iops.round().clamp(1.0, 10_000_000.0) as u32;
    }
    out.network.latency_ms = round(calibration.loopback_latency_ms.clamp(0.0, 10_000.0), 3);
    out
}

fn round(v: f64, places: i32) -> f64 {
    let f = 10f64.powi(places);
    (v * f).round() / f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Calibration {
        Calibration {
            measured_at: 1.0,
            equivalent_ghz: 3.456_78,
            disk_iops: 4_321.6,
            loopback_latency_ms: 0.0456,
            debug_build: false,
            notes: Vec::new(),
        }
    }

    #[test]
    fn applies_rounded_values_and_keeps_the_rest() {
        let base = ServerProfile::default();
        let out = apply_to_profile(&sample(), &base);
        assert_eq!(out.cpu_ghz, 3.46);
        assert_eq!(out.disk.iops, 4_322);
        assert_eq!(out.network.latency_ms, 0.046);
        assert_eq!(out.cpu_cores, base.cpu_cores);
        assert_eq!(out.ram_gb, base.ram_gb);
        assert!(out.validate().is_ok());
    }

    #[test]
    fn extreme_measurements_stay_in_range() {
        let mut c = sample();
        c.equivalent_ghz = 0.0;
        c.disk_iops = 1e12;
        c.loopback_latency_ms = f64::MAX;
        let out = apply_to_profile(&c, &ServerProfile::default());
        assert_eq!(out.cpu_ghz, 0.1);
        assert_eq!(out.disk.iops, 10_000_000);
        assert!(out.validate().is_ok());
    }
}
