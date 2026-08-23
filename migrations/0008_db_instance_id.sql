-- Add a db_instance_id to backup_config.
-- This UUID is unique per database instance. When the DB is recreated from
-- scratch, a new instance_id is generated, so backups use a fresh path
-- in the bucket instead of colliding with old files.
ALTER TABLE backup_config ADD COLUMN db_instance_id TEXT;