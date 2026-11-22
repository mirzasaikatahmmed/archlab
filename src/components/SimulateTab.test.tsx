import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Project } from "../bindings";
import { chaosPlan, serverProfile, topology, trafficProfile, whatIfPlan } from "../test/fixtures";

vi.mock("../ipc", async (importActual) => {
  const actual = await importActual<typeof import("../ipc")>();
  return {
    ...actual,
    api: {
      runSimulation: vi.fn(),
      startLiveRun: vi.fn(),
      controlLiveRun: vi.fn(),
      videoAvailable: vi.fn(() => Promise.resolve(false)),
    },
  };
});

import { SimulateTab } from "./SimulateTab";

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

describe("SimulateTab", () => {
  it("switches between instant and live runs", async () => {
    render(<SimulateTab project={project} />);
    expect(screen.getByRole("button", { name: "Run simulation" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Live run" }));
    expect(screen.getByRole("button", { name: "Start live run" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Run simulation" })).not.toBeInTheDocument();
  });
});
