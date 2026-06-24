-- Task 4: Add user_id to feature tables (data isolation)
-- Task 7: Add CHECK constraints
-- Task 8: Add updated_at timestamps
-- Task 9: Add deleted_at for soft-delete

-- user_id on transactions (references username, not numeric id)
ALTER TABLE transactions ADD COLUMN user_id TEXT REFERENCES users(username);
CREATE INDEX IF NOT EXISTS idx_transactions_user ON transactions(user_id);

-- user_id on budgets
ALTER TABLE budgets ADD COLUMN user_id TEXT REFERENCES users(username);
CREATE INDEX IF NOT EXISTS idx_budgets_user ON budgets(user_id);

-- user_id on savings_goals
ALTER TABLE savings_goals ADD COLUMN user_id TEXT REFERENCES users(username);
CREATE INDEX IF NOT EXISTS idx_goals_user ON savings_goals(user_id);

-- user_id on holidays
ALTER TABLE holidays ADD COLUMN user_id TEXT REFERENCES users(username);
CREATE INDEX IF NOT EXISTS idx_holidays_user ON holidays(user_id);

-- updated_at on savings_goals (supports edits)
ALTER TABLE savings_goals ADD COLUMN updated_at TEXT;

-- deleted_at for soft-delete on all feature tables
ALTER TABLE transactions ADD COLUMN deleted_at TEXT;
ALTER TABLE budgets ADD COLUMN deleted_at TEXT;
ALTER TABLE savings_goals ADD COLUMN deleted_at TEXT;
ALTER TABLE holidays ADD COLUMN deleted_at TEXT;