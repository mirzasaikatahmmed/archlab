use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension};

use super::StorageError;
use crate::domain::calibration::Calibration;

/// History of machine calibrations; the newest one is the current one.
pub struct CalibrationRepository {
    conn: Mutex<Connection>,
}

impl CalibrationRepository {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        self.conn.lock().map_err(|_| StorageError::Poisoned)
    }

    pub fn save(&self, calibration: &Calibration) -> Result<(), StorageError> {
        let data = serde_json::to_string(calibration)?;
        #[allow(clippy::cast_possible_truncation)]
        let at = calibration.measured_at as i64;
        self.lock()?.execute(
            "INSERT INTO calibrations (measured_at, data) VALUES (?1, ?2)",
            rusqlite::params![at, data],
        )?;
        Ok(())
    }

    pub fn latest(&self) -> Result<Option<Calibration>, StorageError> {
        let data: Option<String> = self
            .lock()?
            .query_row(
                "SELECT data FROM calibrations ORDER BY measured_at DESC, id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        data.map(|d| serde_json::from_str(&d).map_err(StorageError::from))
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_in_memory;

    fn calibration(at: f64, ghz: f64) -> Calibration {
        Calibration {
            measured_at: at,
            equivalent_ghz: ghz,
            disk_iops: 1_000.0,
            loopback_latency_ms: 0.05,
            debug_build: false,
            notes: vec!["note".into()],
        }
    }

    #[test]
    fn starts_empty_and_returns_the_newest() {
        let repo = CalibrationRepository::new(open_in_memory().expect("db"));
        assert!(repo.latest().expect("latest").is_none());
        repo.save(&calibration(100.0, 2.0)).expect("save");
        repo.save(&calibration(300.0, 4.0)).expect("save");
        repo.save(&calibration(200.0, 3.0)).expect("save");
        let latest = repo.latest().expect("latest").expect("some");
        assert_eq!(latest, calibration(300.0, 4.0));
    }
}
