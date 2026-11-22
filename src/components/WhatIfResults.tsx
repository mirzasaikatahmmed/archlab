import type {
  CapacityReport,
  CompareReport,
  Outcome,
  SuggestReport,
  Suggestion,
  SweepReport,
  VariantResult,
} from "../bindings";
import { LineChart } from "./LineChart";
import { ms, pct } from "./ReportView";

const n = (v: number | null) => v ?? 0;

/** A signed change; green when it is an improvement, red when it is not. */
function Delta({
  value,
  lowerIsBetter,
  unit,
}: {
  value: number | null;
  lowerIsBetter: boolean;
  unit: string;
}) {
  if (value === null || Math.abs(value) < 0.05) {
    return null;
  }
  const better = lowerIsBetter ? value < 0 : value > 0;
  return (
    <span className="delta" data-good={better}>
      {value > 0 ? "+" : ""}
      {value.toFixed(value >= 100 || value <= -100 ? 0 : 1)}
      {unit}
    </span>
  );
}

function Targets({ outcome }: { outcome: Outcome }) {
  if (outcome.error) {
    return (
      <span className="target" data-met="false">
        Could not run
      </span>
    );
  }
  return (
    <span className="target" data-met={outcome.meetsSlo}>
      {outcome.meetsSlo ? "Met" : "Missed"}
    </span>
  );
}

function CompareRow({ result }: { result: VariantResult }) {
  const { outcome, vsBaseline } = result;
  return (
    <tr>
      <th scope="row">
        {result.name}
        {result.changes.length > 0 && (
          <ul className="change-list">
            {result.changes.map((c) => (
              <li key={c}>{c}</li>
            ))}
          </ul>
        )}
      </th>
      <td>
        {n(outcome.summary.throughputRps).toFixed(1)}
        <Delta value={vsBaseline?.throughputPct ?? null} lowerIsBetter={false} unit="%" />
      </td>
      <td>
        {pct(outcome.summary.errorRate)}
        <Delta value={vsBaseline?.errorRatePoints ?? null} lowerIsBetter unit=" pts" />
      </td>
      <td>
        {ms(outcome.summary.latency.p95Ms)}
        <Delta value={vsBaseline?.p95Pct ?? null} lowerIsBetter unit="%" />
      </td>
      <td>{outcome.bottleneck ? outcome.bottleneck.label : "None"}</td>
      <td>
        <Targets outcome={outcome} />
        {outcome.error && <small className="muted"> {outcome.error}</small>}
      </td>
    </tr>
  );
}

export function CompareResults({ report }: { report: CompareReport }) {
  return (
    <div className="table-wrap">
      <table className="results">
        <caption>
          Baseline and variants (targets: p95 under {n(report.slo.p95Ms)} ms, errors under{" "}
          {pct(report.slo.maxErrorRate)})
        </caption>
        <thead>
          <tr>
            <th>Design</th>
            <th>Throughput (req/s)</th>
            <th>Errors</th>
            <th>p95 latency</th>
            <th>Slowest part</th>
            <th>Targets</th>
          </tr>
        </thead>
        <tbody>
          <CompareRow result={report.baseline} />
          {report.variants.map((v) => (
            <CompareRow key={v.id} result={v} />
          ))}
        </tbody>
      </table>
    </div>
  );
}

const trim = (v: number | null) => String(Number(n(v).toPrecision(3)));

