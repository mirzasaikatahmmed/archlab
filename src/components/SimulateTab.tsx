import { useState } from "react";
import type { Project } from "../bindings";
import { LivePanel } from "./LivePanel";
import { RealPanel } from "./RealPanel";
import { SimulationPanel } from "./SimulationPanel";

type Mode = "instant" | "live" | "real";

const MODES: { id: Mode; label: string }[] = [
  { id: "instant", label: "Instant run" },
  { id: "live", label: "Live run" },
  { id: "real", label: "Real run" },
];

/** Switches between a one-shot run, a live streaming run and a real run on this machine. */
export function SimulateTab({ project }: { project: Project }) {
  const [mode, setMode] = useState<Mode>("instant");
  return (
    <div>
      <div role="group" aria-label="Run mode" className="mode-switch">
        {MODES.map((m) => (
          <button
            key={m.id}
            type="button"
            aria-pressed={mode === m.id}
            onClick={() => {
              setMode(m.id);
            }}
          >
            {m.label}
          </button>
        ))}
      </div>
      {mode === "instant" && <SimulationPanel project={project} />}
      {mode === "live" && <LivePanel project={project} />}
      {mode === "real" && <RealPanel project={project} />}
    </div>
  );
}
