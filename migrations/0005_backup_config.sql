-- One backup config per user. Stores S3/B2 credentials and path.
-- Litestream replicates the SQLite DB to this remote location.
CREATE TABLE IF NOT EXISTS backup_config (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL UNIQUE REFERENCES users(user_id),
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
    -- State
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);