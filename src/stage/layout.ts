import type { NodeKind, Topology } from "../bindings";
import { project, type Pt } from "./iso";

export interface SceneNode {
  id: string;
  label: string;
  kind: NodeKind;
  /** Position on the isometric ground plane. */
  gx: number;
  gy: number;
}

/** Height of the tallest drawing (the capacity ceiling), in ground units. */
export const CEILING_HEIGHT = 96;
/** Half-width of a node's footprint, including its capacity volume. */
export const TILE = 38;

/** Places nodes on the ground exactly where they were drawn on the topology canvas. */
export function sceneNodes(topology: Topology): SceneNode[] {
  const xs = topology.nodes.map((n) => n.position.x ?? 0);
  const ys = topology.nodes.map((n) => n.position.y ?? 0);
  const minX = Math.min(0, ...xs);
  const minY = Math.min(0, ...ys);
  return topology.nodes.map((n) => ({
    id: n.id,
    label: n.label,
    kind: n.kind,
    gx: ((n.position.x ?? 0) - minX) * 1.1,
    gy: ((n.position.y ?? 0) - minY) * 1.1,
  }));
}

/** Screen-space points that must stay inside the canvas: footprints, ceilings and labels. */
export function sceneExtent(nodes: SceneNode[]): Pt[] {
  return nodes.flatMap((n) => [
    project(n.gx - TILE, n.gy - TILE, CEILING_HEIGHT),
    project(n.gx + TILE, n.gy - TILE, CEILING_HEIGHT),
    project(n.gx - TILE, n.gy + TILE, 0),
    project(n.gx + TILE, n.gy + TILE, 0),
  ]);
}
