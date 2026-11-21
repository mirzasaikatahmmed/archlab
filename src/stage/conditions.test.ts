import { describe, expect, it } from "vitest";
import type { Fault, Topology } from "../bindings";
import { calm, conditionsFor, withEdgeLoss } from "./conditions";
import { IDLE_LOAD, buildGraph } from "./particles";

const node = (id: string, kind: "client" | "appServer" | "database") => ({
  id,
  kind,
  label: id,
  position: { x: 0, y: 0 },
  template: null,
  settings: {},
});

const topology: Topology = {
  nodes: [node("c", "client"), node("app", "appServer"), node("db", "database")],
  edges: [
    { id: "e1", source: "c", target: "app" },
    { id: "e2", source: "app", target: "db" },
  ],
};
const graph = buildGraph(topology);

const fault = (id: string, target: string, kind: Fault["kind"]): Fault => ({
  id,
  target,
  kind,
  startS: 10,
  durationS: 5,
});

describe("conditionsFor", () => {
  it("is calm when no fault is active", () => {
    const faults = [fault("a", "db", { kind: "crash" })];
    expect(conditionsFor(graph, faults, [])).toEqual(calm(graph));
  });

  it("applies node and edge faults to the right indexes", () => {
    const faults = [
      fault("a", "db", { kind: "crash" }),
      fault("b", "app", { kind: "slowdown", factor: 3 }),
      fault("c", "app", { kind: "lostWorkers", fraction: 0.5 }),
      fault("d", "e1", { kind: "edgeLatency", extraMs: 80 }),
      fault("e", "e2", { kind: "edgeLoss", loss: 0.25 }),
    ];
    const c = conditionsFor(graph, faults, ["a", "b", "c", "d", "e"]);
    expect(c.nodes[2].down).toBe(true);
    expect(c.nodes[1]).toMatchObject({ slowFactor: 3, lostFraction: 0.5, down: false });
    expect(c.edges[0].extraMs).toBe(80);
    expect(c.edges[1].loss).toBe(0.25);
  });

  it("combines overlapping faults and ignores unknown targets", () => {
    const faults = [
      fault("a", "app", { kind: "slowdown", factor: 2 }),
      fault("b", "app", { kind: "slowdown", factor: 3 }),
      fault("c", "e1", { kind: "edgeLoss", loss: 0.5 }),
      fault("d", "e1", { kind: "edgeLoss", loss: 0.5 }),
      fault("e", "nowhere", { kind: "crash" }),
    ];
    const c = conditionsFor(graph, faults, ["a", "b", "c", "d", "e"]);
    expect(c.nodes[1].slowFactor).toBe(6);
    expect(c.edges[0].loss).toBeCloseTo(0.75);
    expect(c.nodes.some((n) => n.down)).toBe(false);
  });
});

describe("withEdgeLoss", () => {
  it("turns link loss and crashes into drops at the node they affect", () => {
    const faults = [
      fault("a", "e1", { kind: "edgeLoss", loss: 0.4 }),
      fault("b", "db", { kind: "crash" }),
    ];
    const c = conditionsFor(graph, faults, ["a", "b"]);
    const loads = withEdgeLoss(
      graph.kinds.map(() => IDLE_LOAD),
      graph,
      c,
    );
    expect(loads[1].dropFraction).toBeCloseTo(0.4);
    expect(loads[2].dropFraction).toBe(1);
    expect(loads[0].dropFraction).toBe(0);
  });
});
