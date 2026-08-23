-- Add B2 S3-compatible endpoint field for Backblaze B2 backups.
-- B2 uses the S3 protocol via its S3-compatible API endpoint.
ALTER TABLE backup_config ADD COLUMN b2_endpoint TEXT;