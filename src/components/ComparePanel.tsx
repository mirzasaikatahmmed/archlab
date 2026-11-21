import type { ConfigTemplate, Project, SimOptions, WhatIf } from "../bindings";
import { useWhatIfRun } from "../lib/useWhatIfRun";
import { CompareResults } from "./WhatIfResults";
import { VariantEditor } from "./VariantEditor";
import { WhatIfRunBar } from "./WhatIfRunBar";

interface Props {
  project: Project;
  options: SimOptions;
  templates: ConfigTemplate[];
  onChange: (whatif: WhatIf) => void;
}

/** Edits the variants and runs them next to the current design. */
export function ComparePanel({ project, options, templates, onChange }: Props) {
  const run = useWhatIfRun(project, options);
  return (
    <div className="sim-panel">
      <VariantEditor
        value={project.whatif}
        topology={project.topology}
        templates={templates}
        onChange={onChange}
      />
      <WhatIfRunBar
        label="Compare"
        status={run.status}
        progress={run.progress}
        error={run.error}
        onRun={() => {
          void run.start({ kind: "compare" });
        }}
        onCancel={() => {
          void run.cancel();
        }}
      />
      {run.result?.kind === "compare" && <CompareResults report={run.result.report} />}
    </div>
  );
}
