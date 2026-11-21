import { last } from "../lib/numbers";

export interface Series {
  label: string;
  values: number[];
}

/** A stretch of the x axis to shade, in sample indexes (one sample per second). */
export interface Band {
  from: number;
  to: number;
  label: string;
}

interface Props {
  title: string;
  unit: string;
  series: Series[];
  bands?: Band[];
  /** A target to draw as a dashed horizontal line. */
  limit?: { value: number; label: string };
  /** Captions for the first and last sample. */
  xLabels?: [string, string];
}

const WIDTH = 320;
const HEIGHT = 110;
const PAD = 6;

/** Minimal dependency-free SVG line chart; x is the sample index. */
export function LineChart({ title, unit, series, bands = [], limit, xLabels }: Props) {
  const length = Math.max(0, ...series.map((s) => s.values.length));
  const peak = Math.max(1e-9, limit?.value ?? 0, ...series.flatMap((s) => s.values));
  const x = (i: number) => (length <= 1 ? PAD : PAD + (i * (WIDTH - 2 * PAD)) / (length - 1));
  const y = (v: number) => HEIGHT - PAD - (v / peak) * (HEIGHT - 2 * PAD);
  const shaded = bands.filter((b) => length > 1 && b.from <= length - 1);
  const latest = series
    .map((s) => `${s.label} ${(last(s.values) ?? 0).toFixed(1)} ${unit}`)
    .join(", ");

  return (
    <figure className="chart">
      <figcaption>
        {title}{" "}
        <span className="muted">
          (peak {peak < 1e-6 ? 0 : peak.toFixed(1)} {unit})
        </span>
      </figcaption>
      <svg
        viewBox={`0 0 ${String(WIDTH)} ${String(HEIGHT)}`}
        role="img"
        aria-label={`${title}: ${latest}`}
      >
        <line
          className="chart-axis"
          x1={PAD}
          y1={HEIGHT - PAD}
          x2={WIDTH - PAD}
          y2={HEIGHT - PAD}
        />
        {shaded.map((b) => (
          <rect
            key={`${b.label}-${String(b.from)}`}
            className="chart-band"
            x={x(b.from)}
            y={PAD}
            width={Math.max(1, x(Math.min(b.to, length - 1)) - x(b.from))}
            height={HEIGHT - 2 * PAD}
          >
            <title>{b.label}</title>
          </rect>
        ))}
        {series.map((s, index) => (
          <polyline
            key={s.label}
            className={`chart-line series-${String(index)}`}
            fill="none"
            points={s.values.map((v, i) => `${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(" ")}
          />
        ))}
        {limit && (
          <line
            className="chart-limit"
            x1={PAD}
            x2={WIDTH - PAD}
            y1={y(limit.value)}
            y2={y(limit.value)}
          >
            <title>{limit.label}</title>
          </line>
        )}
      </svg>
      {xLabels && (
        <div className="chart-x muted">
          <span>{xLabels[0]}</span>
          <span>{xLabels[1]}</span>
        </div>
      )}
      <ul className="legend">
        {series.map((s, index) => (
          <li key={s.label} className={`series-${String(index)}`}>
            {s.label}
          </li>
        ))}
        {shaded.length > 0 && <li className="band-key">Fault window</li>}
        {limit && <li className="limit-key">{limit.label}</li>}
      </ul>
    </figure>
  );
}
