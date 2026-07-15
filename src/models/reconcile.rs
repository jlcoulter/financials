use crate::error::AppError;
use chrono::NaiveDate;
use sqlx::SqlitePool;
use uuid::Uuid;

// ── CSV Uploads ──

pub async fn save_csv_upload(
    pool: &SqlitePool,
    id: Uuid,
    session_id: Uuid,
    kind: &str,
    raw_text: &str,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO csv_uploads (id, session_id, kind, raw_text) VALUES (?, ?, ?, ?)")
        .bind(id.to_string())
        .bind(session_id.to_string())
        .bind(kind)
        .bind(raw_text)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_csv_upload(
    pool: &SqlitePool,
    id: Uuid,
) -> Result<(Uuid, Uuid, String, String), AppError> {
    let row = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT id, session_id, kind, raw_text FROM csv_uploads WHERE id = ?",
    )
    .bind(id.to_string())
    .fetch_one(pool)
    .await
    .map_err(|_| AppError::BadRequest("CSV upload not found".into()))?;
    Ok((
        Uuid::parse_str(&row.0).map_err(|_| AppError::BadRequest("Invalid UUID".into()))?,
        Uuid::parse_str(&row.1).map_err(|_| AppError::BadRequest("Invalid UUID".into()))?,
        row.2,
        row.3,
    ))
}

pub async fn delete_csv_upload(pool: &SqlitePool, id: Uuid) -> Result<(), AppError> {
    sqlx::query("DELETE FROM csv_uploads WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

// ── Structs ──

#[allow(dead_code)]
pub struct ReconcileSession {
    pub session_id: Uuid,
    pub name: String,
}

#[allow(dead_code)]
pub struct OutgoingTxn {
    pub txn_id: Uuid,
    #[allow(dead_code)]
    pub session_id: Uuid,
    pub date: NaiveDate,
    pub amount: i64,
    pub vendor: String,
    pub matched: bool,
    pub ignored: bool,
}

#[allow(dead_code)]
pub struct ReconciledTxn {
    pub txn_id: Uuid,
    #[allow(dead_code)]
    pub session_id: Uuid,
    pub date: NaiveDate,
    pub amount: i64,
    pub vendor: String,
    pub matched: bool,
    pub ignored: bool,
}

pub struct MatchLink {
    pub match_id: Uuid,
    pub outgoing_id: Uuid,
    pub reconciled_id: Uuid,
}

// ── Session CRUD ──

pub async fn create_session(
    pool: &SqlitePool,
    user_id: Uuid,
    name: &str,
) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO reconcile_sessions (session_id, user_id, name) VALUES (?, ?, ?)")
        .bind(id.to_string())
        .bind(user_id.to_string())
        .bind(name)
        .execute(pool)
        .await?;
    Ok(id)
}

pub async fn list_sessions(
    pool: &SqlitePool,
    user_id: Uuid,
) -> Result<Vec<(Uuid, String)>, AppError> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT session_id, name FROM reconcile_sessions WHERE user_id = ? AND deleted_at IS NULL ORDER BY created_at",
    )
    .bind(user_id.to_string())
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|(id_str, name)| {
            let id = Uuid::parse_str(&id_str)?;
            Ok((id, name))
        })
        .collect()
}

pub async fn get_session(
    pool: &SqlitePool,
    session_id: Uuid,
    user_id: Uuid,
) -> Result<(Uuid, String), AppError> {
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT session_id, name FROM reconcile_sessions WHERE session_id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(session_id.to_string())
    .bind(user_id.to_string())
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::BadRequest("Session not found".into()))?;
    let id = Uuid::parse_str(&row.0)?;
    Ok((id, row.1))
}

