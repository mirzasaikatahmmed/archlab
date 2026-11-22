import type { Project, SimOptions, Suggestion, WhatIf } from "../bindings";
import { useWhatIfRun } from "../lib/useWhatIfRun";
import { SuggestResults } from "./WhatIfResults";
import { WhatIfRunBar } from "./WhatIfRunBar";

interface Props {
  project: Project;
  options: SimOptions;
  onChange: (whatif: WhatIf) => void;
}

const MAX_VARIANTS = 20;

/** Tries likely fixes for the slowest part and offers to keep the good ones as variants. */
export function SuggestPanel({ project, options, onChange }: Props) {
  const run = useWhatIfRun(project, options);
  const full = project.whatif.variants.length >= MAX_VARIANTS;

  const save = (suggestion: Suggestion) => {
    onChange({
      ...project.whatif,
      variants: [
        ...project.whatif.variants,
        { id: crypto.randomUUID(), name: suggestion.title, changes: suggestion.changeDefs },
      ],
    });
  };

  return (
    <div className="sim-panel">
      <p className="muted">
        Finds the slowest part of the design and tries a handful of likely fixes, such as more
        copies of it, more workers, faster hardware or a better cache hit ratio. Changes that help
        are ranked against your targets.
      </p>
      <WhatIfRunBar
        label="Suggest changes"
        status={run.status}
        progress={run.progress}
        error={run.error}
        onRun={() => {
          void run.start({ kind: "suggest" });
        }}
        onCancel={() => {
          void run.cancel();
        }}
      />
      {run.result?.kind === "suggest" && (
        <SuggestResults report={run.result.report} onSave={save} saveDisabled={full} />
      )}
      {full && run.result?.kind === "suggest" && (
        <p className="muted">This project already has {String(MAX_VARIANTS)} variants.</p>
      )}
    </div>
  );
}
