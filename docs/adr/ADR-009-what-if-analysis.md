# ADR-009: What-if analysis

- Status: Accepted
- Date: 2026-11-22

## Context

After a baseline simulation, users ask: what if traffic doubles, what if I add replicas, where does
this design stop meeting its targets? Phase 7 answers those with the existing deterministic engine.

## Decision

1. **Variants are typed changes to a copy of the project**, never edits to it: traffic scale or rate,
   any server-profile number, a node's template setting, a node's replicas, or removing injected
   faults (`domain::whatif`). `apply_changes` builds the copy and runs the normal project validation,
   so a variant is valid exactly when the project it produces is valid. Structural changes (adding a
   node) are out of scope; duplicate the project for those.
2. **Replicas are a node property** (`TopologyNode.replicas`, default 1, 1 to 64). Each replica is a
   machine of the server profile behind round-robin, so workers, waiting room and file descriptors
   multiply. This also gives the canvas a Replicas field, and real mode inherits it.
3. **Three tools**, each a set of seeded simulation runs:
   - Compare: the baseline and up to 20 saved variants, with deltas against the baseline.
   - Sweep: one parameter across 2 to 24 values (traffic geometrically, others linearly, whole
     numbers rounded and de-duplicated).
   - Capacity: double the traffic until the targets are missed (up to 32x), then bisect; if the
     design already misses them, search downward from 1x to 0.02x.
4. **Targets (SLO)**: a p95 limit and an error-rate limit, saved with the project. A run meets them
   when it completed requests and is within both limits.
5. **Execution**: runs are independent, so Compare and Sweep use all cores through scoped threads;
   Capacity is sequential because each probe depends on the last. A job slot gives one analysis at a
   time with a cancel flag and progress events through a typed channel, like the other background jobs.
   A run that cannot finish (for example it needs more than 5 million events) becomes a failed outcome
   for that point instead of failing the analysis.
6. **Stored with the project**: a `whatif` JSON column (migration 004, project schema version 3).
   Rust validates by applying every variant; the UI mirrors the ranges with zod and the shared
   `limits.json` fixture is tested from both sides. Removing a node on the canvas also removes the
   variant changes aimed at it.

## Consequences

- One seed and one run per point: results are repeatable but carry the noise of a single run.
  Probes near the capacity limit can shift slightly with the seed.
- Not included: automatic suggestions, and changing traffic or injecting faults while a live run
  plays. Both are follow-ups.
