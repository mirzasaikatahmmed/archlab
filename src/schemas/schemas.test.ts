import { describe, expect, it } from "vitest";
import limits from "./limits.json";
import { defaultSpike, trafficProfileSchema } from "./trafficProfile";
import { serverProfileSchema } from "./serverProfile";
import { chaosPlanSchema, defaultKind, type FaultKindName } from "./chaos";

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
