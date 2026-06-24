-- Task: Add user_id to portfolios for data isolation, add deleted_at for soft-delete.

-- Add user_id column to portfolios
ALTER TABLE portfolios ADD COLUMN user_id TEXT REFERENCES users(username);
CREATE INDEX IF NOT EXISTS idx_portfolios_user ON portfolios(user_id);

-- Add deleted_at to portfolios for soft-delete
ALTER TABLE portfolios ADD COLUMN deleted_at TEXT;

-- Add deleted_at to wealth_items for soft-delete
ALTER TABLE wealth_items ADD COLUMN deleted_at TEXT;