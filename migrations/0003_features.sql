-- Transactions (income & expenses) — app-level, not per-portfolio
CREATE TABLE IF NOT EXISTS transactions (
    txn_id TEXT PRIMARY KEY,
    txn_date DATE NOT NULL,
    amount BIGINT NOT NULL,          -- positive = income, negative = expense (cents)
    category TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    txn_type TEXT NOT NULL DEFAULT 'expense',  -- 'income' or 'expense'
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_transactions_date ON transactions(txn_date);
CREATE INDEX IF NOT EXISTS idx_transactions_category ON transactions(category);

-- Budget categories — monthly planned amounts
CREATE TABLE IF NOT EXISTS budgets (
    budget_id TEXT PRIMARY KEY,
    category TEXT NOT NULL,
    month TEXT NOT NULL,             -- 'YYYY-MM' format
    planned_amount BIGINT NOT NULL,  -- cents, always positive
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT unique_budget_per_month UNIQUE (category, month)
);

CREATE INDEX IF NOT EXISTS idx_budgets_month ON budgets(month);

-- Savings goals / big purchases
CREATE TABLE IF NOT EXISTS savings_goals (
    goal_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    target_amount BIGINT NOT NULL,  -- cents
    current_amount BIGINT NOT NULL DEFAULT 0,  -- cents saved so far
    target_date TEXT,                -- optional 'YYYY-MM-DD'
    category TEXT NOT NULL DEFAULT 'general',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Holiday periods — tag date ranges as "holidays" for chart shading
CREATE TABLE IF NOT EXISTS holidays (
    holiday_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,              -- e.g. "Christmas 2024", "Summer Vacation"
    start_date DATE NOT NULL,
    end_date DATE NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);