-- Maximum snapshots to retain in the bucket (oldest pruned after each upload).
-- Default 10 keeps about 10 hours of hourly snapshots, or ~2.5 days of 6-hourly.
ALTER TABLE backup_config ADD COLUMN max_snapshots INTEGER NOT NULL DEFAULT 10;