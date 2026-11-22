import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Advice, Project, Requirements } from "../bindings";
import { chaosPlan, serverProfile, topology, trafficProfile, whatIfPlan } from "../test/fixtures";

const api = vi.hoisted(() => ({ defaultRequirements: vi.fn(), adviseDesign: vi.fn() }));
vi.mock("../ipc", async (importActual) => {
  const actual = await importActual<typeof import("../ipc")>();
  return { ...actual, api };
});

import { CommandError } from "../ipc";
import { AdvisorPanel } from "./AdvisorPanel";

const defaults: Requirements = {
  dailyUsers: 1000000,
  requestsPerUserPerDay: 20,
  readRatio: 0.9,
  peakFactor: 5,
  shape: "web",
  availability: "high",
  machine: "medium",
  p95Ms: 300,
  maxErrorRate: 0.01,
};

const project: Project = {
  id: "p",
  name: "1M+ users architecture for 1000000 daily users",
  serverProfile,
  trafficProfile,
  topology,
  chaos: chaosPlan,
  whatif: whatIfPlan,
  createdAt: 0,
  updatedAt: 0,
};

const latency = { meanMs: 12, p50Ms: 10, p95Ms: 30, p99Ms: 55 };

function advice(overrides: Partial<Advice> = {}): Advice {
  return {
    blueprintId: "million-users",
    blueprintTitle: "1M+ users architecture",
    peakRps: 1157.4,
    machines: 12,
    meetsTargets: true,
    rounds: 2,
    nodes: [
      { id: "app", label: "App servers", kind: "appServer", copies: 3, utilization: 0.42 },
      { id: "db", label: "Database shards", kind: "database", copies: 2, utilization: 0.18 },
    ],
    rationale: ["Your peak is about 1157 requests per second.", "Design: 1M+ users architecture."],
    warnings: [],
    assumptions: ["Every machine is the same size."],
    project,
    report: {
      notice: "Estimate from a simulation model.",
      seed: 1,
      durationS: 60,
      warmupS: 5,
      summary: {
        offered: 1000,
        completed: 1000,
        errors: 0,
        retries: 0,
        throughputRps: 100,
        errorRate: 0,
        latency,
      },
      nodes: [],
      bottleneck: null,
      timeline: [],
      faults: [],
      controls: [],
      assumptions: [],
    },
    ...overrides,
  };
}

describe("AdvisorPanel", () => {
  beforeEach(() => {
    api.defaultRequirements.mockReset();
    api.adviseDesign.mockReset();
    api.defaultRequirements.mockResolvedValue(defaults);
  });

  it("starts from the defaults and sends the requirements as typed", async () => {
    api.adviseDesign.mockResolvedValue(advice());
    render(<AdvisorPanel onOpen={vi.fn()} />);
    const users = await screen.findByLabelText("Daily users");
    expect(users).toHaveValue(1000000);
    expect(screen.getByLabelText("Reads (% of requests)")).toHaveValue(90);
    await userEvent.clear(users);
    await userEvent.type(users, "250000");
    await userEvent.clear(screen.getByLabelText("Reads (% of requests)"));
    await userEvent.type(screen.getByLabelText("Reads (% of requests)"), "60");
    await userEvent.selectOptions(screen.getByLabelText("What it mostly does"), "jobs");
    await userEvent.selectOptions(screen.getByLabelText("Availability"), "critical");
    await userEvent.click(screen.getByRole("button", { name: "Design it" }));
    expect(api.adviseDesign).toHaveBeenCalledWith({
      ...defaults,
      dailyUsers: 250000,
      readRatio: 0.6,
      shape: "jobs",
      availability: "critical",
    });
  });

  it("shows the verdict, the design, the reasons and opens it as a project", async () => {
    api.adviseDesign.mockResolvedValue(advice());
    const onOpen = vi.fn();
    render(<AdvisorPanel onOpen={onOpen} />);
    await userEvent.click(await screen.findByRole("button", { name: "Design it" }));
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Meets your targets: 1M+ users architecture, 12 machines, about 1157 requests per second at peak.",
    );
    const table = screen.getByRole("table", { name: "The design" });
    expect(within(table).getByRole("rowheader", { name: "App servers" })).toBeInTheDocument();
    expect(within(table).getByText("42.0%")).toBeInTheDocument();
    expect(screen.getByText("Your peak is about 1157 requests per second.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Open as a project" }));
    expect(onOpen).toHaveBeenCalledWith(project);
    await userEvent.click(screen.getByRole("button", { name: "Show simulation results" }));
    expect(screen.getByText("Estimate from a simulation model.")).toBeInTheDocument();
  });

  it("says plainly when the targets are not met and why", async () => {
    api.adviseDesign.mockResolvedValue(
      advice({ meetsTargets: false, warnings: ["The design does not meet your targets."] }),
    );
    render(<AdvisorPanel onOpen={vi.fn()} />);
    await userEvent.click(await screen.findByRole("button", { name: "Design it" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Does not meet your targets");
    expect(screen.getByRole("list", { name: "Warnings" })).toHaveTextContent(
      "The design does not meet your targets.",
    );
  });

  it("shows why a request was refused and keeps the form", async () => {
    api.adviseDesign.mockRejectedValue(
      new CommandError({
        kind: "validation",
        issues: [
          { field: "dailyUsers", message: "this is about 99999 requests per second at peak" },
        ],
      }),
    );
    render(<AdvisorPanel onOpen={vi.fn()} />);
    await userEvent.click(await screen.findByRole("button", { name: "Design it" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("requests per second at peak");
    expect(screen.getByLabelText("Daily users")).toHaveValue(1000000);
  });

  it("will not submit an empty number", async () => {
    render(<AdvisorPanel onOpen={vi.fn()} />);
    await userEvent.clear(await screen.findByLabelText("Daily users"));
    expect(screen.getByRole("button", { name: "Design it" })).toBeDisabled();
  });
});
