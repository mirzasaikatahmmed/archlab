import { useState } from "react";
import type { Change, ConfigTemplate, Topology, Variant, WhatIf } from "../bindings";
import {
  CHANGE_LABELS,
  SERVER_FIELDS,
  SERVER_FIELD_LABELS,
  defaultChange,
  toWhatIf,
  whatIfSchema,
  type ChangeKind,
  type ServerFieldName,
} from "../schemas/whatif";
import { FormField } from "./FormField";
import { Icon } from "./Icon";

interface Props {
  value: WhatIf;
  topology: Topology;
  templates: ConfigTemplate[];
  onChange: (whatif: WhatIf) => void;
}

const KINDS = Object.keys(CHANGE_LABELS) as ChangeKind[];

/** NaN (an emptied field) and null show as an empty input. */
const show = (v: number | null) => (v === null || Number.isNaN(v) ? "" : v);
const firstId = (nodes: { id: string }[]) => (nodes.length > 0 ? nodes[0].id : "");
const num = (e: { target: { valueAsNumber: number } }) => e.target.valueAsNumber;

/** Variants of the project, each a list of changes. Only valid sets are passed on. */
export function VariantEditor({ value, topology, templates, onChange }: Props) {
  const [variants, setVariants] = useState<Variant[]>(value.variants);
  const parsed = whatIfSchema.safeParse({ slo: value.slo, variants });
  const errors = new Map<string, string>();
  if (!parsed.success) {
    for (const issue of parsed.error.issues) {
      const key = issue.path.join(".");
      if (!errors.has(key)) {
        errors.set(key, issue.message);
      }
    }
  }

  const serverNodes = topology.nodes.filter((n) => n.kind !== "client");
  const templated = serverNodes.filter((n) => n.template !== null);
  const settingsOf = (nodeId: string) => {
    const ref = topology.nodes.find((n) => n.id === nodeId)?.template;
    return templates.find((t) => t.id === ref?.id)?.settings ?? [];
  };

  const update = (next: Variant[]) => {
    setVariants(next);
    const result = whatIfSchema.safeParse({ slo: value.slo, variants: next });
    if (result.success) {
      onChange(toWhatIf(result.data));
    }
  };

  const patchVariant = (index: number, patch: Partial<Variant>) => {
    update(variants.map((v, i) => (i === index ? { ...v, ...patch } : v)));
  };

  const patchChange = (vi: number, ci: number, change: Change) => {
    patchVariant(vi, { changes: variants[vi].changes.map((c, i) => (i === ci ? change : c)) });
  };

  const addVariant = () => {
    update([
      ...variants,
      {
        id: crypto.randomUUID(),
        name: `Variant ${String(variants.length + 1)}`,
        changes: [defaultChange("trafficScale", "")],
      },
    ]);
  };

  const addChange = (vi: number) => {
    const node = firstId(serverNodes);
    patchVariant(vi, { changes: [...variants[vi].changes, defaultChange("replicas", node)] });
  };

  const changeKind = (vi: number, ci: number, kind: ChangeKind) => {
    const node = firstId(kind === "nodeSetting" ? templated : serverNodes);
    patchChange(vi, ci, defaultChange(kind, node));
  };

  const nodeSelect = (
    id: string,
    current: string,
    nodes: typeof serverNodes,
    set: (v: string) => void,
    error?: string,
  ) => (
    <FormField label="Node" htmlFor={id} error={error}>
      <select
        id={id}
        value={current}
        onChange={(e) => {
          set(e.target.value);
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

  const fields = (vi: number, ci: number, change: Change) => {
    const base = `variants.${String(vi)}.changes.${String(ci)}`;
    const id = (name: string) => `v${String(vi)}-c${String(ci)}-${name}`;
    const err = (name: string) => errors.get(`${base}.${name}`);
    switch (change.kind) {
      case "trafficScale":
        return (
          <FormField label="Multiply traffic by" htmlFor={id("factor")} error={err("factor")}>
            <input
              id={id("factor")}
              type="number"
              step="any"
              value={show(change.factor)}
              onChange={(e) => {
                patchChange(vi, ci, { ...change, factor: num(e) });
              }}
            />
          </FormField>
        );
      case "trafficRate":
        return (
          <FormField label="Requests per second" htmlFor={id("rps")} error={err("rps")}>
            <input
              id={id("rps")}
              type="number"
              step="any"
              value={show(change.rps)}
              onChange={(e) => {
                patchChange(vi, ci, { ...change, rps: num(e) });
              }}
            />
          </FormField>
        );
      case "server":
        return (
          <>
            <FormField label="Setting" htmlFor={id("field")}>
              <select
                id={id("field")}
                value={change.field}
                onChange={(e) => {
                  patchChange(vi, ci, { ...change, field: e.target.value as ServerFieldName });
                }}
              >
                {SERVER_FIELDS.map((f) => (
                  <option key={f} value={f}>
                    {SERVER_FIELD_LABELS[f]}
                  </option>
                ))}
              </select>
            </FormField>
            <FormField label="New value" htmlFor={id("value")} error={err("value")}>
              <input
                id={id("value")}
                type="number"
                step="any"
                value={show(change.value)}
                onChange={(e) => {
                  patchChange(vi, ci, { ...change, value: num(e) });
                }}
              />
            </FormField>
          </>
        );
      case "nodeSetting": {
        const settings = settingsOf(change.node);
        return (
          <>
            {nodeSelect(
              id("node"),
              change.node,
              templated,
              (node) => {
                patchChange(vi, ci, { ...change, node, key: "" });
              },
              err("node"),
            )}
            <FormField label="Setting" htmlFor={id("key")} error={err("key")}>
              <select
                id={id("key")}
                value={change.key}
                onChange={(e) => {
                  patchChange(vi, ci, { ...change, key: e.target.value });
                }}
              >
                <option value="">Choose…</option>
                {settings.map((s) => (
                  <option key={s.key} value={s.key}>
                    {s.label}
                  </option>
                ))}
              </select>
            </FormField>
            <FormField label="New value" htmlFor={id("value")} error={err("value")}>
              <input
                id={id("value")}
                type="number"
                step="any"
                value={show(change.value)}
                onChange={(e) => {
                  patchChange(vi, ci, { ...change, value: num(e) });
                }}
              />
            </FormField>
            {templated.length === 0 && (
              <small className="muted">Pick a template for a node on the Topology tab first.</small>
            )}
          </>
        );
      }
      case "replicas":
        return (
          <>
            {nodeSelect(
              id("node"),
              change.node,
              serverNodes,
              (node) => {
                patchChange(vi, ci, { ...change, node });
              },
              err("node"),
            )}
            <FormField label="Copies" htmlFor={id("count")} error={err("count")}>
              <input
                id={id("count")}
                type="number"
                step="1"
                value={show(change.count)}
                onChange={(e) => {
                  patchChange(vi, ci, { ...change, count: num(e) });
                }}
              />
            </FormField>
          </>
        );
      case "chaosOff":
        return <p className="muted">Runs this variant without the Chaos tab's faults.</p>;
    }
  };

  return (
    <div className="variant-list">
      {variants.length === 0 && (
        <p className="muted">
          No variants yet. A variant is a copy of this design with a few changes, such as more
          replicas or double the traffic.
        </p>
      )}
      {variants.map((variant, vi) => {
        const n = String(vi + 1);
        return (
          <fieldset className="variant" key={variant.id}>
            <legend>Variant {n}</legend>
            <div className="variant-head">
              <FormField
                label="Name"
                htmlFor={`variant-${n}-name`}
                error={errors.get(`variants.${String(vi)}.name`)}
              >
                <input
                  id={`variant-${n}-name`}
                  value={variant.name}
                  maxLength={60}
                  onChange={(e) => {
                    patchVariant(vi, { name: e.target.value });
                  }}
                />
              </FormField>
              <button
                type="button"
                className="icon-btn"
                aria-label={`Remove variant ${n}`}
                onClick={() => {
                  update(variants.filter((_, i) => i !== vi));
                }}
              >
                <Icon name="trash" />
              </button>
            </div>
            {variant.changes.map((change, ci) => (
              <div className="change-row" key={`${variant.id}-${String(ci)}`}>
                <FormField label="Change" htmlFor={`v${String(vi)}-c${String(ci)}-kind`}>
                  <select
                    id={`v${String(vi)}-c${String(ci)}-kind`}
                    value={change.kind}
                    onChange={(e) => {
                      changeKind(vi, ci, e.target.value as ChangeKind);
                    }}
                  >
                    {KINDS.map((k) => (
                      <option key={k} value={k}>
                        {CHANGE_LABELS[k]}
                      </option>
                    ))}
                  </select>
                </FormField>
                {fields(vi, ci, change)}
                <button
                  type="button"
                  className="icon-btn"
                  aria-label={`Remove change ${String(ci + 1)} of variant ${n}`}
                  onClick={() => {
                    patchVariant(vi, { changes: variant.changes.filter((_, i) => i !== ci) });
                  }}
                >
                  <Icon name="trash" />
                </button>
              </div>
            ))}
            <button
              type="button"
              onClick={() => {
                addChange(vi);
              }}
            >
              <Icon name="plus" />
              Add change
            </button>
          </fieldset>
        );
      })}
      <div>
        <button type="button" onClick={addVariant}>
          <Icon name="plus" />
          Add variant
        </button>
      </div>
    </div>
  );
}
