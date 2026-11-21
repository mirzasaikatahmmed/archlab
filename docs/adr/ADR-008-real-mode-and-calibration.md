# ADR-008: Real mode on localhost sockets and machine calibration

- Status: Accepted
- Date: 2026-11-22

## Context

Simulation results are estimates. Phase 6 lets the user check them against measurements from the
machine ArchLab runs on, and replaces the guessed values of "Detect this machine" with measured ones.

## Decision

1. **Real mode runs the topology itself**, not external services: every non-client node is a tokio
   task listening on `127.0.0.1` (random port) that queues, does synthetic work and forwards over
   its edges. A load generator sends Poisson traffic, with the same spikes as the simulation, and
   records real latencies. The simulation runs on the same project and seed, and the report shows
   both side by side with the percentage difference.
2. **Same semantics as the engine**: bounded waiting room, a worker is released before the onward
   call, reads try caches first and fall back on a miss, a request older than the timeout is dropped
   when it reaches a worker. The compiled `sim::model::Model` is reused, so templates, settings and
   the server profile mean the same in both.
3. **Work is real**: the CPU share is a chain of dependent integer steps sized by a 150 ms
   self-measurement, so contention shows up; the I/O share is a timer wait; the profile's network
   latency is added per hop with a timer wait.
4. **Wire protocol**: fixed 16-byte request and response frames carrying a request id, several
   pooled connections per edge, no HTTP stack. The only new dependency is `tokio` (already in the
   tree through Tauri) with the net, time, io-util, sync and rt-multi-thread features.
5. **Safety limits**: loopback only, at most 60 s, 5,000 requests per second (spikes included),
   20 nodes, and workers capped at four per host core. Injected faults and retries are not applied
   in real runs. Starting one is refused when a run is already going; it can be cancelled.
6. **Calibration** measures CPU (dependent multiply-add chain, assuming 4 cycles per step, reported
   as an equivalent clock), synced 4 KiB writes for 1.5 s in a scratch file in the app data folder
   that is always removed, and median loopback round trip over 3,000 pings. Results are kept as
   history in a `calibrations` table (migration 003). "Apply" is a Rust command that copies the
   newest result into a profile, clamped to valid ranges, so projects stay self-contained.
7. Results from a debug build are flagged, because it runs far slower than a release build.

## Consequences

- Real runs are not repeatable and depend on what else the computer is doing. Reports say so.
- All nodes share the host's cores, so real mode checks the model; it does not reproduce a
  multi-machine deployment.
- The disk figure is a durable-write rate, usually well below rated random IOPS, so applying it
  makes databases look slower than a drive's spec sheet suggests.
- Timers have operating-system granularity (about 1 ms, 15 ms on some Windows setups), which blurs
  very short service times.
