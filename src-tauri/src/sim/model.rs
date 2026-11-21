//! Compiles a validated project into the numbers the engine runs on.

use std::collections::BTreeMap;

use crate::domain::chaos::{Fault, FaultKind};
use crate::domain::project::Project;
use crate::domain::server_profile::ServerProfile;
use crate::domain::template::{self, SimParams};
use crate::domain::topology::{NodeKind, TopologyNode};
use crate::domain::traffic::{SpikePattern, TrafficProfile};

use super::SimError;

/// Machine the template service times are quoted for.
const REF_GHZ: f64 = 3.0;
const REF_DISK_IOPS: f64 = 20_000.0;
const DEFAULT_HIT_RATIO: f64 = 0.8;
const MAX_SERVERS: f64 = 100_000.0;

#[derive(Debug, Clone)]
pub struct NodeModel {
    pub id: String,
    pub label: String,
    pub kind: NodeKind,
    /// Parallel workers; zero for clients, which only generate traffic.
    pub servers: usize,
    /// Requests allowed in the node at once (workers plus waiting room).
    pub capacity: usize,
    pub read_ms: f64,
    pub write_ms: f64,
    pub sigma: f64,
    pub hit_ratio: f64,
    /// Share of the service time spent on CPU; the rest waits on I/O.
    pub cpu_share: f64,
    /// Next hops that are not caches, in edge order.
    pub next: Vec<usize>,
    /// Caches that are read before `next`.
    pub caches: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultTarget {
    Node(usize),
    Edge(usize),
}

#[derive(Debug, Clone)]
pub struct CompiledFault {
    pub id: String,
    pub kind: FaultKind,
    pub target: FaultTarget,
    pub start_us: u64,
    pub end_us: u64,
    /// Human-readable description for reports.
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct Model {
    pub nodes: Vec<NodeModel>,
    pub clients: Vec<usize>,
    /// Directed edges as (from, to) node indexes, in topology order.
    pub edges: Vec<(usize, usize)>,
    pub edge_index: BTreeMap<(usize, usize), usize>,
    pub faults: Vec<CompiledFault>,
    pub timeout_us: u64,
    pub max_retries: u32,
    pub backoff_us: u64,
    /// Network latency added to every hop and once more for the response.
    pub hop_us: u64,
    pub rps: f64,
    pub read_ratio: f64,
    pub spike: SpikePattern,
    pub assumptions: Vec<String>,
}

impl Model {
    pub fn compile(project: &Project) -> Result<Self, SimError> {
        let topology = &project.topology;
        let server = &project.server_profile;
        let index: BTreeMap<&str, usize> = topology
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();

        let mut nodes: Vec<NodeModel> = topology
            .nodes
            .iter()
            .map(|n| node_model(n, server))
            .collect();
        let mut edges = Vec::new();
        let mut edge_index = BTreeMap::new();
        for (edge_number, edge) in topology.edges.iter().enumerate() {
            let (Some(&from), Some(&to)) = (
                index.get(edge.source.as_str()),
                index.get(edge.target.as_str()),
            ) else {
                return Err(SimError::Model(format!(
                    "edge {} refers to a missing node",
                    edge.id
                )));
            };
            edges.push((from, to));
            edge_index.insert((from, to), edge_number);
            if nodes[to].kind == NodeKind::Cache {
                nodes[from].caches.push(to);
            } else {
                nodes[from].next.push(to);
            }
        }

        let clients: Vec<usize> = nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == NodeKind::Client)
            .map(|(i, _)| i)
            .collect();
        if clients.is_empty() {
            return Err(SimError::Model(
                "add a Client node to generate traffic".into(),
            ));
        }
        for &c in &clients {
            if nodes[c].next.is_empty() && nodes[c].caches.is_empty() {
                return Err(SimError::Model(format!(
                    "client \"{}\" is not connected to anything",
                    nodes[c].label
                )));
            }
        }
        ensure_acyclic(&nodes)?;

        let faults = project
            .chaos
            .faults
            .iter()
            .map(|f| compile_fault(f, &nodes, &index, &edges, topology))
            .collect::<Result<Vec<_>, _>>()?;
        let policy = &project.chaos.policy;

