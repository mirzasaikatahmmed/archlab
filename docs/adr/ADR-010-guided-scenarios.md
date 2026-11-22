# ADR-010: Guided scenarios and suggestions

- Status: Accepted
- Date: 2026-11-22

## Context

Phase 8 adds pre-built stories that teach how systems behave, in the spirit of system-design
explainers: a design, a sequence of changes, and a stage that plays each one. It also adds a
suggestions tool to the what-if analysis.

## Decision

1. **Scenarios are embedded, versioned JSON** in `src-tauri/scenarios/`, one file per scenario,
   parsed and validated once like config templates (`domain::scenario`). A scenario has a starting
   project (server, traffic, topology, faults, targets) and ordered steps. Add one by dropping a file
   in the folder and listing it in `EMBEDDED`.
2. **A step builds on the one before it.** It may replace the topology, apply the typed what-if
   changes, add a fault, set the client policy, or set the whole traffic profile. Rust builds the
   design at any step (`project_at`), so the UI never reimplements the rules. Steps also carry plain
   text narration, nodes to highlight, and how to play (live at a given speed, or instant).
3. **The narration is tested.** Every step carries hidden expectations (meets targets, error rate or
   p95 above or below a value, the bottleneck, node utilization, recovery time). A cargo test
   simulates every step with the player's own options and fails if a claim stops being true, so
   engine changes cannot silently falsify the stories.
4. **Player**: a Scenarios view reachable from the sidebar. Each step plays on the existing Stage
   through the live-run hook (extracted as `useLiveRun`), with narration and Back, Next, Replay and
   Pause. Steps play on a temporary in-memory project; "Open this step as a project" saves it as a
   normal project. Nothing is stored unless the user asks.
5. **Suggestions** (`whatif::suggest`): take the baseline bottleneck, or the busiest node, generate a
   handful of candidates (more replicas, more workers or queue from the node's template, a better
   cache hit ratio, faster CPUs, more cores, faster disks for databases), run them in parallel and
   keep the ones that improve on the baseline, ranked by missed targets, then errors, then p95. A
   suggestion can be saved as a what-if variant.
6. **Faults and the shared server profile shape the content.** All nodes use one server profile and
   a crash takes down a whole node, so the one-machine story scales out with separate nodes behind a
   load balancer that does not health-check, and says so.

## Consequences

- Scenario text is English only. Authoring and sharing scenarios is left to the import and export
  work in Phase 9.
- Live steps depend on the live-run slot: moving between steps stops the previous run and waits
  briefly for the slot to free.
- Retry and timeout lessons reflect this engine's model (for example, a slow database does not tie up
  app workers), so some real-world cascades are intentionally not shown.
