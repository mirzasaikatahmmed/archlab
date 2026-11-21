import { useState } from "react";
import type { Latency, Project, Severity, SimReport } from "../bindings";
import { CommandError, api, describeError } from "../ipc";

interface Props {
  project: Project;
}

const SEVERITY_LABEL: Record<Severity, string> = {
  healthy: "Healthy",
  warning: "Warning",
  saturated: "Saturated",
};

/** Bindings type floats as `number | null`; the engine never emits non-finite values. */
const ms = (v: number | null) => `${(v ?? 0).toFixed(1)} ms`;
const pct = (v: number | null) => `${((v ?? 0) * 100).toFixed(1)}%`;

function LatencyCells({ latency }: { latency: Latency }) {
  return (
    <>
      <td>{ms(latency.p50Ms)}</td>
      <td>{ms(latency.p95Ms)}</td>
      <td>{ms(latency.p99Ms)}</td>
    </>
  );
}

/** Runs the simulation for the open project and shows the report. All maths is in Rust. */
export function SimulationPanel({ project }: Props) {
  const [seed, setSeed] = useState(1);
  const [durationS, setDurationS] = useState(60);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<SimReport | null>(null);

  const run = async () => {
    setRunning(true);
    setError(null);
    try {
      setReport(await api.runSimulation(project, { seed, durationS, warmupS: 5 }));
    } catch (err) {
      setReport(null);
      setError(
        err instanceof CommandError ? describeError(err.error) : "Simulation failed unexpectedly",
      );
    } finally {
      setRunning(false);
    }
  };

  return (
    <div className="sim-panel">
      <div className="form-actions sim-controls">
        <div className="field">
          <label htmlFor="sim-seed">Seed</label>
          <input
            id="sim-seed"
            type="number"
            step="1"
            min="0"
            value={seed}
            onChange={(e) => {
              setSeed(e.target.valueAsNumber);
            }}
          />
        </div>
        <div className="field">
          <label htmlFor="sim-duration">Duration (s)</label>
          <input
            id="sim-duration"
            type="number"
            step="1"
            min="10"
            max="3600"
            value={durationS}
            onChange={(e) => {
              setDurationS(e.target.valueAsNumber);
            }}
          />
        </div>
        <button
          type="button"
          disabled={running}
          onClick={() => {
            void run();
          }}
        >
          {running ? "Running…" : "Run simulation"}
        </button>
      </div>
      {error && <p role="alert">{error}</p>}
      {report && (
        <>
          <p className="muted">{report.notice}</p>
          {report.bottleneck ? (
            <p role="status" className="banner">
              Bottleneck: <strong>{report.bottleneck.label}</strong> (
              {SEVERITY_LABEL[report.bottleneck.severity]}) — {report.bottleneck.reason}
            </p>
          ) : (
            <p role="status">No bottleneck: every node stayed below 70% utilization.</p>
          )}
          <dl className="summary">
            <div>
              <dt>Throughput</dt>
              <dd>{(report.summary.throughputRps ?? 0).toFixed(1)} req/s</dd>
            </div>
            <div>
              <dt>Error rate</dt>
              <dd>{pct(report.summary.errorRate)}</dd>
            </div>
            <div>
              <dt>p50</dt>
              <dd>{ms(report.summary.latency.p50Ms)}</dd>
            </div>
            <div>
              <dt>p95</dt>
              <dd>{ms(report.summary.latency.p95Ms)}</dd>
            </div>
            <div>
              <dt>p99</dt>
              <dd>{ms(report.summary.latency.p99Ms)}</dd>
            </div>
          </dl>
          <table className="results">
            <caption>Per-node results</caption>
            <thead>
              <tr>
                <th>Node</th>
                <th>Status</th>
                <th>Utilization</th>
                <th>Workers</th>
                <th>Mean queue</th>
                <th>Dropped</th>
                <th>p50</th>
                <th>p95</th>
                <th>p99</th>
              </tr>
            </thead>
            <tbody>
              {report.nodes.map((n) => (
                <tr key={n.id}>
                  <th scope="row">{n.label}</th>
                  <td>{SEVERITY_LABEL[n.severity]}</td>
                  <td>{pct(n.utilization)}</td>
                  <td>{n.servers}</td>
                  <td>{(n.meanQueue ?? 0).toFixed(2)}</td>
                  <td>{n.rejected + n.timedOut}</td>
                  <LatencyCells latency={n.latency} />
                </tr>
              ))}
            </tbody>
          </table>
          <details>
            <summary>Modelling assumptions</summary>
            <ul>
              {report.assumptions.map((a) => (
                <li key={a}>{a}</li>
              ))}
            </ul>
          </details>
        </>
      )}
    </div>
  );
}
