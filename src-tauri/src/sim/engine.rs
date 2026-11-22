//! Discrete-event engine. Time is an integer number of microseconds, so the
//! event order never depends on floating-point rounding of timestamps.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};

use super::SimError;
use super::control::{ControlCommand, ControlRecord};
use super::model::{CompiledFault, FaultTarget, Model, ms_to_us};
use super::report::{
    Bottleneck, ESTIMATE_NOTICE, FaultImpact, Latency, LiveNodeTick, LiveTick, NodeReport,
    Severity, SimOptions, SimReport, Summary, TimelinePoint, WindowStats, severity_for,
};
use super::rng::Stream;
use crate::domain::chaos::FaultKind;
use crate::domain::topology::NodeKind;
use crate::domain::validation::Issues;

/// Hard stop so a runaway scenario cannot freeze the app.
pub const MAX_EVENTS: u64 = 5_000_000;
const US_PER_S: u64 = 1_000_000;

#[derive(Debug, Clone, Copy)]
enum EventKind {
    Arrival,
    Reach {
        req: u32,
        node: u32,
    },
    /// `epoch` is the node's crash count when service began; a mismatch means it crashed meanwhile.
    Done {
        req: u32,
        node: u32,
        epoch: u32,
    },
    Retry {
        req: u32,
    },
    FaultStart {
        fault: u32,
    },
    FaultEnd {
        fault: u32,
    },
}

struct Event {
    time: u64,
    seq: u64,
    kind: EventKind,
}

impl PartialEq for Event {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time && self.seq == other.seq
    }
}
impl Eq for Event {}
impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Event {
    /// Reversed so the heap pops the earliest event; `seq` breaks ties in insertion order.
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .time
            .cmp(&self.time)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

struct Request {
    start: u64,
    entered: u64,
    read: bool,
    /// Node to continue from if a cache probe misses and the cache has no onward edge.
    fallback: Option<usize>,
    cache_done: bool,
    /// Client the request came from, where retries start again.
    client: usize,
    attempt: u32,
}

#[derive(Clone, Copy)]
enum DropReason {
    Rejected,
    TimedOut,
    Faulted,
}

struct NodeState {
    rng: Stream,
    busy: usize,
    waiting: VecDeque<u32>,
    rr: usize,
    last_t: u64,
    busy_area: f64,
    /// Worker-time that was available (workers while the node was up), for utilization.
    avail_area: f64,
    queue_area: f64,
    max_queue: usize,
    arrivals: u32,
    completed: u32,
    rejected: u32,
    timed_out: u32,
    faulted: u32,
    latencies: Vec<u32>,
    /// The same areas over the whole run (warm-up included), for live utilization.
    tick_busy: f64,
    tick_avail: f64,
    last_tick_busy: f64,
    last_tick_avail: f64,
    drops_all: u32,
    up: bool,
    epoch: u32,
    slow: f64,
    workers: usize,
}

#[derive(Clone, Copy)]
struct EdgeState {
    extra_us: u64,
    loss: f64,
}

#[derive(Default)]
struct Bucket {
    offered: u32,
    completed: u32,
    errors: u32,
    latencies: Vec<u32>,
}

/// A running simulation. Advance it with [`Simulation::step_until`]; stepping in any
/// pattern gives the same final report as one call to [`run`].
pub struct Simulation<'a> {
    model: &'a Model,
    options: SimOptions,
    tick_index: u32,
    heap: BinaryHeap<Event>,
    seq: u64,
    events: u64,
    clock: u64,
    reqs: Vec<Request>,
    states: Vec<NodeState>,
    edges: Vec<EdgeState>,
    /// The project's faults, then any injected live.
    faults: Vec<CompiledFault>,
    active: Vec<bool>,
    /// Live traffic multiplier, and the largest one seen (it scales the thinning's peak rate).
    live_mult: f64,
    live_peak: f64,
    controls: Vec<ControlRecord>,
    arrivals: Stream,
    attrs: Stream,
    fault_rng: Stream,
    client_rr: usize,
    warmup_us: u64,
    end_us: u64,
    offered: u32,
    completed: u32,
    errors: u32,
    retries: u32,
    latencies: Vec<u32>,
    buckets: Vec<Bucket>,
}

