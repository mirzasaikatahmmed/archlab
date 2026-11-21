CREATE TABLE calibrations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    measured_at INTEGER NOT NULL,
    data TEXT NOT NULL
);

CREATE INDEX idx_calibrations_measured_at ON calibrations (measured_at DESC);
