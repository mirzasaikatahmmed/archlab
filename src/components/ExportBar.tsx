import { useEffect, useRef, useState } from "react";
import type { ExportKind, LiveTick, Project, SimReport } from "../bindings";
import {
  MAX_VIDEO_SECONDS,
  VIDEO,
  createFrameSource,
  encodeVideo,
  type VideoSink,
} from "../export/video";
import { api, CommandError } from "../ipc";

interface Props {
  project: Project;
  /** Result of the last run; exports include it when present. */
  report: SimReport | null;
  /** Ticks of a finished live run; the video option needs them. */
  ticks?: readonly LiveTick[];
}

const EXTENSIONS: Record<ExportKind, string> = {
  json: "archlab.json",
  markdown: "md",
  svg: "svg",
  video: "webm",
};

function fileSafe(name: string): string {
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug === "" ? "archlab" : slug;
}

function messageOf(err: unknown): string {
  if (err instanceof CommandError || err instanceof Error) {
    return err.message;
  }
  return "Something went wrong while saving.";
}

/** Saves the project, its report and a diagram to files; offers a video after a live run. */
export function ExportBar({ project, report, ticks }: Props) {
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [ffmpeg, setFfmpeg] = useState<boolean | null>(null);
  const cancelled = useRef(false);

  useEffect(() => {
    let alive = true;
    api
      .videoAvailable()
      .then((found) => {
        if (alive) {
          setFfmpeg(found);
        }
      })
      .catch(() => {
        if (alive) {
          setFfmpeg(false);
        }
      });
    return () => {
      alive = false;
    };
  }, []);

  const attempt = async (job: () => Promise<string | null>) => {
    setBusy(true);
    setNote(null);
    setError(null);
    try {
      const saved = await job();
      if (saved !== null) {
        setNote(saved);
      }
    } catch (err) {
      setError(messageOf(err));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const saveText = (kind: Exclude<ExportKind, "video">) =>
    attempt(async () => {
      const path = await api.pickSavePath(kind, `${fileSafe(project.name)}.${EXTENSIONS[kind]}`);
      if (path === null) {
        return null;
      }
      if (kind === "json") {
        await api.exportProjectJson(path, project, report);
      } else if (kind === "markdown") {
        await api.exportMarkdown(path, project, report);
      } else {
        await api.exportSvg(path, project, report);
      }
      return `Saved to ${path}`;
    });

  const saveVideo = () =>
    attempt(async () => {
      if (!ticks || ticks.length === 0) {
        return null;
      }
      const path = await api.pickSavePath("video", `${fileSafe(project.name)}.webm`);
      if (path === null) {
        return null;
      }
      cancelled.current = false;
      const sink: VideoSink = {
        start: async () => {
          await api.videoStart(path, VIDEO.width, VIDEO.height, VIDEO.fps);
        },
        frame: (rgba) => api.videoFrame(rgba),
        finish: () => api.videoFinish(),
        cancel: async () => {
          await api.videoCancel();
        },
      };
      const outcome = await encodeVideo(
        createFrameSource({
          title: project.name,
          topology: project.topology,
          faults: project.chaos.faults,
          ticks,
          slo: project.whatif.slo,
        }),
        sink,
        (done, total) => {
          setProgress({ done, total });
        },
        () => cancelled.current,
      );
      return outcome.status === "done" ? `Saved to ${path}` : "Video cancelled.";
    });

  const videoReady = ticks !== undefined && ticks.length > 0;
  const tooLong = ticks !== undefined && ticks.length > MAX_VIDEO_SECONDS;
  const videoDisabled = busy || !videoReady || tooLong || ffmpeg !== true;

  let videoHint: string | null = null;
  if (!videoReady) {
    videoHint = "Finish a live run to save it as a video.";
  } else if (tooLong) {
    videoHint = `Video is limited to ${String(MAX_VIDEO_SECONDS)} s of simulated time.`;
  } else if (ffmpeg === false) {
    videoHint = "Video needs ffmpeg. Install it, or set ARCHLAB_FFMPEG to its path.";
  }

  return (
    <section className="export-bar" aria-label="Export">
      <div className="export-actions">
        <button
          type="button"
          disabled={busy}
          onClick={() => {
            void saveText("json");
          }}
        >
          Save project file
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => {
            void saveText("markdown");
          }}
        >
          Save report (Markdown)
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => {
            void saveText("svg");
          }}
        >
          Save diagram (SVG)
        </button>
        <button
          type="button"
          disabled={videoDisabled}
          onClick={() => {
            void saveVideo();
          }}
        >
          Save video (WebM)
        </button>
        {progress && (
          <>
            <progress max={progress.total} value={progress.done} aria-label="Video progress" />
            <button
              type="button"
              onClick={() => {
                cancelled.current = true;
              }}
            >
              Cancel video
            </button>
          </>
        )}
      </div>
      {!report && (
        <p className="muted">Run a simulation first to include its results in the exports.</p>
      )}
      {videoHint && <p className="muted">{videoHint}</p>}
      {note && <p role="status">{note}</p>}
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
