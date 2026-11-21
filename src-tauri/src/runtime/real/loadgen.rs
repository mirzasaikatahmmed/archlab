//! The load generator: sends requests into the cluster at the configured rate,
//! with the same Poisson arrivals and spikes as the simulation, and records what happened.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use tokio::time::sleep_until;

use super::node::Cluster;
use super::protocol::Status;
use crate::sim::engine::to_u32;
use crate::sim::rng::Stream;

#[derive(Default)]
pub struct Bucket {
    pub offered: u32,
    pub completed: u32,
    pub errors: u32,
    pub latencies: Vec<u32>,
}

pub struct Collector {
    pub buckets: Mutex<Vec<Bucket>>,
    pub offered: AtomicU32,
    pub completed: AtomicU32,
    pub errors: AtomicU32,
    pub latencies: Mutex<Vec<u32>>,
    pub duration: Duration,
    pub warmup: Duration,
}

impl Collector {
    pub fn new(duration: Duration, warmup: Duration) -> Self {
        let seconds = usize::try_from(duration.as_secs() + 1).unwrap_or(1);
        Self {
            buckets: Mutex::new((0..seconds).map(|_| Bucket::default()).collect()),
            offered: AtomicU32::new(0),
            completed: AtomicU32::new(0),
            errors: AtomicU32::new(0),
            latencies: Mutex::new(Vec::new()),
            duration,
            warmup,
        }
    }

    fn with_bucket(&self, since: Duration, f: impl FnOnce(&mut Bucket)) {
        let i = usize::try_from(since.as_secs()).unwrap_or(usize::MAX);
        let mut buckets = self.buckets.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(b) = buckets.get_mut(i) {
            f(b);
        }
    }

    fn arrival(&self, since: Duration) {
        if since >= self.warmup {
            self.offered.fetch_add(1, Ordering::Relaxed);
        }
        self.with_bucket(since, |b| b.offered += 1);
    }

    fn finished(&self, since: Duration, status: Status, latency_us: u32) {
        let ok = status.is_success();
        if since >= self.warmup && since <= self.duration {
            if ok {
                self.completed.fetch_add(1, Ordering::Relaxed);
                self.latencies
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(latency_us);
            } else {
                self.errors.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.with_bucket(since, |b| {
            if ok {
                b.completed += 1;
                b.latencies.push(latency_us);
            } else {
                b.errors += 1;
            }
        });
    }
}

pub struct Params {
    pub seed: u64,
    pub timeout: Duration,
}

/// Sends traffic until the duration is over or `cancel` is set.
pub async fn generate(
    cluster: &Arc<Cluster>,
    collector: &Arc<Collector>,
    params: &Params,
    cancel: &AtomicBool,
) {
    let model = &cluster.model;
    let start = Instant::now();
    let _ = cluster.start.set(start);
    let end = start + collector.duration;
    let peak = model.rps * model.peak_multiplier();
    let mut arrivals = Stream::new(params.seed, 0);
    let mut attrs = Stream::new(params.seed, 1);
    let mut next = start;
    let mut client_rr = 0usize;

    while !cancel.load(Ordering::Relaxed) {
        next += Duration::from_secs_f64(arrivals.exponential(1.0 / peak));
        if next >= end {
            break;
        }
        if next > Instant::now() + Duration::from_micros(500) {
            sleep_until(tokio::time::Instant::from_std(next)).await;
        }
        let now = Instant::now();
        let t_s = now.duration_since(start).as_secs_f64();
        if arrivals.unit() >= model.rate_multiplier(t_s) / model.peak_multiplier() {
            continue;
        }
        let read = attrs.unit() < model.read_ratio;
        let client = model.clients[client_rr % model.clients.len()];
        client_rr += 1;
        collector.arrival(now.duration_since(start));

        let cluster = Arc::clone(cluster);
        let collector = Arc::clone(collector);
        let timeout = params.timeout;
        tokio::spawn(async move {
            let sent = Instant::now();
            let call = cluster.route(client, read, false, sent + timeout);
            let status = tokio::time::timeout(timeout, call)
                .await
                .unwrap_or(Status::TimedOut);
            let done = Instant::now();
            let latency = done.duration_since(sent) + cluster.hop;
            let since = done.duration_since(start);
            collector.finished(
                since,
                status,
                to_u32(u64::try_from(latency.as_micros()).unwrap_or(u64::MAX)),
            );
        });
    }

    if !cancel.load(Ordering::Relaxed) {
        sleep_until(tokio::time::Instant::from_std(end)).await;
    }
}

/// Samples queue lengths every 10 ms for the mean queue figure.
pub async fn sample_queues(cluster: Arc<Cluster>) {
    let mut tick = tokio::time::interval(Duration::from_millis(10));
    loop {
        tick.tick().await;
        if !cluster.measuring() {
            continue;
        }
        for node in &cluster.nodes {
            let waiting = u64::try_from(node.waiting.load(Ordering::Relaxed)).unwrap_or(0);
            node.stats.queue_sum.fetch_add(waiting, Ordering::Relaxed);
            node.stats.queue_samples.fetch_add(1, Ordering::Relaxed);
        }
    }
}
