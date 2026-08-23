CREATE TABLE reconcile_sessions (
    session_id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(user_id),
    name VARCHAR(255) NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMP
);

CREATE TABLE outgoing_txns (
    txn_id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES reconcile_sessions(session_id) ON DELETE CASCADE,
    date DATE NOT NULL,
    amount BIGINT NOT NULL,
    vendor TEXT NOT NULL DEFAULT '',
    matched BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMP
);

CREATE TABLE reconciled_txns (
    txn_id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES reconcile_sessions(session_id) ON DELETE CASCADE,
    date DATE NOT NULL,
    amount BIGINT NOT NULL,
    vendor TEXT NOT NULL DEFAULT '',
    matched BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMP
);

CREATE TABLE match_links (
    match_id UUID PRIMARY KEY,
    outgoing_id UUID NOT NULL REFERENCES outgoing_txns(txn_id) ON DELETE CASCADE,
    reconciled_id UUID NOT NULL REFERENCES reconciled_txns(txn_id) ON DELETE CASCADE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT unique_match UNIQUE (outgoing_id, reconciled_id)
);

CREATE INDEX idx_outgoing_session ON outgoing_txns(session_id);
CREATE INDEX idx_reconciled_session ON reconciled_txns(session_id);
CREATE INDEX idx_match_outgoing ON match_links(outgoing_id);
CREATE INDEX idx_match_reconciled ON match_links(reconciled_id);