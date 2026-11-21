import type {
  ChaosPlan,
  ConfigTemplate,
  ServerProfile,
  Topology,
  WhatIf,
  TrafficProfile,
} from "../bindings";

export const serverProfile: ServerProfile = {
  os: "linux",
  cpuCores: 4,
  cpuGhz: 2.5,
  ramGb: 8,
  disk: { kind: "ssd", iops: 20000 },
  network: { bandwidthMbps: 1000, latencyMs: 0.5 },
  virtualization: "vm",
  fdLimit: 65536,
};

export const chaosPlan: ChaosPlan = {
  faults: [],
  policy: { timeoutMs: 10_000, maxRetries: 0, backoffMs: 100 },
};

export const whatIfPlan: WhatIf = {
  slo: { p95Ms: 200, maxErrorRate: 0.01 },
  variants: [],
};

export const trafficProfile: TrafficProfile = {
  concurrentUsers: 1000,
  requestsPerSecond: 200,
  readRatio: 0.8,
  spike: { kind: "none" },
};

export const topology: Topology = {
  nodes: [
    {
      id: "app",
      kind: "appServer",
      label: "App server",
      position: { x: 0, y: 0 },
      template: null,
      settings: {},
      replicas: 1,
    },
  ],
  edges: [],
};

export const templates: ConfigTemplate[] = [
  {
    schemaVersion: 1,
    id: "node",
    version: 1,
    name: "Node.js",
    description: "Node app server",
    nodeKinds: ["appServer"],
    settings: [
      {
        key: "instances",
        label: "Instances",
        unit: "",
        default: 4,
        min: 1,
        max: 1024,
        description: "Processes",
      },
    ],
  },
  {
    schemaVersion: 1,
    id: "postgres",
    version: 1,
    name: "PostgreSQL",
    description: "Database",
    nodeKinds: ["database"],
    settings: [],
  },
];
