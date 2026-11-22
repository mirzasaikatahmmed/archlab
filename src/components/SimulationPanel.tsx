import { useState } from "react";
import { ExportBar } from "./ExportBar";
import type { Project, SimReport } from "../bindings";
import { CommandError, api, describeError } from "../ipc";
import { Icon } from "./Icon";
import { ReportView } from "./ReportView";

interface Props {
  project: Project;
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
          className="primary"
          disabled={running}
          onClick={() => {
            void run();
          }}
        >
          <Icon name="play" />
          {running ? "Running…" : "Run simulation"}
        </button>
      </div>
      {error && <p role="alert">{error}</p>}
      <ExportBar project={project} report={report} />
      {report && <ReportView report={report} />}
    </div>
  );
}
