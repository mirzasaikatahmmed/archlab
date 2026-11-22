import type { ControlRecord, Fault } from "../bindings";

/**
 * The faults a run's steering commands started, as ordinary faults the Stage and charts can
 * show. They are numbered in order, like the engine does (`live-1`, `live-2`, ...), so a script
 * that has not been applied yet can be drawn too. A fault ended early lasts until that command.
 */
export function liveFaultsFrom(records: readonly ControlRecord[]): Fault[] {
  const faults: Fault[] = [];
  for (const record of records) {
    const { command } = record;
    if (command.kind === "injectFault") {
      faults.push({
        id: `live-${String(faults.length + 1)}`,
        target: command.target,
        kind: command.fault,
        startS: record.tS,
        durationS: command.durationS ?? 0,
      });
    } else if (command.kind === "endFault") {
      const fault = faults.find((f) => f.id === command.faultId);
      if (fault) {
        fault.durationS = Math.min(
          fault.durationS ?? 0,
          Math.max(0, record.tS - (fault.startS ?? 0)),
        );
      }
    }
  }
  return faults;
}
