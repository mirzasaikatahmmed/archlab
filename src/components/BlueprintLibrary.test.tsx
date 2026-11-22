import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BlueprintView, Project } from "../bindings";
import { chaosPlan, serverProfile, topology, trafficProfile, whatIfPlan } from "../test/fixtures";

const api = vi.hoisted(() => ({ listBlueprints: vi.fn(), blueprintProject: vi.fn() }));
vi.mock("../ipc", async (importActual) => {
  const actual = await importActual<typeof import("../ipc")>();
  return { ...actual, api };
});

import { CommandError } from "../ipc";
import { BlueprintLibrary } from "./BlueprintLibrary";

const blueprint: BlueprintView = {
  id: "million-users",
  title: "1M+ users architecture",
  summary: "The full stack for a read-heavy product at scale.",
  requestsPerUserPerDay: 20,
  peakFactor: 5,
  readRatio: 0.9,
  slo: { p95Ms: 300, maxErrorRate: 0.01 },
  tiers: [
    {
      id: "1m",
      label: "1 million daily users",
      dailyUsers: 1000000,
      peakRps: 1157.4,
      runS: 60,
      machines: 13,
    },
    {
      id: "10m",
      label: "10 million daily users",
      dailyUsers: 10000000,
      peakRps: 11574,
      runS: 20,
      machines: 17,
    },
  ],
  why: ["Every layer has more than one copy."],
  topology,
};

const project: Project = {
  id: "p",
  name: "1M+ users architecture (1 million daily users)",
  serverProfile,
  trafficProfile,
  topology,
  chaos: chaosPlan,
  whatif: whatIfPlan,
  createdAt: 0,
  updatedAt: 0,
};

describe("BlueprintLibrary", () => {
  beforeEach(() => {
    api.listBlueprints.mockReset();
    api.blueprintProject.mockReset();
    api.listBlueprints.mockResolvedValue([blueprint]);
  });

  it("shows each blueprint with its parts and the facts of the chosen size", async () => {
    render(<BlueprintLibrary onUse={vi.fn()} />);
    expect(
      await screen.findByRole("heading", { name: "1M+ users architecture" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/About 1,157 requests per second at peak, 13 machines/),
    ).toBeInTheDocument();
    expect(screen.getByText(/Run the simulation for 60 s/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "10 million daily users" }));
    expect(
      screen.getByText(/About 11,574 requests per second at peak, 17 machines/),
    ).toBeInTheDocument();
    expect(screen.getByText(/Run the simulation for 20 s/)).toBeInTheDocument();
    expect(screen.getByText("Every layer has more than one copy.")).toBeInTheDocument();
  });

  it("opens the design at the chosen size", async () => {
    api.blueprintProject.mockResolvedValue(project);
    const onUse = vi.fn();
    render(<BlueprintLibrary onUse={onUse} />);
    await userEvent.click(await screen.findByRole("button", { name: "10 million daily users" }));
    await userEvent.click(screen.getByRole("button", { name: "Use 1M+ users architecture" }));
    expect(api.blueprintProject).toHaveBeenCalledWith("million-users", "10m");
    expect(onUse).toHaveBeenCalledWith(project, expect.objectContaining({ id: "10m" }));
  });

  it("explains a failure to load or to open", async () => {
    api.listBlueprints.mockRejectedValue(new Error("boom"));
    const { unmount } = render(<BlueprintLibrary onUse={vi.fn()} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load blueprints");
    unmount();

    api.listBlueprints.mockResolvedValue([blueprint]);
    api.blueprintProject.mockRejectedValue(
      new CommandError({ kind: "notFound", id: "million-users" }),
    );
    render(<BlueprintLibrary onUse={vi.fn()} />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Use 1M+ users architecture" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent("Not found: million-users");
  });
});
