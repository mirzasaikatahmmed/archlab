import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project, RealComparison, RealEvent, SimReport } from "../bindings";
import { chaosPlan, serverProfile, topology, trafficProfile, whatIfPlan } from "../test/fixtures";

const api = vi.hoisted(() => ({ startRealRun: vi.fn(), cancelRealRun: vi.fn() }));
vi.mock("../ipc", async (importActual) => {
  const actual = await importActual<typeof import("../ipc")>();
  return { ...actual, api };
});

import { CommandError } from "../ipc";
import { RealPanel } from "./RealPanel";

const project: Project = {
  id: "p1",
  name: "Demo",
  serverProfile,
  trafficProfile,
  topology,
  chaos: chaosPlan,
  whatif: whatIfPlan,
  createdAt: 0,
  updatedAt: 0,
};

const latency = { meanMs: 12, p50Ms: 11, p95Ms: 30, p99Ms: 50 };

const comparison: RealComparison = {
  simulated: {} as SimReport,
  real: {
    notice: "Measured on this computer.",
    durationS: 20,
    warmupS: 3,
    hostCores: 8,
    summary: {
      offered: 1000,
      completed: 990,
      errors: 10,
      retries: 0,
      throughputRps: 49,
      errorRate: 0.01,
      latency,
    },
    nodes: [
      {
        id: "app",
        label: "App server",
        kind: "appServer",
        servers: 4,
        capacity: 500,
        arrivals: 1000,
        completed: 990,
        rejected: 10,
        timedOut: 0,
        faulted: 0,
        utilization: 0.42,
        meanQueue: 0.5,
        maxQueue: 6,
        latency,
        severity: "healthy",
        backlog: null,
      },
    ],
    timeline: [],
    notes: ["All nodes share this computer's 8 cores."],
  },
  deltas: [
    { metric: "Throughput (req/s)", simulated: 50, real: 49, differencePct: -2 },
    { metric: "Error rate", simulated: 0, real: 0.01, differencePct: null },
    { metric: "App server utilization", simulated: 0.4, real: 0.42, differencePct: 5 },
  ],
};

async function startRun(): Promise<(e: RealEvent) => void> {
  api.startRealRun.mockResolvedValue(undefined);
  render(<RealPanel project={project} />);
  await userEvent.click(screen.getByRole("button", { name: "Start real run" }));
  await waitFor(() => {
    expect(api.startRealRun).toHaveBeenCalled();
  });
  const cb = api.startRealRun.mock.calls[0]?.[2] as ((e: RealEvent) => void) | undefined;
  if (!cb) {
    throw new Error("no callback passed");
  }
  return cb;
}

describe("RealPanel", () => {
  beforeEach(() => {
    api.startRealRun.mockReset();
    api.cancelRealRun.mockReset();
    api.cancelRealRun.mockResolvedValue(null);
  });

  it("warns that the run uses this computer", () => {
    render(<RealPanel project={project} />);
    expect(screen.getByRole("note")).toHaveTextContent("Runs on this computer");
  });

  it("starts with the chosen options and shows progress", async () => {
    const send = await startRun();
    expect(api.startRealRun).toHaveBeenCalledWith(
      project,
      { seed: 1, durationS: 20, warmupS: 3 },
      expect.any(Function),
    );
    act(() => {
      send({ kind: "progress", elapsedS: 5, totalS: 20, offered: 250, completed: 240, errors: 2 });
    });
    expect(await screen.findByText(/250 sent · 240 answered · 2 failed/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
  });

  it("shows simulated and measured results side by side", async () => {
    const send = await startRun();
    act(() => {
      send({ kind: "finished", comparison });
    });
    expect(await screen.findByText("Simulated vs measured")).toBeInTheDocument();
    expect(screen.getByText("Throughput (req/s)")).toBeInTheDocument();
    expect(screen.getByText("-2%")).toBeInTheDocument();
    expect(screen.getByText("+5%")).toBeInTheDocument();
    expect(screen.getByText("1.0%")).toBeInTheDocument();
    expect(screen.getByText(/share this computer's 8 cores/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start real run" })).toBeEnabled();
  });

  it("cancels a run", async () => {
    const send = await startRun();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(api.cancelRealRun).toHaveBeenCalled();
    act(() => {
      send({ kind: "cancelled" });
    });
    expect(await screen.findByText("Cancelled.")).toBeInTheDocument();
  });

  it("reports a failure from the run and from starting it", async () => {
    const send = await startRun();
    act(() => {
      send({ kind: "failed", message: "could not open a local port" });
    });
    expect(await screen.findByRole("alert")).toHaveTextContent("could not open a local port");

    api.startRealRun.mockRejectedValue(
      new CommandError({
        kind: "validation",
        issues: [{ field: "durationS", message: "real runs are limited to 60 seconds" }],
      }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Start real run" }));
    expect(await screen.findByText(/limited to 60 seconds/)).toBeInTheDocument();
  });

  it("stops the run when the panel is closed", async () => {
    api.startRealRun.mockResolvedValue(undefined);
    const { unmount } = render(<RealPanel project={project} />);
    await userEvent.click(screen.getByRole("button", { name: "Start real run" }));
    await screen.findByRole("button", { name: "Cancel" });
    unmount();
    expect(api.cancelRealRun).toHaveBeenCalled();
  });
});
