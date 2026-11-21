# ADR-002: Storage with rusqlite (bundled) and rusqlite_migration

- Status: Accepted
- Date: 2026-11-21

## Context

ArchLab is a fully local desktop app with a single user and no server. Projects (server profile,
traffic profile, topology) must be saved, listed, loaded and deleted, and the schema will evolve
over ten phases. End users must not install anything, so SQLite has to be bundled.

## Decision

- Use **rusqlite** with the `bundled` feature (SQLite compiled into the binary).
- Use **rusqlite_migration** for schema migrations (`PRAGMA user_version`, SQL files in
  `src-tauri/src/storage/migrations/`). Applied migrations are never edited; add a new file.
- **Hybrid project shape**: a `projects` row holds `id`, `name`, timestamps and a
  `schema_version`, plus JSON text columns for `server_profile`, `traffic_profile` and `topology`.
  A project is always loaded and saved as one document, so normalizing nodes/edges into tables
  would add joins and migrations without benefit. Rust validates every project before it is saved.
- `schema_version` versions the JSON documents. A newer version than the app understands is
  rejected with a clear error instead of being misread.
- A repository struct (`storage::ProjectRepository`) owns the connection behind a `Mutex`; commands
  never see SQL. Database file: `archlab.db` in the OS app-data directory, WAL mode.

## Alternatives considered

- **sqlx**: async and compile-time-checked queries, but needs a runtime and offline query data, and
  a local single-user database gains little from async I/O.
- **Normalized tables**: queryable by SQL, but we never query inside a project.
- **Plain JSON files**: no migrations or atomic writes; later phases (run history, scenarios) need
  queries.

## Consequences

- Positive: zero external dependencies, simple migrations, atomic project saves.
- Negative: `rusqlite_migration` pins the `rusqlite` version (currently 0.41); upgrades go
  together. The bundled SQLite adds a C compile step to the build (a C toolchain is required, which
  Tauri already needs).
- Querying across projects' contents (for example "all projects with a Redis node") needs JSON
  functions or a later denormalized index.
