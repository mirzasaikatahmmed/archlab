import { useCallback, useEffect, useRef, useState } from "react";
import type { LiveEvent, LiveTick, Project, SimOptions, SimReport } from "../bindings";
import { CommandError, api, describeError } from "../ipc";

export type LiveStatus = "idle" | "running" | "paused" | "finished" | "stopped" | "failed";

/** How often buffered ticks are drawn; the engine can emit far more at max speed. */
const FLUSH_MS = 250;
/** A previous run releases its slot a moment after it reports that it stopped. */
const BUSY_RETRIES = 20;
const BUSY_WAIT_MS = 100;

/** Ending a run that already finished is not an error. */
async function stopQuietly(): Promise<void> {
  try {
    await api.controlLiveRun({ kind: "stop" });
  } catch (err) {
    if (!(err instanceof CommandError && err.error.kind === "simulation")) {
      throw err;
    }
  }
}

const sleep = (ms: number) =>
  new Promise<void>((resolve) => {
    setTimeout(resolve, ms);
  });

/** Runs a project live and keeps its ticks, report and status; events are drawn in batches. */
export function useLiveRun(project: Project) {
  const [status, setStatus] = useState<LiveStatus>("idle");
  const [ticks, setTicks] = useState<LiveTick[]>([]);
  const [report, setReport] = useState<SimReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const buffer = useRef<LiveEvent[]>([]);
  const active = status === "running" || status === "paused";
  const activeRef = useRef(active);

  useEffect(() => {
    activeRef.current = active;
  }, [active]);

  const flush = useCallback(() => {
    const events = buffer.current.splice(0);
    if (events.length === 0) {
      return;
    }
    const fresh: LiveTick[] = [];
    for (const event of events) {
      switch (event.kind) {
        case "tick":
          fresh.push(event.tick);
          break;
        case "finished":
          setReport(event.report);
          setStatus("finished");
          break;
        case "stopped":
          setStatus("stopped");
          break;
        case "failed":
          setError(event.message);
          setStatus("failed");
          break;
      }
    }
    if (fresh.length > 0) {
      setTicks((prev) => [...prev, ...fresh]);
    }
  }, []);

  useEffect(() => {
    if (!active) {
      return;
    }
    const id = setInterval(flush, FLUSH_MS);
    return () => {
      clearInterval(id);
      flush();
    };
  }, [active, flush]);

  useEffect(
    () => () => {
      if (activeRef.current) {
        void stopQuietly();
      }
    },
    [],
  );

  const showError = (err: unknown) => {
    setError(
      err instanceof CommandError ? describeError(err.error) : "The live run failed unexpectedly",
    );
  };

  /** Starts a run. With `waitForFree`, a run that is still shutting down is waited for. */
  const start = async (options: SimOptions, speed: number | null, waitForFree = false) => {
    setError(null);
    setReport(null);
    setTicks([]);
    buffer.current = [];
    const attempts = waitForFree ? BUSY_RETRIES : 1;
    for (let attempt = 1; ; attempt++) {
      try {
        await api.startLiveRun(project, options, speed, (event) => {
          buffer.current.push(event);
        });
        setStatus("running");
        return;
      } catch (err) {
        const busy =
          err instanceof CommandError &&
          err.error.kind === "simulation" &&
          err.error.message.includes("already in progress");
        if (busy && attempt < attempts) {
          await sleep(BUSY_WAIT_MS);
          continue;
        }
        showError(err);
        setStatus("failed");
        return;
      }
    }
  };

  const control = async (command: Parameters<typeof api.controlLiveRun>[0]) => {
    try {
      await api.controlLiveRun(command);
    } catch (err) {
      showError(err);
    }
  };

  const pause = async () => {
    await control({ kind: "pause" });
    setStatus("paused");
  };

  const resume = async () => {
    await control({ kind: "resume" });
    setStatus("running");
  };

  return {
    status,
    active,
    ticks,
    report,
    error,
    start,
    pause,
    resume,
    stop: () => control({ kind: "stop" }),
    setSpeed: (speed: number | null) => control({ kind: "setSpeed", speed }),
  };
}