        let traffic: &TrafficProfile = &project.traffic_profile;
        Ok(Self {
            nodes,
            clients,
            edges,
            edge_index,
            faults,
            timeout_us: ms_to_us(policy.timeout_ms),
            max_retries: policy.max_retries,
            backoff_us: ms_to_us(policy.backoff_ms),
            hop_us: ms_to_us(server.network.latency_ms),
            rps: traffic.requests_per_second,
            read_ratio: traffic.read_ratio,
            spike: traffic.spike.clone(),
            assumptions: assumptions(),
        })
    }

    /// Traffic multiplier at simulated time `t_s`.
    pub fn rate_multiplier(&self, t_s: f64) -> f64 {
        match self.spike {
            SpikePattern::None => 1.0,
            SpikePattern::Step {
                at_s,
                multiplier,
                duration_s,
            } if t_s >= at_s && t_s < at_s + duration_s => multiplier,
            SpikePattern::Periodic {
                period_s,
                multiplier,
                duration_s,
            } if t_s % period_s >= period_s - duration_s => multiplier,
            _ => 1.0,
        }
    }

    pub fn peak_multiplier(&self) -> f64 {
        match self.spike {
            SpikePattern::None => 1.0,
            SpikePattern::Step { multiplier, .. } | SpikePattern::Periodic { multiplier, .. } => {
                multiplier.max(1.0)
            }
        }
    }
}

pub fn ms_to_us(ms: f64) -> u64 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        (ms * 1_000.0).round().max(0.0) as u64
    }
}

fn compile_fault(
    fault: &Fault,
    nodes: &[NodeModel],
    index: &BTreeMap<&str, usize>,
    edges: &[(usize, usize)],
    topology: &crate::domain::topology::Topology,
) -> Result<CompiledFault, SimError> {
    let missing = || SimError::Model(format!("fault {} refers to a missing target", fault.id));
    let (target, place) = if fault.kind.targets_edge() {
        let number = topology
            .edges
            .iter()
            .position(|e| e.id == fault.target)
            .ok_or_else(missing)?;
        let (from, to) = edges.get(number).copied().ok_or_else(missing)?;
        (
            FaultTarget::Edge(number),
            format!("{} \u{2192} {}", nodes[from].label, nodes[to].label),
        )
    } else {
        let node = *index.get(fault.target.as_str()).ok_or_else(missing)?;
        (FaultTarget::Node(node), nodes[node].label.clone())
    };
    let label = match &fault.kind {
        FaultKind::Crash => format!("Crash of {place}"),
        FaultKind::Slowdown { factor } => format!("{factor}\u{d7} slowdown of {place}"),
        FaultKind::LostWorkers { fraction } => {
            format!("{:.0}% of workers lost on {place}", fraction * 100.0)
        }
        FaultKind::EdgeLatency { extra_ms } => format!("+{extra_ms} ms latency on {place}"),
        FaultKind::EdgeLoss { loss } => format!("{:.0}% packet loss on {place}", loss * 100.0),
    };
    Ok(CompiledFault {
        id: fault.id.clone(),
        kind: fault.kind.clone(),
        target,
        start_us: ms_to_us(fault.start_s * 1_000.0),
        end_us: ms_to_us(fault.end_s() * 1_000.0),
        label,
    })
}

fn assumptions() -> Vec<String> {
    vec![
        "Open-loop traffic: requests arrive at the configured rate; concurrent users are not used yet."
            .into(),
        "Every node is one machine with the server profile's hardware.".into(),
        "Requests are synchronous and follow edges in order; reads try caches first.".into(),
        "Service times are lognormal, arrivals are Poisson, and a full queue drops the request."
            .into(),
        "A request that finishes after the client timeout counts as failed, but still used server capacity."
            .into(),
    ]
}

