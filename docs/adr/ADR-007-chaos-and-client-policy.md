# ADR-007: Fault injection and the client resilience policy

- Status: Accepted
- Date: 2026-11-22

## Context

Phase 5 adds chaos: break parts of the system on a schedule and see how it copes. Results must
stay deterministic, and retries are needed to show retry storms and masked failures.

## Decision

1. **Faults are scheduled events** (`FaultStart`, `FaultEnd`) in the same engine, so a run with
   faults is exactly as reproducible as one without. Five kinds: crash and restart, slowdown
   (service time x factor), lost workers (share of workers), extra latency on an edge, and
   packet loss on an edge (1.0 is a partition). Overlapping faults combine.
2. **Crash**: arrivals are refused, the queue is flushed, and work in service is lost. In-flight
   work is invalidated with an epoch counter stored in each `Done` event, so no event has to be
   removed from the heap. Clients cannot be faulted; fault the edge instead.
3. **Client policy** (`ClientPolicy`, part of the project): request timeout, retries, and a fixed
   backoff before each retry. A retry starts again at the client. A request fails when its
   attempts are used up. A response that arrives after the timeout counts as a failed attempt but
   still consumed server capacity, which is what produces retry storms. Circuit breakers are out
   of scope for now.
4. **Stored with the project**: a `chaos` JSON column (migration 002, project schema version 2);
   older rows get an empty plan. Validation lives in Rust (`ChaosPlan::validate` checks targets
   against the topology). The UI mirrors the ranges with zod, and `src/schemas/limits.json` is
   tested from both sides. The UI drops faults whose target was deleted from the topology.
5. **Report**: each fault gets error rate, p95 and throughput for equally long windows before,
   during and after it, plus a recovery time: the first second from which two consecutive seconds
   have errors within one point of the baseline and p95 within max(1.25x, +5 ms) of it.
   Utilization is measured against available worker time, so a crashed or shrunken node does not
   look idle.
6. **Live view**: each tick lists the active fault ids. The Stage shows crashed nodes greyed with
   a cross, slow nodes with an amber pulse, and affected links dashed with their loss or delay.
   Charts shade the fault windows.

## Consequences

- Faults are fixed before a run. Injecting during a run belongs to Phase 7.
- Packet loss is per hop and independent; there is no bandwidth or congestion model.
- Recovery is judged from per-second buckets, so it has one-second resolution.
