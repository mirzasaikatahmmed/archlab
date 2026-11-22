import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Topology } from "../../bindings";
import { templates, topology } from "../../test/fixtures";
import { TopologyCanvas } from "./TopologyCanvas";

describe("TopologyCanvas", () => {
  it("offers every node kind in the palette", () => {
    render(<TopologyCanvas topology={topology} templates={templates} onChange={vi.fn()} />);
    const toolbar = screen.getByRole("toolbar", { name: "Add node" });
    expect(toolbar.querySelectorAll("button")).toHaveLength(11);
    for (const label of [
      "Client",
      "Load balancer",
      "App server",
      "Database",
      "Cache",
      "Queue",
      "API gateway",
      "CDN",
      "Worker",
      "Object storage",
      "Search",
    ]) {
      expect(screen.getByRole("button", { name: `+ ${label}` })).toBeInTheDocument();
    }
  });

  it("does not emit changes on mount", () => {
    const onChange = vi.fn();
    render(<TopologyCanvas topology={topology} templates={templates} onChange={onChange} />);
    expect(onChange).not.toHaveBeenCalled();
  });

  it("adds a node of the chosen kind and reports the new topology", async () => {
    const onChange = vi.fn<(t: Topology) => void>();
    render(<TopologyCanvas topology={topology} templates={templates} onChange={onChange} />);
    await userEvent.click(screen.getByRole("button", { name: "+ Database" }));
    const next = onChange.mock.lastCall?.[0];
    expect(next?.nodes).toHaveLength(2);
    expect(next?.nodes[1]).toMatchObject({ kind: "database", label: "Database 1", template: null });
  });
});
