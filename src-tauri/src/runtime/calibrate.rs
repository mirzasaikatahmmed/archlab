//! Short benchmarks of the host: CPU speed, synced disk writes and loopback latency.

use std::fs::{self, File, OpenOptions};
use std::hint::black_box;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::RuntimeError;
use crate::domain::calibration::Calibration;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CalibrationEvent {
    /// A benchmark is starting; `index` counts from 1.
    Step {
        name: String,
        index: u32,
        total: u32,
    },
    Finished {
        calibration: Calibration,
    },
    Cancelled,
    Failed {
        message: String,
    },
}

const STEPS: [&str; 3] = [
    "Measuring CPU speed",
    "Measuring disk writes",
    "Measuring loopback latency",
];
const CPU_TRIALS: u32 = 3;
const CPU_TRIAL: Duration = Duration::from_millis(400);
const DISK_BUDGET: Duration = Duration::from_millis(1_500);
const DISK_FILE_BYTES: u64 = 16 * 1024 * 1024;
const BLOCK: usize = 4096;
const PING_WARMUP: u32 = 500;
const PINGS: u32 = 3_000;
/// A multiply then an add, each depending on the last: about this many cycles per step.
const CYCLES_PER_STEP: f64 = 4.0;

const LCG_MUL: u64 = 6_364_136_223_846_793_005;
const LCG_ADD: u64 = 1_442_695_040_888_963_407;

/// Steps of a dependent integer chain per second, the best of a few short trials.
pub fn cpu_steps_per_second(trial: Duration, trials: u32, cancel: &AtomicBool) -> f64 {
    let mut best = 0.0f64;
    for _ in 0..trials {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let start = Instant::now();
        let mut x = black_box(0x9E37_79B9_7F4A_7C15u64);
        let mut steps = 0u64;
        while start.elapsed() < trial {
            x = chain(x, 100_000);
            steps += 100_000;
        }
        black_box(x);
        #[allow(clippy::cast_precision_loss)]
        let rate = steps as f64 / start.elapsed().as_secs_f64();
        best = best.max(rate);
    }
    best
}

/// Runs `steps` dependent multiply-adds; shared with real mode so both burn the same kind of work.
#[inline(never)]
pub fn chain(mut x: u64, steps: u64) -> u64 {
    for _ in 0..steps {
        x = x.wrapping_mul(LCG_MUL).wrapping_add(LCG_ADD);
    }
    black_box(x)
}

struct TempFile {
    path: PathBuf,
}

impl Drop for TempFile {
    fn drop(&mut self) {
        // Best effort: the file is scratch space in the app data folder.
        let _ = fs::remove_file(&self.path);
    }
}

fn disk_writes_per_second(
    dir: &Path,
    budget: Duration,
    cancel: &AtomicBool,
) -> std::io::Result<f64> {
    fs::create_dir_all(dir)?;
    let path = dir.join("calibration.tmp");
    let _guard = TempFile { path: path.clone() };
    let mut file: File = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;
    file.set_len(DISK_FILE_BYTES)?;
    file.sync_all()?;

    let blocks = DISK_FILE_BYTES / BLOCK as u64;
    let data = [0xA5u8; BLOCK];
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut writes = 0u64;
    let start = Instant::now();
    while start.elapsed() < budget && !cancel.load(Ordering::Relaxed) {
        state = state.wrapping_mul(LCG_MUL).wrapping_add(LCG_ADD);
        let block = (state >> 33) % blocks;
        file.seek(SeekFrom::Start(block * BLOCK as u64))?;
        file.write_all(&data)?;
        file.sync_data()?;
        writes += 1;
    }
    #[allow(clippy::cast_precision_loss)]
    Ok(writes as f64 / start.elapsed().as_secs_f64())
}

/// Half the median round trip of a 16-byte message echoed over localhost, in milliseconds.
async fn loopback_latency_ms(cancel: &AtomicBool) -> std::io::Result<f64> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let echo = tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let _ = socket.set_nodelay(true);
            let mut buf = [0u8; 16];
            while socket.read_exact(&mut buf).await.is_ok() {
                if socket.write_all(&buf).await.is_err() {
                    break;
                }
            }
        }
    });
    let mut client = TcpStream::connect(addr).await?;
    client.set_nodelay(true)?;
    let mut buf = [7u8; 16];
    let mut rtts = Vec::with_capacity(PINGS as usize);
    for i in 0..PING_WARMUP + PINGS {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let start = Instant::now();
        client.write_all(&buf).await?;
        client.read_exact(&mut buf).await?;
        if i >= PING_WARMUP {
            rtts.push(start.elapsed());
        }
    }
    echo.abort();
    if rtts.is_empty() {
        return Ok(0.0);
    }
    rtts.sort_unstable();
    Ok(rtts[rtts.len() / 2].as_secs_f64() * 1_000.0 / 2.0)
}

