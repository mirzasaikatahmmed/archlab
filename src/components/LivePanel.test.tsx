import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LiveEvent, LiveNodeTick, LiveTick, Project, SimReport } from "../bindings";
import { chaosPlan, serverProfile, topology, trafficProfile, whatIfPlan } from "../test/fixtures";

const startLiveRun = vi.hoisted(() => vi.fn());
const controlLiveRun = vi.hoisted(() => vi.fn());
vi.mock("../ipc", async (importActual) => {
  const actual = await importActual<typeof import("../ipc")>();
  return { ...actual, api: { startLiveRun, controlLiveRun } };
});

import { CommandError } from "../ipc";
import { LivePanel } from "./LivePanel";

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

const node: LiveNodeTick = { id: "app", utilization: 0.85, queue: 3, busy: 4, dropped: 2 };

function tick(second: number): LiveTick {
  return {
    point: {
      tS: second,
      offeredRps: 100,
      completedRps: 95,
      errorRps: 5,
      p95Ms: 40 + second,
    },
    nodes: [node],
    progress: (second + 1) / 10,
    activeFaults: [],
  };
}

const latency = { meanMs: 12, p50Ms: 10, p95Ms: 30, p99Ms: 55 };
const report: SimReport = {
  notice: "Estimate from a simulation model.",
  seed: 1,
  durationS: 10,
  warmupS: 5,
  summary: {
    offered: 100,
    completed: 95,
    errors: 5,
    retries: 0,
    throughputRps: 19,
    errorRate: 0.05,
    latency,
  },
  nodes: [],
  bottleneck: null,
  timeline: [],
  faults: [],
  assumptions: [],
};

/** Starts a run and returns the callback the panel gave to the API. */
async function startRun(): Promise<(event: LiveEvent) => void> {
  startLiveRun.mockResolvedValue(undefined);
  render(<LivePanel project={project} />);
  await userEvent.click(screen.getByRole("button", { name: "Start live run" }));
  await waitFor(() => {
    expect(startLiveRun).toHaveBeenCalled();
  });
  const onEvent = startLiveRun.mock.calls[0]?.[3] as ((event: LiveEvent) => void) | undefined;
  if (!onEvent) {
    throw new Error("panel did not pass an event callback");
  }
  return onEvent;
}

describe("LivePanel", () => {
  beforeEach(() => {
    startLiveRun.mockReset();
    controlLiveRun.mockReset();
    controlLiveRun.mockResolvedValue(null);
  });

  it("starts with the chosen options and charts incoming ticks", async () => {
    const onEvent = await startRun();
    expect(startLiveRun).toHaveBeenCalledWith(
      project,
      { seed: 1, durationS: 60, warmupS: 5 },
      5,
      expect.any(Function),
    );
    act(() => {
      onEvent({ kind: "tick", tick: tick(0) });
      onEvent({ kind: "tick", tick: tick(1) });
    });
    await userEvent.click(screen.getByRole("button", { name: "Charts" }));
    expect(
      await screen.findByRole("img", { name: /Latency p95: p95 41\.0 ms/ }),
    ).toBeInTheDocument();
    expect(screen.getByText(/85\.0% busy · queue 3 · dropped 2/)).toBeInTheDocument();
    expect(screen.getByText("Running…")).toBeInTheDocument();
  });

  it("shows the stage by default and switches to charts", async () => {
    await startRun();
    expect(screen.getByRole("img", { name: /Isometric view of the topology/ })).toBeInTheDocument();
    expect(screen.queryByRole("img", { name: /Latency p95/ })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Charts" }));
    expect(screen.queryByRole("img", { name: /Isometric view/ })).not.toBeInTheDocument();
  });

  it("shows the final report when the run finishes and allows another run", async () => {
    const onEvent = await startRun();
    act(() => {
      onEvent({ kind: "tick", tick: tick(0) });
      onEvent({ kind: "finished", report });
    });
    expect(await screen.findByText("Finished.")).toBeInTheDocument();
    expect(screen.getByText("Estimate from a simulation model.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start live run" })).toBeEnabled();
  });

  it("pauses, resumes and stops through the controller", async () => {
    const onEvent = await startRun();
    await userEvent.click(screen.getByRole("button", { name: "Pause" }));
    expect(controlLiveRun).toHaveBeenLastCalledWith({ kind: "pause" });
    expect(screen.getByText("Paused.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Resume" }));
    expect(controlLiveRun).toHaveBeenLastCalledWith({ kind: "resume" });
    await userEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(controlLiveRun).toHaveBeenLastCalledWith({ kind: "stop" });
    act(() => {
      onEvent({ kind: "stopped" });
    });
    expect(await screen.findByText("Stopped.")).toBeInTheDocument();
  });

  it("changes speed during a run", async () => {
    await startRun();
    await userEvent.selectOptions(screen.getByLabelText("Speed"), "max");
    expect(controlLiveRun).toHaveBeenLastCalledWith({ kind: "setSpeed", speed: null });
  });

  it("reports a run that fails to start", async () => {
    startLiveRun.mockRejectedValue(
      new CommandError({ kind: "simulation", message: "a live run is already in progress" }),
    );
    render(<LivePanel project={project} />);
    await userEvent.click(screen.getByRole("button", { name: "Start live run" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("already in progress");
    expect(screen.getByText("Failed.")).toBeInTheDocument();
  });

  it("reports an engine failure from the stream", async () => {
    const onEvent = await startRun();
    act(() => {
      onEvent({ kind: "failed", message: "the simulation exceeded 5000000 events" });
    });
    expect(await screen.findByRole("alert")).toHaveTextContent("exceeded 5000000 events");
  });

  it("stops the run when the panel is closed", async () => {
    startLiveRun.mockResolvedValue(undefined);
    const { unmount } = render(<LivePanel project={project} />);
    await userEvent.click(screen.getByRole("button", { name: "Start live run" }));
    await screen.findByRole("button", { name: "Stop" });
    unmount();
    expect(controlLiveRun).toHaveBeenCalledWith({ kind: "stop" });
  });
});
