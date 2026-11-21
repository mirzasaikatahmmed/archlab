import { useEffect, useRef, useState } from "react";
import type { Project, SimOptions, WhatIfRequest, WhatIfResult } from "../bindings";
import { CommandError, api, describeError } from "../ipc";

export type RunStatus = "idle" | "running" | "finished" | "cancelled" | "failed";

export interface RunProgress {
  done: number;
  total: number;
}

/** Starts a what-if analysis in the background and tracks its progress and result. */
export function useWhatIfRun(project: Project, options: SimOptions) {
  const [status, setStatus] = useState<RunStatus>("idle");
  const [progress, setProgress] = useState<RunProgress | null>(null);
  const [result, setResult] = useState<WhatIfResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const running = status === "running";
  const runningRef = useRef(false);

  useEffect(() => {
    runningRef.current = running;
  }, [running]);

  useEffect(
    () => () => {
      if (runningRef.current) {
        api.cancelWhatIf().catch((err: unknown) => {
          if (!(err instanceof CommandError && err.error.kind === "runtime")) {
            throw err;
          }
        });
      }
    },
    [],
  );

  const fail = (err: unknown) => {
    setError(
      err instanceof CommandError ? describeError(err.error) : "The analysis failed unexpectedly",
    );
  };

  const start = async (request: WhatIfRequest) => {
    setError(null);
    setResult(null);
    setProgress(null);
    setStatus("running");
    try {
      await api.startWhatIf(project, options, request, (event) => {
        switch (event.kind) {
          case "progress":
            setProgress({ done: event.done, total: event.total });
            break;
          case "finished":
            setResult(event.result);
            setStatus("finished");
            break;
          case "cancelled":
            setStatus("cancelled");
            break;
          case "failed":
            setError(event.message);
            setStatus("failed");
            break;
        }
      });
    } catch (err) {
      fail(err);
      setStatus("failed");
    }
  };

  const cancel = async () => {
    try {
      await api.cancelWhatIf();
    } catch (err) {
      fail(err);
    }
  };

  return { status, running, progress, result, error, start, cancel };
}
