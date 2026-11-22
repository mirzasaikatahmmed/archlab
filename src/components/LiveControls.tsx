import { useState } from "react";
import type { ControlCommand, ControlRecord, Fault, FaultKind, Project } from "../bindings";
import { FAULT_LABELS } from "../schemas/chaos";

interface Props {
  project: Project;
  /** Faults in effect right now (ids), from the latest tick. */
  activeFaultIds: readonly string[];
  /** Project faults and the ones injected so far. */
  faults: readonly Fault[];
  /** Commands applied so far in this run. */
  controls: readonly ControlRecord[];
  steer: (command: ControlCommand) => Promise<ControlRecord | null>;
}

type NodeFault = "crash" | "slowdown" | "lostWorkers";

const QUICK_TRAFFIC = [0.5, 1, 2, 5];

function faultKind(type: NodeFault, amount: number): FaultKind {
  switch (type) {
    case "crash":
      return { kind: "crash" };
    case "slowdown":
      return { kind: "slowdown", factor: amount };
    case "lostWorkers":
      return { kind: "lostWorkers", fraction: amount / 100 };
  }
}

/** Steer a live run: change the traffic, start a fault on a node, or end a fault early. */
export function LiveControls({ project, activeFaultIds, faults, controls, steer }: Props) {
  const servers = project.topology.nodes.filter((n) => n.kind !== "client");
  const names = new Map(project.topology.nodes.map((n) => [n.id, n.label]));
  const [multiplier, setMultiplier] = useState(1);
  const [target, setTarget] = useState(servers[0]?.id ?? "");
  const [type, setType] = useState<NodeFault>("crash");
  const [amount, setAmount] = useState(5);
  const [durationS, setDurationS] = useState(20);

  const current = [...controls].reverse().find((c) => c.command.kind === "setTraffic");
  const shown = current?.command.kind === "setTraffic" ? current.command.multiplier : 1;
  const active = faults.filter((f) => activeFaultIds.includes(f.id));

  const label = (f: Fault) => `${FAULT_LABELS[f.kind.kind]} on ${names.get(f.target) ?? f.target}`;

  return (
    <section className="live-controls" aria-label="Steer the run">
      <h3>Steer the run</h3>
      <p className="muted">
        Changes apply at the current simulated second and are logged in the report.
      </p>

      <div className="control-row">
        <div className="field">
          <label htmlFor="ctl-traffic">Traffic (×)</label>
          <input
            id="ctl-traffic"
            type="number"
            step="0.1"
            min="0.1"
            max="10"
            value={multiplier}
            onChange={(e) => {
              setMultiplier(e.target.valueAsNumber);
            }}
          />
        </div>
        <button
          type="button"
          onClick={() => {
            void steer({ kind: "setTraffic", multiplier });
          }}
        >
          Set traffic
        </button>
        {QUICK_TRAFFIC.map((m) => (
          <button
            key={m}
            type="button"
            aria-label={`Traffic ×${String(m)}`}
            onClick={() => {
              setMultiplier(m);
              void steer({ kind: "setTraffic", multiplier: m });
            }}
          >
            ×{m}
          </button>
        ))}
        <span className="muted">Now ×{shown}</span>
      </div>

      <div className="control-row">
        <div className="field">
          <label htmlFor="ctl-node">Node</label>
          <select
            id="ctl-node"
            value={target}
            onChange={(e) => {
              setTarget(e.target.value);
            }}
          >
            {servers.map((n) => (
              <option key={n.id} value={n.id}>
                {n.label}
              </option>
            ))}
          </select>
        </div>
        <div className="field">
          <label htmlFor="ctl-type">Fault</label>
          <select
            id="ctl-type"
            value={type}
            onChange={(e) => {
              const next = e.target.value as NodeFault;
              setType(next);
              setAmount(next === "lostWorkers" ? 50 : 5);
            }}
          >
            <option value="crash">{FAULT_LABELS.crash}</option>
            <option value="slowdown">{FAULT_LABELS.slowdown}</option>
            <option value="lostWorkers">{FAULT_LABELS.lostWorkers}</option>
          </select>
        </div>
        {type !== "crash" && (
          <div className="field">
            <label htmlFor="ctl-amount">
              {type === "slowdown" ? "Slower by (×)" : "Workers lost (%)"}
            </label>
            <input
              id="ctl-amount"
              type="number"
              step="1"
              min={type === "slowdown" ? 1 : 5}
              max={type === "slowdown" ? 100 : 95}
              value={amount}
              onChange={(e) => {
                setAmount(e.target.valueAsNumber);
              }}
            />
          </div>
        )}
        <div className="field">
          <label htmlFor="ctl-duration">Lasts (s)</label>
          <input
            id="ctl-duration"
            type="number"
            step="1"
            min="1"
            max="3600"
            value={durationS}
            onChange={(e) => {
              setDurationS(e.target.valueAsNumber);
            }}
          />
        </div>
        <button
          type="button"
          disabled={target === ""}
          onClick={() => {
            void steer({ kind: "injectFault", target, fault: faultKind(type, amount), durationS });
          }}
        >
          Inject fault
        </button>
      </div>

      {active.length > 0 && (
        <ul className="control-faults" aria-label="Faults in effect">
          {active.map((f) => (
            <li key={f.id}>
              <span>{label(f)}</span>
              <button
                type="button"
                aria-label={`End ${label(f)} now`}
                onClick={() => {
                  void steer({ kind: "endFault", faultId: f.id });
                }}
              >
                End now
              </button>
            </li>
          ))}
        </ul>
      )}

      {controls.length > 0 && (
        <ol className="control-log" aria-label="Applied changes">
          {controls.map((c, i) => (
            <li key={`${String(c.tS)}-${String(i)}`}>
              at {c.tS} s: {c.summary}
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}
