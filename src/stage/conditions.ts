import type { Fault } from "../bindings";
import type { NodeLoad, StageGraph } from "./particles";

export interface NodeCondition {
  down: boolean;
  /** Service time multiplier; 1 means normal. */
  slowFactor: number;
  /** Share of workers lost, 0..1. */
  lostFraction: number;
}

export interface EdgeCondition {
  extraMs: number;
  /** Share of hops lost, 0..1; 1 is a partition. */
  loss: number;
}

export interface Conditions {
  nodes: NodeCondition[];
  edges: EdgeCondition[];
}

export function calm(graph: StageGraph): Conditions {
  return {
    nodes: graph.kinds.map(() => ({ down: false, slowFactor: 1, lostFraction: 0 })),
    edges: graph.edges.map(() => ({ extraMs: 0, loss: 0 })),
  };
}

/** Folds the faults that are active right now into per-node and per-edge conditions. */
export function conditionsFor(
  graph: StageGraph,
  faults: Fault[],
  activeIds: readonly string[],
): Conditions {
  const result = calm(graph);
  const active = new Set(activeIds);
  for (const fault of faults) {
    if (!active.has(fault.id)) {
      continue;
    }
    const node = graph.nodeIds.indexOf(fault.target);
    const edge = graph.edgeIds.indexOf(fault.target);
    const k = fault.kind;
    switch (k.kind) {
      case "crash":
        if (node >= 0) {
          result.nodes[node].down = true;
        }
        break;
      case "slowdown":
        if (node >= 0) {
          result.nodes[node].slowFactor *= k.factor ?? 1;
        }
        break;
      case "lostWorkers":
        if (node >= 0) {
          result.nodes[node].lostFraction = Math.min(
            0.95,
            result.nodes[node].lostFraction + (k.fraction ?? 0),
          );
        }
        break;
      case "edgeLatency":
        if (edge >= 0) {
          result.edges[edge].extraMs += k.extraMs ?? 0;
        }
        break;
      case "edgeLoss":
        if (edge >= 0) {
          const e = result.edges[edge];
          e.loss = 1 - (1 - e.loss) * (1 - (k.loss ?? 0));
        }
        break;
    }
  }
  return result;
}

/** Packet loss on a link shows up as drops at the node it leads to. */
export function withEdgeLoss(
  loads: NodeLoad[],
  graph: StageGraph,
  conditions: Conditions,
): NodeLoad[] {
  const out = loads.map((l) => ({ ...l }));
  graph.edges.forEach((edge, i) => {
    const loss = conditions.edges[i].loss;
    if (loss > 0) {
      const target = out[edge.to];
      target.dropFraction = 1 - (1 - target.dropFraction) * (1 - loss);
    }
  });
  conditions.nodes.forEach((c, i) => {
    if (c.down) {
      out[i].dropFraction = 1;
    }
  });
  return out;
}
