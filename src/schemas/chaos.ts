import { z } from "zod";
import type { ChaosPlan } from "../bindings";

const num = (min: number, max: number) => z.number({ error: "Enter a number" }).min(min).max(max);

export const faultKindSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("crash") }),
  z.object({ kind: z.literal("slowdown"), factor: num(1, 100) }),
  z.object({ kind: z.literal("lostWorkers"), fraction: num(0.05, 0.95) }),
  z.object({ kind: z.literal("edgeLatency"), extraMs: num(0, 10_000) }),
  z.object({ kind: z.literal("edgeLoss"), loss: num(0, 1) }),
]);

export const faultSchema = z.object({
  id: z.string().min(1),
  target: z.string().min(1, "Choose a target"),
  kind: faultKindSchema,
  startS: num(0, 86_400),
  durationS: num(0.001, 3_600),
});

export const clientPolicySchema = z.object({
  timeoutMs: num(50, 120_000),
  maxRetries: z.number({ error: "Enter a number" }).int("Must be a whole number").min(0).max(5),
  backoffMs: num(0, 10_000),
});

/** Mirrors `ChaosPlan::validate` in Rust (ranges are checked against src/schemas/limits.json). */
export const chaosPlanSchema = z.object({
  faults: z.array(faultSchema).max(100),
  policy: clientPolicySchema,
});

export type FaultKindName = z.infer<typeof faultKindSchema>["kind"];

export const toChaosPlan = (values: z.infer<typeof chaosPlanSchema>): ChaosPlan => values;

/** Edge faults act on a connection; the rest act on a node. */
export function targetsEdge(kind: FaultKindName): boolean {
  return kind === "edgeLatency" || kind === "edgeLoss";
}

export function defaultKind(kind: FaultKindName): z.infer<typeof faultKindSchema> {
  switch (kind) {
    case "crash":
      return { kind };
    case "slowdown":
      return { kind, factor: 3 };
    case "lostWorkers":
      return { kind, fraction: 0.5 };
    case "edgeLatency":
      return { kind, extraMs: 100 };
    case "edgeLoss":
      return { kind, loss: 0.2 };
  }
}

export const FAULT_LABELS: Record<FaultKindName, string> = {
  crash: "Crash and restart",
  slowdown: "Slowdown",
  lostWorkers: "Lost workers",
  edgeLatency: "Extra latency",
  edgeLoss: "Packet loss",
};
