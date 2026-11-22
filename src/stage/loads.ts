import type { LiveTick, Topology } from "../bindings";
import { IDLE_LOAD, type NodeLoad } from "./particles";

/** Loads for each topology node. Clients have no metrics: their "queue" sizes the crowd. */
export function loadsFrom(
  topology: Topology,
  latest: LiveTick | undefined,
  previousDropped: Map<string, number>,
): NodeLoad[] {
  const offered = latest?.point.offeredRps ?? 0;
  const byId = new Map(latest?.nodes.map((n) => [n.id, n]));
  return topology.nodes.map((n) => {
    if (n.kind === "client") {
      return { ...IDLE_LOAD, queue: Math.min(40, offered / 10) };
    }
    const tick = byId.get(n.id);
    if (!tick) {
      return IDLE_LOAD;
    }
    const delta = Math.max(0, tick.dropped - (previousDropped.get(n.id) ?? 0));
    return {
      utilization: tick.utilization ?? 0,
      queue: tick.queue,
      dropFraction: Math.min(1, delta / Math.max(1, offered)),
    };
  });
}
