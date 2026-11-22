import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project, SimReport } from "../bindings";
import { chaosPlan, serverProfile, topology, trafficProfile, whatIfPlan } from "../test/fixtures";

const runSimulation = vi.hoisted(() => vi.fn());
const videoAvailable = vi.hoisted(() => vi.fn(() => Promise.resolve(false)));
vi.mock("../ipc", async (importActual) => {
  const actual = await importActual<typeof import("../ipc")>();
  return { ...actual, api: { runSimulation, videoAvailable } };
});

import { CommandError } from "../ipc";
import { SimulationPanel } from "./SimulationPanel";

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

const latency = { meanMs: 12, p50Ms: 10, p95Ms: 30, p99Ms: 55 };

const report: SimReport = {
  notice: "Estimate from a simulation model.",
  seed: 1,
  durationS: 60,
  warmupS: 5,
  summary: {
    offered: 1000,
    completed: 990,
    errors: 10,
    retries: 0,
    throughputRps: 180,
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
      utilization: 0.93,
      meanQueue: 3.2,
      maxQueue: 40,
      latency,
      severity: "saturated",
    },
  ],
  bottleneck: {
    nodeId: "app",
    label: "App server",
    severity: "saturated",
    reason: "93% busy, 10 requests rejected (queue full)",
  },
  timeline: [],
  faults: [],
  assumptions: ["Open-loop traffic."],
};

describe("SimulationPanel", () => {
  beforeEach(() => {
    runSimulation.mockReset();
  });

  it("runs the open project and shows the bottleneck and node results", async () => {
    runSimulation.mockResolvedValue(report);
    render(<SimulationPanel project={project} />);
    await userEvent.click(screen.getByRole("button", { name: "Run simulation" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Bottleneck: App server");
    expect(runSimulation).toHaveBeenCalledWith(project, { seed: 1, durationS: 60, warmupS: 5 });
    expect(screen.getByText("93.0%")).toBeInTheDocument();
    expect(screen.getByText("Estimate from a simulation model.")).toBeInTheDocument();
  });

  it("shows the fault impact table and retries when faults were injected", async () => {
    const window = (errorRate: number, p95Ms: number) => ({ errorRate, p95Ms, throughputRps: 100 });
    runSimulation.mockResolvedValue({
      ...report,
      summary: { ...report.summary, retries: 42 },
      faults: [
        {
          faultId: "f1",
          label: "Crash of App server",
          startS: 20,
          endS: 30,
          before: window(0, 20),
          during: window(0.9, 800),
          after: window(0.01, 25),
          recoveryS: 3,
        },
      ],
    });
    render(<SimulationPanel project={project} />);
    await userEvent.click(screen.getByRole("button", { name: "Run simulation" }));
    expect(await screen.findByText("Crash of App server")).toBeInTheDocument();
    expect(screen.getByText("90.0%")).toBeInTheDocument();
    expect(screen.getByText("3 s")).toBeInTheDocument();
    expect(screen.getByText("Retries sent")).toBeInTheDocument();
  });

  it("sends the chosen seed", async () => {
    runSimulation.mockResolvedValue(report);
    render(<SimulationPanel project={project} />);
    const seed = screen.getByLabelText("Seed");
    await userEvent.clear(seed);
    await userEvent.type(seed, "42");
    await userEvent.click(screen.getByRole("button", { name: "Run simulation" }));
    expect(runSimulation).toHaveBeenCalledWith(project, expect.objectContaining({ seed: 42 }));
  });

  it("shows the engine's error message", async () => {
    runSimulation.mockRejectedValue(
      new CommandError({ kind: "simulation", message: "the topology contains a loop" }),
    );
    render(<SimulationPanel project={project} />);
    await userEvent.click(screen.getByRole("button", { name: "Run simulation" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("the topology contains a loop");
  });
});