fn invalid(field: &str, message: &str) -> SimError {
    let mut issues = Issues::default();
    issues.push(field, message);
    match issues.finish() {
        Err(e) => SimError::Invalid(e),
        Ok(()) => SimError::Model(message.to_owned()),
    }
}

#[cfg(test)]
/// Runs to the end, applying each scripted command at the start of its second. The same script
/// that a live run logged reproduces that run exactly.
pub fn run_scripted(
    model: &Model,
    options: &SimOptions,
    script: &[ControlRecord],
) -> Result<SimReport, SimError> {
    let mut sim = Simulation::new(model, options);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let seconds = options.duration_s.ceil() as u64;
    for second in 0..seconds {
        for record in script.iter().filter(|r| u64::from(r.t_s) == second) {
            sim.apply_control(&record.command)?;
        }
        sim.step_until((second + 1) * US_PER_S)?;
    }
    sim.step_until(sim.end_us())?;
    Ok(sim.finish())
}

pub fn run(model: &Model, options: &SimOptions) -> Result<SimReport, SimError> {
    let mut sim = Simulation::new(model, options);
    sim.step_until(sim.end_us())?;
    Ok(sim.finish())
}

impl<'a> Simulation<'a> {
    pub fn new(model: &'a Model, options: &SimOptions) -> Self {
        let mut sim = Self::build(model, options);
        for (i, fault) in model.faults.iter().enumerate() {
            let id = to_u32(i as u64);
            if fault.start_us <= sim.end_us {
                sim.schedule(fault.start_us, EventKind::FaultStart { fault: id });
            }
            if fault.end_us <= sim.end_us {
                sim.schedule(fault.end_us, EventKind::FaultEnd { fault: id });
            }
        }
        let first = sim.next_arrival_gap();
        sim.schedule(first, EventKind::Arrival);
        sim
    }

