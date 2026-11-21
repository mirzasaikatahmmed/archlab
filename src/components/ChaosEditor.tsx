import { useState } from "react";
import type { ChaosPlan, Fault, Topology } from "../bindings";
import {
  FAULT_LABELS,
  chaosPlanSchema,
  defaultKind,
  targetsEdge,
  toChaosPlan,
  type FaultKindName,
} from "../schemas/chaos";
import { FormField } from "./FormField";
import { Icon } from "./Icon";
import { Section } from "./Section";

interface Props {
  value: ChaosPlan;
  topology: Topology;
  onChange: (plan: ChaosPlan) => void;
}

/** NaN (an emptied field) and null show as an empty input. */
const show = (v: number | null) => (v === null || Number.isNaN(v) ? "" : v);

const KINDS = Object.keys(FAULT_LABELS) as FaultKindName[];

interface Target {
  id: string;
  label: string;
}

function nodeTargets(topology: Topology): Target[] {
  return topology.nodes
    .filter((n) => n.kind !== "client")
    .map((n) => ({ id: n.id, label: n.label }));
}

function edgeTargets(topology: Topology): Target[] {
  const names = new Map(topology.nodes.map((n) => [n.id, n.label]));
  return topology.edges.map((e) => ({
    id: e.id,
    label: `${names.get(e.source) ?? e.source} → ${names.get(e.target) ?? e.target}`,
  }));
}

