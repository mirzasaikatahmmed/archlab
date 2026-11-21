import { useEffect, useState } from "react";
import type { Calibration, ServerProfile } from "../bindings";
import { CommandError, api, describeError } from "../ipc";
import { Icon } from "./Icon";
import { Section } from "./Section";

interface Props {
  profile: ServerProfile;
  onApply: (profile: ServerProfile) => void;
}

interface Step {
  name: string;
  index: number;
  total: number;
}

const failure = (err: unknown) =>
  err instanceof CommandError ? describeError(err.error) : "Calibration failed unexpectedly";

const n = (v: number | null) => v ?? 0;

/** Measures this computer and offers the numbers for the server profile. */
export function CalibrationSection({ profile, onApply }: Props) {
  const [latest, setLatest] = useState<Calibration | null>(null);
  const [running, setRunning] = useState(false);
  const [step, setStep] = useState<Step | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    api
      .latestCalibration()
      .then((c) => {
        if (live) {
          setLatest(c);
        }
      })
      .catch((err: unknown) => {
        if (live) {
          setError(failure(err));
        }
      });
    return () => {
      live = false;
    };
  }, []);

  const start = async () => {
    setError(null);
    setStep(null);
    setRunning(true);
    try {
      await api.startCalibration((event) => {
        switch (event.kind) {
          case "step":
            setStep({ name: event.name, index: event.index, total: event.total });
            break;
          case "finished":
            setLatest(event.calibration);
            setRunning(false);
            break;
          case "cancelled":
            setRunning(false);
            break;
          case "failed":
            setError(event.message);
            setRunning(false);
            break;
        }
      });
    } catch (err) {
      setError(failure(err));
      setRunning(false);
    }
  };

  const cancel = async () => {
    try {
      await api.cancelCalibration();
    } catch (err) {
      setError(failure(err));
    }
  };

  const apply = async () => {
    try {
      onApply(await api.applyCalibration(profile));
    } catch (err) {
      setError(failure(err));
    }
  };

  return (
    <Section
      title="Calibration"
      description="Measure this computer instead of guessing. It takes a few seconds."
    >
      <div className="calibration">
        <div className="page-actions">
          {!running && (
            <button
              type="button"
              onClick={() => {
                void start();
              }}
            >
              <Icon name="detect" />
              Calibrate this machine
            </button>
          )}
          {running && (
            <button
              type="button"
              onClick={() => {
                void cancel();
              }}
            >
              <Icon name="stop" />
              Cancel
            </button>
          )}
          {running && (
            <span role="status" className="muted">
              {step
                ? `${step.name} (${String(step.index)} of ${String(step.total)})…`
                : "Starting…"}
            </span>
          )}
        </div>
        {error && <p role="alert">{error}</p>}
        {latest && (
          <div className="calibration-result">
            <dl className="summary">
              <div>
                <dt>CPU speed score</dt>
                <dd>{n(latest.equivalentGhz).toFixed(2)} GHz</dd>
              </div>
              <div>
                <dt>Disk writes</dt>
                <dd>{Math.round(n(latest.diskIops)).toLocaleString()} IOPS</dd>
              </div>
              <div>
                <dt>Loopback latency</dt>
                <dd>{n(latest.loopbackLatencyMs).toFixed(3)} ms</dd>
              </div>
            </dl>
            <p className="muted">Measured {new Date(n(latest.measuredAt)).toLocaleString()}</p>
            {latest.debugBuild && (
              <p role="status" className="callout" data-severity="warning">
                These numbers came from a development build, which is much slower than the installed
                app. Run a release build for real figures.
              </p>
            )}
            <ul className="notes" aria-label="Calibration notes">
              {latest.notes.map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
            <div>
              <button
                type="button"
                className="primary"
                onClick={() => {
                  void apply();
                }}
              >
                Apply to this profile
              </button>
            </div>
          </div>
        )}
      </div>
    </Section>
  );
}
