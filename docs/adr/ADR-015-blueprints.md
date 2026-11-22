# ADR-015: Blueprints, and more node types

- Status: Accepted
- Date: 2026-11-22

## Context

People want to start from a design that works, not a blank canvas, and want to see what it takes
to serve a million users. That needed two more node types (object storage, search), more
templates, and a way to ship tested architectures.

## Decision

1. **Two new node types.** _Object storage_ moves large objects: a template setting gives the
   object size, and each request's transfer time is `size / bandwidth` times the node's workers, so
   throughput is limited by the network link however many workers there are (each replica has its
   own link). _Search_ is an ordinary pool with heavy reads and costly writes (indexing).
   Ten more templates (Cassandra, DynamoDB-style, Elasticsearch, Envoy, Go service, HAProxy, Java
   Spring, Pulsar, S3-style storage, SQS-style queue). `SimParams` gained `payloadSetting` and
   `defaultPayloadKb`; nothing else about existing templates changed.
2. **Blueprints are embedded, versioned JSON** (`src-tauri/blueprints/`, `domain::blueprint`), like
   scenarios. A blueprint has a topology, the traffic assumptions (requests per user per day,
   peak factor, read share), targets (p95 and error rate), a server profile, plain-language "why"
   lines, and **tiers**: sizes by daily users, each saying how many copies of each node.
   Seven blueprints: simple web app, read-heavy site (CDN + cache), async processing, microservices,
   write-heavy ingestion, media and files, and the 1M+ users architecture.
3. **Users become load by one formula** (`domain::sizing`): daily users x requests per user per day
   / 86,400 x peak factor. One million daily users at 20 requests each is about 1,157 requests per
   second at peak.
4. **Every tier is tested.** A cargo test simulates all of them and fails if one misses its
   targets, runs a node above 85% busy, drops requests, or leaves a queue backlog. Sizes were
   tuned against the simulator and are deliberately not over-provisioned (about 30-60% busy at
   peak, at least two copies of stateless layers where availability matters). A negative test checks
   that a single copy of every node does not cope with a hundred million users.
5. **UI.** A Blueprints view (sidebar) with cards, a size picker, the peak load and machine count,
   the suggested simulation length, and "Why this design". "Use this design" opens a normal,
   editable project.
6. **Run length.** The simulator stops a run at 5 million events. Large tiers carry a shorter
   suggested run (20 s for the 10 million user sizes) instead of being scaled down.

## Consequences

- The model is generous: a 4-core machine serves hundreds of requests per second, so a million
  daily users needs few machines. The numbers follow the model and its stated assumptions, not
  any particular real fleet.
- "Copies" of a database behave like shards (load is shared), not like leader and replicas.
- A request goes to one next hop in round-robin, so forks (such as microservices) split traffic
  evenly; real traffic is rarely this even.
