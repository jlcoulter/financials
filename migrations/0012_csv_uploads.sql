CREATE TABLE csv_uploads (
    id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES reconcile_sessions(session_id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('outgoing', 'reconciled')),
    raw_text TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_csv_uploads_session ON csv_uploads(session_id);