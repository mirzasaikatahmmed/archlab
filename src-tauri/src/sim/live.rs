//! Live runs: the engine advances one simulated second at a time on its own
//! thread, paced against the wall clock, and pushes a tick per second.
//!
//! This module knows nothing about Tauri; the command layer supplies a `sink`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::SimError;
use super::engine::Simulation;
use super::model::Model;
use super::report::{LiveTick, SimOptions, SimReport};
use crate::domain::project::Project;
use crate::domain::validation::{Issues, ValidationError};

const MIN_SPEED: f64 = 0.1;
const MAX_SPEED: f64 = 1_000.0;
/// How often a waiting run checks for pause and stop.
const POLL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LiveEvent {
    Tick {
        tick: LiveTick,
    },
    /// The run reached its end; the report covers the whole run.
    Finished {
        report: SimReport,
    },
    Stopped,
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LiveCommand {
    Pause,
    Resume,
    Stop,
    /// Simulated seconds per wall-clock second; `None` runs as fast as possible.
    SetSpeed {
        speed: Option<f64>,
    },
}

/// Speed is stored as f64 bits; zero means "as fast as possible".
struct Control {
    stop: AtomicBool,
    paused: AtomicBool,
    speed_bits: AtomicU64,
}

impl Control {
    fn new(speed: Option<f64>) -> Self {
        Self {
            stop: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            speed_bits: AtomicU64::new(encode_speed(speed)),
        }
    }

    fn speed(&self) -> Option<f64> {
        let v = f64::from_bits(self.speed_bits.load(Ordering::Relaxed));
        (v > 0.0).then_some(v)
    }
}

fn encode_speed(speed: Option<f64>) -> u64 {
    speed.unwrap_or(0.0).to_bits()
}

fn validate_speed(speed: Option<f64>) -> Result<(), ValidationError> {
    let mut issues = Issues::default();
    if let Some(v) = speed {
        issues.range_f64("speed", v, MIN_SPEED, MAX_SPEED);
    }
    issues.finish()
}

/// Holder of the single live run an application may have at a time.
#[derive(Default)]
pub struct LiveRuns {
    active: Arc<Mutex<Option<Arc<Control>>>>,
}

impl LiveRuns {
    /// Validates the project, then runs it on a new thread. `sink` returns `false`
    /// when nobody is listening any more, which ends the run.
    pub fn start(
        &self,
        project: &Project,
        options: &SimOptions,
        speed: Option<f64>,
        sink: impl Fn(LiveEvent) -> bool + Send + 'static,
    ) -> Result<(), SimError> {
        project.validate()?;
        options.validate()?;
        validate_speed(speed)?;
        let model = Model::compile(project)?;

        let control = Arc::new(Control::new(speed));
        {
            let mut slot = self.active.lock().unwrap_or_else(PoisonError::into_inner);
            if slot.is_some() {
                return Err(SimError::Busy);
            }
            *slot = Some(Arc::clone(&control));
        }

        let options = options.clone();
        let slot = Arc::clone(&self.active);
        let spawned = thread::Builder::new()
            .name("archlab-live-run".into())
            .spawn({
                let control = Arc::clone(&control);
                move || {
                    drive(&model, &options, &control, &sink);
                    *slot.lock().unwrap_or_else(PoisonError::into_inner) = None;
                }
            });
        if let Err(err) = spawned {
            *self.active.lock().unwrap_or_else(PoisonError::into_inner) = None;
            return Err(SimError::Model(format!("could not start the run: {err}")));
        }
        Ok(())
    }

    pub fn control(&self, command: LiveCommand) -> Result<(), SimError> {
        let slot = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        let control = slot.as_ref().ok_or(SimError::NoActiveRun)?;
        match command {
            LiveCommand::Pause => control.paused.store(true, Ordering::Relaxed),
            LiveCommand::Resume => control.paused.store(false, Ordering::Relaxed),
            LiveCommand::Stop => control.stop.store(true, Ordering::Relaxed),
            LiveCommand::SetSpeed { speed } => {
                validate_speed(speed)?;
                control
                    .speed_bits
                    .store(encode_speed(speed), Ordering::Relaxed);
            }
        }
        Ok(())
    }
}

fn drive(
    model: &Model,
    options: &SimOptions,
    control: &Control,
    sink: &impl Fn(LiveEvent) -> bool,
) {
    let mut sim = Simulation::new(model, options);
    let mut second = 0u64;
    let mut deadline = Instant::now();
    loop {
        if control.stop.load(Ordering::Relaxed) {
            sink(LiveEvent::Stopped);
            return;
        }
        if control.paused.load(Ordering::Relaxed) {
            thread::sleep(POLL);
            deadline = Instant::now();
            continue;
        }
        let now = Instant::now();
        if now < deadline {
            thread::sleep((deadline - now).min(POLL));
            continue;
        }

        second += 1;
        if let Err(err) = sim.step_until(second * 1_000_000) {
            sink(LiveEvent::Failed {
                message: err.to_string(),
            });
            return;
        }
        let tick = sim.tick();
        if !sink(LiveEvent::Tick { tick }) {
            return;
        }
        if sim.is_finished() {
            sink(LiveEvent::Finished {
                report: sim.finish(),
            });
            return;
        }
        deadline = match control.speed() {
            Some(speed) => (deadline + Duration::from_secs_f64(1.0 / speed)).max(Instant::now()),
            None => Instant::now(),
        };
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{self, Receiver};

    use super::*;
    use crate::domain::project::Project;

    const WAIT: Duration = Duration::from_secs(10);

    fn project() -> Project {
        let mut p = Project::new("live".to_owned());
        p.traffic_profile.requests_per_second = 50.0;
        p
    }

    fn options(duration_s: f64) -> SimOptions {
        SimOptions {
            seed: 5,
            duration_s,
            warmup_s: 2.0,
        }
    }

    fn start(
        runs: &LiveRuns,
        duration_s: f64,
        speed: Option<f64>,
    ) -> Result<Receiver<LiveEvent>, SimError> {
        let (tx, rx) = mpsc::channel();
        runs.start(&project(), &options(duration_s), speed, move |e| {
            tx.send(e).is_ok()
        })?;
        Ok(rx)
    }

    fn wait_idle(runs: &LiveRuns) {
        let end = Instant::now() + WAIT;
        while runs.control(LiveCommand::Resume).is_ok() {
            assert!(Instant::now() < end, "run did not finish");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_full_run_emits_one_tick_per_second_then_the_same_report_as_a_batch_run() {
        let runs = LiveRuns::default();
        let rx = start(&runs, 12.0, None).expect("start");
        let events: Vec<LiveEvent> = rx.iter().collect();

        let ticks: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                LiveEvent::Tick { tick } => Some(tick.point.t_s),
                _ => None,
            })
            .collect();
        assert_eq!(ticks, (0..12).collect::<Vec<u32>>());

        let Some(LiveEvent::Finished { report }) = events.last() else {
            panic!("last event was {:?}", events.last());
        };
        let batch = super::super::run(&project(), &options(12.0)).expect("batch");
        assert_eq!(report, &batch);
        wait_idle(&runs);
    }

    #[test]
    fn stop_ends_the_run_and_frees_the_slot() {
        let runs = LiveRuns::default();
        let rx = start(&runs, 600.0, Some(1.0)).expect("start");
        runs.control(LiveCommand::Stop).expect("stop");
        let events: Vec<LiveEvent> = rx.iter().collect();
        assert!(matches!(events.last(), Some(LiveEvent::Stopped)));
        wait_idle(&runs);
        assert!(start(&runs, 5.0, None).is_ok());
    }

    #[test]
    fn only_one_run_at_a_time() {
        let runs = LiveRuns::default();
        let rx = start(&runs, 600.0, Some(1.0)).expect("start");
        assert!(matches!(start(&runs, 5.0, None), Err(SimError::Busy)));
        runs.control(LiveCommand::Stop).expect("stop");
        drop(rx);
        wait_idle(&runs);
    }

    #[test]
    fn pause_holds_ticks_until_resume() {
        let runs = LiveRuns::default();
        let rx = start(&runs, 600.0, Some(50.0)).expect("start");
        rx.recv_timeout(WAIT).expect("first tick");
        runs.control(LiveCommand::Pause).expect("pause");
        thread::sleep(Duration::from_millis(150));
        while rx.try_recv().is_ok() {}
        thread::sleep(Duration::from_millis(250));
        assert!(rx.try_recv().is_err(), "ticks arrived while paused");
        runs.control(LiveCommand::Resume).expect("resume");
        rx.recv_timeout(WAIT).expect("tick after resume");
        runs.control(LiveCommand::Stop).expect("stop");
        wait_idle(&runs);
    }

    #[test]
    fn speed_can_change_during_a_run_and_is_validated() {
        let runs = LiveRuns::default();
        let rx = start(&runs, 20.0, Some(1.0)).expect("start");
        assert!(
            runs.control(LiveCommand::SetSpeed { speed: Some(0.0) })
                .is_err()
        );
        runs.control(LiveCommand::SetSpeed { speed: None })
            .expect("max speed");
        let events: Vec<LiveEvent> = rx.iter().collect();
        assert!(matches!(events.last(), Some(LiveEvent::Finished { .. })));
        wait_idle(&runs);
    }

    #[test]
    fn invalid_input_is_rejected_before_starting() {
        let runs = LiveRuns::default();
        assert!(matches!(
            start(&runs, 10.0, Some(5_000.0)),
            Err(SimError::Invalid(_))
        ));
        assert!(matches!(
            runs.control(LiveCommand::Stop),
            Err(SimError::NoActiveRun)
        ));
    }

    #[test]
    fn a_closed_listener_ends_the_run() {
        let runs = LiveRuns::default();
        let rx = start(&runs, 600.0, Some(50.0)).expect("start");
        drop(rx);
        wait_idle(&runs);
    }
}