    fn build(model: &'a Model, options: &SimOptions) -> Self {
        let seed = u64::from(options.seed);
        let end_us = ms_to_us(options.duration_s * 1_000.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let bucket_count = options.duration_s.ceil() as usize;
        let node_count = model.nodes.len() as u64;
        Self {
            model,
            options: options.clone(),
            tick_index: 0,
            heap: BinaryHeap::new(),
            seq: 0,
            events: 0,
            clock: 0,
            reqs: Vec::new(),
            states: model
                .nodes
                .iter()
                .enumerate()
                .map(|(i, spec)| NodeState {
                    rng: Stream::new(seed, 2 + i as u64),
                    busy: 0,
                    waiting: VecDeque::new(),
                    rr: 0,
                    last_t: 0,
                    busy_area: 0.0,
                    avail_area: 0.0,
                    queue_area: 0.0,
                    max_queue: 0,
                    arrivals: 0,
                    completed: 0,
                    rejected: 0,
                    timed_out: 0,
                    faulted: 0,
                    latencies: Vec::new(),
                    tick_busy: 0.0,
                    tick_avail: 0.0,
                    last_tick_busy: 0.0,
                    last_tick_avail: 0.0,
                    drops_all: 0,
                    up: true,
                    epoch: 0,
                    slow: 1.0,
                    workers: spec.servers,
                })
                .collect(),
            edges: vec![
                EdgeState {
                    extra_us: 0,
                    loss: 0.0
                };
                model.edges.len()
            ],
            faults: model.faults.clone(),
            active: vec![false; model.faults.len()],
            live_mult: 1.0,
            live_peak: 1.0,
            controls: Vec::new(),
            arrivals: Stream::new(seed, 0),
            attrs: Stream::new(seed, 1),
            fault_rng: Stream::new(seed, 2 + node_count),
            client_rr: 0,
            warmup_us: ms_to_us(options.warmup_s * 1_000.0),
            end_us,
            offered: 0,
            completed: 0,
            errors: 0,
            retries: 0,
            latencies: Vec::new(),
            buckets: (0..bucket_count).map(|_| Bucket::default()).collect(),
        }
    }

    fn schedule(&mut self, time: u64, kind: EventKind) {
        self.seq += 1;
        self.heap.push(Event {
            time,
            seq: self.seq,
            kind,
        });
    }

    pub fn end_us(&self) -> u64 {
        self.end_us
    }

    /// Processes every event up to and including `limit_us` (capped at the end of the run).
    pub fn step_until(&mut self, limit_us: u64) -> Result<(), SimError> {
        let limit = limit_us.min(self.end_us);
        while self.heap.peek().is_some_and(|e| e.time <= limit) {
            let Some(event) = self.heap.pop() else { break };
            self.events += 1;
            if self.events > MAX_EVENTS {
                return Err(SimError::EventLimit { limit: MAX_EVENTS });
            }
            let now = event.time;
            match event.kind {
                EventKind::Arrival => self.on_arrival(now),
                EventKind::Reach { req, node } => self.on_reach(now, req, node as usize),
                EventKind::Done { req, node, epoch } => {
                    self.on_done(now, req, node as usize, epoch);
                }
                EventKind::Retry { req } => self.on_retry(now, req),
                EventKind::FaultStart { fault } => self.set_fault(now, fault as usize, true),
                EventKind::FaultEnd { fault } => self.set_fault(now, fault as usize, false),
            }
        }
        self.clock = limit;
        Ok(())
    }

    /// Applies a live command at the current simulated second and logs it. Returns the log entry.
    pub fn apply_control(&mut self, command: &ControlCommand) -> Result<ControlRecord, SimError> {
        command.validate()?;
        let now = self.clock;
        let (summary, fault_id) = match command {
            ControlCommand::SetTraffic { multiplier } => {
                self.live_mult = *multiplier;
                self.live_peak = self.live_peak.max(*multiplier);
                (format!("Traffic \u{d7}{multiplier}"), None)
            }
            ControlCommand::InjectFault {
                target,
                fault,
                duration_s,
            } => {
                let node = self
                    .model
                    .nodes
                    .iter()
                    .position(|n| n.id == *target && n.kind != NodeKind::Client)
                    .ok_or_else(|| invalid("target", "must be a server node of the topology"))?;
                let id = format!("live-{}", self.faults.len() - self.model.faults.len() + 1);
                let label = format!(
                    "{} (live)",
                    fault.describe_at(&self.model.nodes[node].label)
                );
                let end_us = now + ms_to_us(duration_s * 1_000.0);
                let index = to_u32(self.faults.len() as u64);
                self.faults.push(CompiledFault {
                    id: id.clone(),
                    kind: fault.clone(),
                    target: FaultTarget::Node(node),
                    start_us: now,
                    end_us,
                    label: label.clone(),
                });
                self.active.push(false);
                self.schedule(now, EventKind::FaultStart { fault: index });
                if end_us <= self.end_us {
                    self.schedule(end_us, EventKind::FaultEnd { fault: index });
                }
                (label, Some(id))
            }
            ControlCommand::EndFault { fault_id } => {
                let index = self
                    .faults
                    .iter()
                    .position(|f| f.id == *fault_id)
                    .filter(|&i| self.active[i])
                    .ok_or_else(|| invalid("faultId", "is not in effect right now"))?;
                self.faults[index].end_us = now;
                self.schedule(
                    now,
                    EventKind::FaultEnd {
                        fault: to_u32(index as u64),
                    },
                );
                (
                    format!("Ended: {}", self.faults[index].label),
                    Some(fault_id.clone()),
                )
            }
        };
        let record = ControlRecord {
            t_s: to_u32(now / US_PER_S),
            command: command.clone(),
            summary,
            fault_id,
        };
        self.controls.push(record.clone());
        Ok(record)
    }

    pub fn is_finished(&self) -> bool {
        self.clock >= self.end_us
    }

    /// Metrics for the second that just completed; call once per whole simulated second.
    /// Only reads simulation state, so ticking never changes the final report.
    pub fn tick(&mut self) -> LiveTick {
        let second = self.tick_index;
        self.tick_index += 1;
        let point = {
            let width = (self.options.duration_s - f64::from(second)).min(1.0);
            let b = &mut self.buckets[second as usize];
            b.latencies.sort_unstable();
            TimelinePoint {
                t_s: second,
                offered_rps: f64::from(b.offered) / width,
                completed_rps: f64::from(b.completed) / width,
                error_rps: f64::from(b.errors) / width,
                p95_ms: percentile_sorted(&b.latencies, 0.95),
            }
        };
        let clock = self.clock;
        let nodes = self
            .model
            .nodes
            .iter()
            .zip(self.states.iter_mut())
            .filter(|(spec, _)| spec.kind != NodeKind::Client)
            .map(|(spec, state)| {
                #[allow(clippy::cast_precision_loss)]
                let since = clock.saturating_sub(state.last_t) as f64;
                #[allow(clippy::cast_precision_loss)]
                let busy = state.tick_busy + state.busy as f64 * since;
                #[allow(clippy::cast_precision_loss)]
                let avail = state.tick_avail + available(state) as f64 * since;
                let (d_busy, d_avail) =
                    (busy - state.last_tick_busy, avail - state.last_tick_avail);
                state.last_tick_busy = busy;
                state.last_tick_avail = avail;
                let utilization = if d_avail > 0.0 {
                    (d_busy / d_avail).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                LiveNodeTick {
                    id: spec.id.clone(),
                    utilization,
                    queue: to_u32(state.waiting.len() as u64),
                    busy: to_u32(state.busy as u64),
                    dropped: state.drops_all,
                }
            })
            .collect();
        #[allow(clippy::cast_precision_loss)]
        let progress = (self.clock as f64 / self.end_us as f64).min(1.0);
        let active_faults = self
            .faults
            .iter()
            .zip(&self.active)
            .filter(|(_, on)| **on)
            .map(|(f, _)| f.id.clone())
            .collect();
        LiveTick {
            point,
            nodes,
            progress,
            active_faults,
        }
    }

    /// Candidate gap of the Poisson process at the peak rate, in microseconds.
    fn next_arrival_gap(&mut self) -> u64 {
        let peak_rps = self.model.rps * self.model.peak_multiplier() * self.live_peak;
        ms_to_us(self.arrivals.exponential(1_000.0 / peak_rps)).max(1)
    }

    fn bucket(&mut self, time: u64) -> Option<&mut Bucket> {
        let i = usize::try_from(time / US_PER_S).ok()?;
        self.buckets.get_mut(i)
    }

    fn on_arrival(&mut self, now: u64) {
        let gap = self.next_arrival_gap();
        self.schedule(now + gap, EventKind::Arrival);

        // Thinning: keep a candidate with probability rate(t) / peak rate.
        #[allow(clippy::cast_precision_loss)]
        let t_s = now as f64 / US_PER_S as f64;
        let keep = self.model.rate_multiplier(t_s) * self.live_mult
            / (self.model.peak_multiplier() * self.live_peak);
        if self.arrivals.unit() >= keep {
            return;
        }

        let read = self.attrs.unit() < self.model.read_ratio;
        let id = to_u32(self.reqs.len() as u64);
        let client = self.model.clients[self.client_rr % self.model.clients.len()];
        self.client_rr += 1;
        self.reqs.push(Request {
            start: now,
            entered: now,
            read,
            fallback: None,
            cache_done: false,
            client,
            attempt: 0,
        });
        if now >= self.warmup_us {
            self.offered += 1;
        }
        if let Some(b) = self.bucket(now) {
            b.offered += 1;
        }

        let target = self.route(id, client);
        self.forward(now, id, client, target);
    }

    fn on_retry(&mut self, now: u64, req: u32) {
        let r = &mut self.reqs[req as usize];
        r.fallback = None;
        r.cache_done = false;
        r.entered = now;
        let client = r.client;
        let target = self.route(req, client);
        self.forward(now, req, client, target);
    }

    /// Sends `req` over the edge `from -> to`, or ends it if there is nowhere to go.
    fn forward(&mut self, now: u64, req: u32, from: usize, target: Option<usize>) {
        match target {
            Some(to) => self.send(now, req, from, to),
            None => self.complete(now, req, from),
        }
    }

    fn send(&mut self, now: u64, req: u32, from: usize, to: usize) {
        let edge = self.model.edge_index.get(&(from, to)).map_or(
            EdgeState {
                extra_us: 0,
                loss: 0.0,
            },
            |&e| self.edges[e],
        );
        if edge.loss > 0.0 && self.fault_rng.unit() < edge.loss {
            self.attempt_failed(now, req, to, DropReason::Faulted);
            return;
        }
        let node = to_u32(to as u64);
        self.schedule(
            now + self.model.hop_us + edge.extra_us,
            EventKind::Reach { req, node },
        );
    }

    fn on_reach(&mut self, now: u64, req: u32, node: usize) {
        self.advance(node, now);
        self.reqs[req as usize].entered = now;
        let measured = now >= self.warmup_us;
        let spec = &self.model.nodes[node];
        let state = &mut self.states[node];
        if measured {
            state.arrivals += 1;
        }
        if !state.up {
            self.attempt_failed(now, req, node, DropReason::Faulted);
        } else if state.busy < state.workers {
            self.start(now, req, node);
        } else if state.busy + state.waiting.len() < spec.capacity {
            state.waiting.push_back(req);
            state.max_queue = state.max_queue.max(state.waiting.len());
        } else {
            self.attempt_failed(now, req, node, DropReason::Rejected);
        }
    }

    /// Puts `req` on a free worker of `node`, or abandons it if it already timed out.
    fn start(&mut self, now: u64, req: u32, node: usize) {
        if now - self.reqs[req as usize].start > self.model.timeout_us {
            self.attempt_failed(now, req, node, DropReason::TimedOut);
            return;
        }
        let spec = &self.model.nodes[node];
        let mean_ms = if self.reqs[req as usize].read {
            spec.read_ms
        } else {
            spec.write_ms
        };
        let state = &mut self.states[node];
        let service_us = ms_to_us(state.rng.lognormal(mean_ms * state.slow, spec.sigma)).max(1);
        state.busy += 1;
        let epoch = state.epoch;
        let node = to_u32(node as u64);
        self.schedule(now + service_us, EventKind::Done { req, node, epoch });
    }

    /// Starts waiting requests while the node has free workers.
    fn pump(&mut self, now: u64, node: usize) {
        while self.states[node].up && self.states[node].busy < self.states[node].workers {
            let Some(next) = self.states[node].waiting.pop_front() else {
                break;
            };
            self.start(now, next, node);
        }
    }

    fn on_done(&mut self, now: u64, req: u32, node: usize, epoch: u32) {
        self.advance(node, now);
        if epoch != self.states[node].epoch {
            // The node crashed while serving this request.
            self.attempt_failed(now, req, node, DropReason::Faulted);
            return;
        }
        let entered = self.reqs[req as usize].entered;
        let state = &mut self.states[node];
        state.busy -= 1;
        if now >= self.warmup_us {
            state.completed += 1;
            state.latencies.push(to_u32(now - entered));
        }
        self.pump(now, node);
        self.after_service(now, req, node);
    }

    fn after_service(&mut self, now: u64, req: u32, node: usize) {
        let spec = &self.model.nodes[node];
        if spec.kind == NodeKind::Cache {
            let hit = self.reqs[req as usize].read && self.attrs.unit() < spec.hit_ratio;
            if hit {
                self.complete(now, req, node);
                return;
            }
            self.reqs[req as usize].cache_done = true;
            let own = self.pick_next(node).map(|to| (node, to));
            let picked = own.or_else(|| {
                let parent = self.reqs[req as usize].fallback.take()?;
                self.pick_next(parent).map(|to| (parent, to))
            });
            match picked {
                Some((owner, to)) => self.send(now, req, owner, to),
                None => self.complete(now, req, node),
            }
        } else {
            let target = self.route(req, node);
            self.forward(now, req, node, target);
        }
    }

    /// Chooses the next hop from a non-cache node: caches first for reads, then round-robin.
    fn route(&mut self, req: u32, node: usize) -> Option<usize> {
        let spec = &self.model.nodes[node];
        let r = &mut self.reqs[req as usize];
        if r.read && !r.cache_done && !spec.caches.is_empty() {
            if !spec.next.is_empty() {
                r.fallback = Some(node);
            }
            let state = &mut self.states[node];
            let pick = spec.caches[state.rr % spec.caches.len()];
            state.rr += 1;
            return Some(pick);
        }
        self.pick_next(node)
    }

    fn pick_next(&mut self, node: usize) -> Option<usize> {
        let spec = &self.model.nodes[node];
        if spec.next.is_empty() {
            return None;
        }
        let state = &mut self.states[node];
        let pick = spec.next[state.rr % spec.next.len()];
        state.rr += 1;
        Some(pick)
    }

    /// The response reached the client. A late response counts as a failed attempt.
    fn complete(&mut self, now: u64, req: u32, last: usize) {
        let done = now + self.model.hop_us;
        let age = done - self.reqs[req as usize].start;
        if age > self.model.timeout_us {
            self.attempt_failed(now, req, last, DropReason::TimedOut);
            return;
        }
        let latency = to_u32(age);
        if done >= self.warmup_us && done <= self.end_us {
            self.completed += 1;
            self.latencies.push(latency);
        }
        if let Some(b) = self.bucket(done) {
            b.completed += 1;
            b.latencies.push(latency);
        }
    }

    fn attempt_failed(&mut self, now: u64, req: u32, node: usize, why: DropReason) {
        let measured = now >= self.warmup_us;
        let state = &mut self.states[node];
        state.drops_all += 1;
        if measured {
            match why {
                DropReason::Rejected => state.rejected += 1,
                DropReason::TimedOut => state.timed_out += 1,
                DropReason::Faulted => state.faulted += 1,
            }
        }

        let r = &mut self.reqs[req as usize];
        let retry_at = now + self.model.backoff_us;
        if r.attempt < self.model.max_retries && retry_at - r.start < self.model.timeout_us {
            r.attempt += 1;
            if measured {
                self.retries += 1;
            }
            self.schedule(retry_at, EventKind::Retry { req });
            return;
        }
        if measured {
            self.errors += 1;
        }
        if let Some(b) = self.bucket(now) {
            b.errors += 1;
        }
    }

    /// Switches a fault on or off and recomputes every node and edge it touches.
    fn set_fault(&mut self, now: u64, fault: usize, on: bool) {
        for node in 0..self.states.len() {
            self.advance(node, now);
        }
        self.active[fault] = on;

        let n = self.states.len();
        let mut up = vec![true; n];
        let mut slow = vec![1.0; n];
        let mut lost = vec![0.0; n];
        let mut extra = vec![0u64; self.edges.len()];
        let mut delivered = vec![1.0; self.edges.len()];
        for (f, _) in self.faults.iter().zip(&self.active).filter(|(_, a)| **a) {
            match (f.target, &f.kind) {
                (FaultTarget::Node(i), FaultKind::Crash) => up[i] = false,
                (FaultTarget::Node(i), FaultKind::Slowdown { factor }) => slow[i] *= factor,
                (FaultTarget::Node(i), FaultKind::LostWorkers { fraction }) => lost[i] += fraction,
                (FaultTarget::Edge(i), FaultKind::EdgeLatency { extra_ms }) => {
                    extra[i] += ms_to_us(*extra_ms);
                }
                (FaultTarget::Edge(i), FaultKind::EdgeLoss { loss }) => delivered[i] *= 1.0 - loss,
                _ => {}
            }
        }
        for (i, edge) in self.edges.iter_mut().enumerate() {
            edge.extra_us = extra[i];
            edge.loss = 1.0 - delivered[i];
        }
        for node in 0..n {
            self.update_node(now, node, up[node], slow[node], lost[node]);
        }
    }

    fn update_node(&mut self, now: u64, node: usize, up: bool, slow: f64, lost: f64) {
        let configured = self.model.nodes[node].servers;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let workers = if configured == 0 {
            0
        } else {
            (((configured as f64) * (1.0 - lost.min(0.95))).floor() as usize).max(1)
        };
        let state = &mut self.states[node];
        let was_up = state.up;
        let changed = state.workers != workers;
        state.up = up;
        state.slow = slow;
        state.workers = workers;
        if was_up && !up {
            // Crash: everything in service is lost (see `Done::epoch`); the queue is flushed.
            state.epoch += 1;
            state.busy = 0;
            let flushed: Vec<u32> = state.waiting.drain(..).collect();
            for req in flushed {
                self.attempt_failed(now, req, node, DropReason::Faulted);
            }
        } else if up && (changed || !was_up) {
            self.pump(now, node);
        }
    }

    /// Accumulates busy, available and queue time up to `now`, counting only the measured window.
    fn advance(&mut self, node: usize, now: u64) {
        let state = &mut self.states[node];
        let all_from = state.last_t.min(self.end_us);
        let all_to = now.min(self.end_us);
        if all_to > all_from {
            #[allow(clippy::cast_precision_loss)]
            {
                let dt = (all_to - all_from) as f64;
                state.tick_busy += state.busy as f64 * dt;
                state.tick_avail += available(state) as f64 * dt;
            }
        }
        let from = state.last_t.max(self.warmup_us);
        let to = now.min(self.end_us);
        if to > from {
            #[allow(clippy::cast_precision_loss)]
            {
                let dt = (to - from) as f64;
                state.busy_area += state.busy as f64 * dt;
                state.avail_area += available(state) as f64 * dt;
                state.queue_area += state.waiting.len() as f64 * dt;
            }
        }
        state.last_t = now;
    }

    /// Closes the books at the end of the run and builds the report.
    pub fn finish(mut self) -> SimReport {
        for node in 0..self.states.len() {
            self.advance(node, self.end_us);
        }
        let options = self.options.clone();
        let window_s = options.duration_s - options.warmup_s;
        let nodes = self.node_reports(window_s);
        let bottleneck = find_bottleneck(&nodes);
        let faults = self.fault_impacts();
        let timeline = self.timeline(options.duration_s);
        let summary = Summary {
            offered: self.offered,
            completed: self.completed,
            errors: self.errors,
            retries: self.retries,
            throughput_rps: f64::from(self.completed) / window_s,
            error_rate: ratio(self.errors, self.offered),
            latency: latency(&mut self.latencies),
        };
        SimReport {
            notice: ESTIMATE_NOTICE.to_owned(),
            seed: options.seed,
            duration_s: options.duration_s,
            warmup_s: options.warmup_s,
            summary,
            nodes,
            bottleneck,
            timeline,
            faults,
            controls: self.controls.clone(),
            assumptions: self.model.assumptions.clone(),
        }
    }

    fn node_reports(&mut self, window_s: f64) -> Vec<NodeReport> {
        let window_us = window_s * 1e6;
        let mut out = Vec::new();
        for (spec, state) in self.model.nodes.iter().zip(self.states.iter_mut()) {
            if spec.kind == NodeKind::Client {
                continue;
            }
            let utilization = if state.avail_area > 0.0 {
                (state.busy_area / state.avail_area).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let dropped = state.rejected + state.timed_out + state.faulted;
            let severity = severity_for(utilization, dropped);
            out.push(NodeReport {
                id: spec.id.clone(),
                label: spec.label.clone(),
                kind: spec.kind,
                servers: to_u32(spec.servers as u64),
                capacity: to_u32(spec.capacity as u64),
                arrivals: state.arrivals,
                completed: state.completed,
                rejected: state.rejected,
                timed_out: state.timed_out,
                faulted: state.faulted,
                utilization,
                mean_queue: state.queue_area / window_us,
                max_queue: to_u32(state.max_queue as u64),
                latency: latency(&mut state.latencies),
                severity,
            });
        }
        out
    }

    fn timeline(&mut self, duration_s: f64) -> Vec<TimelinePoint> {
        self.buckets
            .iter_mut()
            .enumerate()
            .map(|(i, b)| {
                #[allow(clippy::cast_precision_loss)]
                let start = i as f64;
                let width = (duration_s - start).min(1.0);
                b.latencies.sort_unstable();
                TimelinePoint {
                    t_s: to_u32(i as u64),
                    offered_rps: f64::from(b.offered) / width,
                    completed_rps: f64::from(b.completed) / width,
                    error_rps: f64::from(b.errors) / width,
                    p95_ms: percentile_sorted(&b.latencies, 0.95),
                }
            })
            .collect()
    }

    /// Compares each fault window with equally long windows before and after it.
    fn fault_impacts(&self) -> Vec<FaultImpact> {
        let n = self.buckets.len();
        self.faults
            .iter()
            .filter(|f| f.start_us <= self.end_us)
            .map(|f| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let (start_b, end_b) = (
                    ((f.start_us / US_PER_S) as usize).min(n),
                    (f.end_us.div_ceil(US_PER_S) as usize).min(n),
                );
                let width = end_b.saturating_sub(start_b).max(1);
                let before = self.window(start_b.saturating_sub(width), start_b);
                let during = self.window(start_b, end_b);
                let after = self.window(end_b, (end_b + width).min(n));
                let recovery_s = self.recovery(end_b, &before);
                #[allow(clippy::cast_precision_loss)]
                FaultImpact {
                    fault_id: f.id.clone(),
                    label: f.label.clone(),
                    start_s: f.start_us as f64 / 1e6,
                    end_s: f.end_us as f64 / 1e6,
                    before,
                    during,
                    after,
                    recovery_s,
                }
            })
            .collect()
    }

    fn window(&self, from: usize, to: usize) -> WindowStats {
        let buckets = &self.buckets[from.min(to)..to];
        let (mut done, mut failed) = (0u32, 0u32);
        let mut samples: Vec<u32> = Vec::new();
        for b in buckets {
            done += b.completed;
            failed += b.errors;
            samples.extend_from_slice(&b.latencies);
        }
        samples.sort_unstable();
        #[allow(clippy::cast_precision_loss)]
        let seconds = buckets.len().max(1) as f64;
        WindowStats {
            error_rate: ratio(failed, done + failed),
            p95_ms: percentile_sorted(&samples, 0.95),
            throughput_rps: f64::from(done) / seconds,
        }
    }

    /// First second after the fault from which two consecutive seconds look like the baseline.
    fn recovery(&self, end_b: usize, before: &WindowStats) -> Option<f64> {
        let healthy = |i: usize| {
            let Some(b) = self.buckets.get(i) else {
                return false;
            };
            let total = b.completed + b.errors;
            if total == 0 {
                return false;
            }
            let mut samples = b.latencies.clone();
            samples.sort_unstable();
            let p95 = percentile_sorted(&samples, 0.95);
            let slack = (before.p95_ms * 1.25).max(before.p95_ms + 5.0);
            ratio(b.errors, total) <= before.error_rate + 0.01
                && (before.p95_ms == 0.0 || p95 <= slack)
        };
        (end_b..self.buckets.len().saturating_sub(1))
            .find(|&i| healthy(i) && healthy(i + 1))
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                {
                    (i - end_b) as f64
                }
            })
    }
}

