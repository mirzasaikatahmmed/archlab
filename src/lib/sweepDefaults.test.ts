import { describe, expect, it } from "vitest";
import type { Project } from "../bindings";
import {
  chaosPlan,
  serverProfile,
  templates,
  topology,
  trafficProfile,
  whatIfPlan,
} from "../test/fixtures";
import { defaultRange, type SweepChoice } from "./sweepDefaults";

const project: Project = {
  id: "p",
  name: "P",
  serverProfile,
  trafficProfile,
  topology,
  chaos: chaosPlan,
  whatif: whatIfPlan,
  createdAt: 0,
  updatedAt: 0,
};

const choice = (patch: Partial<SweepChoice>): SweepChoice => ({
  kind: "trafficScale",
  field: "cpuCores",
  node: "",
  key: "",
  ...patch,
});

describe("defaultRange", () => {
  it("uses fixed ranges for traffic and replicas", () => {
    expect(defaultRange(choice({}), project, templates)).toEqual({ from: 0.5, to: 4 });
    expect(defaultRange(choice({ kind: "replicas" }), project, templates)).toEqual({
      from: 1,
      to: 8,
    });
  });

  it("scales a server field around its current value and keeps whole numbers whole", () => {
    const r = defaultRange(choice({ kind: "server", field: "cpuCores" }), project, templates);
    expect(r).toEqual({ from: 2, to: 16 });
    const ghz = defaultRange(choice({ kind: "server", field: "cpuGhz" }), project, templates);
    expect(ghz.from).toBeCloseTo(1.25);
    expect(ghz.to).toBeCloseTo(10);
  });

  it("stays inside the allowed range and never returns equal ends", () => {
    const tiny = { ...project, serverProfile: { ...serverProfile, cpuCores: 1 } };
    const r = defaultRange(choice({ kind: "server", field: "cpuCores" }), tiny, templates);
    expect(r.from).toBe(1);
    expect(r.to).toBe(4);
    const huge = { ...project, serverProfile: { ...serverProfile, cpuCores: 1024 } };
    const h = defaultRange(choice({ kind: "server", field: "cpuCores" }), huge, templates);
    expect(h.to).toBe(1024);
    expect(h.from).toBe(512);
  });

  it("falls back for a node setting it cannot find", () => {
    const r = defaultRange(
      choice({ kind: "nodeSetting", node: "ghost", key: "x" }),
      project,
      templates,
    );
    expect(r).toEqual({ from: 1, to: 2 });
  });
});
