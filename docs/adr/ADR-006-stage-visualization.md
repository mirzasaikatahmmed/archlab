# ADR-006: Isometric Stage drawn on a 2D canvas

- Status: Accepted
- Date: 2026-11-22

## Context

Live runs needed a visual that shows a request crowd flowing into machines and a node
approaching its capacity, in the spirit of system-design explainer videos.

## Decision

1. **Hand-written Canvas 2D** with an isometric projection (`src/stage/`). No new dependency.
   Three.js was rejected: about 600 KB and a scene graph for a flat, diagram-like view.
2. **Illustrative flow, not real requests.** Dots are sampled from the per-second live ticks
   (offered rate, utilization, queue, drops). The engine and IPC are unchanged, and the Stage
   says so in its legend. Streaming real per-request events was rejected for now: heavy
   for little added insight.
3. **Pure logic split from drawing**: `iso.ts` (projection, fitting), `layout.ts`,
   `particles.ts` (seeded particle world) are unit tested; `draw.ts` only paints.
4. **Layout reuses the topology canvas positions**, so the scene matches what was drawn.
5. **Capacity is shown as a dashed column**; its fill rises with utilization and the colour
   follows the same healthy / warning / saturated bands as the rest of the app.
6. Respects `prefers-reduced-motion` (static frame redrawn on each tick); animation stops when
   the run is not running.

## Consequences

- Rendering is untested in CI (jsdom has no canvas); the pure modules are.
- Thresholds for the colour bands live in `lib/severity.ts`, mirroring the engine's.
