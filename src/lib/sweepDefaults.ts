import type { ConfigTemplate, Project } from "../bindings";
import limits from "../schemas/limits.json";
import type { ServerFieldName } from "../schemas/whatif";

export type SweepKind = "trafficScale" | "server" | "nodeSetting" | "replicas";

export interface SweepChoice {
  kind: SweepKind;
  field: ServerFieldName;
  node: string;
  key: string;
}

const WHOLE = new Set<ServerFieldName>(["cpuCores", "diskIops", "fdLimit"]);

const FIELD_PATHS: Record<ServerFieldName, string> = {
  cpuCores: "cpuCores",
  cpuGhz: "cpuGhz",
  ramGb: "ramGb",
  diskIops: "disk.iops",
  networkBandwidthMbps: "network.bandwidthMbps",
  networkLatencyMs: "network.latencyMs",
  fdLimit: "fdLimit",
};

function serverValue(project: Project, field: ServerFieldName): number {
  const s = project.serverProfile;
  switch (field) {
    case "cpuCores":
      return s.cpuCores;
    case "cpuGhz":
      return s.cpuGhz ?? 1;
    case "ramGb":
      return s.ramGb ?? 1;
    case "diskIops":
      return s.disk.iops;
    case "networkBandwidthMbps":
      return s.network.bandwidthMbps ?? 1;
    case "networkLatencyMs":
      return s.network.latencyMs ?? 1;
    case "fdLimit":
      return s.fdLimit;
  }
}

const clamp = (v: number, min: number, max: number) => Math.min(max, Math.max(min, v));

/** A sensible first range: half to four times the current value, kept inside the allowed range. */
export function defaultRange(
  choice: SweepChoice,
  project: Project,
  templates: ConfigTemplate[],
): { from: number; to: number } {
  switch (choice.kind) {
    case "trafficScale":
      return { from: 0.5, to: 4 };
    case "replicas":
      return { from: 1, to: 8 };
    case "server": {
      const row = limits.serverProfile.find((r) => r.path === FIELD_PATHS[choice.field]);
      const current = serverValue(project, choice.field);
      let from = clamp(current * 0.5, row?.min ?? 0, row?.max ?? Infinity);
      let to = clamp(current * 4, row?.min ?? 0, row?.max ?? Infinity);
      if (WHOLE.has(choice.field)) {
        from = Math.max(1, Math.round(from));
        to = Math.round(to);
      }
      return from === to ? { from, to: from * 2 } : { from, to };
    }
    case "nodeSetting": {
      const node = project.topology.nodes.find((n) => n.id === choice.node);
      const setting = templates
        .find((t) => t.id === node?.template?.id)
        ?.settings.find((s) => s.key === choice.key);
      if (!node || !setting) {
        return { from: 1, to: 2 };
      }
      const min = setting.min ?? 0;
      const max = setting.max ?? Infinity;
      const current = node.settings[choice.key] ?? setting.default ?? 1;
      const from = clamp(current * 0.5, min, max);
      const to = clamp(current * 4, min, max);
      return from === to ? { from, to: clamp(from * 2, min, max) } : { from, to };
    }
  }
}
