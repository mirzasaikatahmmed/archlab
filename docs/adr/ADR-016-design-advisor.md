# ADR-016: Design advisor

- Status: Accepted
- Date: 2026-11-22

## Context

Given a description of a product, the app should suggest a system design and show that it works.

## Decision

1. **Rules plus simulation, no AI service.** The advisor (`advisor::advise`) runs on this computer
   with no network and no model, and is deterministic: the same requirements give the same answer.
2. **Inputs** (`Requirements`): daily users, requests per user per day, read share, busiest moment
   compared with the average, what the product mostly does (pages and APIs, files and media,
   deferred work, events, independent services), availability (standard, high, critical), machine
   size (small, medium, large), a p95 target and an error budget. All are validated; a load above
   25,000 requests per second at peak is refused with a message, because it cannot be simulated
   within the engine's event limit.
3. **Choosing the architecture** is a small rule table: shape picks the blueprint (web picks by
   size and read share: simple web, read-heavy site, or the 1M+ architecture).
4. **Sizing is a loop.** Start from the blueprint's structure with the minimum copies for the
   availability level (high: at least two copies of every stateless layer; critical: three, and two
   of stateful ones). Simulate; every node busier than 55%, or dropping requests, or with a growing
   backlog gets more copies in proportion to how busy it is; repeat (at most 10 rounds, at most 600
   machines). It stops when nothing is hot. The final design is the one that was simulated, so
   the verdict is a result, not a claim.
5. **The answer** carries the design (copies and how busy each node is), the project, the
   simulation report, numbered plain-language reasons (peak load, why this architecture, the
   blueprint's own reasoning, what availability implied, each layer's size, the simulated p95 and
   error rate against the targets), warnings when targets are not met (and what to try), and the
   assumptions. If no node is busy but a target still fails, the target is below what these
   machines can deliver; the advisor says so instead of adding machines.
6. **UI.** A "Design advisor" view with the form and the result; "Open as a project" saves the
   design as a normal project.

## Consequences

- It cannot find a better topology than the blueprints offer; it sizes, it does not invent.
- Cost, regions, data consistency and operations effort are outside the model, and the answer
  says so.
- Extra copies of a database act as shards in this model; real high availability also needs
  replication and failover, which the text points out at the critical level.
