/**
 * Illustrative request flow for the Stage. Dots are sampled from per-second
 * metrics; they are not individual simulated requests.
 */
import type { Topology } from "../bindings";

export type Rng = () => number;

/** Small seeded generator so the animation logic is reproducible in tests. */
export function mulberry32(seed: number): Rng {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export interface StageGraph {
  nodeIds: string[];
  /** Ids aligned with `edges`. */
  edgeIds: string[];
  kinds: Topology["nodes"][number]["kind"][];
  edges: { from: number; to: number }[];
  /** Edge indexes leaving each node. */
  out: number[][];
  /** Edges leaving client nodes, where requests enter. */
  entries: number[];
}

export function buildGraph(topology: Topology): StageGraph {
  const index = new Map(topology.nodes.map((n, i) => [n.id, i]));
  const kinds = topology.nodes.map((n) => n.kind);
  const edges: StageGraph["edges"] = [];
  const edgeIds: string[] = [];
  for (const e of topology.edges) {
    const from = index.get(e.source);
    const to = index.get(e.target);
    if (from !== undefined && to !== undefined) {
      edges.push({ from, to });
      edgeIds.push(e.id);
    }
  }
  const out: number[][] = kinds.map(() => []);
  edges.forEach((e, i) => out[e.from]?.push(i));
  const entries = edges.flatMap((e, i) => (kinds[e.from] === "client" ? [i] : []));
  return { nodeIds: topology.nodes.map((n) => n.id), edgeIds, kinds, edges, out, entries };
}

export interface NodeLoad {
  /** Busy share of worker time, 0..1. */
  utilization: number;
  /** Requests waiting right now. */
  queue: number;
  /** Share of arriving requests dropped during the last second, 0..1. */
  dropFraction: number;
}

export const IDLE_LOAD: NodeLoad = { utilization: 0, queue: 0, dropFraction: 0 };

export interface Particle {
  edge: number;
  /** Progress along the edge, 0..1. */
  t: number;
  speed: number;
  /** Seconds left to wait at the start of the edge. */
  hold: number;
}

export interface Spark {
  node: number;
  /** Offsets from the node centre, in ground units. */
  x: number;
  y: number;
  z: number;
  vx: number;
  vy: number;
  vz: number;
  life: number;
}

export interface World {
  particles: Particle[];
  sparks: Spark[];
  carry: number;
  rng: Rng;
  completed: number;
}

export const MAX_PARTICLES = 600;
const EDGE_SECONDS = 1.8;

export function createWorld(seed: number): World {
  return { particles: [], sparks: [], carry: 0, rng: mulberry32(seed), completed: 0 };
}

/** Dots spawned per real second for a given offered rate; grows slowly so spikes stay visible. */
export function spawnRate(offeredRps: number): number {
  if (!(offeredRps > 0)) {
    return 0;
  }
  return Math.min(36, Math.max(2, offeredRps * 0.1));
}

/** Time a dot lingers at a node: longer as the node saturates. */
export function dwellSeconds(utilization: number): number {
  const u = Math.min(1, Math.max(0, utilization));
  return 0.05 + u * u * u * 1.0;
}

export interface StepInput {
  offeredRps: number;
  loads: NodeLoad[];
}

export function stepWorld(world: World, graph: StageGraph, input: StepInput, dt: number): void {
  spawn(world, graph, input.offeredRps, dt);
  advanceParticles(world, graph, input.loads, dt);
  advanceSparks(world, dt);
}

function spawn(world: World, graph: StageGraph, offeredRps: number, dt: number): void {
  if (graph.entries.length === 0) {
    return;
  }
  world.carry += spawnRate(offeredRps) * dt;
  while (world.carry >= 1) {
    world.carry -= 1;
    if (world.particles.length >= MAX_PARTICLES) {
      continue;
    }
    const edge = graph.entries[Math.floor(world.rng() * graph.entries.length)];
    world.particles.push({ edge, t: 0, speed: 0.8 + world.rng() * 0.4, hold: 0 });
  }
}

function advanceParticles(world: World, graph: StageGraph, loads: NodeLoad[], dt: number): void {
  const kept: Particle[] = [];
  for (const p of world.particles) {
    if (p.hold > 0) {
      p.hold -= dt;
      kept.push(p);
      continue;
    }
    p.t += (dt * p.speed) / EDGE_SECONDS;
    if (p.t < 1) {
      kept.push(p);
      continue;
    }
    const edge = graph.edges[p.edge];
    const load = loads[edge.to] ?? IDLE_LOAD;
    if (world.rng() < load.dropFraction) {
      burst(world, edge.to);
      continue;
    }
    const next = graph.out[edge.to] ?? [];
    const pick: number | undefined =
      next.length > 0 ? next[Math.floor(world.rng() * next.length)] : undefined;
    if (pick === undefined) {
      world.completed += 1;
      continue;
    }
    p.edge = pick;
    p.t = 0;
    p.hold = dwellSeconds(load.utilization);
    kept.push(p);
  }
  world.particles = kept;
}

function burst(world: World, node: number): void {
  for (let i = 0; i < 6; i++) {
    const angle = world.rng() * Math.PI * 2;
    const speed = 18 + world.rng() * 26;
    world.sparks.push({
      node,
      x: 0,
      y: 0,
      z: 14,
      vx: Math.cos(angle) * speed,
      vy: Math.sin(angle) * speed,
      vz: 20 + world.rng() * 30,
      life: 0.7,
    });
  }
}

function advanceSparks(world: World, dt: number): void {
  const alive: Spark[] = [];
  for (const s of world.sparks) {
    s.life -= dt;
    if (s.life <= 0) {
      continue;
    }
    s.x += s.vx * dt;
    s.y += s.vy * dt;
    s.z = Math.max(0, s.z + s.vz * dt);
    s.vz -= 90 * dt;
    alive.push(s);
  }
  world.sparks = alive;
}
