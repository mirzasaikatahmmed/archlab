import { Channel, invoke } from "@tauri-apps/api/core";
import { check } from "@tauri-apps/plugin-updater";
import {
  commands,
  type AppError,
  type AppInfo,
  type Calibration,
  type ControlRecord,
  type CalibrationEvent,
  type ConfigTemplate,
  type ControlCommand,
  type ExportKind,
  type DetectedProfile,
  type FieldIssue,
  type LiveCommand,
  type LiveEvent,
  type Project,
  type ProjectSummary,
  type RealEvent,
  type ScenarioSummary,
  type ScenarioView,
  type ServerProfile,
  type SimOptions,
  type SimReport,
  type WhatIfEvent,
  type WhatIfRequest,
} from "../bindings";

export type {
  AppError,
  ControlCommand,
  ControlRecord,
  ExportKind,
  FieldIssue,
  LiveCommand,
  LiveEvent,
  CalibrationEvent,
  RealEvent,
  WhatIfEvent,
};

/** A newer version found by the updater. */
export interface AvailableUpdate {
  version: string;
  /** Downloads, verifies the signature and installs; the app must be restarted afterwards. */
  install: () => Promise<void>;
}

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

function isAppError(value: unknown): value is AppError {
  return typeof value === "object" && value !== null && "kind" in value;
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
    /** Logged commands to re-apply at their seconds, to replay a steered run. */
    script: ControlRecord[] = [],
  ): Promise<void> => {
    const channel = new Channel<LiveEvent>();
    channel.onmessage = onEvent;
    await unwrap(commands.startLiveRun(project, options, speed, script, channel));
  },
  /** Pause, resume, stop, change speed or steer; a steering command returns its log entry. */
  controlLiveRun: (command: LiveCommand): Promise<ControlRecord | null> =>
    unwrap(commands.controlLiveRun(command)),
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
  /** Runs a what-if analysis in the background; `onEvent` gets progress, then the result. */
  startWhatIf: async (
    project: Project,
    options: SimOptions,
    request: WhatIfRequest,
    onEvent: (event: WhatIfEvent) => void,
  ): Promise<void> => {
    const channel = new Channel<WhatIfEvent>();
    channel.onmessage = onEvent;
    await unwrap(commands.startWhatIf(project, options, request, channel));
  },
  cancelWhatIf: (): Promise<null> => unwrap(commands.cancelWhatIf()),
  listScenarios: (): Promise<ScenarioSummary[]> => commands.listScenarios(),
  getScenario: (id: string): Promise<ScenarioView> => unwrap(commands.getScenario(id)),
  /** The design after `step` (counting from 0) of a scenario, as a new unsaved project. */
  scenarioStepProject: (id: string, step: number): Promise<Project> =>
    unwrap(commands.scenarioStepProject(id, step)),
  /** Opens a Save dialog; null when cancelled. */
  pickSavePath: (kind: ExportKind, fileName: string): Promise<string | null> =>
    unwrap(commands.pickSavePath(kind, fileName)),
  /** Opens a dialog for a project file; null when cancelled. */
  pickOpenPath: (): Promise<string | null> => unwrap(commands.pickOpenPath()),
  exportProjectJson: (path: string, project: Project, report: SimReport | null): Promise<null> =>
    unwrap(commands.exportProjectJson(path, project, report)),
  exportMarkdown: (path: string, project: Project, report: SimReport | null): Promise<null> =>
    unwrap(commands.exportMarkdown(path, project, report)),
  exportSvg: (path: string, project: Project, report: SimReport | null): Promise<null> =>
    unwrap(commands.exportSvg(path, project, report)),
  /** Reads a project file as a new, unsaved project with a fresh id. */
  importProjectFile: (path: string): Promise<Project> => unwrap(commands.importProjectFile(path)),
  videoAvailable: (): Promise<boolean> => commands.videoAvailable(),
  videoStart: (path: string, width: number, height: number, fps: number): Promise<null> =>
    unwrap(commands.videoStart(path, width, height, fps)),
  /** Sends one raw RGBA frame. Not in the generated bindings: a raw body has no typed signature. */
  videoFrame: async (rgba: Uint8Array): Promise<void> => {
    try {
      await invoke("video_frame", rgba);
    } catch (err) {
      throw isAppError(err) ? new CommandError(err) : err;
    }
  },
  /** Asks the update server for a newer version; null when this one is current. */
  checkForUpdate: async (): Promise<AvailableUpdate | null> => {
    const update = await check();
    return update ? { version: update.version, install: () => update.downloadAndInstall() } : null;
  },
  videoFinish: (): Promise<number> => unwrap(commands.videoFinish()),
  videoCancel: (): Promise<null> => unwrap(commands.videoCancel()),
};
