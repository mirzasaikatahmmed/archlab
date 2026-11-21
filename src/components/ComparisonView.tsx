import type { RealComparison } from "../bindings";
import { LatencyCells, SEVERITY_LABEL, UtilBar, pct } from "./ReportView";

const asPercent = (metric: string) => metric === "Error rate" || metric.endsWith("utilization");

const format = (metric: string, v: number | null) =>
  asPercent(metric) ? pct(v) : (v ?? 0).toFixed(1);

const difference = (d: number | null) =>
  d === null ? "–" : `${d >= 0 ? "+" : ""}${d.toFixed(0)}%`;

/** Simulated and measured results side by side, plus what was measured on each node. */
export function ComparisonView({ comparison }: { comparison: RealComparison }) {
  const { real } = comparison;
  return (
    <>
      <div className="table-wrap">
        <table className="results">
          <caption>Simulated vs measured</caption>
          <thead>
            <tr>
              <th>Metric</th>
              <th>Simulated</th>
              <th>Measured</th>
              <th>Difference</th>
            </tr>
          </thead>
          <tbody>
            {comparison.deltas.map((d) => (
              <tr key={d.metric}>
                <th scope="row">{d.metric}</th>
                <td>{format(d.metric, d.simulated)}</td>
                <td>{format(d.metric, d.real)}</td>
                <td>{difference(d.differencePct)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="table-wrap">
        <table className="results">
          <caption>Measured per node</caption>
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
            {real.nodes.map((n) => (
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
                <td>{n.rejected + n.timedOut}</td>
                <LatencyCells latency={n.latency} />
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="muted">{real.notice}</p>
      <details open>
        <summary>What affects these numbers</summary>
        <ul>
          {real.notes.map((note) => (
            <li key={note}>{note}</li>
          ))}
        </ul>
      </details>
    </>
  );
}
