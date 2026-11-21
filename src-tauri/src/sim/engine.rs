//! Discrete-event engine. Time is an integer number of microseconds, so the
//! event order never depends on floating-point rounding of timestamps.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};

use super::SimError;
use super::model::{Model, ms_to_us};
use super::report::{
    Bottleneck, ESTIMATE_NOTICE, Latency, NodeReport, Severity, SimOptions, SimReport, Summary,
    TimelinePoint,
};
use super::rng::Stream;
use crate::domain::topology::NodeKind;

/// Hard stop so a runaway scenario cannot freeze the app.
pub const MAX_EVENTS: u64 = 5_000_000;
/// A request older than this when it reaches a worker is abandoned.
const TIMEOUT_US: u64 = 10_000_000;
const US_PER_S: u64 = 1_000_000;

#[derive(Debug, Clone, Copy)]
enum EventKind {
    Arrival,
    Reach { req: u32, node: u32 },
    Done { req: u32, node: u32 },
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
}

#[derive(Clone, Copy)]
enum DropReason {
    Rejected,
    TimedOut,
}

struct NodeState {
    rng: Stream,
    busy: usize,
    waiting: VecDeque<u32>,
    rr: usize,
    last_t: u64,
    busy_area: f64,
    queue_area: f64,
    max_queue: usize,
    arrivals: u32,
    completed: u32,
    rejected: u32,
    timed_out: u32,
    latencies: Vec<u32>,
}

#[derive(Default)]
struct Bucket {
    offered: u32,
    completed: u32,
    errors: u32,
    latencies: Vec<u32>,
}

struct Engine<'a> {
    model: &'a Model,
    heap: BinaryHeap<Event>,
    seq: u64,
    events: u64,
    reqs: Vec<Request>,
    states: Vec<NodeState>,
    arrivals: Stream,
    attrs: Stream,
    client_rr: usize,
    warmup_us: u64,
    end_us: u64,
    offered: u32,
    completed: u32,
    errors: u32,
    latencies: Vec<u32>,
    buckets: Vec<Bucket>,
}

