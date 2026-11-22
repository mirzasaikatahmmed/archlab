# ArchLab

Desktop app for capacity planning and distributed-system simulation. Fully local, no backend, no
external dependencies for end users. **All simulation results are estimates.**

> Status: Phase 2 (server profile, config templates, topology canvas). See [Roadmap](#roadmap).

## Tech stack

- [Tauri v2](https://v2.tauri.app/) + Rust core; rusqlite (bundled SQLite) + rusqlite_migration; sysinfo
- `rfd` native file dialogs; ffmpeg (external program) for WebM export; Tauri updater plugin
- tauri-specta generated TypeScript bindings (`src/bindings.ts`)
- Vite + React + TypeScript UI, pnpm; React Flow (`@xyflow/react`), react-hook-form + zod, Zustand
- ESLint (typescript-eslint), Prettier, Vitest + Testing Library; rustfmt, clippy, `cargo test`
- GitHub Actions: Linux / macOS / Windows build matrix

## Prerequisites

- Node.js >= 22 and pnpm 12 (`npm install -g pnpm`)
- Rust stable with `rustfmt` and `clippy` ([rustup](https://rustup.rs))
- Platform deps for Tauri: see the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
  (Xcode CLT on macOS, WebView2 + MSVC on Windows, WebKitGTK on Linux)

## Commands

| Command                             | What it does                               |
| ----------------------------------- | ------------------------------------------ |
| `pnpm install`                      | Install JS dependencies                    |
| `pnpm tauri dev`                    | Run the desktop app with hot reload        |
| `pnpm tauri build`                  | Build an unsigned installer/bundle         |
| `pnpm check:version`                | Check package/Tauri/Cargo versions agree   |
| `pnpm ffmpeg:fetch`                 | Fetch the pinned LGPL ffmpeg (release use) |
| `pnpm typecheck` / `lint` / `test`  | TypeScript checks, ESLint, Vitest          |
| `pnpm format` / `format:check`      | Prettier write / check                     |
| `pnpm fmt:rust:check` / `lint:rust` | rustfmt check, clippy (`-D warnings`)      |
| `pnpm test:rust`                    | Rust tests                                 |
| `pnpm bindings`                     | Regenerate `src/bindings.ts` from Rust     |
| `pnpm check`                        | Everything above (same as CI, minus build) |

## Features (Phases 2-9)

- **Projects**: create, open, save, delete (stored locally in SQLite, in the OS app-data folder).
- **Server profile**: OS, CPU cores/GHz, RAM, disk type + IOPS, network bandwidth/latency,
  virtualization, file-descriptor limit. Fill it manually or click **Detect this machine**
  (sysinfo). Detected values that are guessed (disk IOPS, network, virtualization) are listed as
  notes; they are defaults, not measurements.
- **Config templates**: Nginx, Node.js, Go, Java (Spring), HAProxy, Envoy, API gateway, CDN edge,
  PostgreSQL, MySQL, MongoDB, Cassandra, DynamoDB-style, Redis, Memcached, RabbitMQ, Kafka, Pulsar,
  SQS-style queue, Elasticsearch, S3-style storage and background worker. Embedded versioned JSON in
  `src-tauri/templates/`; pick one per node and override settings. Numbers are rough guides.
- **Node types**: client, load balancer, app server, database, cache, queue, **API gateway** (rate
  limit), **CDN** (serves a share of reads at the edge), **worker**, **object storage** (limited by
  network bandwidth) and **search**. A queue with workers after
  it is asynchronous: producers get an answer once the message is stored, and the report shows
  what is still waiting. See [ADR-014](docs/adr/ADR-014-more-node-types.md). Real mode does not
  support these three behaviours.
- **Blueprints** (sidebar): seven ready-made architectures (simple web app, read-heavy site, async
  processing, microservices, write-heavy ingestion, media and files, and a 1M+ users architecture),
  each in sizes from 10 thousand to 10 million daily users. Every size is simulated in the test suite
  and meets its targets. "Use this design" opens an editable project. See
  [ADR-015](docs/adr/ADR-015-blueprints.md).
- **Design advisor** (sidebar): describe your product (users, requests per user, read share, peak,
  what it mostly does, availability, machine size, latency and error targets); it picks an
  architecture, sizes it, simulates it and explains why. Local and deterministic, no network or AI
  service. See [ADR-016](docs/adr/ADR-016-design-advisor.md).
- **Traffic profile**: concurrent users, requests/s, read/write ratio, spike pattern (none, step,
  periodic).
- **Topology canvas**: Client, LoadBalancer, AppServer, Database, Cache, Queue nodes connected by
  edges (React Flow).
- Validation lives in Rust and runs on every save; zod schemas in the UI give instant feedback
  and are tested against the same range fixture (`src/schemas/limits.json`).

- **Simulation** (Simulate tab): deterministic, seeded discrete-event run of the open project
  (even with unsaved edits). Reports throughput, error rate, p50/p95/p99, per-node utilization,
  queueing, drops and the bottleneck. Same seed and inputs give the same report on every
  platform. Results are estimates; see [ADR-004](docs/adr/ADR-004-simulation-engine.md) for the
  model and its assumptions.
- **Design**: dark by default, light theme, or follow the system (toggle at the bottom of the sidebar).
- **Chaos** (Chaos tab): schedule faults (crash and restart, slowdown, lost workers, extra latency,
  packet loss or a full partition on a connection) and set how clients react (timeout, retries,
  wait before retry). Reports show error rate and p95 before, during and after each fault and how
  long recovery took; live runs shade the fault windows and show faults on the Stage. See
  [ADR-007](docs/adr/ADR-007-chaos-and-client-policy.md).
- **Scenarios** (sidebar): five guided stories (everything on one machine, a cache in front of the
  database, a traffic spike, a retry storm, latency is not capacity). Each step changes the design,
  explains what happens and plays it on the stage; open any step as a normal project. Every claim in
  the narration is checked by a test. Add one by dropping JSON in `src-tauri/scenarios/` (see
  [ADR-010](docs/adr/ADR-010-guided-scenarios.md)).
- **What-if** (What-if tab): ask "what if" against the current design with seeded runs. Compare
  variants (traffic, server settings, node settings, replicas, no faults) with the baseline,
  sweep one parameter and chart latency, errors and throughput, find the highest traffic that
  still meets your p95 and error targets, or ask for suggestions: likely fixes for the slowest part,
  ranked, with a button to keep any of them as a variant. Targets and variants are saved with the project. See
  [ADR-009](docs/adr/ADR-009-what-if-analysis.md). Nodes also have a Replicas setting on the canvas.
- **Calibration** (Server profile tab): benchmarks this computer (CPU speed score, synced disk
  writes, loopback latency) in a few seconds and fills the profile with measured values on request.
- **Real run** (Simulate tab): runs your topology as real localhost servers doing real CPU work and
  compares the measurements with the simulation. All nodes share this computer's cores, so it
  checks the model rather than reproducing many machines; limited to 60 s and 5,000 req/s. See
  [ADR-008](docs/adr/ADR-008-real-mode-and-calibration.md). Use a release build for meaningful
  CPU numbers: debug builds run far slower.
- **Stage** (Live run, default view): isometric scene of your topology. A crowd of users, dots flowing
  along the edges, a glow and queue pile at busy nodes, and a dashed capacity column that fills as
  utilization rises. Dots are illustrative (see [ADR-006](docs/adr/ADR-006-stage-visualization.md)).
- **Live runs** (Simulate tab, Live run): the same simulation streamed once per simulated
  second at 1x, 5x, 20x or max speed, with start/pause/resume/stop, throughput and p95 charts,
  and per-node utilization, queue and drops. A finished live run produces the same report as an
  instant run with the same seed. See [ADR-005](docs/adr/ADR-005-live-runs.md).
- **Steer a live run** ("Steer the run" panel): change the traffic (×0.1 to ×10), inject a crash,
  slowdown or lost workers on a server node, or end a fault early, while the run plays. Changes
  apply at the current simulated second and are logged in the report; "Replay with the same
  changes" reproduces the run exactly. See [ADR-013](docs/adr/ADR-013-live-controls.md).

- **Export** (Simulate tab, below the run controls): save the project as an `.archlab.json` file,
  the result as a Markdown report, the topology as an SVG diagram (coloured by status after a
  run), and a finished live run as a 1080p30 WebM video. **Import project** (sidebar) opens a
  project file as a new project. Video needs [ffmpeg](https://ffmpeg.org/) with libvpx-vp9 on
  this computer, found via `ARCHLAB_FFMPEG`, next to the app, or on `PATH`. See
  [ADR-011](docs/adr/ADR-011-export.md).

- **Uninstall** (sidebar, "Uninstall ArchLab..."): removes an installed copy after a confirmation that
  says exactly what happens (macOS: to the Trash; Windows: starts the uninstaller; Linux: deletes the
  AppImage or removes the package). Your projects are kept unless you tick the box to delete them.
  A development build or an unpacked copy is refused. See
  [ADR-017](docs/adr/ADR-017-updates-uninstall-responsive.md).
- **Any screen size:** below 900 px the sidebar becomes a menu, tabs scroll sideways and controls grow
  on touch screens (the window can be as small as 360 x 480); on wide screens the content grows, the
  Stage follows the window height, and the live run puts steering and node load beside the Stage.

### Adding a template

1. Add `src-tauri/templates/<id>.json` (copy an existing one; `id`, `version`, `nodeKinds`,
   `settings` with `default`/`min`/`max`, and a `sim` block: service time, CPU share, sigma, and
   which settings give the worker count and queue size).
2. Add it to `EMBEDDED` in `src-tauri/src/domain/template.rs` and to the id list in its test.
3. Bump a template's `version` when its settings change.

## Releases and updates

- **Cut a release:** bump the version in `package.json`, `src-tauri/tauri.conf.json` and
  `src-tauri/Cargo.toml` (`pnpm check:version` checks they agree), merge to `main`, then push a tag:
  `git tag v0.2.0 && git push origin v0.2.0`. The Release workflow builds macOS (Apple silicon and
  Intel), Windows and Linux installers and attaches them, with the update manifest, to a **draft**
  GitHub Release. Review and publish it by hand.
- **Secrets** (repository settings, Actions secrets). Only the update key is required for updates;
  the rest enable OS code signing and are optional:
  `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (update signing; generated with
  `pnpm tauri signer generate`, public half is in `tauri.conf.json`);
  `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`,
  `APPLE_PASSWORD`, `APPLE_TEAM_ID` (macOS signing and notarization);
  `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD` (Windows signing).
  Keep the private update key safe: without it, installed apps cannot be updated.
- **Unsigned builds:** macOS asks you to approve the app (System Settings, Privacy & Security, "Open
  Anyway"); Windows SmartScreen shows "More info", then "Run anyway".
- **Updates:** the app checks GitHub Releases at startup (switch off in the sidebar), shows the
  release notes and a progress bar, installs a newer signed version on request and restarts. It
  needs a **published** release (a draft has no public update file).
  See [ADR-012](docs/adr/ADR-012-packaging-and-updates.md).

## Third-party software

Windows and Linux installers include **FFmpeg** (n9.0.2, LGPL v3 or later, built by
[BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds), pinned in `scripts/ffmpeg-manifest.json`)
as the separate program `archlab-ffmpeg`, used only for WebM export. Its licence text is installed
with the app; its source is at [ffmpeg.org](https://ffmpeg.org/download.html) (tag n9.0.2). You can
use your own build by setting `ARCHLAB_FFMPEG`. On macOS, install ffmpeg yourself
(`brew install ffmpeg`).

## Project structure

```
src/                 React UI (presentation only)
  bindings.ts        GENERATED by tauri-specta (do not edit; `pnpm bindings`)
  ipc/               Typed facade over the generated commands (CommandError)
  schemas/           zod form schemas + shared range fixture (limits.json)
  store/             Zustand project store
  stage/             Isometric projection, layout, particle logic and canvas drawing
  components/        Forms, project sidebar, template picker, simulation + live panels, SVG charts, topology/ (React Flow)
src-tauri/src/
  commands/          Thin Tauri command handlers
  domain/            Pure domain types + validation (project, profiles, template, topology)
  storage/           SQLite connection, migrations, repositories
  runtime/           Host detection (sysinfo), calibration benchmarks, real mode (tokio, localhost)
  sim/               Simulation engine (model, engine, seeded RNG, report, live runs)
  whatif/            What-if runner: compare, sweep, capacity search
  export/            Project file, Markdown, SVG and ffmpeg video encoder
  ipc.rs / error.rs  Command registry + binding export, typed AppError
scripts/             Release helpers: ffmpeg fetch (pinned, checksummed), version check
src-tauri/templates/ Embedded config template JSON
src-tauri/scenarios/ Embedded guided scenario JSON
src-tauri/blueprints/ Embedded blueprint JSON (architectures in sizes)
docs/adr/            Architecture decision records
```

Rule: no business logic in the UI; commands only delegate to the Rust modules.

## Roadmap

1. Setup / CI / docs (done)
2. Profile + templates + canvas (done)
3. Simulation engine (done)
4. Load generator + live metrics (done)
5. Chaos (done)
6. Real mode + calibration (done)
7. What-if (done)
8. Scenarios (done)
9. Export (JSON, Markdown, SVG, WebM; PDF later) (done)
10. Packaging, signing, auto-update (current)

## Documentation

- [ADR index](docs/adr/README.md): ADR-001 Tauri, ADR-002 storage, ADR-003 IPC type generation, ADR-004 simulation engine, ADR-005 live runs, ADR-006 Stage, ADR-007 chaos, ADR-008 real mode, ADR-009 what-if, ADR-010 scenarios, ADR-011 export, ADR-012 packaging and updates, ADR-013 live controls, ADR-014 more node types, ADR-015 blueprints, ADR-016 design advisor, ADR-017 updates, uninstall and small screens
