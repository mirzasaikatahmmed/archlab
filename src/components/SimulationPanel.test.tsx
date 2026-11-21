import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project, SimReport } from "../bindings";
import { serverProfile, topology, trafficProfile } from "../test/fixtures";

const runSimulation = vi.hoisted(() => vi.fn());
vi.mock("../ipc", async (importActual) => {
  const actual = await importActual<typeof import("../ipc")>();
  return { ...actual, api: { runSimulation } };
});

import { CommandError } from "../ipc";
import { SimulationPanel } from "./SimulationPanel";

const project: Project = {
  id: "p1",
  name: "Demo",
  serverProfile,
  trafficProfile,
  topology,
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
