ALTER TABLE outgoing_txns ADD COLUMN metadata TEXT NOT NULL DEFAULT '{}';
ALTER TABLE reconciled_txns ADD COLUMN metadata TEXT NOT NULL DEFAULT '{}';
