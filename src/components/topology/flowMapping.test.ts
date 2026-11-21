import { describe, expect, it } from "vitest";
import type { Topology } from "../../bindings";
import { createNode, edgeId, fromFlow, toFlow } from "./flowMapping";

const topology: Topology = {
  nodes: [
    {
      id: "a",
      kind: "appServer",
      label: "App",
      position: { x: 10, y: 20 },
      template: { id: "node", version: 1 },
      settings: { instances: 8 },
      replicas: 3,
    },
    {
      id: "b",
      kind: "database",
      label: "DB",
      position: { x: 300, y: 20 },
      template: null,
      settings: {},
      replicas: 1,
    },
  ],
  edges: [{ id: "e-a-b", source: "a", target: "b" }],
};

describe("flow mapping", () => {
  it("round-trips topology through React Flow state", () => {
    const { nodes, edges } = toFlow(topology);
    expect(nodes).toHaveLength(2);
    expect(nodes[0]?.type).toBe("arch");
    expect(fromFlow(nodes, edges)).toEqual(topology);
  });

  it("treats a node saved without replicas as a single copy", () => {
    const { id, kind, label, position, template, settings } = topology.nodes[0];
    const legacy = { id, kind, label, position, template, settings };
    const { nodes } = toFlow({ nodes: [legacy], edges: [] });
    expect(nodes[0]?.data.replicas).toBe(1);
  });

  it("creates uniquely labelled nodes at distinct positions", () => {
    const { nodes } = toFlow(topology);
    const added = createNode("appServer", nodes, "n3");
    expect(added.data.label).toBe("App server 2");
    expect(added.data.replicas).toBe(1);
    expect(
      nodes.some((n) => n.position.x === added.position.x && n.position.y === added.position.y),
    ).toBe(false);
  });

  it("builds deterministic edge ids", () => {
    expect(edgeId("a", "b")).toBe("e-a-b");
  });
});
