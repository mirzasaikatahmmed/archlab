import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppError, Project } from "../bindings";
import { serverProfile, topology, trafficProfile } from "../test/fixtures";

const api = vi.hoisted(() => ({
  listProjects: vi.fn(),
  listTemplates: vi.fn(),
  getProject: vi.fn(),
  createProject: vi.fn(),
  saveProject: vi.fn(),
  deleteProject: vi.fn(),
}));

vi.mock("../ipc", async () => {
  const actual = await vi.importActual<typeof import("../ipc")>("../ipc");
  return { ...actual, api };
});

import { CommandError } from "../ipc";
import { useProjectStore } from "./projectStore";

const project: Project = {
  id: "p1",
  name: "Demo",
  serverProfile,
  trafficProfile,
  topology,
  createdAt: 1,
  updatedAt: 1,
};

describe("projectStore", () => {
  beforeEach(() => {
    Object.values(api).forEach((m) => {
      m.mockReset();
    });
    useProjectStore.setState({
      projects: [],
      templates: [],
      current: null,
      dirty: false,
      busy: false,
      error: null,
      issues: [],
    });
    api.listProjects.mockResolvedValue([]);
  });

  it("marks the project dirty on edit and clean after save", async () => {
    api.createProject.mockResolvedValue(project);
    api.saveProject.mockImplementation((p: Project) => Promise.resolve({ ...p, updatedAt: 2 }));
    const s = useProjectStore.getState();
    await s.createProject("Demo");
    expect(useProjectStore.getState().dirty).toBe(false);

    useProjectStore.getState().rename("Renamed");
    expect(useProjectStore.getState().dirty).toBe(true);

    await useProjectStore.getState().saveCurrent();
    const state = useProjectStore.getState();
    expect(api.saveProject).toHaveBeenCalledWith(expect.objectContaining({ name: "Renamed" }));
    expect(state.dirty).toBe(false);
    expect(state.current?.updatedAt).toBe(2);
  });

  it("exposes Rust validation issues from a failed save", async () => {
    const error: AppError = {
      kind: "validation",
      issues: [{ field: "serverProfile.cpuCores", message: "must be between 1 and 1024" }],
    };
    api.saveProject.mockRejectedValue(new CommandError(error));
    useProjectStore.setState({ current: project, dirty: true });
    await useProjectStore.getState().saveCurrent();
    const state = useProjectStore.getState();
    expect(state.issues).toEqual(error.issues);
    expect(state.error).toContain("serverProfile.cpuCores");
    expect(state.dirty).toBe(true);
    expect(state.busy).toBe(false);
  });

  it("closes the current project when it is deleted", async () => {
    api.deleteProject.mockResolvedValue(null);
    useProjectStore.setState({ current: project });
    await useProjectStore.getState().deleteProject("p1");
    expect(useProjectStore.getState().current).toBeNull();
  });
});
