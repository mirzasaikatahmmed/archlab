# ADR-004: Deterministic discrete-event simulation engine

- Status: Accepted
- Date: 2026-11-22

## Context

ArchLab must report bottlenecks, saturation and p95/p99 latency for a topology, and the same
inputs must always give the same answer (reproducible reports, testable engine, later
what-if comparisons).

## Decision

1. **Discrete-event simulation** in `src-tauri/src/sim/`. Individual requests flow through
   multi-server FIFO queues. An analytical model (M/M/c formulas) was rejected: it gives no real
   percentiles, bursts or drops.
2. **Virtual clock**, integer microseconds. Event order never depends on float rounding; ties
   break by insertion order. A 60 s scenario runs in milliseconds. Real-time pacing for the live
   view belongs to Phase 4.
3. **Seeded RNG**: `rand_chacha` ChaCha8, one independent stream per purpose (arrivals, request
   attributes, each node's service times), so a change in one place does not shift the others.
   Only `next_u64` is used; floats are built from it by our own code.
4. **`libm`** for `ln`, `exp` and `cos`. The system math library can round differently per
   operating system, which would break cross-platform determinism.
5. **Model**: Poisson arrivals (thinning for spikes), lognormal service times, per-node worker
   count and waiting room, drops when full, 10 s request timeout. Template JSON carries a `sim`
   block (service time, CPU/IO split, sigma, which settings give workers and queue size); service
   times scale with CPU GHz and disk IOPS of the server profile. Nodes without a template use
   per-kind defaults. Every node is one machine of the project's server profile.
6. **Limits**: 5 million events per run, then a clear error. Duration is capped at 1 hour.
7. Reports are labeled as estimates and list the modelling assumptions of the run.

## Consequences

- Reproducible results; tests can pin theory (M/G/1 mean latency) and exact repeats.
- Two new dependencies: `rand_chacha`, `libm`.
- Template versions bumped to 2 when the `sim` block was added; projects saved with version 1
  template references must re-select the template.
- Not modelled yet: retries, circuit breakers, per-node hardware, closed-loop users, RAM.