pub async fn delete_session(pool: &SqlitePool, session_id: Uuid) -> Result<(), AppError> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    // Soft-delete match links first (cascade won't fire on soft-delete)
    sqlx::query("DELETE FROM match_links WHERE outgoing_id IN (SELECT txn_id FROM outgoing_txns WHERE session_id = ?)")
        .bind(session_id.to_string())
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM match_links WHERE reconciled_id IN (SELECT txn_id FROM reconciled_txns WHERE session_id = ?)")
        .bind(session_id.to_string())
        .execute(pool)
        .await?;
    sqlx::query("UPDATE outgoing_txns SET deleted_at = ? WHERE session_id = ?")
        .bind(&now)
        .bind(session_id.to_string())
        .execute(pool)
        .await?;
    sqlx::query("UPDATE reconciled_txns SET deleted_at = ? WHERE session_id = ?")
        .bind(&now)
        .bind(session_id.to_string())
        .execute(pool)
        .await?;
    sqlx::query("UPDATE reconcile_sessions SET deleted_at = ? WHERE session_id = ?")
        .bind(&now)
        .bind(session_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

// ── Outgoing transactions ──

pub async fn list_outgoing(
    pool: &SqlitePool,
    session_id: Uuid,
) -> Result<Vec<OutgoingTxn>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String, i64, String, bool, bool)>(
        "SELECT txn_id, session_id, date, amount, vendor, matched, COALESCE(ignored, FALSE) FROM outgoing_txns WHERE session_id = ? AND deleted_at IS NULL AND (ignored IS NULL OR ignored = FALSE) ORDER BY amount DESC, date, created_at",
    )
    .bind(session_id.to_string())
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(
            |(id_str, sid_str, date_str, amount, vendor, matched, ignored)| {
                Ok(OutgoingTxn {
                    txn_id: Uuid::parse_str(&id_str)?,
                    session_id: Uuid::parse_str(&sid_str)?,
                    date: NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")?,
                    amount,
                    vendor,
                    matched,
                    ignored,
                })
            },
        )
        .collect()
}

pub async fn add_outgoing(
    pool: &SqlitePool,
    session_id: Uuid,
    date: NaiveDate,
    amount: i64,
    vendor: &str,
) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO outgoing_txns (txn_id, session_id, date, amount, vendor) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(session_id.to_string())
    .bind(date.to_string())
    .bind(amount)
    .bind(vendor)
    .execute(pool)
    .await?;
    Ok(id)
}

/// Bulk insert outgoing transactions, deduplicating against existing (date, amount, vendor).
/// Returns the count of new rows inserted.
pub async fn bulk_add_outgoing(
    pool: &SqlitePool,
    session_id: Uuid,
    txns: &[(NaiveDate, i64, String)],
) -> Result<usize, AppError> {
    let mut count = 0usize;
    for (date, amount, vendor) in txns {
        // Check for duplicate
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM outgoing_txns WHERE session_id = ? AND date = ? AND amount = ? AND vendor = ? AND deleted_at IS NULL)",
        )
        .bind(session_id.to_string())
        .bind(date.to_string())
        .bind(*amount)
        .bind(vendor)
        .fetch_one(pool)
        .await?;

        if !exists {
            add_outgoing(pool, session_id, *date, *amount, vendor).await?;
            count += 1;
        }
    }
    Ok(count)
}

// ── Reconciled transactions ──

pub async fn list_reconciled(
    pool: &SqlitePool,
    session_id: Uuid,
) -> Result<Vec<ReconciledTxn>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String, i64, String, bool, bool)>(
        "SELECT txn_id, session_id, date, amount, vendor, matched, COALESCE(ignored, FALSE) FROM reconciled_txns WHERE session_id = ? AND deleted_at IS NULL AND (ignored IS NULL OR ignored = FALSE) ORDER BY amount DESC, date, created_at",
    )
    .bind(session_id.to_string())
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(
            |(id_str, sid_str, date_str, amount, vendor, matched, ignored)| {
                Ok(ReconciledTxn {
                    txn_id: Uuid::parse_str(&id_str)?,
                    session_id: Uuid::parse_str(&sid_str)?,
                    date: NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")?,
                    amount,
                    vendor,
                    matched,
                    ignored,
                })
            },
        )
        .collect()
}

pub async fn add_reconciled(
    pool: &SqlitePool,
    session_id: Uuid,
    date: NaiveDate,
    amount: i64,
    vendor: &str,
) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO reconciled_txns (txn_id, session_id, date, amount, vendor) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(session_id.to_string())
    .bind(date.to_string())
    .bind(amount)
    .bind(vendor)
    .execute(pool)
    .await?;
    Ok(id)
}

/// Bulk insert reconciled transactions, deduplicating against existing (date, amount, vendor).
/// Returns the count of new rows inserted.
pub async fn bulk_add_reconciled(
    pool: &SqlitePool,
    session_id: Uuid,
    txns: &[(NaiveDate, i64, String)],
) -> Result<usize, AppError> {
    let mut count = 0usize;
    for (date, amount, vendor) in txns {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM reconciled_txns WHERE session_id = ? AND date = ? AND amount = ? AND vendor = ? AND deleted_at IS NULL)",
        )
        .bind(session_id.to_string())
        .bind(date.to_string())
        .bind(*amount)
        .bind(vendor)
        .fetch_one(pool)
        .await?;

        if !exists {
            add_reconciled(pool, session_id, *date, *amount, vendor).await?;
            count += 1;
        }
    }
    Ok(count)
}

