import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Calibration, CalibrationEvent } from "../bindings";
import { serverProfile } from "../test/fixtures";

const api = vi.hoisted(() => ({
  latestCalibration: vi.fn(),
  startCalibration: vi.fn(),
  cancelCalibration: vi.fn(),
  applyCalibration: vi.fn(),
}));
vi.mock("../ipc", async (importActual) => {
  const actual = await importActual<typeof import("../ipc")>();
  return { ...actual, api };
});

import { CommandError } from "../ipc";
import { CalibrationSection } from "./CalibrationSection";

const calibration: Calibration = {
  measuredAt: Date.UTC(2026, 10, 22, 12, 0, 0),
  equivalentGhz: 3.456,
  diskIops: 4321.4,
  loopbackLatencyMs: 0.0456,
  debugBuild: false,
  notes: ["Loopback latency is the delay inside this computer."],
};

async function startAndGetCallback(): Promise<(e: CalibrationEvent) => void> {
  api.startCalibration.mockResolvedValue(undefined);
  await userEvent.click(screen.getByRole("button", { name: "Calibrate this machine" }));
  await waitFor(() => {
    expect(api.startCalibration).toHaveBeenCalled();
  });
  const cb = api.startCalibration.mock.calls[0]?.[0] as ((e: CalibrationEvent) => void) | undefined;
  if (!cb) {
    throw new Error("no callback passed");
  }
  return cb;
}

describe("CalibrationSection", () => {
  beforeEach(() => {
    Object.values(api).forEach((m) => {
      m.mockReset();
    });
    api.latestCalibration.mockResolvedValue(null);
  });

  it("shows the stored calibration on load", async () => {
    api.latestCalibration.mockResolvedValue(calibration);
    render(<CalibrationSection profile={serverProfile} onApply={vi.fn()} />);
    expect(await screen.findByText("3.46 GHz")).toBeInTheDocument();
    expect(screen.getByText("4,321 IOPS")).toBeInTheDocument();
    expect(screen.getByText("0.046 ms")).toBeInTheDocument();
    expect(screen.getByText(/Loopback latency is the delay/)).toBeInTheDocument();
  });

  it("shows progress, then the new result", async () => {
    render(<CalibrationSection profile={serverProfile} onApply={vi.fn()} />);
    const send = await startAndGetCallback();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
    act(() => {
      send({ kind: "step", name: "Measuring CPU speed", index: 1, total: 3 });
    });
    expect(screen.getByRole("status")).toHaveTextContent("Measuring CPU speed (1 of 3)");
    act(() => {
      send({ kind: "finished", calibration });
    });
    expect(await screen.findByText("3.46 GHz")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Calibrate this machine" })).toBeInTheDocument();
  });

  it("applies the measurements to the profile it was given", async () => {
    api.latestCalibration.mockResolvedValue(calibration);
    const applied = { ...serverProfile, cpuGhz: 3.46 };
    api.applyCalibration.mockResolvedValue(applied);
    const onApply = vi.fn();
    render(<CalibrationSection profile={serverProfile} onApply={onApply} />);
    await userEvent.click(await screen.findByRole("button", { name: "Apply to this profile" }));
    expect(api.applyCalibration).toHaveBeenCalledWith(serverProfile);
    expect(onApply).toHaveBeenCalledWith(applied);
  });

  it("warns when the numbers came from a development build", async () => {
    api.latestCalibration.mockResolvedValue({ ...calibration, debugBuild: true });
    render(<CalibrationSection profile={serverProfile} onApply={vi.fn()} />);
    expect(await screen.findByText(/development build/)).toBeInTheDocument();
  });

  it("cancels a running calibration", async () => {
    api.cancelCalibration.mockResolvedValue(null);
    render(<CalibrationSection profile={serverProfile} onApply={vi.fn()} />);
    const send = await startAndGetCallback();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(api.cancelCalibration).toHaveBeenCalled();
    act(() => {
      send({ kind: "cancelled" });
    });
    expect(screen.getByRole("button", { name: "Calibrate this machine" })).toBeInTheDocument();
  });

  it("shows a failure from the benchmark and one from starting it", async () => {
    render(<CalibrationSection profile={serverProfile} onApply={vi.fn()} />);
    const send = await startAndGetCallback();
    act(() => {
      send({ kind: "failed", message: "disk benchmark failed: permission denied" });
    });
    expect(await screen.findByRole("alert")).toHaveTextContent("permission denied");

    api.startCalibration.mockRejectedValue(
      new CommandError({ kind: "runtime", message: "calibration is already running" }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Calibrate this machine" }));
    expect(await screen.findByText("calibration is already running")).toBeInTheDocument();
  });
});