export function SweepResults({ report }: { report: SweepReport }) {
  const first = report.points[0];
  const last = report.points[report.points.length - 1];
  const labels: [string, string] = [trim(first.value), trim(last.value)];
  return (
    <>
      <p
        role="status"
        className="callout"
        data-severity={report.firstMiss === null ? "healthy" : "warning"}
      >
        <strong>
          {report.firstMiss === null
            ? "Targets are met across the whole range"
            : `Targets are first missed at ${trim(report.firstMiss)}`}
        </strong>
        <span className="muted">Varying: {report.label}</span>
      </p>
      <div className="charts">
        <LineChart
          title="p95 latency"
          unit="ms"
          series={[
            { label: "p95", values: report.points.map((p) => n(p.outcome.summary.latency.p95Ms)) },
          ]}
          limit={{ value: n(report.slo.p95Ms), label: "Target" }}
          xLabels={labels}
        />
        <LineChart
          title="Errors"
          unit="%"
          series={[
            {
              label: "Error rate",
              values: report.points.map((p) => n(p.outcome.summary.errorRate) * 100),
            },
          ]}
          limit={{ value: n(report.slo.maxErrorRate) * 100, label: "Target" }}
          xLabels={labels}
        />
        <LineChart
          title="Throughput"
          unit="req/s"
          series={[
            {
              label: "Completed",
              values: report.points.map((p) => n(p.outcome.summary.throughputRps)),
            },
          ]}
          xLabels={labels}
        />
      </div>
      <div className="table-wrap">
        <table className="results">
          <caption>{report.label}</caption>
          <thead>
            <tr>
              <th>Value</th>
              <th>p95 latency</th>
              <th>Errors</th>
              <th>Throughput (req/s)</th>
              <th>Slowest part</th>
              <th>Targets</th>
            </tr>
          </thead>
          <tbody>
            {report.points.map((p) => (
              <tr key={p.value}>
                <th scope="row">{trim(p.value)}</th>
                <td>{ms(p.outcome.summary.latency.p95Ms)}</td>
                <td>{pct(p.outcome.summary.errorRate)}</td>
                <td>{n(p.outcome.summary.throughputRps).toFixed(1)}</td>
                <td>{p.outcome.bottleneck ? p.outcome.bottleneck.label : "None"}</td>
                <td>
                  <Targets outcome={p.outcome} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}

export function CapacityResults({ report }: { report: CapacityReport }) {
  const max = report.maxRps;
  const headroom = report.headroom;
  let headline: string;
  let severity: "healthy" | "warning" | "saturated" = "healthy";
  if (max === null || headroom === null) {
    headline = "The targets cannot be met, even at a small share of today's traffic";
    severity = "saturated";
  } else if (report.reachedSearchLimit) {
    headline = `No limit found up to ${trim(max)} req/s (${trim(headroom)}× today's traffic)`;
  } else if (headroom < 1) {
    headline = `Over capacity: the targets hold up to about ${trim(max)} req/s`;
    severity = "saturated";
  } else {
    headline = `Handles up to about ${trim(max)} req/s (${trim(headroom)}× today's traffic)`;
    severity = headroom < 1.5 ? "warning" : "healthy";
  }
  return (
    <>
      <p role="status" className="callout" data-severity={severity}>
        <strong>{headline}</strong>
        <span className="muted">
          Today: {trim(report.currentRps)} req/s, which{" "}
          {report.currentMeetsSlo ? "meets" : "misses"} the targets.
        </span>
        {report.limit && (
          <span className="muted">
            Limited by {report.limit.node}: {report.limit.reason}. Missed just above the limit:{" "}
            {report.limit.missed.join(", ") || "nothing"}.
          </span>
        )}
      </p>
      <div className="table-wrap">
        <table className="results">
          <caption>Rates tried</caption>
          <thead>
            <tr>
              <th>Requests per second</th>
              <th>p95 latency</th>
              <th>Errors</th>
              <th>Targets</th>
            </tr>
          </thead>
          <tbody>
            {[...report.probes]
              .sort((a, b) => n(a.rps) - n(b.rps))
              .map((p) => (
                <tr key={n(p.rps)}>
                  <th scope="row">{trim(p.rps)}</th>
                  <td>{ms(p.p95Ms)}</td>
                  <td>{pct(p.errorRate)}</td>
                  <td>
                    <span className="target" data-met={p.meetsSlo}>
                      {p.error ? "Could not run" : p.meetsSlo ? "Met" : "Missed"}
                    </span>
                  </td>
                </tr>
              ))}
          </tbody>
        </table>
      </div>
    </>
  );
}

interface SuggestProps {
  report: SuggestReport;
  /** Saves a suggestion as a variant of the project; absent hides the button. */
  onSave?: (suggestion: Suggestion) => void;
  saveDisabled?: boolean;
}

export function SuggestResults({ report, onSave, saveDisabled }: SuggestProps) {
  const meets = report.baseline.meetsSlo;
  return (
    <>
      <p role="status" className="callout" data-severity={meets ? "healthy" : "warning"}>
        <strong>
          {meets ? "The design already meets its targets" : "The design misses its targets"}
        </strong>
        <span className="muted">
          {report.target ? `Aimed at: ${report.target}. ` : ""}
          {report.suggestions.length === 0
            ? "No simple change improved on the current design."
            : `${String(report.suggestions.length)} changes helped, best first.`}
        </span>
      </p>
      {report.suggestions.length > 0 && (
        <div className="table-wrap">
          <table className="results">
            <caption>Suggested changes</caption>
            <thead>
              <tr>
                <th>Change</th>
                <th>Throughput (req/s)</th>
                <th>Errors</th>
                <th>p95 latency</th>
                <th>Targets</th>
                {onSave && <th aria-label="Actions" />}
              </tr>
            </thead>
            <tbody>
              {report.suggestions.map((s) => (
                <tr key={s.title}>
                  <th scope="row">{s.title}</th>
                  <td>
                    {n(s.outcome.summary.throughputRps).toFixed(1)}
                    <Delta value={s.vsBaseline.throughputPct} lowerIsBetter={false} unit="%" />
                  </td>
                  <td>
                    {pct(s.outcome.summary.errorRate)}
                    <Delta value={s.vsBaseline.errorRatePoints} lowerIsBetter unit=" pts" />
                  </td>
                  <td>
                    {ms(s.outcome.summary.latency.p95Ms)}
                    <Delta value={s.vsBaseline.p95Pct} lowerIsBetter unit="%" />
                  </td>
                  <td>
                    <Targets outcome={s.outcome} />
                  </td>
                  {onSave && (
                    <td>
                      <button
                        type="button"
                        disabled={saveDisabled}
                        aria-label={`Save ${s.title} as a variant`}
                        onClick={() => {
                          onSave(s);
                        }}
                      >
                        Save as variant
                      </button>
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}
