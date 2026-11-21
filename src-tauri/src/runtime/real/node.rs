//! Real nodes: TCP servers on localhost that queue, do synthetic work, and forward
//! requests along the topology's edges, the way the simulation does.

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Semaphore, mpsc, oneshot};

use super::protocol::{
    FRAME_LEN, Request, Response, Status, decode_request, decode_response, encode_request,
    encode_response, read_frame,
};
use crate::domain::topology::NodeKind;
use crate::runtime::calibrate::chain;
use crate::sim::engine::to_u32;
use crate::sim::model::Model;
use crate::sim::rng::Stream;

/// Connections per link; requests are spread over them round-robin.
const CONNECTIONS_PER_LINK: usize = 4;
/// CPU bursts shorter than this run in place; longer ones hand the thread over first.
const INLINE_BURN_MS: f64 = 1.0;

#[derive(Default)]
pub struct NodeStats {
    pub arrivals: AtomicU32,
    pub completed: AtomicU32,
    pub rejected: AtomicU32,
    pub timed_out: AtomicU32,
    pub busy_us: AtomicU64,
    pub latencies: Mutex<Vec<u32>>,
    pub queue_sum: AtomicU64,
    pub queue_samples: AtomicU64,
    pub max_queue: AtomicUsize,
}

pub struct NodeRuntime {
    workers: Semaphore,
    /// Requests waiting or being worked on, bounded by the node's capacity.
    in_system: AtomicUsize,
    pub waiting: AtomicUsize,
    rr: AtomicUsize,
    rng: Mutex<Stream>,
    pub stats: NodeStats,
}

/// Everything the nodes and the load generator share while a real run is going.
pub struct Cluster {
    pub model: Model,
    pub nodes: Vec<NodeRuntime>,
    /// Network latency added before every hop.
    pub hop: Duration,
    /// When measuring began; set once by the load generator.
    pub start: OnceLock<Instant>,
    pub warmup: Duration,
    /// CPU chain steps that take one millisecond on this machine.
    pub steps_per_ms: f64,
    links: OnceLock<HashMap<(usize, usize), Link>>,
}

impl Cluster {
    pub fn new(model: Model, seed: u64, warmup: Duration, steps_per_ms: f64) -> Self {
        let hop = Duration::from_micros(model.hop_us);
        let nodes = model
            .nodes
            .iter()
            .enumerate()
            .map(|(i, spec)| NodeRuntime {
                workers: Semaphore::new(spec.servers),
                in_system: AtomicUsize::new(0),
                waiting: AtomicUsize::new(0),
                rr: AtomicUsize::new(0),
                rng: Mutex::new(Stream::new(seed, 100 + i as u64)),
                stats: NodeStats::default(),
            })
            .collect();
        Self {
            model,
            nodes,
            hop,
            start: OnceLock::new(),
            warmup,
            steps_per_ms,
            links: OnceLock::new(),
        }
    }

    /// True once the warm-up period of the measurement is over.
    pub fn measuring(&self) -> bool {
        self.start.get().is_some_and(|s| s.elapsed() >= self.warmup)
    }

    pub fn set_links(&self, links: HashMap<(usize, usize), Link>) {
        // Set once, before the first request.
        let _ = self.links.set(links);
    }

    fn rng<T>(&self, node: usize, f: impl FnOnce(&mut Stream) -> T) -> T {
        f(&mut self.nodes[node]
            .rng
            .lock()
            .unwrap_or_else(PoisonError::into_inner))
    }

    fn next_of(&self, node: usize, list: &[usize]) -> Option<usize> {
        if list.is_empty() {
            return None;
        }
        let n = self.nodes[node].rr.fetch_add(1, Ordering::Relaxed);
        Some(list[n % list.len()])
    }

