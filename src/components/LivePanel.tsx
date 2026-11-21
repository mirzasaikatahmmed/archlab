import { useCallback, useEffect, useRef, useState } from "react";
import type { LiveEvent, LiveNodeTick, LiveTick, Project, SimReport } from "../bindings";
import { CommandError, api, describeError } from "../ipc";
import { last } from "../lib/numbers";
import { FAULT_LABELS } from "../schemas/chaos";
import { LineChart, type Band } from "./LineChart";
import { Stage } from "./Stage";
import { Icon } from "./Icon";
import { levelFor } from "../lib/severity";
import { ReportView, UtilBar, pct } from "./ReportView";

interface Props {
  project: Project;
}

type View = "stage" | "charts";
type Status = "idle" | "running" | "paused" | "finished" | "stopped" | "failed";

const SPEEDS: { label: string; value: number | null }[] = [
  { label: "1×", value: 1 },
  { label: "5×", value: 5 },
  { label: "20×", value: 20 },
  { label: "Max", value: null },
];

/** How often buffered ticks are drawn; the engine can emit far more at max speed. */
const FLUSH_MS = 250;

const num = (v: number | null) => v ?? 0;

/** Ending a run that already finished is not an error. */
async function stopQuietly(): Promise<void> {
  try {
    await api.controlLiveRun({ kind: "stop" });
  } catch (err) {
    if (!(err instanceof CommandError && err.error.kind === "simulation")) {
      throw err;
    }
  }
}

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
  const [status, setStatus] = useState<Status>("idle");
  const [view, setView] = useState<View>("stage");
  const [ticks, setTicks] = useState<LiveTick[]>([]);
  const [report, setReport] = useState<SimReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const buffer = useRef<LiveEvent[]>([]);
  const active = status === "running" || status === "paused";
  const activeRef = useRef(active);
  useEffect(() => {
    activeRef.current = active;
  }, [active]);

  const flush = useCallback(() => {
    const events = buffer.current.splice(0);
    if (events.length === 0) {
      return;
    }
    const fresh: LiveTick[] = [];
    for (const event of events) {
      switch (event.kind) {
        case "tick":
          fresh.push(event.tick);
          break;
        case "finished":
          setReport(event.report);
          setStatus("finished");
          break;
        case "stopped":
          setStatus("stopped");
          break;
        case "failed":
          setError(event.message);
          setStatus("failed");
          break;
      }
    }
    if (fresh.length > 0) {
      setTicks((prev) => [...prev, ...fresh]);
    }
  }, []);

  useEffect(() => {
    if (!active) {
      return;
    }
    const id = setInterval(flush, FLUSH_MS);
    return () => {
      clearInterval(id);
      flush();
    };
  }, [active, flush]);

  useEffect(
    () => () => {
      if (activeRef.current) {
        void stopQuietly();
      }
    },
    [],
  );

  const showError = (err: unknown) => {
    setError(
      err instanceof CommandError ? describeError(err.error) : "The live run failed unexpectedly",
    );
  };

  const start = async () => {
    setError(null);
    setReport(null);
    setTicks([]);
    buffer.current = [];
    try {
      await api.startLiveRun(project, { seed, durationS, warmupS: 5 }, speed, (event) => {
        buffer.current.push(event);
      });
      setStatus("running");
    } catch (err) {
      showError(err);
      setStatus("failed");
    }
  };

  const control = async (command: Parameters<typeof api.controlLiveRun>[0]) => {
    try {
      await api.controlLiveRun(command);
    } catch (err) {
      showError(err);
    }
  };

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
                void control({ kind: "setSpeed", speed: next });
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
              void start();
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
                const next = status === "paused" ? "resume" : "pause";
                void control({ kind: next });
                setStatus(status === "paused" ? "running" : "paused");
              }}
            >
              <Icon name={status === "paused" ? "play" : "pause"} />
              {status === "paused" ? "Resume" : "Pause"}
            </button>
            <button
              type="button"
              onClick={() => {
                void control({ kind: "stop" });
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
