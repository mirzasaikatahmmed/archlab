# ADR-003: Generate TypeScript IPC bindings with tauri-specta

- Status: Accepted
- Date: 2026-11-21

## Context

The UI must contain no business logic and talk to Rust only through typed commands. Hand-written
TypeScript mirrors of Rust types drift silently.

## Decision

- Use **tauri-specta** (`=2.0.0-rc.29`) with **specta** (`=2.0.0-rc.28`) and
  **specta-typescript**. Commands are annotated with `#[specta::specta]` and registered once in
  `src-tauri/src/ipc.rs`; domain types derive `specta::Type`.
- Bindings are generated into `src/bindings.ts` and **committed**. Debug builds regenerate them on
  startup; `pnpm bindings` regenerates without launching the app.
- A Rust test (`committed_bindings_are_up_to_date`) fails in CI when `src/bindings.ts` is stale.
- Commands return `Result<T, AppError>` and use `ErrorHandlingMode::Result`, so the UI receives
  `{ status: "ok" | "error" }` with a typed `AppError` (validation issues carry field paths).
  `src/ipc/index.ts` is a thin facade that turns errors into a `CommandError`.
- `disable_serde_phases()` is set so each type has a single TypeScript shape.
- IPC numbers are `f64`/`u32` only (timestamps are epoch milliseconds as `f64`), because specta
  refuses 64-bit integers by default.

## Alternatives considered

- **Hand-written types**: no tooling, but drift is only caught at runtime.
- **ts-rs**: types only, no command signatures.

## Consequences

- Positive: one source of truth (Rust); renames break the TypeScript build immediately.
- Negative: tauri-specta and specta are release candidates, so versions are pinned with `=` and
  upgrades are deliberate. Specta renders every float as `number | null` (NaN serializes to
  `null`); `src/lib/numbers.ts` strips the null for validated profiles.
- zod schemas stay hand-written for instant form feedback and are checked against the Rust
  ranges through the shared fixture `src/schemas/limits.json` (tested on both sides). Rust remains
  the source of truth and re-validates on every save.
