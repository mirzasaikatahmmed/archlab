import type { CSSProperties } from "react";
import type { FaultImpact, Latency, Severity, SimReport } from "../bindings";

export const SEVERITY_LABEL: Record<Severity, string> = {
  healthy: "Healthy",
  warning: "Warning",
  saturated: "Saturated",
};

/** Bindings type floats as `number | null`; the engine never emits non-finite values. */
export const ms = (v: number | null) => `${(v ?? 0).toFixed(1)} ms`;
export const pct = (v: number | null) => `${((v ?? 0) * 100).toFixed(1)}%`;

export function UtilBar({ value, level }: { value: number | null; level: Severity }) {
  const v = Math.min(1, Math.max(0, value ?? 0));
  return (
    <span
      className="bar"
      data-level={level}
      style={{ "--v": v } as CSSProperties}
      aria-hidden="true"
    />
  );
}

export function LatencyCells({ latency }: { latency: Latency }) {
  return (
    <>
      <td>{ms(latency.p50Ms)}</td>
      <td>{ms(latency.p95Ms)}</td>
      <td>{ms(latency.p99Ms)}</td>
    </>
  );
}

const seconds = (v: number | null) => `${(v ?? 0).toFixed(0)} s`;

/** before → during → after, with the fault window itself emphasised. */
function Trend({ before, during, after }: { before: string; during: string; after: string }) {
  return (
    <span className="trend">
      <span className="muted">{before}</span>
      <span aria-hidden="true">→</span>
      <strong>{during}</strong>
      <span aria-hidden="true">→</span>
      <span className="muted">{after}</span>
    </span>
  );
}

function FaultImpactTable({ faults }: { faults: FaultImpact[] }) {
  return (
    <div className="table-wrap">
      <table className="results">
        <caption>Fault impact (before → during → after)</caption>
        <thead>
          <tr>
            <th>Fault</th>
            <th>Window</th>
            <th>Errors</th>
            <th>p95 latency</th>
            <th>Recovery</th>
          </tr>
        </thead>
        <tbody>
          {faults.map((f) => (
            <tr key={f.faultId}>
              <th scope="row">{f.label}</th>
              <td>
                {seconds(f.startS)}–{seconds(f.endS)}
              </td>
              <td>
                <Trend
                  before={pct(f.before.errorRate)}
                  during={pct(f.during.errorRate)}
                  after={pct(f.after.errorRate)}
                />
              </td>
              <td>
                <Trend
                  before={ms(f.before.p95Ms)}
                  during={ms(f.during.p95Ms)}
                  after={ms(f.after.p95Ms)}
                />
              </td>
              <td>{f.recoveryS === null ? "Not in this run" : seconds(f.recoveryS)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** Result of a finished run: summary, bottleneck, per-node table and assumptions. */
export function ReportView({ report }: { report: SimReport }) {
  return (
    <>
      {report.bottleneck ? (
        <div role="status" className="callout" data-severity={report.bottleneck.severity}>
          <strong>Bottleneck: {report.bottleneck.label}</strong>
          <span className="muted">
            {SEVERITY_LABEL[report.bottleneck.severity]} — {report.bottleneck.reason}
          </span>
        </div>
      ) : (
        <div role="status" className="callout" data-severity="healthy">
          <strong>No bottleneck</strong>
          <span className="muted">Every node stayed below 70% utilization.</span>
        </div>
      )}
      <dl className="summary">
        <div>
          <dt>Throughput</dt>
          <dd>{(report.summary.throughputRps ?? 0).toFixed(1)} req/s</dd>
        </div>
        <div>
          <dt>Error rate</dt>
          <dd>{pct(report.summary.errorRate)}</dd>
        </div>
        <div>
          <dt>p50</dt>
          <dd>{ms(report.summary.latency.p50Ms)}</dd>
        </div>
        <div>
          <dt>p95</dt>
          <dd>{ms(report.summary.latency.p95Ms)}</dd>
        </div>
        <div>
          <dt>p99</dt>
          <dd>{ms(report.summary.latency.p99Ms)}</dd>
        </div>
        {report.summary.retries > 0 && (
          <div>
            <dt>Retries sent</dt>
            <dd>{report.summary.retries}</dd>
          </div>
        )}
      </dl>
      <div className="table-wrap">
        <table className="results">
          <caption>Per-node results</caption>
          <thead>
            <tr>
              <th>Node</th>
              <th>Status</th>
              <th>Utilization</th>
              <th>Workers</th>
              <th>Mean queue</th>
              <th>Dropped</th>
              <th>p50</th>
              <th>p95</th>
              <th>p99</th>
            </tr>
          </thead>
          <tbody>
            {report.nodes.map((n) => (
              <tr key={n.id}>
                <th scope="row">{n.label}</th>
                <td>
                  <span className="severity" data-level={n.severity}>
                    {SEVERITY_LABEL[n.severity]}
                  </span>
                </td>
                <td>
                  <span className="util">
                    <UtilBar value={n.utilization} level={n.severity} />
                    <span>{pct(n.utilization)}</span>
                  </span>
                </td>
                <td>{n.servers}</td>
                <td>{(n.meanQueue ?? 0).toFixed(2)}</td>
                <td>{n.rejected + n.timedOut + n.faulted}</td>
                <LatencyCells latency={n.latency} />
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {report.faults.length > 0 && <FaultImpactTable faults={report.faults} />}
      {report.controls.length > 0 && (
        <section aria-label="Live controls">
          <h3>Live controls</h3>
          <p className="muted">
            This run was steered while it played. Replaying these changes reproduces it.
          </p>
          <ul>
            {report.controls.map((c, i) => (
              <li key={`${String(c.tS)}-${String(i)}`}>
                at {c.tS} s: {c.summary}
              </li>
            ))}
          </ul>
        </section>
      )}
      <p className="muted">{report.notice}</p>
      <details>
        <summary>Modelling assumptions</summary>
        <ul>
          {report.assumptions.map((a) => (
            <li key={a}>{a}</li>
          ))}
        </ul>
      </details>
    </>
  );
}
