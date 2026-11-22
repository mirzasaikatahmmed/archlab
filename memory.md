# ArchLab memory

## Project concept

Cross-platform desktop app (Tauri v2) for capacity planning and deterministic distributed-system
simulation. Fully local; results are always labeled estimates.

## Tech stack and architecture

Tauri v2 + Rust (single crate `src-tauri/`), Vite + React + TS UI, pnpm. Rust: commands (thin),
domain (project, server_profile, traffic, topology, template, validation), storage (rusqlite +
rusqlite_migration, `ProjectRepository`), runtime/detect (sysinfo), sim/ (model compile, engine, rng, report), ipc.rs (tauri-specta), error.rs
(`AppError`). UI: `src/bindings.ts` (GENERATED, committed), `ipc/` facade, `schemas/` zod,
`store/` Zustand, `components/` (+ `topology/` React Flow). Templates = JSON in `src-tauri/templates/`.

## Current state

Phases 1-10, live controls (ADR-013) and new node types (ADR-014) are on `main` (pushed to private GitHub repo
`mirzasaikatahmmed/archlab`, head `d54ee46`). GUI never run by hand (`pnpm tauri dev`); visuals checked only
in headless Chrome with a mocked backend; real-mode timing only in Rust tests; sim-vs-real divergence not measured by
the user. Release builds needed for meaningful calibration. Video needs a system ffmpeg (libvpx-vp9); not
yet run end to end with a real ffmpeg (none installed here; encoder tested with a fake script). First release `v0.1.0` exists as a DRAFT on GitHub (ADR-012). Updates need a public repo. Local release build

- 8 s launch OK on macOS. Scenario import/export deferred. Blueprints, the design advisor, object storage/search nodes and 10 more templates are on `feat/blueprints-advisor`, UNCOMMITTED. Next: try the app by hand, publish the draft, polish.

## Key decisions

- ESLint + Prettier + Vitest; single crate; CI build matrix, unsigned, no release until Phase 10.
- ADR-002: rusqlite 0.41 (pinned by rusqlite_migration 2.7), hybrid project row + JSON columns.
- ADR-003: tauri-specta =2.0.0-rc.29 / specta =2.0.0-rc.28, bindings committed + stale-check test.
- Forms: react-hook-form + zod; Rust re-validates on save; ranges synced via `src/schemas/limits.json`.
- Host detection lives in `runtime/detect.rs`; guessed values reported as notes.
- Templates: numeric settings plus a `sim` block (ADR-004); templates are version 2 now.
- ADR-004: discrete-event sim, virtual µs clock, ChaCha8 streams + `libm` for determinism, 5M-event cap,
  open-loop Poisson arrivals, lognormal service, every node = one machine of the server profile.
- UI design (2026-11-22): tokens in `src/index.css` (dark default, light via `data-theme` or OS), indigo accent,
  system font, hand-made SVG icons (`Icon.tsx`), `Section` groups fields, `ThemeToggle` (localStorage, try/catch).
  Status colours = healthy/warning/saturated everywhere. Don't restyle bare `button`/`input` per component.
- ADR-010: scenarios = embedded JSON (`src-tauri/scenarios/`, `domain/scenario.rs`), steps stack (topology/changes/
  addFault/setPolicy/setTraffic), hidden `expect` claims tested in cargo, player uses `useLiveRun` + Stage highlight;
  suggestions = `whatif/suggest.rs` (candidates for the bottleneck, ranked, save-as-variant). Scenario text English only.
- ADR-009: what-if = typed `Change`s applied to a project copy (`domain/whatif.rs`), `TopologyNode.replicas`, tools
  compare/sweep/capacity in `whatif/` (parallel scoped threads; capacity = doubling then bisection), SLO + variants
  in `Project.whatif` (migration 004, schema v3); UI prunes variant changes for removed nodes.
- ADR-008: real mode = tokio servers on 127.0.0.1 (16-byte frames, pooled conns), same Model/semantics as sim, real
  CPU chain + timer waits, limits 60 s/5000 rps/20 nodes, no faults; calibration (CPU/disk/loopback) stored in
  `calibrations` (migration 003), applied via Rust `apply_calibration`; `JobSlot` = one job per kind + cancel flag.
- ADR-007: faults = scheduled engine events (crash via epoch, slowdown, lost workers, edge latency/loss);
  `ClientPolicy` (timeout/retries/backoff) + faults live in `Project.chaos` (migration 002, schema v2);
  report has per-fault before/during/after + recovery; UI prunes faults whose target was removed.
- ADR-006: Stage = Canvas 2D isometric (src/stage: iso/layout/particles tested, draw.ts untested), dots are
  illustrative samples of ticks, reuses topology canvas positions, reduced-motion = static frames.
- ADR-005: live = same engine stepped per simulated second; sink-based controller (no Tauri in it);
  one live run at a time; UI buffers ticks, draws every 250 ms; leaving the Simulate tab stops the run.
- Sim does not use `concurrentUsers` yet. Reads try caches first; writes skip them; loops are rejected.

## Conventions and gotchas

