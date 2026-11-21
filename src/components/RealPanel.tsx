import { useEffect, useRef, useState } from "react";
import type { Project, RealComparison } from "../bindings";
import { CommandError, api, describeError } from "../ipc";
import { ComparisonView } from "./ComparisonView";
import { Icon } from "./Icon";

interface Props {
  project: Project;
}

type Status = "idle" | "running" | "finished" | "cancelled" | "failed";

interface Progress {
  elapsedS: number;
  totalS: number;
  offered: number;
  completed: number;
  errors: number;
}

const n = (v: number | null) => v ?? 0;

/** Runs the project on real localhost sockets and compares the result with the simulation. */
export function RealPanel({ project }: Props) {
  const [seed, setSeed] = useState(1);
  const [durationS, setDurationS] = useState(20);
  const [status, setStatus] = useState<Status>("idle");
  const [progress, setProgress] = useState<Progress | null>(null);
  const [comparison, setComparison] = useState<RealComparison | null>(null);
  const [error, setError] = useState<string | null>(null);
  const runningRef = useRef(false);
  const running = status === "running";

  useEffect(() => {
    runningRef.current = running;
  }, [running]);

  useEffect(
    () => () => {
      if (runningRef.current) {
        api.cancelRealRun().catch((err: unknown) => {
          if (!(err instanceof CommandError && err.error.kind === "runtime")) {
            throw err;
          }
        });
      }
    },
    [],
  );

  const fail = (err: unknown) => {
    setError(
      err instanceof CommandError ? describeError(err.error) : "The real run failed unexpectedly",
    );
  };

  const start = async () => {
    setError(null);
    setComparison(null);
    setProgress(null);
    setStatus("running");
    try {
      await api.startRealRun(project, { seed, durationS, warmupS: 3 }, (event) => {
        switch (event.kind) {
          case "progress":
            setProgress({
              elapsedS: n(event.elapsedS),
              totalS: n(event.totalS),
              offered: event.offered,
              completed: event.completed,
              errors: event.errors,
            });
            break;
          case "finished":
            setComparison(event.comparison);
            setStatus("finished");
            break;
          case "cancelled":
            setStatus("cancelled");
            break;
          case "failed":
            setError(event.message);
            setStatus("failed");
            break;
        }
      });
    } catch (err) {
      fail(err);
      setStatus("failed");
    }
  };

  const cancel = async () => {
    try {
      await api.cancelRealRun();
    } catch (err) {
      fail(err);
    }
  };

  return (
    <div className="sim-panel">
      <div role="note" className="callout" data-severity="warning">
        <strong>Runs on this computer</strong>
        <span className="muted">
          Each node becomes a real network server on localhost and does real CPU work, so the
          computer will be busy. All nodes share its cores, so this checks the model; it does not
          reproduce a setup spread over many machines.
        </span>
      </div>
      <div className="form-actions sim-controls">
        <div className="field">
          <label htmlFor="real-seed">Seed</label>
          <input
            id="real-seed"
            type="number"
            step="1"
            min="0"
            disabled={running}
            value={seed}
            onChange={(e) => {
              setSeed(e.target.valueAsNumber);
            }}
          />
        </div>
        <div className="field">
          <label htmlFor="real-duration">Duration (s)</label>
          <input
            id="real-duration"
            type="number"
            step="1"
            min="10"
            max="60"
            disabled={running}
            value={durationS}
            onChange={(e) => {
              setDurationS(e.target.valueAsNumber);
            }}
          />
        </div>
        {!running && (
          <button
            type="button"
            className="primary"
            onClick={() => {
              void start();
            }}
          >
            <Icon name="play" />
            Start real run
          </button>
        )}
        {running && (
          <button
            type="button"
            onClick={() => {
              void cancel();
            }}
          >
            <Icon name="stop" />
            Cancel
          </button>
        )}
      </div>
      {error && <p role="alert">{error}</p>}
      <p role="status" className="muted">
        {status === "idle" && "Not started."}
        {status === "running" && "Running…"}
        {status === "finished" && "Finished."}
        {status === "cancelled" && "Cancelled."}
        {status === "failed" && "Failed."}
      </p>
      {progress && running && (
        <>
          <progress
            max={progress.totalS}
            value={progress.elapsedS}
            aria-label="Real run progress"
          />
          <p className="muted">
            {progress.offered.toLocaleString()} sent · {progress.completed.toLocaleString()}{" "}
            answered · {progress.errors.toLocaleString()} failed
          </p>
        </>
      )}
      {comparison && <ComparisonView comparison={comparison} />}
    </div>
  );
}
