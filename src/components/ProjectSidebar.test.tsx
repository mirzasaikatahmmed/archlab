import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ProjectSidebar } from "./ProjectSidebar";

const projects = [
  { id: "1", name: "Alpha", nodeCount: 3, updatedAt: 2 },
  { id: "2", name: "Beta", nodeCount: 5, updatedAt: 1 },
];

const handlers = () => ({ onCreate: vi.fn(), onOpen: vi.fn(), onDelete: vi.fn() });

describe("ProjectSidebar", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("shows an empty state", () => {
    render(<ProjectSidebar projects={[]} currentId={null} busy={false} {...handlers()} />);
    expect(screen.getByText("No projects yet.")).toBeInTheDocument();
  });

  it("creates a trimmed project name and disables blank input", async () => {
    const h = handlers();
    render(<ProjectSidebar projects={[]} currentId={null} busy={false} {...h} />);
    const create = screen.getByRole("button", { name: "Create" });
    expect(create).toBeDisabled();
    await userEvent.type(screen.getByLabelText("New project"), "  My app  ");
    await userEvent.click(create);
    expect(h.onCreate).toHaveBeenCalledWith("My app");
  });

  it("opens a project", async () => {
    const h = handlers();
    render(<ProjectSidebar projects={projects} currentId="1" busy={false} {...h} />);
    await userEvent.click(screen.getByRole("button", { name: "Beta" }));
    expect(h.onOpen).toHaveBeenCalledWith("2");
  });

  it("deletes only after confirmation", async () => {
    const h = handlers();
    const confirm = vi.spyOn(window, "confirm").mockReturnValueOnce(false);
    render(<ProjectSidebar projects={projects} currentId="1" busy={false} {...h} />);
    await userEvent.click(screen.getByRole("button", { name: "Delete Alpha" }));
    expect(h.onDelete).not.toHaveBeenCalled();
    confirm.mockReturnValueOnce(true);
    await userEvent.click(screen.getByRole("button", { name: "Delete Alpha" }));
    expect(h.onDelete).toHaveBeenCalledWith("1");
  });

  it("offers the scenario library only when it is wired up", async () => {
    const { rerender } = render(
      <ProjectSidebar projects={[]} currentId={null} busy={false} {...handlers()} />,
    );
    expect(screen.queryByRole("button", { name: "Scenarios" })).not.toBeInTheDocument();
    const onOpenScenarios = vi.fn();
    rerender(
      <ProjectSidebar
        projects={[]}
        currentId={null}
        busy={false}
        {...handlers()}
        onOpenScenarios={onOpenScenarios}
        scenariosActive
      />,
    );
    const button = screen.getByRole("button", { name: "Scenarios" });
    expect(button).toHaveAttribute("aria-pressed", "true");
    await userEvent.click(button);
    expect(onOpenScenarios).toHaveBeenCalled();
  });

  it("offers project import only when a handler is given", async () => {
    const { rerender } = render(
      <ProjectSidebar projects={[]} currentId={null} busy={false} {...handlers()} />,
    );
    expect(screen.queryByRole("button", { name: "Import project" })).not.toBeInTheDocument();
    const onImport = vi.fn();
    rerender(
      <ProjectSidebar
        projects={[]}
        currentId={null}
        busy={false}
        {...handlers()}
        onImport={onImport}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Import project" }));
    expect(onImport).toHaveBeenCalledTimes(1);
  });
});
