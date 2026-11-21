import { z } from "zod";
import type { WhatIf } from "../bindings";
import limits from "./limits.json";

const num = (min: number, max: number) => z.number({ error: "Enter a number" }).min(min).max(max);

export const SERVER_FIELDS = [
  "cpuCores",
  "cpuGhz",
  "ramGb",
  "diskIops",
  "networkBandwidthMbps",
  "networkLatencyMs",
  "fdLimit",
] as const;

export type ServerFieldName = (typeof SERVER_FIELDS)[number];

export const SERVER_FIELD_LABELS: Record<ServerFieldName, string> = {
  cpuCores: "CPU cores",
  cpuGhz: "CPU clock (GHz)",
  ramGb: "RAM (GB)",
  diskIops: "Disk IOPS",
  networkBandwidthMbps: "Network bandwidth (Mbps)",
  networkLatencyMs: "Network latency (ms)",
  fdLimit: "File descriptor limit",
};

/** Where each server field lives in the profile, for its range in the shared fixture. */
const FIELD_PATHS: Record<ServerFieldName, string> = {
  cpuCores: "cpuCores",
  cpuGhz: "cpuGhz",
  ramGb: "ramGb",
  diskIops: "disk.iops",
  networkBandwidthMbps: "network.bandwidthMbps",
  networkLatencyMs: "network.latencyMs",
  fdLimit: "fdLimit",
};

const WHOLE: readonly ServerFieldName[] = ["cpuCores", "diskIops", "fdLimit"];

export const sloSchema = z.object({
  p95Ms: num(1, 60_000),
  maxErrorRate: num(0, 1),
});

export const changeSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("trafficScale"), factor: num(0.01, 100) }),
  z.object({ kind: z.literal("trafficRate"), rps: num(0.001, 10_000_000) }),
  z
    .object({
      kind: z.literal("server"),
      field: z.enum(SERVER_FIELDS),
      value: z.number({ error: "Enter a number" }),
    })
    .superRefine((c, ctx) => {
      const row = limits.serverProfile.find((r) => r.path === FIELD_PATHS[c.field]);
      if (row && (c.value < row.min || c.value > row.max)) {
        ctx.addIssue({
          code: "custom",
          path: ["value"],
          message: `Must be between ${String(row.min)} and ${String(row.max)}`,
        });
      }
      if (WHOLE.includes(c.field) && !Number.isInteger(c.value)) {
        ctx.addIssue({ code: "custom", path: ["value"], message: "Must be a whole number" });
      }
    }),
  z.object({
    kind: z.literal("nodeSetting"),
    node: z.string().min(1, "Choose a node"),
    key: z.string().min(1, "Choose a setting"),
    value: z.number({ error: "Enter a number" }),
  }),
  z.object({
    kind: z.literal("replicas"),
    node: z.string().min(1, "Choose a node"),
    count: z.number({ error: "Enter a number" }).int("Must be a whole number").min(1).max(64),
  }),
  z.object({ kind: z.literal("chaosOff") }),
]);

export const variantSchema = z.object({
  id: z.string().min(1),
  name: z.string().trim().min(1, "Give the variant a name").max(60),
  changes: z.array(changeSchema).max(10),
});

/** Mirrors `WhatIf::validate` in Rust (ranges are checked against src/schemas/limits.json). */
export const whatIfSchema = z.object({
  slo: sloSchema,
  variants: z.array(variantSchema).max(20),
});

export const sweepParamSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("trafficScale") }),
  z.object({ kind: z.literal("server"), field: z.enum(SERVER_FIELDS) }),
  z.object({
    kind: z.literal("nodeSetting"),
    node: z.string().min(1, "Choose a node"),
    key: z.string().min(1, "Choose a setting"),
  }),
  z.object({ kind: z.literal("replicas"), node: z.string().min(1, "Choose a node") }),
]);

export const sweepSpecSchema = z
  .object({
    param: sweepParamSchema,
    from: z.number({ error: "Enter a number" }),
    to: z.number({ error: "Enter a number" }),
    points: z.number({ error: "Enter a number" }).int("Must be a whole number").min(2).max(24),
  })
  .refine((s) => s.from !== s.to, { path: ["to"], message: "Must differ from the start value" });

export type ChangeKind = z.infer<typeof changeSchema>["kind"];

export const toWhatIf = (values: z.infer<typeof whatIfSchema>): WhatIf => values;

export const CHANGE_LABELS: Record<ChangeKind, string> = {
  trafficScale: "Traffic multiplier",
  trafficRate: "Traffic rate",
  server: "Server setting",
  nodeSetting: "Node setting",
  replicas: "Replicas",
  chaosOff: "Remove injected faults",
};

export function defaultChange(kind: ChangeKind, node: string): z.infer<typeof changeSchema> {
  switch (kind) {
    case "trafficScale":
      return { kind, factor: 2 };
    case "trafficRate":
      return { kind, rps: 500 };
    case "server":
      return { kind, field: "cpuCores", value: 8 };
    case "nodeSetting":
      return { kind, node, key: "", value: 1 };
    case "replicas":
      return { kind, node, count: 2 };
    case "chaosOff":
      return { kind };
  }
}
