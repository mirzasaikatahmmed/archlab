import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LiveTick, Project, SimReport } from "../bindings";
import { chaosPlan, serverProfile, topology, trafficProfile, whatIfPlan } from "../test/fixtures";

const api = vi.hoisted(() => ({
  videoAvailable: vi.fn(),
  pickSavePath: vi.fn(),
  exportProjectJson: vi.fn(),
  exportMarkdown: vi.fn(),
  exportSvg: vi.fn(),
}));

vi.mock("../ipc", async (importActual) => {
  const actual = await importActual<typeof import("../ipc")>();
  return { ...actual, api };
});

import { CommandError } from "../ipc";
import { ExportBar } from "./ExportBar";

const project: Project = {
  id: "p1",
  name: "Shop Backend",
  serverProfile,
  trafficProfile,
  topology,
  chaos: chaosPlan,
  whatif: whatIfPlan,
  createdAt: 0,
  updatedAt: 0,
};

const report = { notice: "Estimate." } as SimReport;
const tick = { point: { tS: 1 } } as LiveTick;

describe("ExportBar", () => {
  beforeEach(() => {
    Object.values(api).forEach((m) => {
      m.mockReset();
    });
    api.videoAvailable.mockResolvedValue(true);
  });

  it("saves the project file with the report under a safe default name", async () => {
    api.pickSavePath.mockResolvedValue("/home/me/shop.archlab.json");
    api.exportProjectJson.mockResolvedValue(null);
    render(<ExportBar project={project} report={report} />);
    await userEvent.click(screen.getByRole("button", { name: "Save project file" }));
    expect(api.pickSavePath).toHaveBeenCalledWith("json", "shop-backend.archlab.json");
    expect(api.exportProjectJson).toHaveBeenCalledWith(
      "/home/me/shop.archlab.json",
      project,
      report,
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Saved to /home/me/shop.archlab.json",
    );
  });

  it("writes nothing when the Save dialog is cancelled", async () => {
    api.pickSavePath.mockResolvedValue(null);
    render(<ExportBar project={project} report={null} />);
    await userEvent.click(screen.getByRole("button", { name: "Save report (Markdown)" }));
    expect(api.exportMarkdown).not.toHaveBeenCalled();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.getByText(/Run a simulation first/)).toBeInTheDocument();
  });

  it("shows the engine's message when saving fails", async () => {
    api.pickSavePath.mockResolvedValue("/ro/diagram.svg");
    api.exportSvg.mockRejectedValue(
      new CommandError({ kind: "runtime", message: "could not write /ro/diagram.svg" }),
    );
    render(<ExportBar project={project} report={report} />);
    await userEvent.click(screen.getByRole("button", { name: "Save diagram (SVG)" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("could not write /ro/diagram.svg");
  });

  it("explains why video is unavailable", async () => {
    const { rerender } = render(<ExportBar project={project} report={report} />);
    expect(screen.getByRole("button", { name: "Save video (WebM)" })).toBeDisabled();
    expect(screen.getByText(/Finish a live run/)).toBeInTheDocument();

    api.videoAvailable.mockResolvedValue(false);
    rerender(<ExportBar key="again" project={project} report={report} ticks={[tick]} />);
    expect(await screen.findByText(/Video needs ffmpeg/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save video (WebM)" })).toBeDisabled();
  });

  it("enables video after a finished run when ffmpeg is present", async () => {
    render(<ExportBar project={project} report={report} ticks={[tick]} />);
    expect(await screen.findByRole("button", { name: "Save video (WebM)" })).toBeEnabled();
  });
});
