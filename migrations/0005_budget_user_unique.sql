-- Fix budget unique constraint to include user_id (prevents cross-user upsert conflicts).
-- SQLite doesn't support ALTER TABLE DROP CONSTRAINT, so we must recreate the table.

-- Step 1: Create a new budgets table with the correct unique constraint
CREATE TABLE budgets_new (
    budget_id TEXT PRIMARY KEY,
    category TEXT NOT NULL,
    month TEXT NOT NULL,
    planned_amount BIGINT NOT NULL,
    user_id TEXT REFERENCES users(username),
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TEXT,
    CONSTRAINT unique_budget_per_user_month UNIQUE (user_id, category, month)
);

-- Step 2: Copy data from the old table
INSERT INTO budgets_new (budget_id, category, month, planned_amount, user_id, created_at, deleted_at)
    SELECT budget_id, category, month, planned_amount, user_id, created_at, deleted_at FROM budgets;

-- Step 3: Drop the old table and rename
DROP TABLE budgets;
ALTER TABLE budgets_new RENAME TO budgets;

-- Step 4: Recreate indexes
CREATE INDEX IF NOT EXISTS idx_budgets_month ON budgets(month);
CREATE INDEX IF NOT EXISTS idx_budgets_user ON budgets(user_id);