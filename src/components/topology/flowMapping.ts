import type { Edge, Node } from "@xyflow/react";
import { stripNulls } from "../../lib/numbers";
import type { NodeKind, TemplateRef, Topology } from "../../bindings";

export interface ArchNodeData extends Record<string, unknown> {
  kind: NodeKind;
  label: string;
  template: TemplateRef | null;
  settings: Record<string, number>;
  replicas: number;
}

export type ArchNode = Node<ArchNodeData, "arch">;

export const NODE_KINDS: { kind: NodeKind; label: string }[] = [
  { kind: "client", label: "Client" },
  { kind: "loadBalancer", label: "Load balancer" },
  { kind: "appServer", label: "App server" },
  { kind: "database", label: "Database" },
  { kind: "cache", label: "Cache" },
  { kind: "queue", label: "Queue" },
  { kind: "apiGateway", label: "API gateway" },
  { kind: "cdn", label: "CDN" },
  { kind: "worker", label: "Worker" },
  { kind: "objectStorage", label: "Object storage" },
  { kind: "search", label: "Search" },
];

export function kindLabel(kind: NodeKind): string {
  return NODE_KINDS.find((k) => k.kind === kind)?.label ?? kind;
}

export function toFlow(topology: Topology): { nodes: ArchNode[]; edges: Edge[] } {
  return {
    nodes: topology.nodes.map((n) => ({
      id: n.id,
      type: "arch",
      position: stripNulls({ x: n.position.x, y: n.position.y }),
      data: {
        kind: n.kind,
        label: n.label,
        template: n.template,
        replicas: n.replicas ?? 1,
        settings: Object.fromEntries(
          Object.entries(n.settings).map(([k, v]) => [k, v ?? Number.NaN]),
        ),
      },
    })),
    edges: topology.edges.map((e) => ({ id: e.id, source: e.source, target: e.target })),
  };
}

export function fromFlow(nodes: ArchNode[], edges: Edge[]): Topology {
  return {
    nodes: nodes.map((n) => ({
      id: n.id,
      kind: n.data.kind,
      label: n.data.label,
      position: { x: n.position.x, y: n.position.y },
      template: n.data.template,
      settings: n.data.settings,
      replicas: n.data.replicas,
    })),
    edges: edges.map((e) => ({ id: e.id, source: e.source, target: e.target })),
  };
}

export function edgeId(source: string, target: string): string {
  return `e-${source}-${target}`;
}

export function createNode(kind: NodeKind, existing: ArchNode[], id: string): ArchNode {
  const count = existing.filter((n) => n.data.kind === kind).length + 1;
  const column = existing.length % 5;
  const row = Math.floor(existing.length / 5);
  return {
    id,
    type: "arch",
    position: { x: 40 + column * 220, y: 40 + row * 140 },
    data: {
      kind,
      label: `${kindLabel(kind)} ${String(count)}`,
      template: null,
      settings: {},
      replicas: 1,
    },
  };
}
