import type { Project, SimOptions } from "../bindings";
import { useWhatIfRun } from "../lib/useWhatIfRun";
import { CapacityResults } from "./WhatIfResults";
import { WhatIfRunBar } from "./WhatIfRunBar";

/** Finds the highest traffic this design can take while meeting the targets. */
export function CapacityPanel({ project, options }: { project: Project; options: SimOptions }) {
  const run = useWhatIfRun(project, options);
  return (
    <div className="sim-panel">
      <p className="muted">
        Raises the traffic until the targets are missed, then narrows in on the highest rate that
        still meets them. It takes about a dozen short runs.
      </p>
      <WhatIfRunBar
        label="Find capacity"
        status={run.status}
        progress={run.progress}
        error={run.error}
        onRun={() => {
          void run.start({ kind: "capacity" });
        }}
        onCancel={() => {
          void run.cancel();
        }}
      />
      {run.result?.kind === "capacity" && <CapacityResults report={run.result.report} />}
    </div>
  );
}
