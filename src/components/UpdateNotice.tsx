import { useCallback, useEffect, useRef, useState } from "react";
import { api, type AvailableUpdate } from "../ipc";

const AUTO_KEY = "archlab.autoUpdateCheck";

type State =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current" }
  | { kind: "available"; update: AvailableUpdate }
  | { kind: "installing"; version: string }
  | { kind: "installed"; version: string }
  | { kind: "failed"; message: string };

/** Startup checks are on unless the person turned them off; a blocked store means on. */
function readAuto(): boolean {
  try {
    return localStorage.getItem(AUTO_KEY) !== "off";
  } catch {
    return true;
  }
}

function saveAuto(on: boolean): void {
  try {
    localStorage.setItem(AUTO_KEY, on ? "on" : "off");
  } catch {
    // Storage can be blocked; the choice then lasts until the app is closed.
  }
}

/** Looks for a newer version on request, and at startup unless switched off. */
export function UpdateNotice() {
  const [state, setState] = useState<State>({ kind: "idle" });
  const [auto, setAuto] = useState(readAuto);
  const started = useRef(false);

  const settle = useCallback((pending: Promise<AvailableUpdate | null>, quiet: boolean) => {
    pending.then(
      (update) => {
        setState(update ? { kind: "available", update } : { kind: "current" });
      },
      (err: unknown) => {
        // A failed startup check says nothing: the person did not ask for it.
        setState(
          quiet
            ? { kind: "idle" }
            : {
                kind: "failed",
                message: err instanceof Error ? err.message : "Could not check for updates.",
              },
        );
      },
    );
  }, []);

  useEffect(() => {
    if (started.current) {
      return;
    }
    started.current = true;
    if (auto) {
      settle(api.checkForUpdate(), true);
    }
  }, [auto, settle]);

  const install = async (update: AvailableUpdate) => {
    setState({ kind: "installing", version: update.version });
    try {
      await update.install();
      setState({ kind: "installed", version: update.version });
    } catch (err) {
      setState({
        kind: "failed",
        message: err instanceof Error ? err.message : "The update could not be installed.",
      });
    }
  };

  return (
    <div className="update-notice">
      {state.kind === "available" && (
        <>
          <p role="status">Version {state.update.version} is available.</p>
          <button
            type="button"
            className="primary"
            onClick={() => {
              void install(state.update);
            }}
          >
            Install update
          </button>
        </>
      )}
      {state.kind === "installing" && <p role="status">Installing {state.version}…</p>}
      {state.kind === "installed" && (
        <p role="status">Version {state.version} is installed. Restart ArchLab to use it.</p>
      )}
      {state.kind === "current" && <p role="status">ArchLab is up to date.</p>}
      {state.kind === "failed" && <p role="alert">{state.message}</p>}
      <button
        type="button"
        disabled={state.kind === "checking" || state.kind === "installing"}
        onClick={() => {
          setState({ kind: "checking" });
          settle(api.checkForUpdate(), false);
        }}
      >
        Check for updates
      </button>
      <label className="check">
        <input
          type="checkbox"
          checked={auto}
          onChange={(e) => {
            setAuto(e.target.checked);
            saveAuto(e.target.checked);
          }}
        />
        Check at startup
      </label>
    </div>
  );
}
