import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  CapacityReport,
  CompareReport,
  Outcome,
  Project,
  SweepReport,
  WhatIf,
  WhatIfEvent,
} from "../bindings";
import {
  chaosPlan,
  serverProfile,
  templates,
  topology,
  trafficProfile,
  whatIfPlan,
} from "../test/fixtures";

const api = vi.hoisted(() => ({ startWhatIf: vi.fn(), cancelWhatIf: vi.fn() }));
vi.mock("../ipc", async (importActual) => {
  const actual = await importActual<typeof import("../ipc")>();
  return { ...actual, api };
});

import { CommandError } from "../ipc";
import { WhatIfTab } from "./WhatIfTab";

const latency = (p95: number) => ({
  meanMs: p95 / 2,
  p50Ms: p95 / 2,
  p95Ms: p95,
  p99Ms: p95 * 1.5,
});
const outcome = (p95: number, errors: number, meets: boolean, rps = 100): Outcome => ({
  summary: {
    offered: 1000,
    completed: 990,
    errors: 10,
    retries: 0,
    throughputRps: rps,
    errorRate: errors,
    latency: latency(p95),
  },
  bottleneck: meets
    ? null
    : { nodeId: "app", label: "App server", severity: "saturated", reason: "96% busy" },
  meetsSlo: meets,
  nodes: [],
  error: null,
});

const slo = { p95Ms: 200, maxErrorRate: 0.01 };

const project: Project = {
  id: "p1",
  name: "Demo",
  serverProfile,
  trafficProfile,
  topology,
  chaos: chaosPlan,
  whatif: {
    ...whatIfPlan,
    variants: [
      { id: "v1", name: "Two copies", changes: [{ kind: "replicas", node: "app", count: 2 }] },
    ],
  },
  createdAt: 0,
  updatedAt: 0,
};

const compare: CompareReport = {
  slo,
  baseline: {
    id: "baseline",
    name: "Baseline",
    changes: [],
    outcome: outcome(450, 0.2, false, 80),
    vsBaseline: null,
  },
  variants: [
    {
      id: "v1",
      name: "Two copies",
      changes: ["App server ×2 replicas"],
      outcome: outcome(45, 0, true, 180),
      vsBaseline: { throughputPct: 125, p95Pct: -90, errorRatePoints: -20 },
    },
  ],
};

const sweep: SweepReport = {
  slo,
  label: "Traffic (× current)",
  points: [0.5, 1, 2, 4].map((v) => ({
    value: v,
    outcome: outcome(v * 60, v > 2 ? 0.05 : 0, v <= 2, 100 * v),
  })),
  firstMiss: 4,
};

const capacity: CapacityReport = {
  slo,
  currentRps: 200,
  currentMeetsSlo: true,
  maxRps: 450,
  headroom: 2.25,
  reachedSearchLimit: false,
  limit: { node: "App server", reason: "96% busy", missed: ["p95 above 200 ms"] },
  probes: [
    { rps: 200, p95Ms: 40, errorRate: 0, meetsSlo: true, error: null },
    { rps: 800, p95Ms: 900, errorRate: 0.3, meetsSlo: false, error: null },
  ],
};

async function start(label: string): Promise<(e: WhatIfEvent) => void> {
  api.startWhatIf.mockResolvedValue(undefined);
  await userEvent.click(screen.getByRole("button", { name: label }));
  await waitFor(() => {
    expect(api.startWhatIf).toHaveBeenCalled();
  });
  const cb = api.startWhatIf.mock.calls[0]?.[3] as ((e: WhatIfEvent) => void) | undefined;
  if (!cb) {
    throw new Error("no callback passed");
  }
  return cb;
}

