import { useEffect, useState } from "react";
import type { Advice, Availability, Machine, Project, Requirements, Shape } from "../bindings";
import { CommandError, api, describeError } from "../ipc";
import { ReportView, pct } from "./ReportView";

interface Props {
  /** Opens the advised design as a new project. */
  onOpen: (project: Project) => void;
}

const SHAPES: { value: Shape; label: string }[] = [
  { value: "web", label: "Pages and APIs for people" },
  { value: "files", label: "Large files and media" },
  { value: "jobs", label: "Work that can finish later" },
  { value: "events", label: "Very many small writes (events, telemetry)" },
  { value: "services", label: "Many independent services" },
];

const AVAILABILITY: { value: Availability; label: string }[] = [
  { value: "standard", label: "Standard: some downtime is fine" },
  { value: "high", label: "High: about 99.9%, no single point of failure" },
  { value: "critical", label: "Critical: about 99.99%, survive any one machine" },
];

const MACHINES: { value: Machine; label: string }[] = [
  { value: "small", label: "Small: 2 CPUs, 8 GB" },
  { value: "medium", label: "Medium: 4 CPUs, 16 GB" },
  { value: "large", label: "Large: 8 CPUs, 32 GB, 10 Gbps" },
];

const nonNull = (n: number | null) => n ?? Number.NaN;

