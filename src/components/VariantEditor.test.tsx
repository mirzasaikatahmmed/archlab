import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { NodeKind, Topology, WhatIf } from "../bindings";
import { templates, whatIfPlan } from "../test/fixtures";
import { VariantEditor } from "./VariantEditor";

const node = (id: string, kind: NodeKind, label: string, template: string | null = null) => ({
  id,
  kind,
  label,
  position: { x: 0, y: 0 },
  template: template ? { id: template, version: 1 } : null,
  settings: {},
  replicas: 1,
});

const topology: Topology = {
  nodes: [
    node("c", "client", "Clients"),
    node("app", "appServer", "App", "node"),
    node("db", "database", "DB"),
  ],
  edges: [],
};

const withVariant: WhatIf = {
  ...whatIfPlan,
  variants: [
    {
      id: "v1",
      name: "Bigger app",
      changes: [{ kind: "replicas", node: "app", count: 2 }],
    },
  ],
};

describe("VariantEditor", () => {
  it("adds a variant that doubles traffic", async () => {
    const onChange = vi.fn<(w: WhatIf) => void>();
    render(
      <VariantEditor
        value={whatIfPlan}
        topology={topology}
        templates={templates}
        onChange={onChange}
      />,
    );
    expect(screen.getByText(/No variants yet/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Add variant" }));
    const sent = onChange.mock.lastCall?.[0];
    expect(sent?.variants).toHaveLength(1);
    expect(sent?.variants[0]).toMatchObject({
      name: "Variant 1",
      changes: [{ kind: "trafficScale", factor: 2 }],
    });
    expect(screen.getByLabelText("Multiply traffic by")).toHaveValue(2);
  });

  it("adds a replicas change on the first server node, never the client", async () => {
    const onChange = vi.fn<(w: WhatIf) => void>();
    render(
      <VariantEditor
        value={withVariant}
        topology={topology}
        templates={templates}
        onChange={onChange}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Add change" }));
    const changes = onChange.mock.lastCall?.[0].variants[0]?.changes;
    expect(changes?.[1]).toMatchObject({ kind: "replicas", node: "app", count: 2 });
    const options = screen.getAllByRole("option").map((o) => o.textContent);
    expect(options).not.toContain("Clients");
  });

  it("offers the template's settings for a node setting change", async () => {
    const onChange = vi.fn<(w: WhatIf) => void>();
    render(
      <VariantEditor
        value={withVariant}
        topology={topology}
        templates={templates}
        onChange={onChange}
      />,
    );
    await userEvent.selectOptions(screen.getByLabelText("Change"), "nodeSetting");
    expect(screen.getByRole("option", { name: "Instances" })).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Setting"), "instances");
    const change = onChange.mock.lastCall?.[0].variants[0]?.changes[0];
    expect(change).toMatchObject({ kind: "nodeSetting", node: "app", key: "instances" });
  });

  it("explains when no node has a template to change", async () => {
    const bare: Topology = {
      ...topology,
      nodes: topology.nodes.map((n) => ({ ...n, template: null })),
    };
    render(
      <VariantEditor
        value={withVariant}
        topology={bare}
        templates={templates}
        onChange={vi.fn()}
      />,
    );
    await userEvent.selectOptions(screen.getByLabelText("Change"), "nodeSetting");
    expect(screen.getByText(/Pick a template for a node/)).toBeInTheDocument();
  });

  it("flags an out-of-range value and does not pass it on", async () => {
    const onChange = vi.fn<(w: WhatIf) => void>();
    const scale: WhatIf = {
      ...whatIfPlan,
      variants: [{ id: "v", name: "Traffic", changes: [{ kind: "trafficScale", factor: 2 }] }],
    };
    render(
      <VariantEditor value={scale} topology={topology} templates={templates} onChange={onChange} />,
    );
    const factor = screen.getByLabelText("Multiply traffic by");
    await userEvent.clear(factor);
    await userEvent.type(factor, "500");
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(onChange.mock.calls.some(([w]) => JSON.stringify(w).includes("500"))).toBe(false);
  });

  it("requires a name and removes variants and changes", async () => {
    const onChange = vi.fn<(w: WhatIf) => void>();
    render(
      <VariantEditor
        value={withVariant}
        topology={topology}
        templates={templates}
        onChange={onChange}
      />,
    );
    await userEvent.clear(screen.getByLabelText("Name"));
    expect(await screen.findByText("Give the variant a name")).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Name"), "Renamed");
    expect(onChange.mock.lastCall?.[0].variants[0]?.name).toBe("Renamed");
    await userEvent.click(screen.getByRole("button", { name: "Remove change 1 of variant 1" }));
    expect(onChange.mock.lastCall?.[0].variants[0]?.changes).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: "Remove variant 1" }));
    expect(onChange.mock.lastCall?.[0].variants).toEqual([]);
  });
});
