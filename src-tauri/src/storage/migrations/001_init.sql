CREATE TABLE projects (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    server_profile TEXT NOT NULL,
    traffic_profile TEXT NOT NULL,
    topology TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_projects_updated_at ON projects (updated_at DESC);
