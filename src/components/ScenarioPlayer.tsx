import { useEffect, useRef, useState } from "react";
import type { Project, ScenarioView, SimReport } from "../bindings";
import { CommandError, api, describeError } from "../ipc";
import { last } from "../lib/numbers";
import { useLiveRun } from "../lib/useLiveRun";
import { Icon } from "./Icon";
import { ReportView } from "./ReportView";
import { Stage } from "./Stage";

interface Props {
  scenarioId: string;
  onExit: () => void;
  /** Saves the project as it stands in the current step and opens it. */
  onOpenAsProject: (project: Project) => void;
}

const WARMUP_S = 5;

const failure = (err: unknown) =>
  err instanceof CommandError ? describeError(err.error) : "Something went wrong";

/** Walks through a scenario: narration beside a stage that plays each step. */
export function ScenarioPlayer({ scenarioId, onExit, onOpenAsProject }: Props) {
  const [scenario, setScenario] = useState<ScenarioView | null>(null);
  const [stepIndex, setStepIndex] = useState(0);
  const [project, setProject] = useState<Project | null>(null);
  const [replay, setReplay] = useState(0);
  const [instant, setInstant] = useState<SimReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const run = useLiveRun(project ?? PLACEHOLDER);
  const activeRef = useRef(run.active);
  useEffect(() => {
    activeRef.current = run.active;
  }, [run.active]);

  useEffect(() => {
    let live = true;
    api
      .getScenario(scenarioId)
      .then((s) => {
        if (live) {
          setScenario(s);
          setStepIndex(0);
        }
      })
      .catch((err: unknown) => {
        if (live) {
          setError(failure(err));
        }
      });
    return () => {
      live = false;
    };
  }, [scenarioId]);

  useEffect(() => {
    if (!scenario) {
      return;
    }
    let live = true;
    api
      .scenarioStepProject(scenarioId, stepIndex)
      .then((p) => {
        if (live) {
          setProject(p);
        }
      })
      .catch((err: unknown) => {
        if (live) {
          setError(failure(err));
        }
      });
    return () => {
      live = false;
    };
  }, [scenario, scenarioId, stepIndex, replay]);

  const step = scenario?.steps[stepIndex];

  // Play the step whenever its project is ready (a new step, or a replay).
  useEffect(() => {
    if (!project || !step) {
      return;
    }
    const options = { seed: 1, durationS: step.play.durationS ?? 30, warmupS: WARMUP_S };
    void (async () => {
      if (activeRef.current) {
        await run.stop();
      }
      if (step.play.mode === "instant") {
        try {
          setInstant(await api.runSimulation(project, options));
        } catch (err) {
          setError(failure(err));
        }
        return;
      }
      await run.start(options, step.play.speed ?? null, true);
    })();
    // The run controls are recreated on every render; only the project decides when to play.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [project]);

  if (error) {
    return (
      <div className="player">
        <p role="alert">{error}</p>
        <button type="button" onClick={onExit}>
          Back to scenarios
        </button>
      </div>
    );
  }
  if (!scenario || !step || !project) {
    return <p className="muted player">Loading…</p>;
  }

  const total = scenario.steps.length;
  const latest = last(run.ticks);
  const go = (next: number) => {
    setInstant(null);
    setProject(null);
    setStepIndex(next);
  };

  return (
    <div className="player">
      <header className="player-head">
        <button type="button" className="link" onClick={onExit}>
          ← Scenarios
        </button>
        <h1>{scenario.title}</h1>
        <span className="muted">
          Step {stepIndex + 1} of {total}
        </span>
      </header>
      <div className="player-body">
        <div className="player-stage">
          <Stage
            topology={project.topology}
            latest={latest}
            running={run.status === "running"}
            faults={project.chaos.faults}
            active={run.active}
            highlight={step.highlight}
          />
          {run.active && latest && (
            <progress max={1} value={latest.progress ?? 0} aria-label="Step progress" />
          )}
          {run.error && <p role="alert">{run.error}</p>}
          {run.report && <ReportView report={run.report} />}
          {instant && <ReportView report={instant} />}
        </div>
        <aside className="narration" aria-label="Narration">
          <h2>{step.title}</h2>
          {step.narration.map((paragraph) => (
            <p key={paragraph}>{paragraph}</p>
          ))}
          {step.changes.length > 0 && (
            <div className="changes">
              <strong>What changed</strong>
              <ul>
                {step.changes.map((c) => (
                  <li key={c}>{c}</li>
                ))}
              </ul>
            </div>
          )}
          <div className="step-nav">
            <button
              type="button"
              disabled={stepIndex === 0}
              onClick={() => {
                go(stepIndex - 1);
              }}
            >
              Back
            </button>
            <button
              type="button"
              onClick={() => {
                setInstant(null);
                setProject(null);
                setReplay((n) => n + 1);
              }}
            >
              <Icon name="play" />
              Replay
            </button>
            {run.active && (
              <button
                type="button"
                onClick={() => {
                  void (run.status === "paused" ? run.resume() : run.pause());
                }}
              >
                <Icon name={run.status === "paused" ? "play" : "pause"} />
                {run.status === "paused" ? "Resume" : "Pause"}
              </button>
            )}
            <button
              type="button"
              className="primary"
              onClick={() => {
                if (stepIndex + 1 < total) {
                  go(stepIndex + 1);
                } else {
                  onExit();
                }
              }}
            >
              {stepIndex + 1 < total ? "Next" : "Finish"}
            </button>
          </div>
          <button
            type="button"
            className="link"
            onClick={() => {
              onOpenAsProject(project);
            }}
          >
            Open this step as a project
          </button>
        </aside>
      </div>
    </div>
  );
}

/** The live hook needs a project before the first step has loaded; it is never run. */
const PLACEHOLDER: Project = {
  id: "",
  name: "",
  serverProfile: {
    os: "linux",
    cpuCores: 1,
    cpuGhz: 3,
    ramGb: 1,
    disk: { kind: "ssd", iops: 1000 },
    network: { bandwidthMbps: 100, latencyMs: 1 },
    virtualization: "vm",
    fdLimit: 1024,
  },
  trafficProfile: {
    concurrentUsers: 1,
    requestsPerSecond: 1,
    readRatio: 1,
    spike: { kind: "none" },
  },
  topology: { nodes: [], edges: [] },
  chaos: { faults: [], policy: { timeoutMs: 1000, maxRetries: 0, backoffMs: 0 } },
  whatif: { slo: { p95Ms: 1, maxErrorRate: 0 }, variants: [] },
  createdAt: 0,
  updatedAt: 0,
};
