import {
  commands,
  type AppError,
  type AppInfo,
  type ConfigTemplate,
  type DetectedProfile,
  type FieldIssue,
  type Project,
  type ProjectSummary,
  type SimOptions,
  type SimReport,
} from "../bindings";

export type { AppError, FieldIssue };

/** Thrown when a Rust command returns a typed error. */
export class CommandError extends Error {
  readonly error: AppError;

  constructor(error: AppError) {
    super(describeError(error));
    this.name = "CommandError";
    this.error = error;
  }

  get issues(): FieldIssue[] {
    return this.error.kind === "validation" ? this.error.issues : [];
  }
}

export function describeError(error: AppError): string {
  switch (error.kind) {
    case "validation":
      return error.issues.map((i) => `${i.field}: ${i.message}`).join("; ");
    case "notFound":
      return `Not found: ${error.id}`;
    case "storage":
    case "simulation":
      return error.message;
  }
}

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: AppError };

export async function unwrap<T>(result: Promise<Result<T>>): Promise<T> {
  const r = await result;
  if (r.status === "error") {
    throw new CommandError(r.error);
  }
  return r.data;
}

/** Typed facade over the generated bindings; every method throws CommandError on failure. */
export const api = {
  appInfo: (): Promise<AppInfo> => commands.appInfo(),
  detectMachine: (): Promise<DetectedProfile> => commands.detectMachine(),
  listTemplates: (): Promise<ConfigTemplate[]> => commands.listTemplates(),
  listProjects: (): Promise<ProjectSummary[]> => unwrap(commands.listProjects()),
  getProject: (id: string): Promise<Project> => unwrap(commands.getProject(id)),
  createProject: (name: string): Promise<Project> => unwrap(commands.createProject(name)),
  saveProject: (project: Project): Promise<Project> => unwrap(commands.saveProject(project)),
  deleteProject: (id: string): Promise<null> => unwrap(commands.deleteProject(id)),
  runSimulation: (project: Project, options: SimOptions): Promise<SimReport> =>
    unwrap(commands.runSimulation(project, options)),
};
