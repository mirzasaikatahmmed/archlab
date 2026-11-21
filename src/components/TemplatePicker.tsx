import { stripNulls } from "../lib/numbers";
import type { ConfigTemplate, NodeKind, TemplateRef } from "../bindings";

interface Props {
  nodeKind: NodeKind;
  templates: ConfigTemplate[];
  template: TemplateRef | null;
  settings: Record<string, number>;
  onChange: (template: TemplateRef | null, settings: Record<string, number>) => void;
}

/** Picks a config template for a node and edits per-node setting overrides. */
export function TemplatePicker({ nodeKind, templates, template, settings, onChange }: Props) {
  const compatible = stripNulls(templates).filter((t) => t.nodeKinds.includes(nodeKind));
  const selected = template ? compatible.find((t) => t.id === template.id) : undefined;

  return (
    <div className="template-picker">
      <label htmlFor="template-select">Config template</label>
      <select
        id="template-select"
        value={template?.id ?? ""}
        onChange={(e) => {
          const next = compatible.find((t) => t.id === e.target.value);
          onChange(next ? { id: next.id, version: next.version } : null, {});
        }}
      >
        <option value="">None</option>
        {compatible.map((t) => (
          <option key={t.id} value={t.id}>
            {t.name} (v{t.version})
          </option>
        ))}
      </select>
      {selected && <p className="muted">{selected.description}</p>}
      {selected?.settings.map((s) => {
        const raw = settings[s.key] as number | undefined;
        const invalid = raw !== undefined && !(raw >= s.min && raw <= s.max);
        const id = `setting-${s.key}`;
        return (
          <div className="field" key={s.key}>
            <label htmlFor={id}>
              {s.label}
              {s.unit ? ` (${s.unit})` : ""}
            </label>
            <input
              id={id}
              type="number"
              step="any"
              min={s.min}
              max={s.max}
              placeholder={String(s.default)}
              value={raw ?? ""}
              aria-invalid={invalid}
              onChange={(e) => {
                const others = Object.fromEntries(
                  Object.entries(settings).filter(([key]) => key !== s.key),
                );
                onChange(
                  template,
                  e.target.value === "" ? others : { ...others, [s.key]: e.target.valueAsNumber },
                );
              }}
            />
            <small title={s.description}>
              Default {s.default}; range {s.min}–{s.max}
            </small>
            {invalid && (
              <small role="alert" className="field-error">
                Must be between {s.min} and {s.max}
              </small>
            )}
          </div>
        );
      })}
    </div>
  );
}
