import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  ControlRecord,
  LiveEvent,
  LiveNodeTick,
  LiveTick,
  Project,
  SimReport,
} from "../bindings";
import { chaosPlan, serverProfile, topology, trafficProfile, whatIfPlan } from "../test/fixtures";

const startLiveRun = vi.hoisted(() => vi.fn());
const controlLiveRun = vi.hoisted(() => vi.fn());
const videoAvailable = vi.hoisted(() => vi.fn(() => Promise.resolve(false)));
vi.mock("../ipc", async (importActual) => {
  const actual = await importActual<typeof import("../ipc")>();
  return { ...actual, api: { startLiveRun, controlLiveRun, videoAvailable } };
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
  controls: [],
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
      [],
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

  describe("steering", () => {
    const record = (summary: string, command: ControlRecord["command"], tS = 3): ControlRecord => ({
      tS,
      command,
      summary,
      faultId: null,
    });

    it("shows the controls only while a run is going", async () => {
      expect(screen.queryByRole("region", { name: "Steer the run" })).not.toBeInTheDocument();
      const onEvent = await startRun();
      expect(screen.getByRole("region", { name: "Steer the run" })).toBeInTheDocument();
      act(() => {
        onEvent({ kind: "finished", report });
      });
      await waitFor(() => {
        expect(screen.queryByRole("region", { name: "Steer the run" })).not.toBeInTheDocument();
      });
    });

    it("changes the traffic and logs what was applied", async () => {
      controlLiveRun.mockResolvedValue(record("Traffic ×2", { kind: "setTraffic", multiplier: 2 }));
      await startRun();
      await userEvent.click(screen.getByRole("button", { name: "Traffic ×2" }));
      expect(controlLiveRun).toHaveBeenLastCalledWith({
        kind: "control",
        command: { kind: "setTraffic", multiplier: 2 },
      });
      expect(await screen.findByText("at 3 s: Traffic ×2")).toBeInTheDocument();
      expect(screen.getByText("Now ×2")).toBeInTheDocument();
    });

    it("injects a fault on a server node and can end it early", async () => {
      const onEvent = await startRun();
      controlLiveRun.mockResolvedValue({
        ...record("Slowdown of App (live)", {
          kind: "injectFault",
          target: "app",
          fault: { kind: "slowdown", factor: 5 },
          durationS: 20,
        }),
        faultId: "live-1",
      });
      await userEvent.selectOptions(screen.getByLabelText("Fault"), "slowdown");
      await userEvent.click(screen.getByRole("button", { name: "Inject fault" }));
      expect(controlLiveRun).toHaveBeenLastCalledWith({
        kind: "control",
        command: {
          kind: "injectFault",
          target: "app",
          fault: { kind: "slowdown", factor: 5 },
          durationS: 20,
        },
      });
      act(() => {
        onEvent({ kind: "tick", tick: { ...tick(3), activeFaults: ["live-1"] } });
      });
      controlLiveRun.mockResolvedValue(
        record("Ended: Slowdown", { kind: "endFault", faultId: "live-1" }, 5),
      );
      await userEvent.click(await screen.findByRole("button", { name: /End Slowdown on .* now/ }));
      expect(controlLiveRun).toHaveBeenLastCalledWith({
        kind: "control",
        command: { kind: "endFault", faultId: "live-1" },
      });
    });

    it("only offers server nodes as fault targets", async () => {
      await startRun();
      const options = within(screen.getByLabelText("Node")).getAllByRole("option");
      expect(options.map((o) => o.textContent)).not.toContain("Clients");
      expect(options.length).toBeGreaterThan(0);
    });

    it("shows why a change was refused and keeps the run going", async () => {
      await startRun();
      controlLiveRun.mockRejectedValueOnce(
        new CommandError({
          kind: "validation",
          issues: [{ field: "multiplier", message: "must be between 0.1 and 10" }],
        }),
      );
      await userEvent.click(screen.getByRole("button", { name: "Set traffic" }));
      expect(await screen.findByRole("alert")).toHaveTextContent("must be between 0.1 and 10");
      expect(screen.getByText("Running…")).toBeInTheDocument();
    });

    it("replays a steered run with the logged changes", async () => {
      const onEvent = await startRun();
      const steered = record("Traffic ×3", { kind: "setTraffic", multiplier: 3 }, 4);
      act(() => {
        onEvent({ kind: "finished", report: { ...report, controls: [steered] } });
      });
      await userEvent.click(
        await screen.findByRole("button", { name: "Replay with the same changes" }),
      );
      await waitFor(() => {
        expect(startLiveRun).toHaveBeenCalledTimes(2);
      });
      expect(startLiveRun.mock.calls[1]?.[4]).toEqual([steered]);
      expect(startLiveRun.mock.calls[1]?.[1]).toEqual({ seed: 1, durationS: 10, warmupS: 5 });
    });
  });
});
