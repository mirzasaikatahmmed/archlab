import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ScenarioSummary } from "../bindings";

const api = vi.hoisted(() => ({ listScenarios: vi.fn() }));
vi.mock("../ipc", async (importActual) => {
  const actual = await importActual<typeof import("../ipc")>();
  return { ...actual, api };
});

import { ScenarioLibrary } from "./ScenarioLibrary";

const scenarios: ScenarioSummary[] = [
  {
    id: "one-machine",
    title: "Everything on one machine",
    summary: "Follow a product from one server to a cluster.",
    level: "beginner",
    minutes: 5,
    stepCount: 7,
  },
  {
    id: "retry-storm",
    title: "A retry storm",
    summary: "Retries can make an outage worse.",
    level: "intermediate",
    minutes: 4,
    stepCount: 4,
  },
];

describe("ScenarioLibrary", () => {
  beforeEach(() => {
    api.listScenarios.mockReset();
  });

  it("lists each scenario with its level and length", async () => {
    api.listScenarios.mockResolvedValue(scenarios);
    render(<ScenarioLibrary onPlay={vi.fn()} />);
    expect(await screen.findByText("Everything on one machine")).toBeInTheDocument();
    expect(screen.getByText("Beginner")).toBeInTheDocument();
    expect(screen.getByText("Intermediate")).toBeInTheDocument();
    expect(screen.getByText("7 steps · about 5 min")).toBeInTheDocument();
  });

  it("plays the chosen scenario", async () => {
    api.listScenarios.mockResolvedValue(scenarios);
    const onPlay = vi.fn();
    render(<ScenarioLibrary onPlay={onPlay} />);
    await userEvent.click(await screen.findByRole("button", { name: "Play A retry storm" }));
    expect(onPlay).toHaveBeenCalledWith("retry-storm");
  });

  it("shows a message when the list cannot be loaded", async () => {
    api.listScenarios.mockRejectedValue(new Error("boom"));
    render(<ScenarioLibrary onPlay={vi.fn()} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load scenarios");
  });
});
