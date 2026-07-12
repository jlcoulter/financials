-- Add automatic snapshot interval (in minutes). Default 60 = hourly.
ALTER TABLE backup_config ADD COLUMN interval_minutes INTEGER NOT NULL DEFAULT 60;