/** Lists faults and the client policy. Only valid plans are passed on; Rust re-validates on save. */
export function ChaosEditor({ value, topology, onChange }: Props) {
  const [draft, setDraft] = useState(value);
  const parsed = chaosPlanSchema.safeParse(draft);
  const errors = new Map<string, string>();
  if (!parsed.success) {
    for (const issue of parsed.error.issues) {
      const key = issue.path.join(".");
      if (!errors.has(key)) {
        errors.set(key, issue.message);
      }
    }
  }

  const update = (next: ChaosPlan) => {
    setDraft(next);
    const result = chaosPlanSchema.safeParse(next);
    if (result.success) {
      onChange(toChaosPlan(result.data));
    }
  };

  const targetsFor = (kind: FaultKindName) =>
    targetsEdge(kind) ? edgeTargets(topology) : nodeTargets(topology);

  const firstTarget = (kind: FaultKindName): string => {
    const options = targetsFor(kind);
    return options.length > 0 ? options[0].id : "";
  };

  const addFault = () => {
    const kind: FaultKindName = "crash";
    update({
      ...draft,
      faults: [
        ...draft.faults,
        {
          id: crypto.randomUUID(),
          target: firstTarget(kind),
          kind: defaultKind(kind),
          startS: 20,
          durationS: 10,
        },
      ],
    });
  };

  const patchFault = (index: number, patch: Partial<Fault>) => {
    update({
      ...draft,
      faults: draft.faults.map((f, i) => (i === index ? { ...f, ...patch } : f)),
    });
  };

  const changeKind = (index: number, fault: Fault, kind: FaultKindName) => {
    const keep = targetsEdge(kind) === targetsEdge(fault.kind.kind);
    patchFault(index, {
      kind: defaultKind(kind),
      target: keep ? fault.target : firstTarget(kind),
    });
  };

  const number = (e: { target: { valueAsNumber: number } }) => e.target.valueAsNumber;

  const paramField = (index: number, fault: Fault) => {
    const id = `fault-${String(index)}-param`;
    const key = `faults.${String(index)}.kind`;
    const setParam = (field: string, v: number) => {
      patchFault(index, { kind: { ...fault.kind, [field]: v } });
    };
    switch (fault.kind.kind) {
      case "slowdown":
        return (
          <FormField label="Slower by (×)" htmlFor={id} error={errors.get(`${key}.factor`)}>
            <input
              id={id}
              type="number"
              step="any"
              value={show(fault.kind.factor)}
              onChange={(e) => {
                setParam("factor", number(e));
              }}
            />
          </FormField>
        );
      case "lostWorkers":
        return (
          <FormField label="Workers lost (0–1)" htmlFor={id} error={errors.get(`${key}.fraction`)}>
            <input
              id={id}
              type="number"
              step="0.05"
              value={show(fault.kind.fraction)}
              onChange={(e) => {
                setParam("fraction", number(e));
              }}
            />
          </FormField>
        );
      case "edgeLatency":
        return (
          <FormField label="Extra delay (ms)" htmlFor={id} error={errors.get(`${key}.extraMs`)}>
            <input
              id={id}
              type="number"
              step="any"
              value={show(fault.kind.extraMs)}
              onChange={(e) => {
                setParam("extraMs", number(e));
              }}
            />
          </FormField>
        );
      case "edgeLoss":
        return (
          <FormField
            label="Share lost (0–1)"
            htmlFor={id}
            error={errors.get(`${key}.loss`)}
            hint="1 cuts the link completely"
          >
            <input
              id={id}
              type="number"
              step="0.05"
              value={show(fault.kind.loss)}
              onChange={(e) => {
                setParam("loss", number(e));
              }}
            />
          </FormField>
        );
      case "crash":
        return null;
    }
  };

  return (
    <form
      className="form-page"
      onSubmit={(e) => {
        e.preventDefault();
      }}
      noValidate
    >
      <Section
        title="Faults"
        description="Break part of the system for a while, then watch how it copes and recovers."
      >
        <div className="fault-list">
          {draft.faults.length === 0 && (
            <p className="muted">
              No faults yet. Add one, then run a simulation longer than its start time.
            </p>
          )}
          {draft.faults.map((fault, i) => {
            const at = `faults.${String(i)}`;
            const n = String(i + 1);
            const targets = targetsFor(fault.kind.kind);
            return (
              <fieldset className="fault-row" key={fault.id}>
                <legend>Fault {n}</legend>
                <div className="fault-fields">
                  <FormField label="Fault type" htmlFor={`fault-${String(i)}-kind`}>
                    <select
                      id={`fault-${String(i)}-kind`}
                      value={fault.kind.kind}
                      onChange={(e) => {
                        changeKind(i, fault, e.target.value as FaultKindName);
                      }}
                    >
                      {KINDS.map((k) => (
                        <option key={k} value={k}>
                          {FAULT_LABELS[k]}
                        </option>
                      ))}
                    </select>
                  </FormField>
                  <FormField
                    label={targetsEdge(fault.kind.kind) ? "Connection" : "Node"}
                    htmlFor={`fault-${String(i)}-target`}
                    error={errors.get(`${at}.target`)}
                  >
                    <select
                      id={`fault-${String(i)}-target`}
                      value={fault.target}
                      onChange={(e) => {
                        patchFault(i, { target: e.target.value });
                      }}
                    >
                      {targets.length === 0 && <option value="">Nothing to target</option>}
                      {targets.map((t) => (
                        <option key={t.id} value={t.id}>
                          {t.label}
                        </option>
                      ))}
                    </select>
                  </FormField>
                  {paramField(i, fault)}
                  <FormField
                    label="Starts at (s)"
                    htmlFor={`fault-${String(i)}-start`}
                    error={errors.get(`${at}.startS`)}
                  >
                    <input
                      id={`fault-${String(i)}-start`}
                      type="number"
                      step="any"
                      value={show(fault.startS)}
                      onChange={(e) => {
                        patchFault(i, { startS: number(e) });
                      }}
                    />
                  </FormField>
                  <FormField
                    label="Lasts (s)"
                    htmlFor={`fault-${String(i)}-duration`}
                    error={errors.get(`${at}.durationS`)}
                  >
                    <input
                      id={`fault-${String(i)}-duration`}
                      type="number"
                      step="any"
                      value={show(fault.durationS)}
                      onChange={(e) => {
                        patchFault(i, { durationS: number(e) });
                      }}
                    />
                  </FormField>
                  <button
                    type="button"
                    className="icon-btn"
                    aria-label={`Remove fault ${n}`}
                    onClick={() => {
                      update({ ...draft, faults: draft.faults.filter((_, j) => j !== i) });
                    }}
                  >
                    <Icon name="trash" />
                  </button>
                </div>
              </fieldset>
            );
          })}
          <div>
            <button type="button" onClick={addFault}>
              <Icon name="plus" />
              Add fault
            </button>
          </div>
        </div>
      </Section>

      <Section
        title="Client behaviour"
        description="How callers react when something fails: how long they wait and whether they retry."
      >
        <FormField
          label="Request timeout (ms)"
          htmlFor="policy-timeout"
          error={errors.get("policy.timeoutMs")}
        >
          <input
            id="policy-timeout"
            type="number"
            step="any"
            value={show(draft.policy.timeoutMs)}
            onChange={(e) => {
              update({ ...draft, policy: { ...draft.policy, timeoutMs: number(e) } });
            }}
          />
        </FormField>
        <FormField
          label="Retries"
          htmlFor="policy-retries"
          error={errors.get("policy.maxRetries")}
          hint="Extra attempts after a failure"
        >
          <input
            id="policy-retries"
            type="number"
            step="1"
            value={show(draft.policy.maxRetries)}
            onChange={(e) => {
              update({ ...draft, policy: { ...draft.policy, maxRetries: number(e) } });
            }}
          />
        </FormField>
        <FormField
          label="Wait before retry (ms)"
          htmlFor="policy-backoff"
          error={errors.get("policy.backoffMs")}
        >
          <input
            id="policy-backoff"
            type="number"
            step="any"
            value={show(draft.policy.backoffMs)}
            onChange={(e) => {
              update({ ...draft, policy: { ...draft.policy, backoffMs: number(e) } });
            }}
          />
        </FormField>
      </Section>
    </form>
  );
}
