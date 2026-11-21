# ArchLab memory

## Project concept

Cross-platform desktop app (Tauri v2) for capacity planning and deterministic distributed-system
simulation. Fully local; results are always labeled estimates.

## Tech stack and architecture

Tauri v2 + Rust (single crate `src-tauri/`), Vite + React + TS UI, pnpm. Rust: commands (thin),
domain (project, server_profile, traffic, topology, template, validation), storage (rusqlite +
rusqlite_migration, `ProjectRepository`), runtime/detect (sysinfo), ipc.rs (tauri-specta), error.rs
(`AppError`). UI: `src/bindings.ts` (GENERATED, committed), `ipc/` facade, `schemas/` zod,
`store/` Zustand, `components/` (+ `topology/` React Flow). Templates = JSON in `src-tauri/templates/`.

## Current state

Phase 1 done. Phase 2 implemented on branch `feat/phase-2-profile-canvas` (nothing committed; Phase 1
also uncommitted). Commands: app_info, detect_machine, list_templates, list/get/create/save/delete
project. Not yet verified by running the GUI (`pnpm tauri dev`). Next: Phase 3 (sim engine).

## Key decisions

- ESLint + Prettier + Vitest; single crate; CI build matrix, unsigned, no release until Phase 10.
- ADR-002: rusqlite 0.41 (pinned by rusqlite_migration 2.7), hybrid project row + JSON columns.
- ADR-003: tauri-specta =2.0.0-rc.29 / specta =2.0.0-rc.28, bindings committed + stale-check test.
- Forms: react-hook-form + zod; Rust re-validates on save; ranges synced via `src/schemas/limits.json`.
- Host detection lives in `runtime/detect.rs`; guessed values reported as notes.
- Template settings are numeric only (id, version, min/max); simulation parameters come in Phase 3.

## Conventions and gotchas

- Work on feature branches; never commit to main; commit only when asked.
- Node 26 via nvm has no corepack: pnpm installed with `npm i -g pnpm`.
- vitest `beforeEach(() => mock.mockReset())` returns the mock and breaks teardown: use braces.
- After changing Rust commands/types run `pnpm bindings`; `cargo test` fails if bindings are stale.
- specta types floats as `number | null`; use `stripNulls` (src/lib/numbers.ts) for validated data.
- specta forbids 64-bit ints: use u32/f64 in IPC types (timestamps = epoch ms as f64).
- @xyflow/react 13: use `useNodesState`/`useEdgesState` (no applyNodeChanges export).
- Keep `@tauri-apps/api` on the same minor as the `tauri` crate (2.11): `tauri build` fails on mismatch.
- sysinfo 0.40: `System::new()` and `Disks::new_with_refreshed_list()` return `Result`.

## Setup and run

See README. `pnpm check` runs all checks; `pnpm tauri dev` runs the app.

## Last updated

2026-11-21: Phase 2 (profile, templates, traffic, canvas, storage, bindings, ADR-002/003).
