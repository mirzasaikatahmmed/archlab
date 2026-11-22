import { useEffect, useState } from "react";
import type { ScenarioSummary } from "../bindings";
import { CommandError, api, describeError } from "../ipc";
import { Icon } from "./Icon";

interface Props {
  onPlay: (id: string) => void;
}

const LEVEL_LABEL = { beginner: "Beginner", intermediate: "Intermediate" } as const;

/** Cards for the guided scenarios that ship with the app. */
export function ScenarioLibrary({ onPlay }: Props) {
  const [scenarios, setScenarios] = useState<ScenarioSummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    api
      .listScenarios()
      .then((list) => {
        if (live) {
          setScenarios(list);
        }
      })
      .catch((err: unknown) => {
        if (live) {
          setError(
            err instanceof CommandError ? describeError(err.error) : "Could not load scenarios",
          );
        }
      });
    return () => {
      live = false;
    };
  }, []);

  return (
    <div className="library">
      <header className="library-head">
        <h1>Scenarios</h1>
        <p className="muted">
          Short guided stories that show how real systems behave. Each step changes the design, then
          plays it on the stage so you can see what happens.
        </p>
      </header>
      {error && <p role="alert">{error}</p>}
      {!scenarios && !error && <p className="muted">Loading…</p>}
      <ul className="scenario-grid">
        {scenarios?.map((s) => (
          <li key={s.id} className="scenario-card">
            <div className="scenario-meta">
              <span className="chip" data-level={s.level}>
                {LEVEL_LABEL[s.level]}
              </span>
              <span className="muted">
                {s.stepCount} steps · about {s.minutes} min
              </span>
            </div>
            <h2>{s.title}</h2>
            <p className="muted">{s.summary}</p>
            <button
              type="button"
              className="primary"
              aria-label={`Play ${s.title}`}
              onClick={() => {
                onPlay(s.id);
              }}
            >
              <Icon name="play" />
              Play
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
