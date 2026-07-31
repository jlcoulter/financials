CREATE UNIQUE INDEX idx_outgoing_dedup ON outgoing_txns(session_id, date, amount, vendor) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX idx_reconciled_dedup ON reconciled_txns(session_id, date, amount, vendor) WHERE deleted_at IS NULL;
