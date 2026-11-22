# ADR-013: Live controls

- Status: Accepted
- Date: 2026-11-22

## Context

A live run could only be paused, stopped or sped up. People want to poke the system while it
plays: raise the traffic, break a node, and end a fault early to watch recovery.

## Decision

1. **Three commands** (`sim::control::ControlCommand`): set a traffic multiplier (0.1 to 10, applied
   on top of the profile's rate and spikes), inject a node fault (crash, slowdown, lost workers) for a
   given duration, and end a fault that is in effect (injected or from the project). Replicas,
   client policy and connection faults are not live-editable: they change the pool or the topology
   under running requests.
2. **Applied between ticks.** The engine only ever stops at whole simulated seconds, so a command
   takes effect at the current second, on the run's own thread, in order. The caller waits for the
   outcome (up to 5 s, and at once if the run ends), so a refused command comes back as a normal
   typed error and the run carries on. Commands also work while paused.
3. **Traffic.** Arrivals use thinning against a peak rate. A live multiplier scales both the keep
   probability and the peak (`peak x largest multiplier seen`), so the process stays valid. With no
   commands the factors are exactly 1.0 and every earlier result is unchanged (tested). One candidate
   arrival already scheduled keeps its old gap, which is at most one inter-arrival time of lag.
4. **Faults.** The engine now owns its fault list (the project's, then injected ones, ids `live-1`,
   `live-2`, ...). An injected fault is scheduled as an ordinary fault event; ending one sets its end
   to now. They appear in the report's fault table, labelled "(live)", and are not written back to
   the project.
5. **Reproducible.** Every applied command is logged in `SimReport.controls` (second, command,
   plain-language summary, fault id). `start_live_run` accepts a `script` of logged commands and
   re-applies each at its second, so "Replay with the same changes" gives the identical report
   (tested against a batch run, `run_scripted`). The Markdown report and the report view list the
   controls.
6. **UI.** A "Steer the run" panel under the Stage while a run is active: traffic presets and a
   field, a node-fault form, a list of faults in effect with "End now", and the log. Injected faults
   reach the Stage and charts as ordinary faults (`liveFaultsFrom`, numbered like the engine does so
   a replayed script can be drawn too).
7. **Types.** `LiveEvent::Finished` now boxes its report (the report grew), as `RealEvent` does.
   `control_live_run` is async and returns the log entry; `start_live_run` gained a `script`.

## Consequences

- Reports of steered runs are only reproducible together with their log; the seed alone is not
  enough. The report says so.
- Scope is the live run. The scenario player, instant and what-if runs are unchanged.
