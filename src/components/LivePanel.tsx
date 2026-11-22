import { useState } from "react";
import type { LiveNodeTick, Project } from "../bindings";
import { last } from "../lib/numbers";
import { levelFor } from "../lib/severity";
import { useLiveRun } from "../lib/useLiveRun";
import { FAULT_LABELS } from "../schemas/chaos";
import { Icon } from "./Icon";
import { LineChart, type Band } from "./LineChart";
import { ReportView, UtilBar, pct } from "./ReportView";
import { Stage } from "./Stage";

interface Props {
  project: Project;
}

type View = "stage" | "charts";

const SPEEDS: { label: string; value: number | null }[] = [
  { label: "1×", value: 1 },
  { label: "5×", value: 5 },
  { label: "20×", value: 20 },
  { label: "Max", value: null },
];

const num = (v: number | null) => v ?? 0;

function NodeBar({ node, label }: { node: LiveNodeTick; label: string }) {
  return (
    <li className="node-live">
      <span>{label}</span>
      <UtilBar value={node.utilization} level={levelFor(num(node.utilization))} />
      <span>
        {pct(node.utilization)} busy · queue {node.queue} · dropped {node.dropped}
      </span>
    </li>
  );
}

/** Starts a paced, streaming run of the open project and charts it as it happens. */
export function LivePanel({ project }: Props) {
  const [seed, setSeed] = useState(1);
  const [durationS, setDurationS] = useState(60);
  const [speed, setSpeed] = useState<number | null>(5);
  const [view, setView] = useState<View>("stage");
  const run = useLiveRun(project);
  const { status, active, ticks, report, error } = run;

  const latest = last(ticks);
  const names = new Map(project.topology.nodes.map((n) => [n.id, n.label]));
  const points = ticks.map((t) => t.point);
  const bands: Band[] = project.chaos.faults.map((f) => ({
    from: num(f.startS),
    to: num(f.startS) + num(f.durationS),
    label: FAULT_LABELS[f.kind.kind],
  }));

  return (
    <div className="sim-panel">
      <div className="form-actions sim-controls">
        <div className="field">
          <label htmlFor="live-seed">Seed</label>
          <input
            id="live-seed"
            type="number"
            step="1"
            min="0"
            disabled={active}
            value={seed}
            onChange={(e) => {
              setSeed(e.target.valueAsNumber);
            }}
          />
        </div>
        <div className="field">
          <label htmlFor="live-duration">Duration (s)</label>
          <input
            id="live-duration"
            type="number"
            step="1"
            min="10"
            max="3600"
            disabled={active}
            value={durationS}
            onChange={(e) => {
              setDurationS(e.target.valueAsNumber);
            }}
          />
        </div>
        <div className="field">
          <label htmlFor="live-speed">Speed</label>
          <select
            id="live-speed"
            value={speed === null ? "max" : String(speed)}
            onChange={(e) => {
              const next = e.target.value === "max" ? null : Number(e.target.value);
              setSpeed(next);
              if (active) {
                void run.setSpeed(next);
              }
            }}
          >
            {SPEEDS.map((s) => (
              <option key={s.label} value={s.value === null ? "max" : String(s.value)}>
                {s.label}
              </option>
            ))}
          </select>
        </div>
        {!active && (
          <button
            type="button"
            className="primary"
            onClick={() => {
              void run.start({ seed, durationS, warmupS: 5 }, speed);
            }}
          >
            <Icon name="play" />
            Start live run
          </button>
        )}
        {active && (
          <>
            <button
              type="button"
              onClick={() => {
                void (status === "paused" ? run.resume() : run.pause());
              }}
            >
              <Icon name={status === "paused" ? "play" : "pause"} />
              {status === "paused" ? "Resume" : "Pause"}
            </button>
            <button
              type="button"
              onClick={() => {
                void run.stop();
              }}
            >
              <Icon name="stop" />
              Stop
            </button>
          </>
        )}
      </div>
      {error && <p role="alert">{error}</p>}
      <p role="status" className="muted run-state-text">
        {status === "idle" && "Not started."}
        {status === "running" && "Running…"}
        {status === "paused" && "Paused."}
        {status === "finished" && "Finished."}
        {status === "stopped" && "Stopped."}
        {status === "failed" && "Failed."}
      </p>
      <div role="group" aria-label="Live view" className="mode-switch">
        <button
          type="button"
          aria-pressed={view === "stage"}
          onClick={() => {
            setView("stage");
          }}
        >
          Stage
        </button>
        <button
          type="button"
          aria-pressed={view === "charts"}
          onClick={() => {
            setView("charts");
          }}
        >
          Charts
        </button>
      </div>
      {view === "stage" && (
        <Stage
          topology={project.topology}
          latest={latest}
          running={status === "running"}
          faults={project.chaos.faults}
          active={active}
        />
      )}
      {latest && (
        <>
          <progress max={1} value={num(latest.progress)} aria-label="Run progress" />
          {view === "charts" && (
            <div className="charts">
              <LineChart
                title="Requests per second"
                unit="req/s"
                bands={bands}
                series={[
                  { label: "Offered", values: points.map((p) => num(p.offeredRps)) },
                  { label: "Completed", values: points.map((p) => num(p.completedRps)) },
                  { label: "Errors", values: points.map((p) => num(p.errorRps)) },
                ]}
              />
              <LineChart
                title="Latency p95"
                unit="ms"
                bands={bands}
                series={[{ label: "p95", values: points.map((p) => num(p.p95Ms)) }]}
              />
            </div>
          )}
          <ul className="node-live-list" aria-label="Live node load">
            {latest.nodes.map((n) => (
              <NodeBar key={n.id} node={n} label={names.get(n.id) ?? n.id} />
            ))}
          </ul>
        </>
      )}
      {report && <ReportView report={report} />}
    </div>
  );
}
