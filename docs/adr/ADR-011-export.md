# ADR-011: Export (JSON, Markdown, SVG, WebM)

- Status: Accepted
- Date: 2026-11-22

## Context

Phase 9 lets people take a design and its results out of the app: a project file they can
re-import, a Markdown report, an SVG diagram, and a video of a live run.

## Decision

1. **Text exporters are pure Rust functions** (`export::project_file`, `markdown`, `svg`): project
   and optional report in, string out. They add no dependency and are unit-tested. Markdown and SVG
   always carry the "estimate, not a measurement" note when a result is attached.
2. **Project file** (`.archlab.json`): `{ format: "archlab-project", schemaVersion: 1, project,
report? }`. Import checks the format and version (a newer file asks the user to update), gives the
   project a **new id** so it never overwrites an existing one, and runs `Project::validate`.
   Files over 16 MiB are refused.
3. **File access is narrow.** The webview never picks a path itself: Rust opens the native dialog
   (`rfd`) and the write commands accept only the extension their exporter produces. A new
   dependency, `rfd 0.15`, was chosen over `tauri-plugin-dialog`: the plugin's latest releases
   require `tauri` 2.12 (and a file-system plugin), while the project is on 2.11 with matching JS
   packages. `rfd` is the library the plugin wraps and has no tauri coupling.
4. **Video is rendered in the webview and encoded by ffmpeg.** The frames come from the Stage's own
   drawing code (`stage/draw.ts`) run off screen at 1920x1080 and a fixed 30 fps, driven by the
   recorded per-second ticks of a finished live run (one simulated second = one second of video, up
   to 600 s). A fixed time step makes the same run give the same video. Each raw RGBA frame
   (8 MB) is sent to Rust as a raw IPC body (`video_frame`) and written to ffmpeg's stdin; ffmpeg
   encodes VP9 (`libvpx-vp9`, CRF 32) to WebM. Commands are async so a slow encoder never blocks
   the UI thread. Cancel or any failure kills ffmpeg and removes the partial file.
   `video_frame` takes a raw `Request`, which specta cannot type, so it is registered beside the
   generated handler and called by name from `src/ipc/index.ts`.
5. **ffmpeg is a separate program**, never linked in, which keeps the app's license simple (ffmpeg
   builds with `libvpx` are LGPL or GPL depending on the build). It is found, in order, at
   `ARCHLAB_FFMPEG`, next to the app executable (where a bundled sidecar goes), then on `PATH`.
   The video button explains when none is found.

## Consequences

- Video works today with any ffmpeg that has libvpx-vp9 (`brew install ffmpeg`, a distro package,
  or a static build). **Bundling one per platform is deferred to Phase 10**: Tauri's `externalBin`
  requires the binary to exist at build time on every platform, so it belongs with packaging, where
  the per-OS builds, sizes (about 30-80 MB) and license notices are decided together.
- Encoding speed depends on the machine: frames render in about 4 ms, VP9 at `-cpu-used 4` is
  usually the slower side.
- PDF export and scenario import/export are not part of this phase.