fn now_ms() -> f64 {
    #[allow(clippy::cast_precision_loss)]
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64)
}

/// Runs the three benchmarks, reporting each step to `sink` (which returns false to stop).
pub fn run(
    scratch_dir: &Path,
    cancel: &AtomicBool,
    sink: &impl Fn(CalibrationEvent) -> bool,
) -> Result<Option<Calibration>, RuntimeError> {
    let total = u32::try_from(STEPS.len()).unwrap_or(3);
    let step = |i: usize| {
        sink(CalibrationEvent::Step {
            name: STEPS[i].to_owned(),
            index: u32::try_from(i + 1).unwrap_or(1),
            total,
        })
    };
    let mut notes = Vec::new();

    if !step(0) || cancel.load(Ordering::Relaxed) {
        return Ok(None);
    }
    let steps_per_s = cpu_steps_per_second(CPU_TRIAL, CPU_TRIALS, cancel);
    let equivalent_ghz = steps_per_s * CYCLES_PER_STEP / 1e9;

    if !step(1) || cancel.load(Ordering::Relaxed) {
        return Ok(None);
    }
    let disk_iops = disk_writes_per_second(scratch_dir, DISK_BUDGET, cancel)
        .map_err(|e| RuntimeError::Failed(format!("disk benchmark failed: {e}")))?;
    notes.push(
        "Disk figure uses synced 4 KiB writes, which is usually lower than a drive's rated random IOPS."
            .to_owned(),
    );

    if !step(2) || cancel.load(Ordering::Relaxed) {
        return Ok(None);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| RuntimeError::Failed(format!("could not start the network benchmark: {e}")))?;
    let loopback = runtime
        .block_on(loopback_latency_ms(cancel))
        .map_err(|e| RuntimeError::Failed(format!("network benchmark failed: {e}")))?;
    notes
        .push("Loopback latency is the delay inside this computer, not a real network.".to_owned());

    if cancel.load(Ordering::Relaxed) {
        return Ok(None);
    }
    let debug_build = cfg!(debug_assertions);
    if debug_build {
        notes.push(
            "Measured by a debug build, which runs much slower than a release build. Use a release build for real numbers."
                .to_owned(),
        );
    }
    notes.push("CPU score assumes a 4-cycle dependent integer step, so it is a relative speed, not a clock.".to_owned());
    Ok(Some(Calibration {
        measured_at: now_ms(),
        equivalent_ghz,
        disk_iops,
        loopback_latency_ms: loopback,
        debug_build,
        notes,
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[test]
    fn the_chain_is_deterministic_and_changes_its_input() {
        assert_eq!(chain(1, 1_000), chain(1, 1_000));
        assert_ne!(chain(1, 1_000), chain(2, 1_000));
        assert_ne!(chain(1, 1_000), chain(1, 1_001));
    }

    #[test]
    fn cpu_benchmark_reports_a_plausible_rate_and_honours_cancel() {
        let never = AtomicBool::new(false);
        let rate = cpu_steps_per_second(Duration::from_millis(40), 1, &never);
        assert!(rate > 1e6, "{rate} steps/s");
        let cancelled = AtomicBool::new(true);
        assert_eq!(
            cpu_steps_per_second(Duration::from_millis(40), 3, &cancelled),
            0.0
        );
    }

    #[test]
    fn disk_benchmark_writes_and_cleans_up() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cancel = AtomicBool::new(false);
        let rate = disk_writes_per_second(dir.path(), Duration::from_millis(150), &cancel)
            .expect("benchmark");
        assert!(rate > 0.0);
        assert!(!dir.path().join("calibration.tmp").exists());
    }

    #[test]
    fn loopback_latency_is_small_and_positive() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let ms = runtime
            .block_on(loopback_latency_ms(&AtomicBool::new(false)))
            .expect("latency");
        assert!(ms > 0.0 && ms < 50.0, "{ms} ms");
    }

    #[test]
    fn a_full_run_reports_every_step_then_a_calibration() {
        let dir = tempfile::tempdir().expect("tempdir");
        let events = Mutex::new(Vec::new());
        let result = run(dir.path(), &AtomicBool::new(false), &|e| {
            events.lock().expect("lock").push(e);
            true
        })
        .expect("run")
        .expect("calibration");
        assert!(result.equivalent_ghz > 0.0 && result.disk_iops > 0.0);
        let names: Vec<_> = events
            .lock()
            .expect("lock")
            .iter()
            .map(|e| match e {
                CalibrationEvent::Step { index, .. } => *index,
                _ => 0,
            })
            .collect();
        assert_eq!(names, [1, 2, 3]);
    }

    #[test]
    fn cancel_before_start_returns_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let result = run(dir.path(), &AtomicBool::new(true), &|_| true).expect("run");
        assert!(result.is_none());
    }
}
