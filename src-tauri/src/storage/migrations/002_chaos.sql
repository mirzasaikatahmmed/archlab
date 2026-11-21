ALTER TABLE projects ADD COLUMN chaos TEXT NOT NULL
    DEFAULT '{"faults":[],"policy":{"timeoutMs":10000,"maxRetries":0,"backoffMs":100}}';
