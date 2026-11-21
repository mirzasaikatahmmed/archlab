import { describe, expect, it } from "vitest";
import limits from "./limits.json";
import { defaultSpike, trafficProfileSchema } from "./trafficProfile";
import { serverProfileSchema } from "./serverProfile";
import { chaosPlanSchema, defaultKind, type FaultKindName } from "./chaos";
import { sweepSpecSchema, whatIfSchema } from "./whatif";

const server = {
  os: "linux",
  cpuCores: 4,
  cpuGhz: 2.5,
  ramGb: 8,
  disk: { kind: "ssd", iops: 20000 },
  network: { bandwidthMbps: 1000, latencyMs: 0.5 },
  virtualization: "vm",
  fdLimit: 65536,
};
const traffic = {
  concurrentUsers: 1000,
  requestsPerSecond: 200,
  readRatio: 0.8,
  spike: { kind: "none" },
};

function withPath(base: object, path: string, value: number): unknown {
  const copy = structuredClone(base) as Record<string, unknown>;
  const parts = path.split(".");
  let cur = copy;
  for (const p of parts.slice(0, -1)) {
    cur = cur[p] as Record<string, unknown>;
  }
  cur[parts[parts.length - 1]] = value;
  return copy;
}

describe.each([
  ["serverProfile", serverProfileSchema, server, limits.serverProfile],
  ["trafficProfile", trafficProfileSchema, traffic, limits.trafficProfile],
] as const)("%s ranges match the shared Rust fixture", (_name, schema, base, rows) => {
  it("accepts the base value", () => {
    expect(schema.safeParse(base).success).toBe(true);
  });

  it.each(rows)("$path: accepts min/max, rejects below/above", (row) => {
    expect(schema.safeParse(withPath(base, row.path, row.min)).success).toBe(true);
    expect(schema.safeParse(withPath(base, row.path, row.max)).success).toBe(true);
    expect(schema.safeParse(withPath(base, row.path, row.below)).success).toBe(false);
    expect(schema.safeParse(withPath(base, row.path, row.above)).success).toBe(false);
  });
});

describe("spike schema", () => {
  it("accepts every default spike", () => {
    for (const kind of ["none", "step", "periodic"] as const) {
      expect(
        trafficProfileSchema.safeParse({ ...traffic, spike: defaultSpike(kind) }).success,
      ).toBe(true);
    }
  });

  it("rejects a periodic spike longer than its period", () => {
    const r = trafficProfileSchema.safeParse({
      ...traffic,
      spike: { kind: "periodic", periodS: 10, multiplier: 2, durationS: 10 },
    });
    expect(r.success).toBe(false);
  });

  it("rejects NaN and non-integers where integers are required", () => {
    expect(serverProfileSchema.safeParse({ ...server, cpuCores: Number.NaN }).success).toBe(false);
    expect(serverProfileSchema.safeParse({ ...server, cpuCores: 2.5 }).success).toBe(false);
  });
});

const policy = { timeoutMs: 10_000, maxRetries: 0, backoffMs: 100 };
const fault = (kind: FaultKindName) => ({
  id: "f1",
  target: "db",
  kind: defaultKind(kind),
  startS: 10,
  durationS: 5,
});

describe("chaos ranges match the shared Rust fixture", () => {
  it("accepts a plan with every kind of fault", () => {
    const faults = (["crash", "slowdown", "lostWorkers", "edgeLatency", "edgeLoss"] as const).map(
      fault,
    );
    expect(chaosPlanSchema.safeParse({ faults, policy }).success).toBe(true);
  });

  const check = (base: unknown, path: string, row: (typeof limits.clientPolicy)[number]) => {
    const parse = (v: number) => chaosPlanSchema.safeParse(withPath(base as object, path, v));
    expect(parse(row.min).success).toBe(true);
    expect(parse(row.max).success).toBe(true);
    expect(parse(row.below).success).toBe(false);
    expect(parse(row.above).success).toBe(false);
  };

  it.each(limits.clientPolicy)("policy $path", (row) => {
    check({ faults: [], policy }, `policy.${row.path}`, row);
  });

  it.each(limits.faultTiming)("fault $path", (row) => {
    check({ faults: [fault("crash")], policy }, `faults.0.${row.path}`, row);
  });

  it.each(limits.faultParams)("$kind $path", (row) => {
    check({ faults: [fault(row.kind as FaultKindName)], policy }, `faults.0.${row.path}`, row);
  });

  it("rejects a fault with no target and a fractional retry count", () => {
    expect(
      chaosPlanSchema.safeParse({ faults: [{ ...fault("crash"), target: "" }], policy }).success,
    ).toBe(false);
    expect(
      chaosPlanSchema.safeParse({ faults: [], policy: { ...policy, maxRetries: 1.5 } }).success,
    ).toBe(false);
  });
});

describe("what-if ranges match the shared Rust fixture", () => {
  const slo = { p95Ms: 200, maxErrorRate: 0.01 };
  const variant = (change: unknown) => ({
    slo,
    variants: [{ id: "v1", name: "Variant", changes: [change] }],
  });
  const check = (
    schema: { safeParse: (v: unknown) => { success: boolean } },
    base: unknown,
    path: string,
    row: { min: number; max: number; below: number; above: number },
  ) => {
    const parse = (v: number) => schema.safeParse(withPath(base as object, path, v)).success;
    expect(parse(row.min)).toBe(true);
    expect(parse(row.max)).toBe(true);
    expect(parse(row.below)).toBe(false);
    expect(parse(row.above)).toBe(false);
  };

  it.each(limits.slo)("target $path", (row) => {
    check(whatIfSchema, { slo, variants: [] }, `slo.${row.path}`, row);
  });

  it.each(limits.changeParams)("$kind $path", (row) => {
    const change =
      row.kind === "trafficScale"
        ? { kind: "trafficScale", factor: 2 }
        : { kind: "replicas", node: "app", count: 2 };
    check(whatIfSchema, variant(change), `variants.0.changes.0.${row.path}`, row);
  });

  it.each(limits.sweepPoints)("sweep $path", (row) => {
    const spec = { param: { kind: "trafficScale" }, from: 1, to: 2, points: 5 };
    check(sweepSpecSchema, spec, row.path, row);
  });

  it("checks server changes against the profile ranges", () => {
    const server = (field: string, value: number) =>
      whatIfSchema.safeParse(variant({ kind: "server", field, value })).success;
    expect(server("cpuCores", 16)).toBe(true);
    expect(server("cpuCores", 2.5)).toBe(false);
    expect(server("cpuCores", 5000)).toBe(false);
    expect(server("cpuGhz", 3.5)).toBe(true);
    expect(server("cpuGhz", 99)).toBe(false);
    expect(server("diskIops", 100_000)).toBe(true);
  });

  it("rejects an unnamed variant and an empty node choice", () => {
    expect(
      whatIfSchema.safeParse({
        slo,
        variants: [{ id: "v", name: "  ", changes: [] }],
      }).success,
    ).toBe(false);
    expect(whatIfSchema.safeParse(variant({ kind: "replicas", node: "", count: 2 })).success).toBe(
      false,
    );
  });

  it("rejects a sweep whose start equals its end", () => {
    const spec = { param: { kind: "trafficScale" }, from: 2, to: 2, points: 5 };
    expect(sweepSpecSchema.safeParse(spec).success).toBe(false);
  });
});
