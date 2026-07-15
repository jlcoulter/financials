-- Backup config is no longer tied to a user; it's a global singleton for the admin.
-- The app is single-user; there's one backup config.
-- SQLite cannot DROP a UNIQUE column, so we recreate the table.

CREATE TABLE IF NOT EXISTS backup_config_new (
    id UUID PRIMARY KEY,
    provider TEXT NOT NULL CHECK (provider IN ('s3', 'b2')),
    -- Common fields
    bucket TEXT NOT NULL,
    path TEXT NOT NULL DEFAULT 'financials-backups',
    region TEXT NOT NULL DEFAULT 'us-east-1',
    endpoint TEXT,
    -- S3-specific
    access_key_id TEXT,
    secret_access_key TEXT,
    -- B2-specific
    b2_key_id TEXT,
    b2_application_key TEXT,
    b2_endpoint TEXT,
    -- State
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO backup_config_new (id, provider, bucket, path, region, endpoint,
    access_key_id, secret_access_key, b2_key_id, b2_application_key, b2_endpoint,
    enabled, created_at, updated_at)
SELECT id, provider, bucket, path, region, endpoint,
    access_key_id, secret_access_key, b2_key_id, b2_application_key, b2_endpoint,
    enabled, created_at, updated_at
FROM backup_config;

DROP TABLE backup_config;
ALTER TABLE backup_config_new RENAME TO backup_config;