    /// Takes a request through `node`: admission, a worker, synthetic work, then onward.
    pub async fn serve(&self, node: usize, req: Request) -> Status {
        let entered = Instant::now();
        let deadline = entered + Duration::from_micros(u64::from(req.budget_us));
        let spec = &self.model.nodes[node];
        let runtime = &self.nodes[node];
        let measured = self.measuring();
        if measured {
            runtime.stats.arrivals.fetch_add(1, Ordering::Relaxed);
        }

        if runtime.in_system.fetch_add(1, Ordering::AcqRel) >= spec.capacity {
            runtime.in_system.fetch_sub(1, Ordering::AcqRel);
            if measured {
                runtime.stats.rejected.fetch_add(1, Ordering::Relaxed);
            }
            return Status::Rejected;
        }
        let waiting = runtime.waiting.fetch_add(1, Ordering::AcqRel) + 1;
        runtime
            .stats
            .max_queue
            .fetch_max(waiting, Ordering::Relaxed);
        let permit = runtime.workers.acquire().await;
        runtime.waiting.fetch_sub(1, Ordering::AcqRel);
        let Ok(permit) = permit else {
            runtime.in_system.fetch_sub(1, Ordering::AcqRel);
            return Status::Failed;
        };

        if Instant::now() > deadline {
            drop(permit);
            runtime.in_system.fetch_sub(1, Ordering::AcqRel);
            if measured {
                runtime.stats.timed_out.fetch_add(1, Ordering::Relaxed);
            }
            return Status::TimedOut;
        }

        let started = Instant::now();
        self.work(node, &req).await;
        let worked = started.elapsed();
        drop(permit);
        runtime.in_system.fetch_sub(1, Ordering::AcqRel);
        if measured {
            let stats = &runtime.stats;
            stats.completed.fetch_add(1, Ordering::Relaxed);
            stats.busy_us.fetch_add(
                u64::try_from(worked.as_micros()).unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
            stats
                .latencies
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(to_u32(
                    u64::try_from(entered.elapsed().as_micros()).unwrap_or(u64::MAX),
                ));
        }
        self.route(node, req.read, req.cache_done, deadline).await
    }

    /// Spends the node's service time: a CPU burn for the CPU share, then a wait for the I/O share.
    async fn work(&self, node: usize, req: &Request) {
        let spec = &self.model.nodes[node];
        let mean_ms = if req.read {
            spec.read_ms
        } else {
            spec.write_ms
        };
        let total_ms = self.rng(node, |r| r.lognormal(mean_ms, spec.sigma));
        let cpu_ms = total_ms * spec.cpu_share;
        let io_ms = total_ms - cpu_ms;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = (cpu_ms * self.steps_per_ms) as u64;
        if steps > 0 {
            let seed = req.id;
            if cpu_ms < INLINE_BURN_MS {
                chain(seed, steps);
            } else {
                tokio::task::block_in_place(|| chain(seed, steps));
            }
        }
        if io_ms > 0.05 {
            tokio::time::sleep(Duration::from_secs_f64(io_ms / 1_000.0)).await;
        }
    }

    /// Decides where a request goes after `node`: caches first for reads, then onward.
    pub async fn route(
        &self,
        node: usize,
        read: bool,
        cache_done: bool,
        deadline: Instant,
    ) -> Status {
        let spec = &self.model.nodes[node];
        if spec.kind == NodeKind::Cache {
            if read && self.rng(node, Stream::unit) < spec.hit_ratio {
                return Status::Hit;
            }
            return match self.next_of(node, &spec.next) {
                Some(next) => self.call(node, next, read, true, deadline).await,
                None => Status::Miss,
            };
        }

        let mut cache_done = cache_done;
        if read
            && !cache_done
            && let Some(cache) = self.next_of(node, &spec.caches)
        {
            match self.call(node, cache, read, false, deadline).await {
                Status::Hit | Status::Ok => return Status::Ok,
                Status::Miss => cache_done = true,
                failure => return failure,
            }
        }
        match self.next_of(node, &spec.next) {
            Some(next) => match self.call(node, next, read, cache_done, deadline).await {
                Status::Hit | Status::Ok | Status::Miss => Status::Ok,
                failure => failure,
            },
            None => Status::Ok,
        }
    }

    async fn call(
        &self,
        from: usize,
        to: usize,
        read: bool,
        cache_done: bool,
        deadline: Instant,
    ) -> Status {
        if !self.hop.is_zero() {
            tokio::time::sleep(self.hop).await;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Status::TimedOut;
        }
        let Some(link) = self.links.get().and_then(|l| l.get(&(from, to))) else {
            return Status::Failed;
        };
        let budget_us = u32::try_from(left.as_micros()).unwrap_or(u32::MAX);
        link.request(read, cache_done, budget_us).await
    }
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Status>>>>;

struct Connection {
    outbox: mpsc::UnboundedSender<[u8; FRAME_LEN]>,
    pending: Pending,
}

/// A few pooled connections to one node, multiplexing requests by id.
pub struct Link {
    connections: Vec<Connection>,
    next: AtomicUsize,
    next_id: AtomicU64,
}

impl Link {
    pub async fn connect(port: u16) -> io::Result<Self> {
        let mut connections = Vec::with_capacity(CONNECTIONS_PER_LINK);
        for _ in 0..CONNECTIONS_PER_LINK {
            let stream = TcpStream::connect(("127.0.0.1", port)).await?;
            stream.set_nodelay(true)?;
            let (mut reader, mut writer) = stream.into_split();
            let pending: Pending = Arc::default();
            let (outbox, mut inbox) = mpsc::unbounded_channel::<[u8; FRAME_LEN]>();
            tokio::spawn(async move {
                while let Some(frame) = inbox.recv().await {
                    if writer.write_all(&frame).await.is_err() {
                        break;
                    }
                }
            });
            let waiting = Arc::clone(&pending);
            tokio::spawn(async move {
                while let Ok(frame) = read_frame(&mut reader).await {
                    let response = decode_response(&frame);
                    let sender = waiting
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .remove(&response.id);
                    if let Some(sender) = sender {
                        let _ = sender.send(response.status);
                    }
                }
                // The connection ended: everything still waiting has failed.
                let orphans: Vec<_> = waiting
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .drain()
                    .collect();
                for (_, sender) in orphans {
                    let _ = sender.send(Status::Failed);
                }
            });
            connections.push(Connection { outbox, pending });
        }
        Ok(Self {
            connections,
            next: AtomicUsize::new(0),
            next_id: AtomicU64::new(1),
        })
    }

    pub async fn request(&self, read: bool, cache_done: bool, budget_us: u32) -> Status {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let conn =
            &self.connections[self.next.fetch_add(1, Ordering::Relaxed) % self.connections.len()];
        let (tx, rx) = oneshot::channel();
        conn.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, tx);
        let frame = encode_request(&Request {
            id,
            read,
            cache_done,
            budget_us,
        });
        if conn.outbox.send(frame).is_err() {
            return Status::Failed;
        }
        rx.await.unwrap_or(Status::Failed)
    }
}

/// Accepts connections for `node` until the runtime shuts down.
pub async fn serve_listener(cluster: Arc<Cluster>, node: usize, listener: TcpListener) {
    while let Ok((stream, _)) = listener.accept().await {
        if stream.set_nodelay(true).is_err() {
            continue;
        }
        let cluster = Arc::clone(&cluster);
        tokio::spawn(async move {
            let (mut reader, mut writer) = stream.into_split();
            let (outbox, mut inbox) = mpsc::unbounded_channel::<[u8; FRAME_LEN]>();
            tokio::spawn(async move {
                while let Some(frame) = inbox.recv().await {
                    if writer.write_all(&frame).await.is_err() {
                        break;
                    }
                }
            });
            while let Ok(frame) = read_frame(&mut reader).await {
                let request = decode_request(&frame);
                let cluster = Arc::clone(&cluster);
                let outbox = outbox.clone();
                tokio::spawn(async move {
                    let status = cluster.serve(node, request).await;
                    let _ = outbox.send(encode_response(&Response {
                        id: request.id,
                        status,
                    }));
                });
            }
        });
    }
}