describe("WhatIfTab", () => {
  beforeEach(() => {
    api.startWhatIf.mockReset();
    api.cancelWhatIf.mockReset();
    api.cancelWhatIf.mockResolvedValue(null);
  });

  it("compares variants with the baseline and colours the changes", async () => {
    render(<WhatIfTab project={project} templates={templates} onChange={vi.fn()} />);
    const send = await start("Compare");
    expect(api.startWhatIf).toHaveBeenCalledWith(
      project,
      { seed: 1, durationS: 30, warmupS: 5 },
      { kind: "compare" },
      expect.any(Function),
    );
    act(() => {
      send({ kind: "progress", done: 1, total: 2 });
    });
    expect(await screen.findByText(/1 of 2 runs/)).toBeInTheDocument();
    act(() => {
      send({ kind: "finished", result: { kind: "compare", report: compare } });
    });
    expect(await screen.findByText("Two copies", { selector: "th" })).toBeInTheDocument();
    expect(screen.getByText("Missed")).toBeInTheDocument();
    expect(screen.getByText("Met")).toBeInTheDocument();
    const better = screen.getByText("-90.0%");
    expect(better).toHaveAttribute("data-good", "true");
    expect(screen.getByText("-20.0 pts")).toHaveAttribute("data-good", "true");
  });

  it("sweeps a parameter and says where the targets are first missed", async () => {
    render(<WhatIfTab project={project} templates={templates} onChange={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "Sweep a parameter" }));
    const send = await start("Run sweep");
    expect(api.startWhatIf.mock.calls[0]?.[2]).toEqual({
      kind: "sweep",
      spec: { param: { kind: "trafficScale" }, from: 0.5, to: 4, points: 10 },
    });
    act(() => {
      send({ kind: "finished", result: { kind: "sweep", report: sweep } });
    });
    expect(await screen.findByText("Targets are first missed at 4")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: /p95 latency/ })).toBeInTheDocument();
  });

  it("blocks a sweep with a bad range", async () => {
    render(<WhatIfTab project={project} templates={templates} onChange={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "Sweep a parameter" }));
    const to = screen.getByLabelText("To");
    await userEvent.clear(to);
    await userEvent.type(to, "0.5");
    expect(await screen.findByText("Must differ from the start value")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Run sweep" })).toBeDisabled();
  });

  it("reports capacity and what limits it", async () => {
    render(<WhatIfTab project={project} templates={templates} onChange={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "Capacity" }));
    const send = await start("Find capacity");
    expect(api.startWhatIf.mock.calls[0]?.[2]).toEqual({ kind: "capacity" });
    act(() => {
      send({ kind: "finished", result: { kind: "capacity", report: capacity } });
    });
    expect(
      await screen.findByText(/Handles up to about 450 req\/s \(2\.25× today's traffic\)/),
    ).toBeInTheDocument();
    expect(screen.getByText(/Limited by App server: 96% busy/)).toBeInTheDocument();
  });

  it("saves the targets when they are valid", async () => {
    const onChange = vi.fn<(w: WhatIf) => void>();
    render(<WhatIfTab project={project} templates={templates} onChange={onChange} />);
    const p95 = screen.getByLabelText("p95 latency target (ms)");
    await userEvent.clear(p95);
    await userEvent.type(p95, "150");
    expect(onChange.mock.lastCall?.[0].slo.p95Ms).toBe(150);
    const errors = screen.getByLabelText("Error limit (%)");
    await userEvent.clear(errors);
    await userEvent.type(errors, "250");
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(onChange.mock.calls.every(([w]) => (w.slo.maxErrorRate ?? 0) <= 1)).toBe(true);
  });

  it("cancels a run and reports failures", async () => {
    render(<WhatIfTab project={project} templates={templates} onChange={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "Capacity" }));
    const send = await start("Find capacity");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(api.cancelWhatIf).toHaveBeenCalled();
    act(() => {
      send({ kind: "cancelled" });
    });
    expect(await screen.findByText("Cancelled.")).toBeInTheDocument();

    api.startWhatIf.mockRejectedValue(
      new CommandError({
        kind: "validation",
        issues: [{ field: "spec.points", message: "must be between 2 and 24" }],
      }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Find capacity" }));
    expect(await screen.findByText(/must be between 2 and 24/)).toBeInTheDocument();
  });

  it("stops the analysis when the tab is closed", async () => {
    api.startWhatIf.mockResolvedValue(undefined);
    const { unmount } = render(
      <WhatIfTab project={project} templates={templates} onChange={vi.fn()} />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Compare" }));
    await screen.findByRole("button", { name: "Cancel" });
    unmount();
    expect(api.cancelWhatIf).toHaveBeenCalled();
  });
});
