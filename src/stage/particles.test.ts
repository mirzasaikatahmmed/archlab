import { describe, expect, it } from "vitest";
import type { NodeKind, Topology } from "../bindings";
import { withAlpha } from "./draw";
import { project, fitScene } from "./iso";
import {
  IDLE_LOAD,
  MAX_PARTICLES,
  buildGraph,
  createWorld,
  dwellSeconds,
  mulberry32,
  spawnRate,
  stepWorld,
  type NodeLoad,
} from "./particles";

const node = (id: string, kind: NodeKind, x: number) => ({
  id,
  kind,
  label: id,
  position: { x, y: 0 },
  template: null,
  settings: {},
});

const topology: Topology = {
  nodes: [
    node("c", "client", 0),
    node("lb", "loadBalancer", 200),
    node("app", "appServer", 400),
    node("db", "database", 600),
  ],
  edges: [
    { id: "e1", source: "c", target: "lb" },
    { id: "e2", source: "lb", target: "app" },
    { id: "e3", source: "app", target: "db" },
  ],
};

const graph = buildGraph(topology);
const idle = graph.kinds.map(() => IDLE_LOAD);

function run(loads: NodeLoad[], seconds: number, offered = 200, seed = 1) {
  const world = createWorld(seed);
  for (let i = 0; i < seconds * 60; i++) {
    stepWorld(world, graph, { offeredRps: offered, loads }, 1 / 60);
  }
  return world;
}

describe("buildGraph", () => {
  it("finds the client entry edges and each node's outgoing edges", () => {
    expect(graph.entries.length).toBeGreaterThan(0);
    for (const e of graph.entries) {
      expect(graph.kinds[graph.edges[e].from]).toBe("client");
    }
    expect(graph.out).toHaveLength(graph.kinds.length);
  });

  it("ignores edges that point at unknown nodes", () => {
    const broken = buildGraph({
      nodes: topology.nodes,
      edges: [{ id: "x", source: "nope", target: "app" }],
    });
    expect(broken.edges).toHaveLength(0);
    expect(broken.entries).toHaveLength(0);
  });
});

describe("rates", () => {
  it("spawns nothing when idle and caps heavy load", () => {
    expect(spawnRate(0)).toBe(0);
    expect(spawnRate(Number.NaN)).toBe(0);
    expect(spawnRate(1)).toBe(2);
    expect(spawnRate(100_000)).toBe(36);
  });

  it("makes dots linger longer at busier nodes", () => {
    expect(dwellSeconds(0.95)).toBeGreaterThan(dwellSeconds(0.5));
    expect(dwellSeconds(0.5)).toBeGreaterThan(dwellSeconds(0));
    expect(dwellSeconds(5)).toBe(dwellSeconds(1));
  });
});

describe("stepWorld", () => {
  it("is reproducible for a given seed", () => {
    const a = run(idle, 3);
    const b = run(idle, 3);
    expect(a.particles).toEqual(b.particles);
    expect(a.completed).toBe(b.completed);
  });

  it("lets dots reach the end of the path when nothing is dropped", () => {
    const world = run(idle, 8);
    expect(world.completed).toBeGreaterThan(0);
    expect(world.sparks).toHaveLength(0);
  });

  it("turns dots into sparks at a node that drops everything", () => {
    const loads = graph.kinds.map((k) =>
      k === "appServer" ? { utilization: 1, queue: 20, dropFraction: 1 } : IDLE_LOAD,
    );
    const world = run(loads, 5);
    expect(world.completed).toBe(0);
  });

  it("spawns no dots without load or without a client", () => {
    expect(run(idle, 2, 0).particles).toHaveLength(0);
    const noClient = buildGraph({ nodes: topology.nodes.slice(1), edges: [] });
    const world = createWorld(1);
    stepWorld(world, noClient, { offeredRps: 500, loads: [] }, 1);
    expect(world.particles).toHaveLength(0);
  });

  it("never exceeds the particle cap", () => {
    const saturated = graph.kinds.map(() => ({ utilization: 1, queue: 0, dropFraction: 0 }));
    const world = run(saturated, 20, 100_000);
    expect(world.particles.length).toBeLessThanOrEqual(MAX_PARTICLES);
  });
});

describe("withAlpha", () => {
  it("adds an alpha to hex colours and leaves other formats alone", () => {
    expect(withAlpha("#3cc596", 0.5)).toBe("rgba(60, 197, 150, 0.5)");
    expect(withAlpha("rgb(1, 2, 3)", 0.5)).toBe("rgb(1, 2, 3)");
  });
});

describe("rng and projection", () => {
  it("mulberry32 is deterministic and in [0, 1)", () => {
    const a = mulberry32(7);
    const b = mulberry32(7);
    for (let i = 0; i < 50; i++) {
      const v = a();
      expect(v).toBe(b());
      expect(v).toBeGreaterThanOrEqual(0);
      expect(v).toBeLessThan(1);
    }
  });

  it("projects ground x down-right, ground y down-left and z up", () => {
    const origin = project(0, 0);
    expect(project(10, 0).x).toBeGreaterThan(origin.x);
    expect(project(0, 10).x).toBeLessThan(origin.x);
    expect(project(10, 0).y).toBeGreaterThan(origin.y);
    expect(project(0, 0, 10).y).toBeLessThan(origin.y);
  });

  it("fits a scene inside the canvas and centres it", () => {
    const pts = [project(0, 0), project(400, 0), project(0, 300)];
    const size = { w: 800, h: 400 };
    const fit = fitScene(pts, size, { top: 40, right: 40, bottom: 40, left: 40 });
    for (const p of pts) {
      const x = p.x * fit.scale + fit.offset.x;
      const y = p.y * fit.scale + fit.offset.y;
      expect(x).toBeGreaterThanOrEqual(0);
      expect(x).toBeLessThanOrEqual(size.w);
      expect(y).toBeGreaterThanOrEqual(0);
      expect(y).toBeLessThanOrEqual(size.h);
    }
    expect(fitScene([], size, { top: 0, right: 0, bottom: 0, left: 0 }).scale).toBe(1);
  });
});
