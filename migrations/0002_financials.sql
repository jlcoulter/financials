CREATE TABLE portfolios (
	portfolio_id UUID PRIMARY KEY,
	name VARCHAR(255) NOT NULL,
	created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE wealth_items (
	item_id UUID PRIMARY KEY,
	portfolio_id INT NOT NULL REFERENCES portfolios(portfolio_id) ON DELETE CASCADE,
	name VARCHAR(255) NOT NULL,
	item_type VARCHAR(50) NOT NULL,
	created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

	CONSTRAINT unique_item_name_per_portfolio UNIQUE (portfolio_id, name)
);

CREATE TABLE balance_logs (
	log_id UUID PRIMARY KEY,
	item_id UUID NOT NULL REFERENCES wealth_items(item_id) ON DELETE CASCADE,
	log_date DATE NOT NULL,
	balance_value BIGINT NOT NULL,
	updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

	CONSTRAINT unique_item_balance_per_data UNIQUE (item_id, log_date)
);

CREATE INDEX idx_balance_logs_date ON balance_logs(log_date);
CREATE INDEX idx_wealth_items_portfolio ON wealth_items(portfolio_id);
