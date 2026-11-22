# ADR-014: More node types and templates

- Status: Accepted
- Date: 2026-11-22

## Context

The simulator knew six node types and four templates. Real systems also have gateways that limit
traffic, CDNs that absorb reads, and queues that decouple producers from background workers.

## Decision

1. **Three new node types**, each with its own behaviour in the engine, an icon, a Stage drawing
   and a palette entry:
   - **API gateway** (`apiGateway`): like a load balancer, with an optional rate limit (setting
     `rate_limit_rps`, 0 = unlimited). A token bucket with a burst of a tenth of a second rejects
     the excess at once; the limit is multiplied by the node's replicas.
   - **CDN** (`cdn`): sits in the request path. A read is served from the edge with probability
     `hit_ratio` and goes no further; writes and misses continue to the next node.
   - **Worker** (`worker`): a pool that consumes messages from a queue.
2. **A queue with at least one worker after it is asynchronous.** The producer's request is stored
   (taking the queue's own service time), answered at once, and a background message waits in the
   queue's backlog until a worker of that queue is free (oldest first, round-robin over workers).
   A full backlog (`max_queue_length` or `retention_messages`, default 10,000) rejects the
   producer. The message runs through the worker and any nodes after it like a request, but has no
   client: no retries, no client latency, and its failures are counted on the queue. A crashed
   worker loses the messages it was holding; the queue keeps its backlog. Delivery from the
   queue to a worker is instant (edge faults on that link have no effect) and messages never time
   out while queued.
3. **A queue without workers behaves exactly as before** (a pool that forwards the request). Every
   earlier result is unchanged: the engine's new state is inert for old topologies, and the
   existing tests pass unmodified. Reports and assumptions only gain lines when a topology uses
   the new behaviour.
4. **Reporting.** A queue with workers carries a `backlog` section in its node report (queued,
   processed, failed, still waiting, and how long messages waited, p95 included). Its queue depth
   uses the backlog (`mean_queue`, `max_queue`, and the live tick's `queue`). The queue is marked
   saturated, and the bottleneck text says so, when more than 5% of what it took in is still
   waiting at the end. Client latency does not include worker time.
5. **Eight new templates** (JSON, all numbers are rough guides, not benchmarks): API gateway, CDN
   edge, background worker, RabbitMQ, Kafka, MySQL, MongoDB and Memcached. Two optional template
   fields, `rateLimitSetting` and `backlogSetting`, name the settings behind the new behaviour.
   Existing templates are unchanged.
6. **Real mode** runs topologies as ordinary servers, so it refuses ones with a message queue that
   has workers, a CDN, or a gateway rate limit and points the person to the simulation.

## Consequences

- Object storage and other bandwidth-bound nodes are not modelled yet.
- A queue's workers are exactly its direct child worker nodes; other children of an asynchronous
  queue are ignored.
