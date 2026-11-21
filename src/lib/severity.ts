import type { Severity } from "../bindings";

/** Colour band for a load figure; the engine's own severity is used wherever it is available. */
export function levelFor(utilization: number): Severity {
  if (utilization >= 0.9) {
    return "saturated";
  }
  return utilization >= 0.7 ? "warning" : "healthy";
}