pub fn run(model: &Model, options: &SimOptions) -> Result<SimReport, SimError> {
    let seed = u64::from(options.seed);
    let end_us = ms_to_us(options.duration_s * 1_000.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let bucket_count = options.duration_s.ceil() as usize;
    let mut engine = Engine {
        model,
        heap: BinaryHeap::new(),
        seq: 0,
        events: 0,
        reqs: Vec::new(),
        states: (0..model.nodes.len())
            .map(|i| NodeState {
                rng: Stream::new(seed, 2 + i as u64),
                busy: 0,
                waiting: VecDeque::new(),
                rr: 0,
                last_t: 0,
                busy_area: 0.0,
                queue_area: 0.0,
                max_queue: 0,
                arrivals: 0,
                completed: 0,
                rejected: 0,
                timed_out: 0,
                latencies: Vec::new(),
            })
            .collect(),
        arrivals: Stream::new(seed, 0),
        attrs: Stream::new(seed, 1),
        client_rr: 0,
        warmup_us: ms_to_us(options.warmup_s * 1_000.0),
        end_us,
        offered: 0,
        completed: 0,
        errors: 0,
        latencies: Vec::new(),
        buckets: (0..bucket_count).map(|_| Bucket::default()).collect(),
    };
    engine.simulate()?;
    Ok(engine.report(options))
}

impl Engine<'_> {
    fn schedule(&mut self, time: u64, kind: EventKind) {
        self.seq += 1;
        self.heap.push(Event {
            time,
            seq: self.seq,
            kind,
        });
    }

    fn simulate(&mut self) -> Result<(), SimError> {
        let first = self.next_arrival_gap();
        self.schedule(first, EventKind::Arrival);
        while let Some(event) = self.heap.pop() {
            if event.time > self.end_us {
                break;
            }
            self.events += 1;
            if self.events > MAX_EVENTS {
                return Err(SimError::EventLimit { limit: MAX_EVENTS });
            }
            match event.kind {
                EventKind::Arrival => self.on_arrival(event.time),
                EventKind::Reach { req, node } => self.on_reach(event.time, req, node as usize),
                EventKind::Done { req, node } => self.on_done(event.time, req, node as usize),
            }
        }
        for node in 0..self.states.len() {
            self.advance(node, self.end_us);
        }
        Ok(())
    }

    /// Candidate gap of the Poisson process at the peak rate, in microseconds.
    fn next_arrival_gap(&mut self) -> u64 {
        let peak_rps = self.model.rps * self.model.peak_multiplier();
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
        let keep = self.model.rate_multiplier(t_s) / self.model.peak_multiplier();
        if self.arrivals.unit() >= keep {
            return;
        }

        let read = self.attrs.unit() < self.model.read_ratio;
        let id = to_u32(self.reqs.len() as u64);
        self.reqs.push(Request {
            start: now,
            entered: now,
            read,
            fallback: None,
            cache_done: false,
        });
        if now >= self.warmup_us {
            self.offered += 1;
        }
        if let Some(b) = self.bucket(now) {
            b.offered += 1;
        }

        let client = self.model.clients[self.client_rr % self.model.clients.len()];
        self.client_rr += 1;
        let target = self.route(id, client);
        self.forward(now, id, target);
    }

    fn send(&mut self, now: u64, req: u32, node: usize) {
        let node = to_u32(node as u64);
        self.schedule(now + self.model.hop_us, EventKind::Reach { req, node });
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
        if state.busy < spec.servers {
            self.start(now, req, node);
        } else if state.busy + state.waiting.len() < spec.capacity {
            state.waiting.push_back(req);
            state.max_queue = state.max_queue.max(state.waiting.len());
        } else {
            self.drop_request(now, node, DropReason::Rejected);
        }
    }

    /// Puts `req` on a free worker of `node`, or abandons it if it already timed out.
    fn start(&mut self, now: u64, req: u32, node: usize) {
        if now - self.reqs[req as usize].start > TIMEOUT_US {
            self.drop_request(now, node, DropReason::TimedOut);
            return;
        }
        let spec = &self.model.nodes[node];
        let mean_ms = if self.reqs[req as usize].read {
            spec.read_ms
        } else {
            spec.write_ms
        };
        let state = &mut self.states[node];
        let service_us = ms_to_us(state.rng.lognormal(mean_ms, spec.sigma)).max(1);
        state.busy += 1;
        let node = to_u32(node as u64);
        self.schedule(now + service_us, EventKind::Done { req, node });
    }

    fn on_done(&mut self, now: u64, req: u32, node: usize) {
        self.advance(node, now);
        let entered = self.reqs[req as usize].entered;
        let state = &mut self.states[node];
        state.busy -= 1;
        if now >= self.warmup_us {
            state.completed += 1;
            state.latencies.push(to_u32(now - entered));
        }

        while self.states[node].busy < self.model.nodes[node].servers {
            let Some(next) = self.states[node].waiting.pop_front() else {
                break;
            };
            self.start(now, next, node);
        }

        self.after_service(now, req, node);
    }

    fn after_service(&mut self, now: u64, req: u32, node: usize) {
        let spec = &self.model.nodes[node];
        if spec.kind == NodeKind::Cache {
            let hit = self.reqs[req as usize].read && self.attrs.unit() < spec.hit_ratio;
            if hit {
                self.finish(now, req);
                return;
            }
            self.reqs[req as usize].cache_done = true;
            let target = self.pick_next(node).or_else(|| {
                let parent = self.reqs[req as usize].fallback.take()?;
                self.pick_next(parent)
            });
            self.forward(now, req, target);
        } else {
            let target = self.route(req, node);
            self.forward(now, req, target);
        }
    }

    fn forward(&mut self, now: u64, req: u32, target: Option<usize>) {
        match target {
            Some(next) => self.send(now, req, next),
            None => self.finish(now, req),
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

    fn finish(&mut self, now: u64, req: u32) {
        let done = now + self.model.hop_us;
        let latency = to_u32(done - self.reqs[req as usize].start);
        if done >= self.warmup_us && done <= self.end_us {
            self.completed += 1;
            self.latencies.push(latency);
        }
        if let Some(b) = self.bucket(done) {
            b.completed += 1;
            b.latencies.push(latency);
        }
    }

    fn drop_request(&mut self, now: u64, node: usize, why: DropReason) {
        if now >= self.warmup_us {
            self.errors += 1;
            let state = &mut self.states[node];
            match why {
                DropReason::Rejected => state.rejected += 1,
                DropReason::TimedOut => state.timed_out += 1,
            }
        }
        if let Some(b) = self.bucket(now) {
            b.errors += 1;
        }
    }

    /// Accumulates busy and queue time up to `now`, counting only the measured window.
    fn advance(&mut self, node: usize, now: u64) {
        let state = &mut self.states[node];
        let from = state.last_t.max(self.warmup_us);
        let to = now.min(self.end_us);
        if to > from {
            #[allow(clippy::cast_precision_loss)]
            {
                let dt = (to - from) as f64;
                state.busy_area += state.busy as f64 * dt;
                state.queue_area += state.waiting.len() as f64 * dt;
            }
        }
        state.last_t = now;
    }

    fn report(mut self, options: &SimOptions) -> SimReport {
        let window_s = options.duration_s - options.warmup_s;
        let nodes = self.node_reports(window_s);
        let bottleneck = find_bottleneck(&nodes);
        let timeline = self.timeline(options.duration_s);
        let summary = Summary {
            offered: self.offered,
            completed: self.completed,
            errors: self.errors,
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
            #[allow(clippy::cast_precision_loss)]
            let utilization = (state.busy_area / (spec.servers as f64 * window_us)).clamp(0.0, 1.0);
            let dropped = state.rejected + state.timed_out;
            let severity = if utilization >= 0.9 || dropped > 0 {
                Severity::Saturated
            } else if utilization >= 0.7 {
                Severity::Warning
            } else {
                Severity::Healthy
            };
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
}

fn find_bottleneck(nodes: &[NodeReport]) -> Option<Bottleneck> {
    let drops = |n: &NodeReport| n.rejected + n.timed_out;
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
    Some(Bottleneck {
        node_id: worst.id.clone(),
        label: worst.label.clone(),
        severity: worst.severity,
        reason,
    })
}

fn ratio(part: u32, whole: u32) -> f64 {
    if whole == 0 {
        0.0
    } else {
        f64::from(part) / f64::from(whole)
    }
}

fn to_u32(v: u64) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

fn latency(samples: &mut [u32]) -> Latency {
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
fn percentile_sorted(sorted: &[u32], p: f64) -> f64 {
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
