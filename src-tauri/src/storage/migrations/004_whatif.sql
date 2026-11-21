ALTER TABLE projects ADD COLUMN whatif TEXT NOT NULL
    DEFAULT '{"slo":{"p95Ms":200,"maxErrorRate":0.01},"variants":[]}';