// ── Matching ──

pub async fn link_transactions(
    pool: &SqlitePool,
    outgoing_id: Uuid,
    reconciled_id: Uuid,
) -> Result<(), AppError> {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO match_links (match_id, outgoing_id, reconciled_id) VALUES (?, ?, ?)")
        .bind(id.to_string())
        .bind(outgoing_id.to_string())
        .bind(reconciled_id.to_string())
        .execute(pool)
        .await?;

    // Mark both as matched
    sqlx::query("UPDATE outgoing_txns SET matched = TRUE WHERE txn_id = ?")
        .bind(outgoing_id.to_string())
        .execute(pool)
        .await?;
    sqlx::query("UPDATE reconciled_txns SET matched = TRUE WHERE txn_id = ?")
        .bind(reconciled_id.to_string())
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn unlink_transaction(pool: &SqlitePool, match_id: Uuid) -> Result<(), AppError> {
    // Get the pair before deleting
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT outgoing_id, reconciled_id FROM match_links WHERE match_id = ?",
    )
    .bind(match_id.to_string())
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::BadRequest("Match not found".into()))?;

    let outgoing_id = Uuid::parse_str(&row.0)?;
    let reconciled_id = Uuid::parse_str(&row.1)?;

    sqlx::query("DELETE FROM match_links WHERE match_id = ?")
        .bind(match_id.to_string())
        .execute(pool)
        .await?;

    // Check if outgoing still has other matches
    let outgoing_matched: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM match_links WHERE outgoing_id = ?)")
            .bind(outgoing_id.to_string())
            .fetch_one(pool)
            .await?;

    let reconciled_matched: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM match_links WHERE reconciled_id = ?)")
            .bind(reconciled_id.to_string())
            .fetch_one(pool)
            .await?;

    if !outgoing_matched {
        sqlx::query("UPDATE outgoing_txns SET matched = FALSE WHERE txn_id = ?")
            .bind(outgoing_id.to_string())
            .execute(pool)
            .await?;
    }
    if !reconciled_matched {
        sqlx::query("UPDATE reconciled_txns SET matched = FALSE WHERE txn_id = ?")
            .bind(reconciled_id.to_string())
            .execute(pool)
            .await?;
    }

    Ok(())
}

pub async fn list_matches(pool: &SqlitePool, session_id: Uuid) -> Result<Vec<MatchLink>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT m.match_id, m.outgoing_id, m.reconciled_id \
         FROM match_links m \
         JOIN outgoing_txns o ON m.outgoing_id = o.txn_id \
         JOIN reconciled_txns r ON m.reconciled_id = r.txn_id \
         WHERE o.session_id = ? AND o.deleted_at IS NULL AND r.deleted_at IS NULL",
    )
    .bind(session_id.to_string())
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|(mid, oid, rid)| {
            Ok(MatchLink {
                match_id: Uuid::parse_str(&mid)?,
                outgoing_id: Uuid::parse_str(&oid)?,
                reconciled_id: Uuid::parse_str(&rid)?,
            })
        })
        .collect()
}