/// Workers able to serve right now: none while the node is down.
fn available(state: &NodeState) -> usize {
    if state.up { state.workers } else { 0 }
}

fn find_bottleneck(nodes: &[NodeReport]) -> Option<Bottleneck> {
    let drops = |n: &NodeReport| n.rejected + n.timed_out + n.faulted;
    let worst = nodes
        .iter()
        .filter(|n| n.severity != Severity::Healthy)
        .max_by(|a, b| {
            drops(a)
                .cmp(&drops(b))
                .then(a.utilization.total_cmp(&b.utilization))
        })?;
    let mut reason = format!("{:.0}% busy", worst.utilization * 100.0);
    if worst.rejected > 0 {
        reason.push_str(&format!(
            ", {} requests rejected (queue full)",
            worst.rejected
        ));
    }
    if worst.timed_out > 0 {
        reason.push_str(&format!(", {} requests timed out", worst.timed_out));
    }
    if worst.faulted > 0 {
        reason.push_str(&format!(
            ", {} requests lost to injected faults",
            worst.faulted
        ));
    }
    Some(Bottleneck {
        node_id: worst.id.clone(),
        label: worst.label.clone(),
        severity: worst.severity,
        reason,
    })
}

pub(crate) fn ratio(part: u32, whole: u32) -> f64 {
    if whole == 0 {
        0.0
    } else {
        f64::from(part) / f64::from(whole)
    }
}

pub(crate) fn to_u32(v: u64) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

pub(crate) fn latency(samples: &mut [u32]) -> Latency {
    samples.sort_unstable();
    #[allow(clippy::cast_precision_loss)]
    let mean = if samples.is_empty() {
        0.0
    } else {
        samples.iter().map(|&v| f64::from(v)).sum::<f64>() / samples.len() as f64
    };
    Latency {
        mean_ms: mean / 1_000.0,
        p50_ms: percentile_sorted(samples, 0.50),
        p95_ms: percentile_sorted(samples, 0.95),
        p99_ms: percentile_sorted(samples, 0.99),
    }
}

/// Nearest-rank percentile of sorted microsecond samples, in milliseconds.
pub(crate) fn percentile_sorted(sorted: &[u32], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let rank = ((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    f64::from(sorted[rank - 1]) / 1_000.0
}