- Work on feature branches; never commit to main; commit only when asked.
- Node 26 via nvm has no corepack: pnpm installed with `npm i -g pnpm`.
- vitest `beforeEach(() => mock.mockReset())` returns the mock and breaks teardown: use braces.
- After changing Rust commands/types run `pnpm bindings`; `cargo test` fails if bindings are stale.
- specta types floats as `number | null`; use `stripNulls` (src/lib/numbers.ts) for validated data, but
  not on `SimReport` (its real `null`s, e.g. `bottleneck`, would become NaN); format with `?? 0` instead.
- specta forbids 64-bit ints: use u32/f64 in IPC types (timestamps = epoch ms as f64).
- @xyflow/react 13: use `useNodesState`/`useEdgesState` (no applyNodeChanges export).
- Keep `@tauri-apps/api` on the same minor as the `tauri` crate (2.11): `tauri build` fails on mismatch.
- Export (ADR-011): `export/` = project_file, markdown, svg, video (ffmpeg child, stdin RGBA). Files are written
  only by `export::write_text` with a fixed extension; dialogs are Rust (`rfd`), not a tauri plugin (the dialog
  plugin needs tauri 2.12). `video_frame` is a raw-body command outside the typed registry (see lib.rs).
  After `drawScene` reset transform/textAlign before drawing overlays (it leaves them set).
- Packaging (ADR-012): updater plugin pinned 2.12.0 (newest compatible with tauri 2.11.6; dialog/updater latest need
  2.12). Update key at `~/.tauri/archlab-updater.key` (NOT in repo; secret `TAURI_SIGNING_PRIVATE_KEY`). Sidecar
  `archlab-ffmpeg` (Win/Linux only, LGPL, pinned in scripts/ffmpeg-manifest.json); macOS uses brew ffmpeg.
  `createUpdaterArtifacts`/`externalBin` live only in `tauri.release.conf.json`/`tauri.ffmpeg.conf.json` overlays.
- GitHub Actions re-enabled by the user (2026-11-22). CI is green on all 3 OSes (main `d54ee46`); Windows needed
  `src-tauri/windows-app.manifest` embedded in every target (build.rs) so Rust tests start. Secret `TAURI_SIGNING_PRIVATE_KEY`
  is set. Tag `v0.1.0` built a DRAFT release (4 installers sets + latest.json, all signed); not published, unsigned for
  macOS/Windows OS signing, updates need a public repo.
- `.gitattributes` forces LF: without it the Windows CI job failed `format:check` (CRLF checkout). First CI run on `main`:
  macOS green, Windows stopped at format (rest unverified), Linux build step cancelled (unknown): re-run to confirm.
- Live controls (ADR-013): `sim/control.rs` (ControlCommand/ControlRecord), `Simulation::apply_control`, `LiveRuns::control`
  waits for the run's reply; `SimReport.controls` logs them; `start_live_run(.., script, ..)` replays; `LiveEvent::Finished`
  boxes the report (clippy size lint). UI: `LiveControls`, `lib/liveFaults.ts`.
- Node types (ADR-014): `apiGateway` (token-bucket rate limit), `cdn` (read hits end at the edge), `worker`; a Queue with
  Worker children is async (`NodeModel.async_queue`, backlog in `NodeState`, jobs are `Request{job_of}`); `NodeReport.backlog`.
  Queue without workers is unchanged. Real mode refuses the new behaviours. 8 new templates (JSON only).
- Blueprints (ADR-015): `src-tauri/blueprints/*.json` + `domain/blueprint.rs`; every tier is simulated by a cargo test (meets
  SLO, nodes <85% busy). Tune with a builder script (not in repo) and `cargo test print_blueprints -- --ignored --nocapture`.
  `domain/sizing.rs` = users -> peak rps. Object storage (payload/bandwidth) and search kinds added.
- Advisor (ADR-016): `src-tauri/src/advisor/mod.rs`; rules choose the blueprint, a loop adds copies where busy >55%, the verdict
  comes from the simulated final design; refuses peak >25k rps. Native number inputs with `step` + `min` can silently block
  form submit (use `step="any"` + `noValidate`).
- Uninstall (ADR-017): `src-tauri/src/uninstall/mod.rs`; pure `choose`/`describe` tested everywhere, only installed copies
  (mac /Applications bundle, Windows uninstall.exe, Linux AppImage or dpkg-owned) are acted on; dev builds refused. Windows/
  Linux-package paths unrun. Responsive: <900px sidebar = menu (`data-nav` on `.shell`); `.tabs` needs `flex:none` or it
  collapses when scrollable. Repo is PUBLIC since 2026-11-22; v0.1.0 is still a draft (updates need a published release).
- sysinfo 0.40: `System::new()` and `Disks::new_with_refreshed_list()` return `Result`.

## Setup and run

See README. `pnpm check` runs all checks; `pnpm tauri dev` runs the app.

## Last updated

2026-11-22: Phase 10 packaging: icons, updater, release workflow, ffmpeg sidecar scripts; branch feat/phase-10-packaging.