pub async fn ignore_outgoing(pool: &SqlitePool, txn_id: Uuid) -> Result<(), AppError> {
    sqlx::query("UPDATE outgoing_txns SET ignored = TRUE WHERE txn_id = ?")
        .bind(txn_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn ignore_reconciled(pool: &SqlitePool, txn_id: Uuid) -> Result<(), AppError> {
    sqlx::query("UPDATE reconciled_txns SET ignored = TRUE WHERE txn_id = ?")
        .bind(txn_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Auto-match: for each unmatched outgoing, find a single unmatched reconciled txn
/// with the exact same amount, or a set of unmatched reconciled txns whose amounts sum
/// to the outgoing amount (up to 4 transactions).
/// Returns the number of new matches created.
pub struct Proposal {
    pub outgoing_id: Uuid,
    pub reconciled_ids: Vec<Uuid>,
}

pub async fn auto_match(
    pool: &SqlitePool,
    session_id: Uuid,
    skip_ids: &[Uuid],
) -> Result<Vec<Proposal>, AppError> {
    let outgoing = list_outgoing(pool, session_id).await?;
    let reconciled = list_reconciled(pool, session_id).await?;

    Ok(match_exact(&outgoing, &reconciled, skip_ids))
}

/// Propose exact 1:1 matches — one outgoing to one reconciled with the same amount.
/// Does not combine multiple reconciled items to match one outgoing.
fn match_exact(
    outgoing: &[OutgoingTxn],
    reconciled: &[ReconciledTxn],
    skip_ids: &[Uuid],
) -> Vec<Proposal> {
    let unmatched_outgoing: Vec<&OutgoingTxn> = outgoing
        .iter()
        .filter(|o| !o.matched && !skip_ids.contains(&o.txn_id))
        .collect();
    let unmatched_reconciled: Vec<&ReconciledTxn> =
        reconciled.iter().filter(|r| !r.matched).collect();

    let mut proposals = Vec::new();
    let mut used: std::collections::HashSet<Uuid> = std::collections::HashSet::new();

    for o in &unmatched_outgoing {
        if let Some(r) = unmatched_reconciled
            .iter()
            .find(|r| !used.contains(&r.txn_id) && r.amount == o.amount)
        {
            proposals.push(Proposal {
                outgoing_id: o.txn_id,
                reconciled_ids: vec![r.txn_id],
            });
            used.insert(r.txn_id);
        }
    }

    proposals
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn txn(id: &str, amount: i64) -> ReconciledTxn {
        ReconciledTxn {
            txn_id: Uuid::parse_str(id).unwrap(),
            session_id: Uuid::nil(),
            date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            amount,
            vendor: "test".to_string(),
            matched: false,
            ignored: false,
        }
    }

    fn out(id: &str, amount: i64) -> OutgoingTxn {
        OutgoingTxn {
            txn_id: Uuid::parse_str(id).unwrap(),
            session_id: Uuid::nil(),
            date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            amount,
            vendor: "test".to_string(),
            matched: false,
            ignored: false,
        }
    }

    #[test]
    fn match_exact_single() {
        let o1 = out("00000000-0000-0000-0000-000000000010", 100_00);
        let r1 = txn("00000000-0000-0000-0000-000000000001", 100_00);
        let o1_id = o1.txn_id;
        let r1_id = r1.txn_id;
        let result = match_exact(&[o1], &[r1], &[]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].outgoing_id, o1_id);
        assert_eq!(result[0].reconciled_ids, vec![r1_id]);
    }

    #[test]
    fn match_exact_no_combination() {
        // Should NOT combine multiple reconciled items to match one outgoing
        let o1 = out("00000000-0000-0000-0000-000000000010", 100_00);
        let r1 = txn("00000000-0000-0000-0000-000000000001", 60_00);
        let r2 = txn("00000000-0000-0000-0000-000000000002", 40_00);
        let result = match_exact(&[o1], &[r1, r2], &[]);
        assert_eq!(result.len(), 0, "should not combine items to match");
    }

    #[test]
    fn match_exact_multiple_same_amount() {
        let o1 = out("00000000-0000-0000-0000-000000000010", 100_00);
        let o2 = out("00000000-0000-0000-0000-000000000011", 100_00);
        let r1 = txn("00000000-0000-0000-0000-000000000001", 100_00);
        let r2 = txn("00000000-0000-0000-0000-000000000002", 100_00);
        let result = match_exact(&[o1, o2], &[r1, r2], &[]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn match_exact_no_match_different_amount() {
        let o1 = out("00000000-0000-0000-0000-000000000010", 50_00);
        let r1 = txn("00000000-0000-0000-0000-000000000001", 100_00);
        let result = match_exact(&[o1], &[r1], &[]);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn match_exact_skips_matched() {
        let o1 = out("00000000-0000-0000-0000-000000000010", 100_00);
        let mut r1 = txn("00000000-0000-0000-0000-000000000001", 100_00);
        r1.matched = true;
        let result = match_exact(&[o1], &[r1], &[]);
        assert_eq!(result.len(), 0, "should skip already-matched items");
    }

    #[test]
    fn match_exact_skip_ids() {
        let o1 = out("00000000-0000-0000-0000-000000000010", 100_00);
        let skip = o1.txn_id;
        let r1 = txn("00000000-0000-0000-0000-000000000001", 100_00);
        let result = match_exact(&[o1], &[r1], &[skip]);
        assert_eq!(result.len(), 0, "should skip outgoing in skip_ids");
    }
}