fn defaults_for(kind: NodeKind) -> SimParams {
    let p = |base_service_ms, cpu_fraction, sigma, write_multiplier, default_queue| SimParams {
        base_service_ms,
        cpu_fraction,
        sigma,
        write_multiplier,
        concurrency_setting: None,
        concurrency_per_unit: 1.0,
        queue_setting: None,
        default_queue,
        hit_ratio_setting: None,
    };
    match kind {
        NodeKind::Client => p(0.001, 1.0, 0.0, 1.0, 0),
        NodeKind::LoadBalancer => p(0.2, 0.9, 0.3, 1.0, 1024),
        NodeKind::AppServer => p(10.0, 0.6, 0.6, 1.0, 512),
        NodeKind::Database => p(4.0, 0.4, 0.8, 3.0, 256),
        NodeKind::Cache => p(0.1, 0.8, 0.4, 1.0, 1024),
        NodeKind::Queue => p(0.3, 0.7, 0.4, 1.0, 4096),
    }
}

fn node_model(node: &TopologyNode, server: &ServerProfile) -> NodeModel {
    let template = node.template.as_ref().and_then(|r| template::find(&r.id));
    let params = template
        .and_then(|t| t.sim.clone())
        .unwrap_or_else(|| defaults_for(node.kind));
    let setting = |key: &Option<String>| -> Option<f64> {
        let key = key.as_deref()?;
        node.settings.get(key).copied().or_else(|| {
            template?
                .settings
                .iter()
                .find(|s| s.key == key)
                .map(|s| s.default)
        })
    };

    let mut model = NodeModel {
        id: node.id.clone(),
        label: node.label.clone(),
        kind: node.kind,
        servers: 0,
        capacity: 0,
        read_ms: 0.0,
        write_ms: 0.0,
        sigma: params.sigma,
        hit_ratio: DEFAULT_HIT_RATIO,
        cpu_share: 1.0,
        next: Vec::new(),
        caches: Vec::new(),
    };
    if node.kind == NodeKind::Client {
        return model;
    }

    let cores = f64::from(server.cpu_cores);
    let wanted =
        setting(&params.concurrency_setting).map_or(cores, |v| v * params.concurrency_per_unit);
    // More workers than this cannot all be on the CPU at once.
    let cpu_limit = (cores / params.cpu_fraction).ceil();
    let servers = wanted.min(cpu_limit).clamp(1.0, MAX_SERVERS);
    let queue = setting(&params.queue_setting).unwrap_or_else(|| f64::from(params.default_queue));

    let cpu_ms = params.base_service_ms * params.cpu_fraction * (REF_GHZ / server.cpu_ghz);
    let io_scale = (REF_DISK_IOPS / f64::from(server.disk.iops)).clamp(0.05, 20.0);
    let io_ms = params.base_service_ms * (1.0 - params.cpu_fraction) * io_scale;

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        // Each replica is its own machine: workers, waiting room and file descriptors all multiply.
        let replicas = node.replicas.max(1) as usize;
        let servers = servers.round() as usize * replicas;
        let capacity = (servers + queue.max(0.0).round() as usize * replicas)
            .min(server.fd_limit as usize * replicas)
            .max(1);
        model.servers = servers.min(capacity);
        model.capacity = capacity;
    }
    model.read_ms = cpu_ms + io_ms;
    if model.read_ms > 0.0 {
        model.cpu_share = cpu_ms / model.read_ms;
    }
    model.write_ms = model.read_ms * params.write_multiplier;
    if node.kind == NodeKind::Cache {
        model.hit_ratio = setting(&params.hit_ratio_setting)
            .unwrap_or(DEFAULT_HIT_RATIO)
            .clamp(0.0, 1.0);
    }
    model
}

/// Rejects loops: a request must reach the end of its path.
fn ensure_acyclic(nodes: &[NodeModel]) -> Result<(), SimError> {
    let n = nodes.len();
    let mut indegree = vec![0usize; n];
    for node in nodes {
        for &to in node.next.iter().chain(&node.caches) {
            indegree[to] += 1;
        }
    }
    let mut ready: Vec<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
    let mut seen = 0;
    while let Some(i) = ready.pop() {
        seen += 1;
        for &to in nodes[i].next.iter().chain(&nodes[i].caches) {
            indegree[to] -= 1;
            if indegree[to] == 0 {
                ready.push(to);
            }
        }
    }
    if seen == n {
        Ok(())
    } else {
        Err(SimError::Model(
            "the topology contains a loop; requests would never finish".into(),
        ))
    }
}
