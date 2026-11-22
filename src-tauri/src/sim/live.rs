//! Live runs: the engine advances one simulated second at a time on its own
//! thread, paced against the wall clock, and pushes a tick per second.
//!
//! This module knows nothing about Tauri; the command layer supplies a `sink`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::SimError;
use super::control::{ControlCommand, ControlRecord, MAX_SCRIPT};
use super::engine::Simulation;
use super::model::Model;
use super::report::{LiveTick, SimOptions, SimReport};
use crate::domain::project::Project;
use crate::domain::validation::{Issues, ValidationError};

const MIN_SPEED: f64 = 0.1;
const MAX_SPEED: f64 = 1_000.0;
/// How often a waiting run checks for pause and stop.
const POLL: Duration = Duration::from_millis(25);
/// How long a command waits for the run to apply it before giving up.
const CONTROL_REPLY: Duration = Duration::from_secs(5);

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
        report: Box<SimReport>,
    },
    Stopped,
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
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
    /// Changes the simulated system while it runs; see [`ControlCommand`].
    Control {
        command: ControlCommand,
    },
}

/// A steering command waiting for the run, with where to send its outcome.
type PendingCommand = (ControlCommand, Sender<Result<ControlRecord, SimError>>);

/// Speed is stored as f64 bits; zero means "as fast as possible".
struct Control {
    stop: AtomicBool,
    paused: AtomicBool,
    speed_bits: AtomicU64,
    /// Commands waiting for the run to apply them, each with where to send the outcome.
    pending: Mutex<Vec<PendingCommand>>,
}

impl Control {
    fn new(speed: Option<f64>) -> Self {
        Self {
            stop: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            speed_bits: AtomicU64::new(encode_speed(speed)),
            pending: Mutex::new(Vec::new()),
        }
    }

    fn take_pending(&self) -> Vec<PendingCommand> {
        std::mem::take(&mut *self.pending.lock().unwrap_or_else(PoisonError::into_inner))
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
        script: &[ControlRecord],
        sink: impl Fn(LiveEvent) -> bool + Send + 'static,
    ) -> Result<(), SimError> {
        project.validate()?;
        options.validate()?;
        validate_speed(speed)?;
        validate_script(script)?;
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
        let mut script = script.to_vec();
        script.sort_by_key(|r| r.t_s);
        let script: VecDeque<ControlRecord> = script.into();
        let slot = Arc::clone(&self.active);
        let spawned = thread::Builder::new()
            .name("archlab-live-run".into())
            .spawn({
                let control = Arc::clone(&control);
                move || {
                    drive(&model, &options, &control, script, &sink);
                    *slot.lock().unwrap_or_else(PoisonError::into_inner) = None;
                }
            });
        if let Err(err) = spawned {
            *self.active.lock().unwrap_or_else(PoisonError::into_inner) = None;
            return Err(SimError::Model(format!("could not start the run: {err}")));
        }
        Ok(())
    }

    /// Applies a command to the run. A change to the simulated system returns its log entry, once
    /// the run has applied it at the current simulated second.
    pub fn control(&self, command: LiveCommand) -> Result<Option<ControlRecord>, SimError> {
        let control = {
            let slot = self.active.lock().unwrap_or_else(PoisonError::into_inner);
            Arc::clone(slot.as_ref().ok_or(SimError::NoActiveRun)?)
        };
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
            LiveCommand::Control { command } => {
                command.validate()?;
                let (reply, outcome) = mpsc::channel();
                control
                    .pending
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push((command, reply));
                return match outcome.recv_timeout(CONTROL_REPLY) {
                    Ok(result) => result.map(Some),
                    Err(_) => Err(SimError::NoActiveRun),
                };
            }
        }
        Ok(None)
    }
}

fn validate_script(script: &[ControlRecord]) -> Result<(), ValidationError> {
    let mut issues = Issues::default();
    issues.check(
        script.len() <= MAX_SCRIPT,
        "script",
        &format!("at most {MAX_SCRIPT} commands"),
    );
    for (i, record) in script.iter().enumerate() {
        issues.nested(&format!("script.{i}"), record.command.validate());
    }
    issues.finish()
}

/// Answers every command still waiting when the run ends, so nobody waits for a reply.
struct Closing<'a>(&'a Control);

impl Drop for Closing<'_> {
    fn drop(&mut self) {
        for (_, reply) in self.0.take_pending() {
            let _ = reply.send(Err(SimError::NoActiveRun));
        }
    }
}

fn apply_pending(control: &Control, sim: &mut Simulation) {
    for (command, reply) in control.take_pending() {
        // The caller may have given up waiting; the command is applied either way.
        let _ = reply.send(sim.apply_control(&command));
    }
}