/** Describe the product and its needs; get a design, sized and checked by simulation. */
export function AdvisorPanel({ onOpen }: Props) {
  const [form, setForm] = useState<Requirements | null>(null);
  const [advice, setAdvice] = useState<Advice | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showReport, setShowReport] = useState(false);

  useEffect(() => {
    let live = true;
    api
      .defaultRequirements()
      .then((r) => {
        if (live) {
          setForm(r);
        }
      })
      .catch(() => {
        if (live) {
          setError("Could not load the advisor");
        }
      });
    return () => {
      live = false;
    };
  }, []);

  if (!form) {
    return (
      <div className="library">
        {error ? <p role="alert">{error}</p> : <p className="muted">Loading…</p>}
      </div>
    );
  }

  const set = <K extends keyof Requirements>(key: K, value: Requirements[K]) => {
    setForm((prev) => (prev ? { ...prev, [key]: value } : prev));
  };
  const valid = [
    form.dailyUsers,
    form.requestsPerUserPerDay,
    form.readRatio,
    form.peakFactor,
    form.p95Ms,
    form.maxErrorRate,
  ].every((n) => Number.isFinite(nonNull(n)));

  const submit = async () => {
    setBusy(true);
    setError(null);
    setAdvice(null);
    setShowReport(false);
    try {
      setAdvice(await api.adviseDesign(form));
    } catch (err) {
      setError(
        err instanceof CommandError ? describeError(err.error) : "The advisor could not finish",
      );
    } finally {
      setBusy(false);
    }
  };

  const num = (
    id: string,
    label: string,
    value: number | null,
    onChange: (n: number) => void,
    props: { step?: string; min?: string; max?: string } = {},
  ) => (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      <input
        id={id}
        type="number"
        value={Number.isFinite(nonNull(value)) ? (value ?? "") : ""}
        {...props}
        onChange={(e) => {
          onChange(e.target.valueAsNumber);
        }}
      />
    </div>
  );

  const select = <T extends string>(
    id: string,
    label: string,
    value: T,
    options: { value: T; label: string }[],
    onChange: (v: T) => void,
  ) => (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      <select
        id={id}
        value={value}
        onChange={(e) => {
          onChange(e.target.value as T);
        }}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </div>
  );

  return (
    <div className="library advisor">
      <header className="library-head">
        <h1>Design advisor</h1>
        <p className="muted">
          Describe your product and what it must handle. The advisor picks an architecture, sizes
          it, and checks it by simulation. It runs on this computer with no network. Results are
          estimates.
        </p>
      </header>
      <form
        className="advisor-form"
        noValidate
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <div className="advisor-grid">
          {num(
            "adv-users",
            "Daily users",
            form.dailyUsers,
            (n) => {
              set("dailyUsers", n);
            },
            { step: "any", min: "100" },
          )}
          {num(
            "adv-requests",
            "Requests per user per day",
            form.requestsPerUserPerDay,
            (n) => {
              set("requestsPerUserPerDay", n);
            },
            { step: "any", min: "0.1" },
          )}
          {num(
            "adv-read",
            "Reads (% of requests)",
            form.readRatio === null ? null : Math.round(form.readRatio * 1000) / 10,
            (n) => {
              set("readRatio", n / 100);
            },
            { step: "any", min: "0", max: "100" },
          )}
          {num(
            "adv-peak",
            "Busiest moment (× the average)",
            form.peakFactor,
            (n) => {
              set("peakFactor", n);
            },
            { step: "any", min: "1" },
          )}
          {select("adv-shape", "What it mostly does", form.shape, SHAPES, (v) => {
            set("shape", v);
          })}
          {select("adv-availability", "Availability", form.availability, AVAILABILITY, (v) => {
            set("availability", v);
          })}
          {select("adv-machine", "Machine size", form.machine, MACHINES, (v) => {
            set("machine", v);
          })}
          {num(
            "adv-p95",
            "Latency target, p95 (ms)",
            form.p95Ms,
            (n) => {
              set("p95Ms", n);
            },
            { step: "any", min: "1" },
          )}
          {num(
            "adv-errors",
            "Errors allowed (%)",
            form.maxErrorRate === null ? null : Math.round(form.maxErrorRate * 10000) / 100,
            (n) => {
              set("maxErrorRate", n / 100);
            },
            { step: "any", min: "0", max: "100" },
          )}
        </div>
        <button type="submit" className="primary" disabled={busy || !valid}>
          {busy ? "Designing…" : "Design it"}
        </button>
      </form>
      {error && <p role="alert">{error}</p>}
      {advice && (
        <section className="advice" aria-label="Advice">
          <p role="status" className="verdict" data-ok={advice.meetsTargets}>
            {advice.meetsTargets ? "Meets your targets" : "Does not meet your targets"}:{" "}
            {advice.blueprintTitle}, {advice.machines}{" "}
            {advice.machines === 1 ? "machine" : "machines"}, about{" "}
            {Math.round(advice.peakRps ?? 0)} requests per second at peak.
          </p>
          {advice.warnings.length > 0 && (
            <ul className="warnings" aria-label="Warnings">
              {advice.warnings.map((w) => (
                <li key={w}>{w}</li>
              ))}
            </ul>
          )}
          <div className="table-wrap">
            <table className="results">
              <caption>The design</caption>
              <thead>
                <tr>
                  <th>Node</th>
                  <th>Copies</th>
                  <th>Busy at peak</th>
                </tr>
              </thead>
              <tbody>
                {advice.nodes.map((n) => (
                  <tr key={n.id}>
                    <th scope="row">{n.label}</th>
                    <td>{n.copies}</td>
                    <td>{pct(n.utilization)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <h2>Why this design</h2>
          <ol className="rationale">
            {advice.rationale.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ol>
          <details>
            <summary>What it assumed</summary>
            <ul>
              {advice.assumptions.map((a) => (
                <li key={a}>{a}</li>
              ))}
            </ul>
          </details>
          <div className="form-actions">
            <button
              type="button"
              className="primary"
              onClick={() => {
                onOpen(advice.project);
              }}
            >
              Open as a project
            </button>
            <button
              type="button"
              onClick={() => {
                setShowReport((v) => !v);
              }}
            >
              {showReport ? "Hide simulation results" : "Show simulation results"}
            </button>
          </div>
          {showReport && <ReportView report={advice.report} />}
        </section>
      )}
    </div>
  );
}
