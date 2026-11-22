import { create } from "zustand";
import type {
  ChaosPlan,
  ConfigTemplate,
  Project,
  ProjectSummary,
  ServerProfile,
  Topology,
  TrafficProfile,
  WhatIf,
} from "../bindings";
import { CommandError, api, type FieldIssue } from "../ipc";

interface ProjectState {
  projects: ProjectSummary[];
  templates: ConfigTemplate[];
  current: Project | null;
  dirty: boolean;
  busy: boolean;
  error: string | null;
  issues: FieldIssue[];

  init: () => Promise<void>;
  createProject: (name: string) => Promise<void>;
  openProject: (id: string) => Promise<void>;
  saveCurrent: () => Promise<void>;
  deleteProject: (id: string) => Promise<void>;
  /** Saves a project that was built elsewhere, such as a scenario step, and opens it. */
  importProject: (project: Project) => Promise<void>;
  /** Asks for an `.archlab.json` file and opens it as a new project; false if cancelled or failed. */
  importFile: () => Promise<boolean>;
  closeProject: () => void;
  rename: (name: string) => void;
  setServerProfile: (profile: ServerProfile) => void;
  setTrafficProfile: (profile: TrafficProfile) => void;
  setTopology: (topology: Topology) => void;
  setChaos: (chaos: ChaosPlan) => void;
  setWhatIf: (whatif: WhatIf) => void;
}

function failure(err: unknown): { error: string; issues: FieldIssue[] } {
  if (err instanceof CommandError) {
    return { error: err.message, issues: err.issues };
  }
  return { error: err instanceof Error ? err.message : String(err), issues: [] };
}

export const useProjectStore = create<ProjectState>()((set, get) => {
  const run = async (action: () => Promise<void>): Promise<void> => {
    set({ busy: true, error: null, issues: [] });
    try {
      await action();
    } catch (err) {
      set(failure(err));
    } finally {
      set({ busy: false });
    }
  };

  const edit = (patch: Partial<Project>): void => {
    const { current } = get();
    if (current) {
      set({ current: { ...current, ...patch }, dirty: true });
    }
  };

  return {
    projects: [],
    templates: [],
    current: null,
    dirty: false,
    busy: false,
    error: null,
    issues: [],

    init: () =>
      run(async () => {
        const [projects, templates] = await Promise.all([api.listProjects(), api.listTemplates()]);
        set({ projects, templates });
      }),

    createProject: (name) =>
      run(async () => {
        const project = await api.createProject(name);
        set({ current: project, dirty: false, projects: await api.listProjects() });
      }),

    importProject: (project) =>
      run(async () => {
        const saved = await api.saveProject(project);
        set({ current: saved, dirty: false, projects: await api.listProjects() });
      }),

    importFile: async () => {
      let opened = false;
      await run(async () => {
        const path = await api.pickOpenPath();
        if (path === null) {
          return;
        }
        const saved = await api.saveProject(await api.importProjectFile(path));
        set({ current: saved, dirty: false, projects: await api.listProjects() });
        opened = true;
      });
      return opened;
    },

    openProject: (id) =>
      run(async () => {
        set({ current: await api.getProject(id), dirty: false });
      }),

    saveCurrent: () =>
      run(async () => {
        const { current } = get();
        if (!current) {
          return;
        }
        const saved = await api.saveProject(current);
        set({ current: saved, dirty: false, projects: await api.listProjects() });
      }),

    deleteProject: (id) =>
      run(async () => {
        await api.deleteProject(id);
        const projects = await api.listProjects();
        set({
          projects,
          ...(get().current?.id === id ? { current: null, dirty: false } : {}),
        });
      }),

    closeProject: () => {
      set({ current: null, dirty: false, error: null, issues: [] });
    },

    rename: (name) => {
      edit({ name });
    },
    setServerProfile: (serverProfile) => {
      edit({ serverProfile });
    },
    setTrafficProfile: (trafficProfile) => {
      edit({ trafficProfile });
    },
    /** Faults and what-if changes aimed at a node or edge that was removed are dropped with it. */
    setTopology: (topology) => {
      const ids = new Set([...topology.nodes.map((n) => n.id), ...topology.edges.map((e) => e.id)]);
      const { current } = get();
      const patch: Partial<Project> = { topology };
      if (current) {
        const faults = current.chaos.faults.filter((f) => ids.has(f.target));
        if (faults.length !== current.chaos.faults.length) {
          patch.chaos = { ...current.chaos, faults };
        }
        const variants = current.whatif.variants.map((v) => ({
          ...v,
          changes: v.changes.filter(
            (c) => !(c.kind === "nodeSetting" || c.kind === "replicas") || ids.has(c.node),
          ),
        }));
        if (
          variants.some((v, i) => v.changes.length !== current.whatif.variants[i].changes.length)
        ) {
          patch.whatif = { ...current.whatif, variants };
        }
      }
      edit(patch);
    },
    setChaos: (chaos) => {
      edit({ chaos });
    },
    setWhatIf: (whatif) => {
      edit({ whatif });
    },
  };
});
