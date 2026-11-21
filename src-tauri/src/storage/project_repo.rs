use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use super::StorageError;
use crate::domain::project::{Project, ProjectSummary};

/// Version of the JSON documents stored in the `projects` table.
const PROJECT_SCHEMA_VERSION: u32 = 1;

pub struct ProjectRepository {
    conn: Mutex<Connection>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

impl ProjectRepository {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        self.conn.lock().map_err(|_| StorageError::Poisoned)
    }

    /// Lists projects, most recently updated first.
    pub fn list(&self) -> Result<Vec<ProjectSummary>, StorageError> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, topology, updated_at FROM projects ORDER BY updated_at DESC, id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, name, topology, updated_at) = row?;
            let topology: crate::domain::topology::Topology = serde_json::from_str(&topology)?;
            out.push(ProjectSummary {
                id,
                name,
                node_count: u32::try_from(topology.nodes.len()).unwrap_or(u32::MAX),
                updated_at: updated_at as f64,
            });
        }
        Ok(out)
    }

    pub fn get(&self, id: &str) -> Result<Project, StorageError> {
        let conn = self.lock()?;
        let row = conn
            .query_row(
                "SELECT id, name, schema_version, server_profile, traffic_profile, topology, \
                 created_at, updated_at FROM projects WHERE id = ?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, u32>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, i64>(6)?,
                        r.get::<_, i64>(7)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound(id.to_owned()))?;
        let (id, name, version, server, traffic, topology, created, updated) = row;
        if version > PROJECT_SCHEMA_VERSION {
            return Err(StorageError::NewerSchema(version));
        }
        Ok(Project {
            id,
            name,
            server_profile: serde_json::from_str(&server)?,
            traffic_profile: serde_json::from_str(&traffic)?,
            topology: serde_json::from_str(&topology)?,
            created_at: created as f64,
            updated_at: updated as f64,
        })
    }

    /// Inserts or updates a project. The caller must validate it first.
    /// Returns the stored project with fresh timestamps.
    pub fn save(&self, project: &Project) -> Result<Project, StorageError> {
        let server = serde_json::to_string(&project.server_profile)?;
        let traffic = serde_json::to_string(&project.traffic_profile)?;
        let topology = serde_json::to_string(&project.topology)?;
        let now = now_ms();
        {
            let conn = self.lock()?;
            conn.execute(
                "INSERT INTO projects (id, name, schema_version, server_profile, traffic_profile, \
                 topology, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7) \
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, \
                 schema_version = excluded.schema_version, \
                 server_profile = excluded.server_profile, \
                 traffic_profile = excluded.traffic_profile, topology = excluded.topology, \
                 updated_at = excluded.updated_at",
                params![
                    project.id,
                    project.name.trim(),
                    PROJECT_SCHEMA_VERSION,
                    server,
                    traffic,
                    topology,
                    now
                ],
            )?;
        }
        self.get(&project.id)
    }

    pub fn delete(&self, id: &str) -> Result<(), StorageError> {
        let conn = self.lock()?;
        let changed = conn.execute("DELETE FROM projects WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(StorageError::NotFound(id.to_owned()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_in_memory;

    fn repo() -> ProjectRepository {
        ProjectRepository::new(open_in_memory().expect("db"))
    }

    #[test]
    fn save_then_get_round_trips() {
        let repo = repo();
        let p = Project::new("Demo");
        let saved = repo.save(&p).expect("save");
        assert_eq!(saved.id, p.id);
        assert!(saved.created_at > 0.0);
        let mut expected = p;
        expected.created_at = saved.created_at;
        expected.updated_at = saved.updated_at;
        assert_eq!(repo.get(&saved.id).expect("get"), expected);
    }

    #[test]
    fn save_twice_updates_and_keeps_created_at() {
        let repo = repo();
        let first = repo.save(&Project::new("A")).expect("save");
        let mut edited = first.clone();
        edited.name = "B".into();
        edited.server_profile.cpu_cores = 16;
        let second = repo.save(&edited).expect("update");
        assert_eq!(second.name, "B");
        assert_eq!(second.server_profile.cpu_cores, 16);
        assert_eq!(second.created_at, first.created_at);
        assert!(second.updated_at >= first.updated_at);
        assert_eq!(repo.list().expect("list").len(), 1);
    }

    #[test]
    fn list_returns_summaries_newest_first() {
        let repo = repo();
        let a = repo.save(&Project::new("A")).expect("a");
        std::thread::sleep(std::time::Duration::from_millis(5));
        let b = repo.save(&Project::new("B")).expect("b");
        let list = repo.list().expect("list");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, b.id);
        assert_eq!(list[1].id, a.id);
        assert_eq!(list[0].node_count, 3);
    }

    #[test]
    fn delete_removes_and_missing_is_not_found() {
        let repo = repo();
        let p = repo.save(&Project::new("A")).expect("save");
        repo.delete(&p.id).expect("delete");
        assert!(matches!(repo.get(&p.id), Err(StorageError::NotFound(_))));
        assert!(matches!(repo.delete(&p.id), Err(StorageError::NotFound(_))));
    }

    #[test]
    fn rejects_documents_from_newer_schema() {
        let repo = repo();
        let p = repo.save(&Project::new("A")).expect("save");
        repo.lock()
            .expect("lock")
            .execute("UPDATE projects SET schema_version = 99", [])
            .expect("update");
        assert!(matches!(
            repo.get(&p.id),
            Err(StorageError::NewerSchema(99))
        ));
    }
}
