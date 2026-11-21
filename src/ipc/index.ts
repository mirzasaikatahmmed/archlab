import { Channel } from "@tauri-apps/api/core";
import {
  commands,
  type AppError,
  type AppInfo,
  type Calibration,
  type CalibrationEvent,
  type ConfigTemplate,
  type DetectedProfile,
  type FieldIssue,
  type LiveCommand,
  type LiveEvent,
  type Project,
  type ProjectSummary,
  type RealEvent,
  type ServerProfile,
  type SimOptions,
  type SimReport,
} from "../bindings";

export type { AppError, FieldIssue, LiveCommand, LiveEvent, CalibrationEvent, RealEvent };

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
    case "runtime":
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
  /** Starts a live run; `onEvent` receives one tick per simulated second. `speed` null = max. */
  startLiveRun: async (
    project: Project,
    options: SimOptions,
    speed: number | null,
    onEvent: (event: LiveEvent) => void,
  ): Promise<void> => {
    const channel = new Channel<LiveEvent>();
    channel.onmessage = onEvent;
    await unwrap(commands.startLiveRun(project, options, speed, channel));
  },
  controlLiveRun: (command: LiveCommand): Promise<null> => unwrap(commands.controlLiveRun(command)),
  /** Starts the machine benchmarks; `onEvent` reports each step and the result. */
  startCalibration: async (onEvent: (event: CalibrationEvent) => void): Promise<void> => {
    const channel = new Channel<CalibrationEvent>();
    channel.onmessage = onEvent;
    await unwrap(commands.startCalibration(channel));
  },
  cancelCalibration: (): Promise<null> => unwrap(commands.cancelCalibration()),
  latestCalibration: (): Promise<Calibration | null> => unwrap(commands.latestCalibration()),
  applyCalibration: (profile: ServerProfile): Promise<ServerProfile> =>
    unwrap(commands.applyCalibration(profile)),
  /** Runs the project on real localhost sockets; `onEvent` gets progress, then the comparison. */
  startRealRun: async (
    project: Project,
    options: SimOptions,
    onEvent: (event: RealEvent) => void,
  ): Promise<void> => {
    const channel = new Channel<RealEvent>();
    channel.onmessage = onEvent;
    await unwrap(commands.startRealRun(project, options, channel));
  },
  cancelRealRun: (): Promise<null> => unwrap(commands.cancelRealRun()),
};
