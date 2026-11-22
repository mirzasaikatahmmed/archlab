import { describe, expect, it } from "vitest";
import type { ControlCommand, ControlRecord } from "../bindings";
import { liveFaultsFrom } from "./liveFaults";

const at = (tS: number, command: ControlCommand): ControlRecord => ({
  tS,
  command,
  summary: "",
  faultId: null,
});

describe("liveFaultsFrom", () => {
  it("numbers injected faults in order and ignores traffic changes", () => {
    const faults = liveFaultsFrom([
      at(3, { kind: "setTraffic", multiplier: 2 }),
      at(5, { kind: "injectFault", target: "app", fault: { kind: "crash" }, durationS: 10 }),
      at(9, {
        kind: "injectFault",
        target: "db",
        fault: { kind: "slowdown", factor: 4 },
        durationS: 20,
      }),
    ]);
    expect(faults.map((f) => [f.id, f.target, f.startS, f.durationS])).toEqual([
      ["live-1", "app", 5, 10],
      ["live-2", "db", 9, 20],
    ]);
  });

  it("shortens a fault that was ended early, but never lengthens it", () => {
    const inject = at(5, {
      kind: "injectFault",
      target: "app",
      fault: { kind: "crash" },
      durationS: 10,
    });
    expect(
      liveFaultsFrom([inject, at(8, { kind: "endFault", faultId: "live-1" })])[0]?.durationS,
    ).toBe(3);
    expect(
      liveFaultsFrom([inject, at(40, { kind: "endFault", faultId: "live-1" })])[0]?.durationS,
    ).toBe(10);
    expect(liveFaultsFrom([at(8, { kind: "endFault", faultId: "f1" })])).toEqual([]);
  });
});
