import { useState } from "react";
import type { ConfigTemplate, Project, SimOptions, SweepParam } from "../bindings";
import { defaultRange, type SweepChoice, type SweepKind } from "../lib/sweepDefaults";
import { useWhatIfRun } from "../lib/useWhatIfRun";
import {
  SERVER_FIELDS,
  SERVER_FIELD_LABELS,
  sweepSpecSchema,
  type ServerFieldName,
} from "../schemas/whatif";
import { FormField } from "./FormField";
import { SweepResults } from "./WhatIfResults";
import { WhatIfRunBar } from "./WhatIfRunBar";

interface Props {
  project: Project;
  options: SimOptions;
  templates: ConfigTemplate[];
}

const KIND_LABELS: Record<SweepKind, string> = {
  trafficScale: "Traffic",
  server: "Server setting",
  nodeSetting: "Node setting",
  replicas: "Replicas of a node",
};

const num = (e: { target: { valueAsNumber: number } }) => e.target.valueAsNumber;
const show = (v: number) => (Number.isNaN(v) ? "" : v);

function toParam(c: SweepChoice): SweepParam {
  switch (c.kind) {
    case "trafficScale":
      return { kind: "trafficScale" };
    case "server":
      return { kind: "server", field: c.field };
    case "nodeSetting":
      return { kind: "nodeSetting", node: c.node, key: c.key };
    case "replicas":
      return { kind: "replicas", node: c.node };
  }
}

/** Varies one parameter across a range and charts how latency, errors and throughput respond. */
export function SweepPanel({ project, options, templates }: Props) {
  const serverNodes = project.topology.nodes.filter((n) => n.kind !== "client");
  const templated = serverNodes.filter((n) => n.template !== null);
  const [choice, setChoice] = useState<SweepChoice>({
    kind: "trafficScale",
    field: "cpuCores",
    node: serverNodes.length > 0 ? serverNodes[0].id : "",
    key: "",
  });
  const [range, setRange] = useState(() => defaultRange(choice, project, templates));
  const [points, setPoints] = useState(10);
  const run = useWhatIfRun(project, options);

  const spec = { param: toParam(choice), from: range.from, to: range.to, points };
  const parsed = sweepSpecSchema.safeParse(spec);
  const errors = new Map<string, string>();
  if (!parsed.success) {
    for (const issue of parsed.error.issues) {
      const key = issue.path.join(".");
      if (!errors.has(key)) {
        errors.set(key, issue.message);
      }
    }
  }

  const choose = (next: SweepChoice) => {
    setChoice(next);
    setRange(defaultRange(next, project, templates));
  };

  const settingsOf = (nodeId: string) => {
    const ref = project.topology.nodes.find((n) => n.id === nodeId)?.template;
    return templates.find((t) => t.id === ref?.id)?.settings ?? [];
  };

  const nodeSelect = (nodes: typeof serverNodes, error?: string) => (
    <FormField label="Node" htmlFor="sweep-node" error={error}>
      <select
        id="sweep-node"
        value={choice.node}
        onChange={(e) => {
          choose({ ...choice, node: e.target.value, key: "" });
        }}
      >
        {nodes.length === 0 && <option value="">No node to choose</option>}
        {nodes.map((n) => (
          <option key={n.id} value={n.id}>
            {n.label}
          </option>
        ))}
      </select>
    </FormField>
  );

  return (
    <div className="sim-panel">
      <div className="whatif-bar">
        <FormField label="Vary" htmlFor="sweep-kind">
          <select
            id="sweep-kind"
            value={choice.kind}
            onChange={(e) => {
              const kind = e.target.value as SweepKind;
              const pool = kind === "nodeSetting" ? templated : serverNodes;
              choose({ ...choice, kind, node: pool.length > 0 ? pool[0].id : "", key: "" });
            }}
          >
            {(Object.keys(KIND_LABELS) as SweepKind[]).map((k) => (
              <option key={k} value={k}>
                {KIND_LABELS[k]}
              </option>
            ))}
          </select>
        </FormField>
        {choice.kind === "server" && (
          <FormField label="Setting" htmlFor="sweep-field">
            <select
              id="sweep-field"
              value={choice.field}
              onChange={(e) => {
                choose({ ...choice, field: e.target.value as ServerFieldName });
              }}
            >
              {SERVER_FIELDS.map((f) => (
                <option key={f} value={f}>
                  {SERVER_FIELD_LABELS[f]}
                </option>
              ))}
            </select>
          </FormField>
        )}
        {choice.kind === "replicas" && nodeSelect(serverNodes, errors.get("param.node"))}
        {choice.kind === "nodeSetting" && (
          <>
            {nodeSelect(templated, errors.get("param.node"))}
            <FormField label="Setting" htmlFor="sweep-key" error={errors.get("param.key")}>
              <select
                id="sweep-key"
                value={choice.key}
                onChange={(e) => {
                  const next = { ...choice, key: e.target.value };
                  choose(next);
                }}
              >
                <option value="">Choose…</option>
                {settingsOf(choice.node).map((s) => (
                  <option key={s.key} value={s.key}>
                    {s.label}
                  </option>
                ))}
              </select>
            </FormField>
          </>
        )}
        <FormField label="From" htmlFor="sweep-from" error={errors.get("from")}>
          <input
            id="sweep-from"
            type="number"
            step="any"
            value={show(range.from)}
            onChange={(e) => {
              setRange({ ...range, from: num(e) });
            }}
          />
        </FormField>
        <FormField label="To" htmlFor="sweep-to" error={errors.get("to")}>
          <input
            id="sweep-to"
            type="number"
            step="any"
            value={show(range.to)}
            onChange={(e) => {
              setRange({ ...range, to: num(e) });
            }}
          />
        </FormField>
        <FormField label="Values to try" htmlFor="sweep-points" error={errors.get("points")}>
          <input
            id="sweep-points"
            type="number"
            step="1"
            value={show(points)}
            onChange={(e) => {
              setPoints(num(e));
            }}
          />
        </FormField>
      </div>
      <WhatIfRunBar
        label="Run sweep"
        status={run.status}
        progress={run.progress}
        error={run.error}
        disabled={!parsed.success}
        onRun={() => {
          if (parsed.success) {
            void run.start({ kind: "sweep", spec: parsed.data });
          }
        }}
        onCancel={() => {
          void run.cancel();
        }}
      />
      {run.result?.kind === "sweep" && <SweepResults report={run.result.report} />}
    </div>
  );
}
