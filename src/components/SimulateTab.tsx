import { useState } from "react";
import type { Project } from "../bindings";
import { LivePanel } from "./LivePanel";
import { SimulationPanel } from "./SimulationPanel";

type Mode = "instant" | "live";

/** Switches between a one-shot run and a live, streaming run. */
export function SimulateTab({ project }: { project: Project }) {
  const [mode, setMode] = useState<Mode>("instant");
  return (
    <div>
      <div role="group" aria-label="Run mode" className="mode-switch">
        <button
          type="button"
          aria-pressed={mode === "instant"}
          onClick={() => {
            setMode("instant");
          }}
        >
          Instant run
        </button>
        <button
          type="button"
          aria-pressed={mode === "live"}
          onClick={() => {
            setMode("live");
          }}
        >
          Live run
        </button>
      </div>
      {mode === "instant" ? <SimulationPanel project={project} /> : <LivePanel project={project} />}
    </div>
  );
}
