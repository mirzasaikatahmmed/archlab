import { useState } from "react";
import type { ConfigTemplate, Project, SimOptions, WhatIf } from "../bindings";
import { sloSchema } from "../schemas/whatif";
import { CapacityPanel } from "./CapacityPanel";
import { ComparePanel } from "./ComparePanel";
import { FormField } from "./FormField";
import { SuggestPanel } from "./SuggestPanel";
import { SweepPanel } from "./SweepPanel";

interface Props {
  project: Project;
  templates: ConfigTemplate[];
  onChange: (whatif: WhatIf) => void;
}

type Mode = "compare" | "sweep" | "capacity" | "suggest";

const MODES: { id: Mode; label: string }[] = [
  { id: "compare", label: "Compare variants" },
  { id: "sweep", label: "Sweep a parameter" },
  { id: "capacity", label: "Capacity" },
  { id: "suggest", label: "Suggestions" },
];

const num = (e: { target: { valueAsNumber: number } }) => e.target.valueAsNumber;
const show = (v: number) => (Number.isNaN(v) ? "" : v);

/** Targets and run settings shared by the tools, then the tool itself. */
export function WhatIfTab({ project, templates, onChange }: Props) {
  const [mode, setMode] = useState<Mode>("compare");
  const [p95Ms, setP95Ms] = useState(project.whatif.slo.p95Ms ?? 200);
  const [errorPct, setErrorPct] = useState((project.whatif.slo.maxErrorRate ?? 0.01) * 100);
  const [seed, setSeed] = useState(1);
  const [durationS, setDurationS] = useState(30);

  const slo = { p95Ms, maxErrorRate: errorPct / 100 };
  const sloResult = sloSchema.safeParse(slo);
  const issues = new Map<string, string>();
  if (!sloResult.success) {
    for (const issue of sloResult.error.issues) {
      issues.set(String(issue.path[0]), issue.message);
    }
  }
  const options: SimOptions = { seed, durationS, warmupS: 5 };

  const commit = (nextP95: number, nextErrorPct: number) => {
    const result = sloSchema.safeParse({ p95Ms: nextP95, maxErrorRate: nextErrorPct / 100 });
    if (result.success) {
      onChange({ ...project.whatif, slo: result.data });
    }
  };

  return (
    <div>
      <div className="whatif-bar">
        <FormField label="p95 latency target (ms)" htmlFor="slo-p95" error={issues.get("p95Ms")}>
          <input
            id="slo-p95"
            type="number"
            step="any"
            value={show(p95Ms)}
            onChange={(e) => {
              setP95Ms(num(e));
              commit(num(e), errorPct);
            }}
          />
        </FormField>
        <FormField label="Error limit (%)" htmlFor="slo-errors" error={issues.get("maxErrorRate")}>
          <input
            id="slo-errors"
            type="number"
            step="any"
            value={show(errorPct)}
            onChange={(e) => {
              setErrorPct(num(e));
              commit(p95Ms, num(e));
            }}
          />
        </FormField>
        <FormField label="Seed" htmlFor="whatif-seed">
          <input
            id="whatif-seed"
            type="number"
            step="1"
            min="0"
            value={show(seed)}
            onChange={(e) => {
              setSeed(num(e));
            }}
          />
        </FormField>
        <FormField label="Run length (s)" htmlFor="whatif-duration">
          <input
            id="whatif-duration"
            type="number"
            step="1"
            min="10"
            max="3600"
            value={show(durationS)}
            onChange={(e) => {
              setDurationS(num(e));
            }}
          />
        </FormField>
      </div>
      <div role="group" aria-label="What-if tool" className="mode-switch">
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
      {mode === "compare" && (
        <ComparePanel
          project={project}
          options={options}
          templates={templates}
          onChange={onChange}
        />
      )}
      {mode === "sweep" && <SweepPanel project={project} options={options} templates={templates} />}
      {mode === "capacity" && <CapacityPanel project={project} options={options} />}
      {mode === "suggest" && (
        <SuggestPanel project={project} options={options} onChange={onChange} />
      )}
    </div>
  );
}
