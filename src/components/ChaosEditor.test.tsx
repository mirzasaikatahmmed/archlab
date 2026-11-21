import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ChaosPlan, NodeKind, Topology } from "../bindings";
import { chaosPlan } from "../test/fixtures";
import { ChaosEditor } from "./ChaosEditor";

const node = (id: string, kind: NodeKind, label: string) => ({
  id,
  kind,
  label,
  position: { x: 0, y: 0 },
  template: null,
  settings: {},
});

const topology: Topology = {
  nodes: [
    node("c", "client", "Clients"),
    node("app", "appServer", "App"),
    node("db", "database", "DB"),
  ],
  edges: [
    { id: "e1", source: "c", target: "app" },
    { id: "e2", source: "app", target: "db" },
  ],
};

const withFault: ChaosPlan = {
  ...chaosPlan,
  faults: [
    { id: "f1", target: "db", kind: { kind: "slowdown", factor: 3 }, startS: 20, durationS: 10 },
  ],
};

describe("ChaosEditor", () => {
  it("adds a crash on the first server node", async () => {
    const onChange = vi.fn<(p: ChaosPlan) => void>();
    render(<ChaosEditor value={chaosPlan} topology={topology} onChange={onChange} />);
    expect(screen.getByText(/No faults yet/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Add fault" }));
    const plan = onChange.mock.lastCall?.[0];
    expect(plan?.faults).toHaveLength(1);
    expect(plan?.faults[0]).toMatchObject({ target: "app", kind: { kind: "crash" }, startS: 20 });
    expect(screen.getByLabelText("Node")).toHaveValue("app");
  });

  it("never offers the client as a target for node faults", () => {
    render(<ChaosEditor value={withFault} topology={topology} onChange={vi.fn()} />);
    const options = screen.getAllByRole("option").map((o) => o.textContent);
    expect(options).not.toContain("Clients");
  });

  it("switches to connections when an edge fault type is chosen", async () => {
    const onChange = vi.fn<(p: ChaosPlan) => void>();
    render(<ChaosEditor value={withFault} topology={topology} onChange={onChange} />);
    await userEvent.selectOptions(screen.getByLabelText("Fault type"), "edgeLoss");
    expect(screen.getByLabelText("Connection")).toHaveValue("e1");
    expect(screen.getByRole("option", { name: "Clients → App" })).toBeInTheDocument();
    expect(onChange.mock.lastCall?.[0].faults[0]).toMatchObject({
      target: "e1",
      kind: { kind: "edgeLoss", loss: 0.2 },
    });
  });

  it("flags an out-of-range value and does not pass it on", async () => {
    const onChange = vi.fn<(p: ChaosPlan) => void>();
    render(<ChaosEditor value={withFault} topology={topology} onChange={onChange} />);
    const factor = screen.getByLabelText("Slower by (×)");
    await userEvent.clear(factor);
    await userEvent.type(factor, "500");
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    const sent = onChange.mock.calls.map(([plan]) => JSON.stringify(plan.faults[0]?.kind));
    expect(sent.some((kind) => kind.includes("500"))).toBe(false);
  });

  it("removes a fault", async () => {
    const onChange = vi.fn<(p: ChaosPlan) => void>();
    render(<ChaosEditor value={withFault} topology={topology} onChange={onChange} />);
    await userEvent.click(screen.getByRole("button", { name: "Remove fault 1" }));
    expect(onChange).toHaveBeenLastCalledWith(expect.objectContaining({ faults: [] }));
  });

  it("edits the client policy and rejects fractional retries", async () => {
    const onChange = vi.fn<(p: ChaosPlan) => void>();
    render(<ChaosEditor value={chaosPlan} topology={topology} onChange={onChange} />);
    const retries = screen.getByLabelText("Retries");
    await userEvent.clear(retries);
    await userEvent.type(retries, "2");
    expect(onChange.mock.lastCall?.[0].policy.maxRetries).toBe(2);
    await userEvent.clear(retries);
    await userEvent.type(retries, "1.5");
    expect(await screen.findByText("Must be a whole number")).toBeInTheDocument();
  });
});
