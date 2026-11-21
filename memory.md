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

Phases 1-3 committed (`feat/phase-3-sim-engine`, unpushed). Everything since is UNCOMMITTED in the working
tree, stacked on branches phase-4-live-metrics -> stage-visualization -> `feat/phase-5-chaos` (current):
live runs (steppable `Simulation`, `sim/live.rs`, Channel commands), UI redesign, isometric Stage, chaos
(faults, client policy, impact report, Chaos tab). GUI never run by hand (`pnpm tauri dev`); visuals were
checked only in headless Chrome with a mocked backend. Next: Phase 6 (real mode + calibration).

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
- sysinfo 0.40: `System::new()` and `Disks::new_with_refreshed_list()` return `Result`.

## Setup and run

See README. `pnpm check` runs all checks; `pnpm tauri dev` runs the app.

## Last updated

2026-11-22: Phase 5 chaos (faults, client policy, impact report, Chaos tab, Stage visuals).
