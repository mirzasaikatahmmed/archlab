import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project, ScenarioView, SimReport } from "../bindings";
import { chaosPlan, serverProfile, topology, trafficProfile, whatIfPlan } from "../test/fixtures";

const api = vi.hoisted(() => ({
  getScenario: vi.fn(),
  scenarioStepProject: vi.fn(),
  startLiveRun: vi.fn(),
  controlLiveRun: vi.fn(),
  runSimulation: vi.fn(),
}));
vi.mock("../ipc", async (importActual) => {
  const actual = await importActual<typeof import("../ipc")>();
  return { ...actual, api };
});

import { CommandError } from "../ipc";
import { ScenarioPlayer } from "./ScenarioPlayer";

const scenario: ScenarioView = {
  id: "demo",
  title: "Demo scenario",
  summary: "A demo.",
  level: "beginner",
  minutes: 3,
  steps: [
    {
      title: "Where it starts",
      narration: ["First paragraph.", "Second paragraph."],
      highlight: ["app"],
      changes: [],
      play: { mode: "live", durationS: 30, speed: 5 },
    },
    {
      title: "Traffic grows",
      narration: ["Things get busy."],
      highlight: [],
      changes: ["Traffic 450 req/s"],
      play: { mode: "instant", durationS: 20 },
    },
  ],
};

const stepProject = (n: number): Project => ({
  id: `p${String(n)}`,
  name: `Demo (step ${String(n)})`,
  serverProfile,
  trafficProfile,
  topology,
  chaos: chaosPlan,
  whatif: whatIfPlan,
  createdAt: 0,
  updatedAt: 0,
});

const latency = { meanMs: 12, p50Ms: 10, p95Ms: 30, p99Ms: 55 };
const report = {
  notice: "Estimate.",
  seed: 1,
  durationS: 20,
  warmupS: 5,
  summary: {
    offered: 100,
    completed: 99,
    errors: 1,
    retries: 0,
    throughputRps: 5,
    errorRate: 0.01,
    latency,
  },
  nodes: [],
  bottleneck: null,
  timeline: [],
  faults: [],
  controls: [],
  assumptions: [],
} as SimReport;

describe("ScenarioPlayer", () => {
  beforeEach(() => {
    Object.values(api).forEach((m) => {
      m.mockReset();
    });
    api.getScenario.mockResolvedValue(scenario);
    api.scenarioStepProject.mockImplementation((_id: string, step: number) =>
      Promise.resolve(stepProject(step + 1)),
    );
    api.startLiveRun.mockResolvedValue(undefined);
    api.controlLiveRun.mockResolvedValue(null);
    api.runSimulation.mockResolvedValue(report);
  });

  it("shows the first step and plays it live with the step's settings", async () => {
    render(<ScenarioPlayer scenarioId="demo" onExit={vi.fn()} onOpenAsProject={vi.fn()} />);
    expect(await screen.findByText("Where it starts")).toBeInTheDocument();
    expect(screen.getByText("First paragraph.")).toBeInTheDocument();
    expect(screen.getByText("Step 1 of 2")).toBeInTheDocument();
    await waitFor(() => {
      expect(api.startLiveRun).toHaveBeenCalled();
    });
    const [project, options, speed] = api.startLiveRun.mock.calls[0] as [Project, unknown, unknown];
    expect(project.id).toBe("p1");
    expect(options).toEqual({ seed: 1, durationS: 30, warmupS: 5 });
    expect(speed).toBe(5);
    expect(screen.getByRole("button", { name: "Back" })).toBeDisabled();
  });

  it("moves to the next step, stopping the running one first, and lists what changed", async () => {
    render(<ScenarioPlayer scenarioId="demo" onExit={vi.fn()} onOpenAsProject={vi.fn()} />);
    await screen.findByText("Where it starts");
    await waitFor(() => {
      expect(api.startLiveRun).toHaveBeenCalledTimes(1);
    });
    await userEvent.click(screen.getByRole("button", { name: "Next" }));
    expect(await screen.findByText("Traffic grows")).toBeInTheDocument();
    expect(screen.getByText("Traffic 450 req/s")).toBeInTheDocument();
    expect(api.scenarioStepProject).toHaveBeenLastCalledWith("demo", 1);
    expect(api.controlLiveRun).toHaveBeenCalledWith({ kind: "stop" });
    // The second step is an instant run, so it shows a report instead of starting a live run.
    await waitFor(() => {
      expect(api.runSimulation).toHaveBeenCalled();
    });
    expect(api.startLiveRun).toHaveBeenCalledTimes(1);
    expect(await screen.findByText("Estimate.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Finish" })).toBeInTheDocument();
  });

  it("goes back, replays and finishes", async () => {
    const onExit = vi.fn();
    render(<ScenarioPlayer scenarioId="demo" onExit={onExit} onOpenAsProject={vi.fn()} />);
    await screen.findByText("Where it starts");
    await userEvent.click(screen.getByRole("button", { name: "Next" }));
    await screen.findByText("Traffic grows");
    await userEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(await screen.findByText("Where it starts")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Replay" }));
    await waitFor(() => {
      expect(api.startLiveRun.mock.calls.length).toBeGreaterThanOrEqual(2);
    });
    await userEvent.click(screen.getByRole("button", { name: "Next" }));
    await screen.findByText("Traffic grows");
    await userEvent.click(screen.getByRole("button", { name: "Finish" }));
    expect(onExit).toHaveBeenCalled();
  });

  it("opens the current step as a project", async () => {
    const onOpen = vi.fn();
    render(<ScenarioPlayer scenarioId="demo" onExit={vi.fn()} onOpenAsProject={onOpen} />);
    await screen.findByText("Where it starts");
    await userEvent.click(screen.getByRole("button", { name: "Open this step as a project" }));
    expect(onOpen).toHaveBeenCalledWith(expect.objectContaining({ id: "p1" }));
  });

  it("retries starting while the previous run is still shutting down", async () => {
    api.startLiveRun
      .mockRejectedValueOnce(
        new CommandError({ kind: "simulation", message: "a live run is already in progress" }),
      )
      .mockResolvedValue(undefined);
    render(<ScenarioPlayer scenarioId="demo" onExit={vi.fn()} onOpenAsProject={vi.fn()} />);
    await screen.findByText("Where it starts");
    await waitFor(() => {
      expect(api.startLiveRun).toHaveBeenCalledTimes(2);
    });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows an error and a way back when the scenario cannot be loaded", async () => {
    api.getScenario.mockRejectedValue(new CommandError({ kind: "notFound", id: "gone" }));
    const onExit = vi.fn();
    render(<ScenarioPlayer scenarioId="gone" onExit={onExit} onOpenAsProject={vi.fn()} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Not found: gone");
    await userEvent.click(screen.getByRole("button", { name: "Back to scenarios" }));
    expect(onExit).toHaveBeenCalled();
  });
});
