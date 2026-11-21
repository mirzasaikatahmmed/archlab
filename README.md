# ArchLab

Desktop app for capacity planning and distributed-system simulation. Fully local, no backend, no
external dependencies for end users. **All simulation results are estimates.**

> Status: Phase 2 (server profile, config templates, topology canvas). See [Roadmap](#roadmap).

## Tech stack

- [Tauri v2](https://v2.tauri.app/) + Rust core; rusqlite (bundled SQLite) + rusqlite_migration; sysinfo
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
| `pnpm typecheck` / `lint` / `test`  | TypeScript checks, ESLint, Vitest          |
| `pnpm format` / `format:check`      | Prettier write / check                     |
| `pnpm fmt:rust:check` / `lint:rust` | rustfmt check, clippy (`-D warnings`)      |
| `pnpm test:rust`                    | Rust tests                                 |
| `pnpm bindings`                     | Regenerate `src/bindings.ts` from Rust     |
| `pnpm check`                        | Everything above (same as CI, minus build) |

## Features (Phases 2-5)

- **Projects**: create, open, save, delete (stored locally in SQLite, in the OS app-data folder).
- **Server profile**: OS, CPU cores/GHz, RAM, disk type + IOPS, network bandwidth/latency,
  virtualization, file-descriptor limit. Fill it manually or click **Detect this machine**
  (sysinfo). Detected values that are guessed (disk IOPS, network, virtualization) are listed as
  notes; they are defaults, not measurements.
- **Config templates**: Nginx, Node.js, PostgreSQL, Redis. Embedded versioned JSON in
  `src-tauri/templates/`; pick one per node and override settings.
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
- **Stage** (Live run, default view): isometric scene of your topology. A crowd of users, dots flowing
  along the edges, a glow and queue pile at busy nodes, and a dashed capacity column that fills as
  utilization rises. Dots are illustrative (see [ADR-006](docs/adr/ADR-006-stage-visualization.md)).
- **Live runs** (Simulate tab, Live run): the same simulation streamed once per simulated
  second at 1x, 5x, 20x or max speed, with start/pause/resume/stop, throughput and p95 charts,
  and per-node utilization, queue and drops. A finished live run produces the same report as an
  instant run with the same seed. See [ADR-005](docs/adr/ADR-005-live-runs.md).

### Adding a template

1. Add `src-tauri/templates/<id>.json` (copy an existing one; `id`, `version`, `nodeKinds`,
   `settings` with `default`/`min`/`max`, and a `sim` block: service time, CPU share, sigma, and
   which settings give the worker count and queue size).
2. Add it to `EMBEDDED` in `src-tauri/src/domain/template.rs` and to the id list in its test.
3. Bump a template's `version` when its settings change.

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
  runtime/           Host detection (sysinfo); embedded real mode later
  sim/               Simulation engine (model, engine, seeded RNG, report, live runs)
  chaos/ export/     Later phases
  ipc.rs / error.rs  Command registry + binding export, typed AppError
src-tauri/templates/ Embedded config template JSON
docs/adr/            Architecture decision records
```

Rule: no business logic in the UI; commands only delegate to the Rust modules.

## Roadmap

1. Setup / CI / docs (done)
2. Profile + templates + canvas (done)
3. Simulation engine (done)
4. Load generator + live metrics (done)
5. Chaos (current)
6. Real mode + calibration
7. What-if
8. Scenarios
9. Export (JSON, Markdown, SVG, WebM; PDF later)
10. Packaging, signing, auto-update

## Documentation

- [ADR index](docs/adr/README.md): ADR-001 Tauri, ADR-002 storage, ADR-003 IPC type generation, ADR-004 simulation engine, ADR-005 live runs, ADR-006 Stage, ADR-007 chaos
