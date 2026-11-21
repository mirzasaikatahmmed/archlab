import type { RunProgress, RunStatus } from "../lib/useWhatIfRun";
import { Icon } from "./Icon";

interface Props {
  label: string;
  status: RunStatus;
  progress: RunProgress | null;
  error: string | null;
  onRun: () => void;
  onCancel: () => void;
  disabled?: boolean;
}

const STATUS_TEXT: Record<RunStatus, string> = {
  idle: "Not run yet.",
  running: "Running…",
  finished: "Finished.",
  cancelled: "Cancelled.",
  failed: "Failed.",
};

/** Run and cancel buttons with progress, shared by the three what-if tools. */
export function WhatIfRunBar({ label, status, progress, error, onRun, onCancel, disabled }: Props) {
  const running = status === "running";
  return (
    <div className="sim-panel">
      <div className="page-actions">
        {!running && (
          <button type="button" className="primary" disabled={disabled} onClick={onRun}>
            <Icon name="play" />
            {label}
          </button>
        )}
        {running && (
          <button type="button" onClick={onCancel}>
            <Icon name="stop" />
            Cancel
          </button>
        )}
        <span role="status" className="muted">
          {STATUS_TEXT[status]}
          {running && progress ? ` ${String(progress.done)} of ${String(progress.total)} runs` : ""}
        </span>
      </div>
      {running && progress && (
        <progress max={progress.total} value={progress.done} aria-label="Analysis progress" />
      )}
      {error && <p role="alert">{error}</p>}
    </div>
  );
}