fn drive(
    model: &Model,
    options: &SimOptions,
    control: &Control,
    mut script: VecDeque<ControlRecord>,
    sink: &impl Fn(LiveEvent) -> bool,
) {
    let _closing = Closing(control);
    let mut sim = Simulation::new(model, options);
    let mut second = 0u64;
    let mut deadline = Instant::now();
    loop {
        if control.stop.load(Ordering::Relaxed) {
            sink(LiveEvent::Stopped);
            return;
        }
        apply_pending(control, &mut sim);
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

        while script.front().is_some_and(|r| u64::from(r.t_s) <= second) {
            let Some(record) = script.pop_front() else {
                break;
            };
            if let Err(err) = sim.apply_control(&record.command) {
                sink(LiveEvent::Failed {
                    message: err.to_string(),
                });
                return;
            }
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
                report: Box::new(sim.finish()),
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
        runs.start(&project(), &options(duration_s), speed, &[], move |e| {
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
        assert_eq!(**report, batch);
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

    fn app_crash(duration_s: f64) -> ControlCommand {
        ControlCommand::InjectFault {
            target: "app".to_owned(),
            fault: crate::domain::chaos::FaultKind::Crash,
            duration_s,
        }
    }

    #[test]
    fn a_steering_command_waits_for_the_run_and_returns_its_log_entry() {
        let runs = LiveRuns::default();
        let rx = start(&runs, 20.0, Some(1.0)).expect("start");
        runs.control(LiveCommand::Pause).expect("pause");
        let record = runs
            .control(LiveCommand::Control {
                command: app_crash(5.0),
            })
            .expect("applied")
            .expect("a log entry");
        assert_eq!(record.fault_id.as_deref(), Some("live-1"));
        let traffic = runs
            .control(LiveCommand::Control {
                command: ControlCommand::SetTraffic { multiplier: 2.0 },
            })
            .expect("applied");
        assert_eq!(
            traffic.map(|r| r.summary),
            Some("Traffic \u{d7}2".to_owned())
        );
        runs.control(LiveCommand::SetSpeed { speed: None })
            .expect("speed");
        runs.control(LiveCommand::Resume).expect("resume");

        let events: Vec<LiveEvent> = rx.iter().collect();
        let Some(LiveEvent::Finished { report }) = events.last() else {
            panic!("last event was {:?}", events.last());
        };
        assert_eq!(report.controls.len(), 2);
        assert!(report.faults.iter().any(|f| f.fault_id == "live-1"));
        let ticks: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                LiveEvent::Tick { tick } => Some(tick),
                _ => None,
            })
            .collect();
        assert!(ticks.iter().any(|t| t.active_faults == ["live-1"]));
        wait_idle(&runs);
    }

    #[test]
    fn a_refused_command_comes_back_as_an_error_and_the_run_goes_on() {
        let runs = LiveRuns::default();
        let rx = start(&runs, 12.0, Some(1.0)).expect("start");
        runs.control(LiveCommand::Pause).expect("pause");
        let refused = runs.control(LiveCommand::Control {
            command: ControlCommand::EndFault {
                fault_id: "nothing".to_owned(),
            },
        });
        assert!(matches!(refused, Err(SimError::Invalid(_))));
        let invalid = runs.control(LiveCommand::Control {
            command: ControlCommand::SetTraffic { multiplier: 99.0 },
        });
        assert!(matches!(invalid, Err(SimError::Invalid(_))));
        runs.control(LiveCommand::SetSpeed { speed: None })
            .expect("speed");
        runs.control(LiveCommand::Resume).expect("resume");
        let events: Vec<LiveEvent> = rx.iter().collect();
        assert!(
            matches!(events.last(), Some(LiveEvent::Finished { report }) if report.controls.is_empty())
        );
        wait_idle(&runs);
    }

    #[test]
    fn a_command_for_a_finished_run_fails_fast() {
        let runs = LiveRuns::default();
        let rx = start(&runs, 10.0, None).expect("start");
        rx.iter().for_each(drop);
        wait_idle(&runs);
        let started = Instant::now();
        let result = runs.control(LiveCommand::Control {
            command: ControlCommand::SetTraffic { multiplier: 2.0 },
        });
        assert!(matches!(result, Err(SimError::NoActiveRun)));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_script_replays_a_logged_run_exactly_and_is_validated() {
        let script = vec![
            ControlRecord {
                t_s: 4,
                command: ControlCommand::SetTraffic { multiplier: 3.0 },
                summary: String::new(),
                fault_id: None,
            },
            ControlRecord {
                t_s: 7,
                command: app_crash(4.0),
                summary: String::new(),
                fault_id: None,
            },
        ];
        let runs = LiveRuns::default();
        let (tx, rx) = mpsc::channel();
        runs.start(&project(), &options(20.0), None, &script, move |e| {
            tx.send(e).is_ok()
        })
        .expect("start");
        let events: Vec<LiveEvent> = rx.iter().collect();
        let Some(LiveEvent::Finished { report }) = events.last() else {
            panic!("last event was {:?}", events.last());
        };
        let model = Model::compile(&project()).expect("model");
        let batch =
            super::super::engine::run_scripted(&model, &options(20.0), &script).expect("batch");
        assert_eq!(**report, batch);
        assert_eq!(
            report.controls.iter().map(|r| r.t_s).collect::<Vec<_>>(),
            [4, 7]
        );
        wait_idle(&runs);

        let bad = vec![ControlRecord {
            t_s: 1,
            command: ControlCommand::SetTraffic { multiplier: 99.0 },
            summary: String::new(),
            fault_id: None,
        }];
        let refused = runs.start(&project(), &options(20.0), None, &bad, |_| true);
        assert!(matches!(refused, Err(SimError::Invalid(_))));
    }
}
