use crate::error::AppError;
use chrono::NaiveDate;
use sqlx::SqlitePool;
use std::collections::HashMap;
use uuid::Uuid;

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
    pub metadata: HashMap<String, String>,
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
    pub metadata: HashMap<String, String>,
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
    // Soft-delete match links (consistent with other tables)
    sqlx::query("UPDATE match_links SET deleted_at = ? WHERE outgoing_id IN (SELECT txn_id FROM outgoing_txns WHERE session_id = ?)")
        .bind(&now)
        .bind(session_id.to_string())
        .execute(pool)
        .await?;
    sqlx::query("UPDATE match_links SET deleted_at = ? WHERE reconciled_id IN (SELECT txn_id FROM reconciled_txns WHERE session_id = ?)")
        .bind(&now)
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

/// Sort order for listing transactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    #[default]
    Date,
    Amount,
}

impl std::fmt::Display for SortOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Date => write!(f, "date"),
            Self::Amount => write!(f, "amount"),
        }
    }
}

// ── Outgoing transactions ──

pub async fn list_outgoing(
    pool: &SqlitePool,
    session_id: Uuid,
    sort: SortOrder,
) -> Result<Vec<OutgoingTxn>, AppError> {
    let rows = match sort {
        SortOrder::Date => {
            sqlx::query_as::<_, (String, String, String, i64, String, bool, bool, String)>(
                "SELECT txn_id, session_id, date, amount, vendor, matched, COALESCE(ignored, FALSE), COALESCE(metadata, '{}') FROM outgoing_txns WHERE session_id = ? AND deleted_at IS NULL AND (ignored IS NULL OR ignored = FALSE) ORDER BY date DESC, created_at DESC",
            )
            .bind(session_id.to_string())
            .fetch_all(pool)
            .await?
        }
        SortOrder::Amount => {
            sqlx::query_as::<_, (String, String, String, i64, String, bool, bool, String)>(
                "SELECT txn_id, session_id, date, amount, vendor, matched, COALESCE(ignored, FALSE), COALESCE(metadata, '{}') FROM outgoing_txns WHERE session_id = ? AND deleted_at IS NULL AND (ignored IS NULL OR ignored = FALSE) ORDER BY amount DESC, date, created_at",
            )
            .bind(session_id.to_string())
            .fetch_all(pool)
            .await?
        }
    };

    rows.into_iter()
        .map(
            |(id_str, sid_str, date_str, amount, vendor, matched, ignored, metadata_str)| {
                let metadata: HashMap<String, String> =
                    serde_json::from_str(&metadata_str).unwrap_or_default();
                Ok(OutgoingTxn {
                    txn_id: Uuid::parse_str(&id_str)?,
                    session_id: Uuid::parse_str(&sid_str)?,
                    date: NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")?,
                    amount,
                    vendor,
                    matched,
                    ignored,
                    metadata,
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
    metadata: &HashMap<String, String>,
) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    let metadata_json = serde_json::to_string(metadata).unwrap_or_else(|_| "{}".to_string());
    sqlx::query(
        "INSERT INTO outgoing_txns (txn_id, session_id, date, amount, vendor, metadata) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(session_id.to_string())
    .bind(date.to_string())
    .bind(amount)
    .bind(vendor)
    .bind(&metadata_json)
    .execute(pool)
    .await?;
    Ok(id)
}

/// Bulk insert outgoing transactions, deduplicating against existing (date, amount, vendor).
/// Uses INSERT OR IGNORE with a unique index to avoid N+1 queries.
/// Returns the count of new rows inserted.
pub async fn bulk_add_outgoing(
    pool: &SqlitePool,
    session_id: Uuid,
    txns: &[crate::models::csv_import::CsvRow],
) -> Result<usize, AppError> {
    let mut count = 0usize;
    for row in txns {
        let id = Uuid::now_v7();
        let metadata_json =
            serde_json::to_string(&row.metadata).unwrap_or_else(|_| "{}".to_string());
        let result = sqlx::query(
            "INSERT OR IGNORE INTO outgoing_txns (txn_id, session_id, date, amount, vendor, metadata) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(session_id.to_string())
        .bind(row.date.to_string())
        .bind(row.amount)
        .bind(&row.vendor)
        .bind(&metadata_json)
        .execute(pool)
        .await?;
        if result.rows_affected() > 0 {
            count += 1;
        }
    }
    Ok(count)
}

// ── Reconciled transactions ──

pub async fn list_reconciled(
    pool: &SqlitePool,
    session_id: Uuid,
    sort: SortOrder,
) -> Result<Vec<ReconciledTxn>, AppError> {
    let rows = match sort {
        SortOrder::Date => {
            sqlx::query_as::<_, (String, String, String, i64, String, bool, bool, String)>(
                "SELECT txn_id, session_id, date, amount, vendor, matched, COALESCE(ignored, FALSE), COALESCE(metadata, '{}') FROM reconciled_txns WHERE session_id = ? AND deleted_at IS NULL AND (ignored IS NULL OR ignored = FALSE) ORDER BY date DESC, created_at DESC",
            )
            .bind(session_id.to_string())
            .fetch_all(pool)
            .await?
        }
        SortOrder::Amount => {
            sqlx::query_as::<_, (String, String, String, i64, String, bool, bool, String)>(
                "SELECT txn_id, session_id, date, amount, vendor, matched, COALESCE(ignored, FALSE), COALESCE(metadata, '{}') FROM reconciled_txns WHERE session_id = ? AND deleted_at IS NULL AND (ignored IS NULL OR ignored = FALSE) ORDER BY amount DESC, date, created_at",
            )
            .bind(session_id.to_string())
            .fetch_all(pool)
            .await?
        }
    };

    rows.into_iter()
        .map(
            |(id_str, sid_str, date_str, amount, vendor, matched, ignored, metadata_str)| {
                let metadata: HashMap<String, String> =
                    serde_json::from_str(&metadata_str).unwrap_or_default();
                Ok(ReconciledTxn {
                    txn_id: Uuid::parse_str(&id_str)?,
                    session_id: Uuid::parse_str(&sid_str)?,
                    date: NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")?,
                    amount,
                    vendor,
                    matched,
                    ignored,
                    metadata,
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
    metadata: &HashMap<String, String>,
) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    let metadata_json = serde_json::to_string(metadata).unwrap_or_else(|_| "{}".to_string());
    sqlx::query(
        "INSERT INTO reconciled_txns (txn_id, session_id, date, amount, vendor, metadata) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(session_id.to_string())
    .bind(date.to_string())
    .bind(amount)
    .bind(vendor)
    .bind(&metadata_json)
    .execute(pool)
    .await?;
    Ok(id)
}

/// Bulk insert reconciled transactions, deduplicating against existing (date, amount, vendor).
/// Uses INSERT OR IGNORE with a unique index to avoid N+1 queries.
/// Returns the count of new rows inserted.
pub async fn bulk_add_reconciled(
    pool: &SqlitePool,
    session_id: Uuid,
    txns: &[crate::models::csv_import::CsvRow],
) -> Result<usize, AppError> {
    let mut count = 0usize;
    for row in txns {
        let id = Uuid::now_v7();
        let metadata_json =
            serde_json::to_string(&row.metadata).unwrap_or_else(|_| "{}".to_string());
        let result = sqlx::query(
            "INSERT OR IGNORE INTO reconciled_txns (txn_id, session_id, date, amount, vendor, metadata) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(session_id.to_string())
        .bind(row.date.to_string())
        .bind(row.amount)
        .bind(&row.vendor)
        .bind(&metadata_json)
        .execute(pool)
        .await?;
        if result.rows_affected() > 0 {
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
    let result = sqlx::query(
        "INSERT OR IGNORE INTO match_links (match_id, outgoing_id, reconciled_id) VALUES (?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(outgoing_id.to_string())
    .bind(reconciled_id.to_string())
    .execute(pool)
    .await?;

    if result.rows_affected() > 0 {
        // New link created — mark both as matched
        sqlx::query("UPDATE outgoing_txns SET matched = TRUE WHERE txn_id = ?")
            .bind(outgoing_id.to_string())
            .execute(pool)
            .await?;
        sqlx::query("UPDATE reconciled_txns SET matched = TRUE WHERE txn_id = ?")
            .bind(reconciled_id.to_string())
            .execute(pool)
            .await?;
    }

    Ok(())
}

pub async fn unlink_transaction(pool: &SqlitePool, match_id: Uuid) -> Result<(), AppError> {
    // Get the pair and delete in one query
    let row = sqlx::query_as::<_, (String, String)>(
        "DELETE FROM match_links WHERE match_id = ? RETURNING outgoing_id, reconciled_id",
    )
    .bind(match_id.to_string())
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::BadRequest("Match not found".into()))?;

    let outgoing_id = Uuid::parse_str(&row.0)?;
    let reconciled_id = Uuid::parse_str(&row.1)?;

    // Only set matched = FALSE if no other match_links remain for this txn
    sqlx::query(
        "UPDATE outgoing_txns SET matched = FALSE WHERE txn_id = ? AND NOT EXISTS (SELECT 1 FROM match_links WHERE outgoing_id = ? AND deleted_at IS NULL)",
    )
    .bind(outgoing_id.to_string())
    .bind(outgoing_id.to_string())
    .execute(pool)
    .await?;

    sqlx::query(
        "UPDATE reconciled_txns SET matched = FALSE WHERE txn_id = ? AND NOT EXISTS (SELECT 1 FROM match_links WHERE reconciled_id = ? AND deleted_at IS NULL)",
    )
    .bind(reconciled_id.to_string())
    .bind(reconciled_id.to_string())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn list_matches(pool: &SqlitePool, session_id: Uuid) -> Result<Vec<MatchLink>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT m.match_id, m.outgoing_id, m.reconciled_id \
         FROM match_links m \
         JOIN outgoing_txns o ON m.outgoing_id = o.txn_id \
         JOIN reconciled_txns r ON m.reconciled_id = r.txn_id \
         WHERE o.session_id = ? AND o.deleted_at IS NULL AND r.deleted_at IS NULL AND m.deleted_at IS NULL",
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

/// Delete all match_links for an outgoing txn and update matched flags in one transaction.
pub async fn unlink_all_for_outgoing(pool: &SqlitePool, outgoing_id: Uuid) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM match_links WHERE outgoing_id = ?")
        .bind(outgoing_id.to_string())
        .execute(&mut *tx)
        .await?;

    sqlx::query("UPDATE outgoing_txns SET matched = FALSE WHERE txn_id = ?")
        .bind(outgoing_id.to_string())
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

/// Delete all match_links for a reconciled txn and update matched flags in one transaction.
pub async fn unlink_all_for_reconciled(
    pool: &SqlitePool,
    reconciled_id: Uuid,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM match_links WHERE reconciled_id = ?")
        .bind(reconciled_id.to_string())
        .execute(&mut *tx)
        .await?;

    sqlx::query("UPDATE reconciled_txns SET matched = FALSE WHERE txn_id = ?")
        .bind(reconciled_id.to_string())
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
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

pub async fn unignore_outgoing(pool: &SqlitePool, txn_id: Uuid) -> Result<(), AppError> {
    sqlx::query("UPDATE outgoing_txns SET ignored = FALSE WHERE txn_id = ?")
        .bind(txn_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn unignore_reconciled(pool: &SqlitePool, txn_id: Uuid) -> Result<(), AppError> {
    sqlx::query("UPDATE reconciled_txns SET ignored = FALSE WHERE txn_id = ?")
        .bind(txn_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_ignored_outgoing(
    pool: &SqlitePool,
    session_id: Uuid,
    sort: SortOrder,
) -> Result<Vec<OutgoingTxn>, AppError> {
    let rows = match sort {
        SortOrder::Date => {
            sqlx::query_as::<_, (String, String, String, i64, String, bool, bool, String)>(
                "SELECT txn_id, session_id, date, amount, vendor, matched, COALESCE(ignored, FALSE), COALESCE(metadata, '{}') FROM outgoing_txns WHERE session_id = ? AND deleted_at IS NULL AND ignored = TRUE ORDER BY date DESC, created_at DESC",
            )
            .bind(session_id.to_string())
            .fetch_all(pool)
            .await?
        }
        SortOrder::Amount => {
            sqlx::query_as::<_, (String, String, String, i64, String, bool, bool, String)>(
                "SELECT txn_id, session_id, date, amount, vendor, matched, COALESCE(ignored, FALSE), COALESCE(metadata, '{}') FROM outgoing_txns WHERE session_id = ? AND deleted_at IS NULL AND ignored = TRUE ORDER BY amount DESC, date, created_at",
            )
            .bind(session_id.to_string())
            .fetch_all(pool)
            .await?
        }
    };

    rows.into_iter()
        .map(
            |(id_str, sid_str, date_str, amount, vendor, matched, ignored, metadata_str)| {
                let metadata: HashMap<String, String> =
                    serde_json::from_str(&metadata_str).unwrap_or_default();
                Ok(OutgoingTxn {
                    txn_id: Uuid::parse_str(&id_str)?,
                    session_id: Uuid::parse_str(&sid_str)?,
                    date: NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")?,
                    amount,
                    vendor,
                    matched,
                    ignored,
                    metadata,
                })
            },
        )
        .collect()
}

pub async fn list_ignored_reconciled(
    pool: &SqlitePool,
    session_id: Uuid,
    sort: SortOrder,
) -> Result<Vec<ReconciledTxn>, AppError> {
    let rows = match sort {
        SortOrder::Date => {
            sqlx::query_as::<_, (String, String, String, i64, String, bool, bool, String)>(
                "SELECT txn_id, session_id, date, amount, vendor, matched, COALESCE(ignored, FALSE), COALESCE(metadata, '{}') FROM reconciled_txns WHERE session_id = ? AND deleted_at IS NULL AND ignored = TRUE ORDER BY date DESC, created_at DESC",
            )
            .bind(session_id.to_string())
            .fetch_all(pool)
            .await?
        }
        SortOrder::Amount => {
            sqlx::query_as::<_, (String, String, String, i64, String, bool, bool, String)>(
                "SELECT txn_id, session_id, date, amount, vendor, matched, COALESCE(ignored, FALSE), COALESCE(metadata, '{}') FROM reconciled_txns WHERE session_id = ? AND deleted_at IS NULL AND ignored = TRUE ORDER BY amount DESC, date, created_at",
            )
            .bind(session_id.to_string())
            .fetch_all(pool)
            .await?
        }
    };

    rows.into_iter()
        .map(
            |(id_str, sid_str, date_str, amount, vendor, matched, ignored, metadata_str)| {
                let metadata: HashMap<String, String> =
                    serde_json::from_str(&metadata_str).unwrap_or_default();
                Ok(ReconciledTxn {
                    txn_id: Uuid::parse_str(&id_str)?,
                    session_id: Uuid::parse_str(&sid_str)?,
                    date: NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")?,
                    amount,
                    vendor,
                    matched,
                    ignored,
                    metadata,
                })
            },
        )
        .collect()
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
    let outgoing = list_outgoing(pool, session_id, SortOrder::Amount).await?;
    let reconciled = list_reconciled(pool, session_id, SortOrder::Amount).await?;

    // Sort unmatched outgoing by amount descending so larger values get matched first
    let mut unmatched_outgoing: Vec<&OutgoingTxn> = outgoing
        .iter()
        .filter(|o| !o.matched && !skip_ids.contains(&o.txn_id))
        .collect();
    unmatched_outgoing.sort_by_key(|b| std::cmp::Reverse(b.amount));
    let unmatched_reconciled: Vec<&ReconciledTxn> =
        reconciled.iter().filter(|r| !r.matched).collect();

    let mut proposals = Vec::new();
    let mut used: std::collections::HashSet<Uuid> = std::collections::HashSet::new();

    for o in &unmatched_outgoing {
        // Try exact single match first
        if let Some(r) = unmatched_reconciled
            .iter()
            .find(|r| !used.contains(&r.txn_id) && r.amount == o.amount)
        {
            proposals.push(Proposal {
                outgoing_id: o.txn_id,
                reconciled_ids: vec![r.txn_id],
            });
            used.insert(r.txn_id);
            continue;
        }

        // Try sum match with up to 4 reconciled transactions
        let available: Vec<&ReconciledTxn> = unmatched_reconciled
            .iter()
            .filter(|r| !used.contains(&r.txn_id))
            .copied()
            .collect();

        if let Some(combo) = find_subset_sum(&available, o.amount, 4) {
            for r_id in &combo {
                used.insert(*r_id);
            }
            proposals.push(Proposal {
                outgoing_id: o.txn_id,
                reconciled_ids: combo,
            });
        }
    }

    Ok(proposals)
}

/// Find a subset of up to `max_len` items whose amounts sum to `target`.
/// Returns the txn_ids of the matching subset, or None.
///
/// Uses a hash-based approach:
/// - Pre-computes all 2-item sums into a HashMap (O(n²))
/// - 2-item match: O(1) lookup
/// - 3-item match: iterate items, check `target - amount` in 2-sum map (O(n))
/// - 4-item match: iterate 2-sum entries, check `target - sum` in 2-sum map (O(n²))
///
/// This avoids the O(2ⁿ) exponential blowup of the recursive subset-sum approach.
fn find_subset_sum(items: &[&ReconciledTxn], target: i64, max_len: usize) -> Option<Vec<Uuid>> {
    if items.len() < 2 || max_len < 2 {
        return None;
    }

    let n = items.len();
    let max_len = max_len.min(4);

    // Pre-compute all 2-item sums: HashMap<sum, Vec<[txn_id; 2]>>
    let mut sum2: HashMap<i64, Vec<[Uuid; 2]>> = HashMap::new();
    for i in 0..n {
        for j in i + 1..n {
            let sum = items[i].amount + items[j].amount;
            sum2.entry(sum)
                .or_default()
                .push([items[i].txn_id, items[j].txn_id]);
        }
    }

    // 2-item match
    if max_len >= 2
        && let Some(combos) = sum2.get(&target)
        && let Some(combo) = combos.first()
    {
        return Some(vec![combo[0], combo[1]]);
    }

    // 3-item match: for each item, check if target - amount is in sum2
    if max_len >= 3 {
        for item in items {
            let remaining = target - item.amount;
            if let Some(combos) = sum2.get(&remaining) {
                for combo in combos {
                    if combo[0] != item.txn_id && combo[1] != item.txn_id {
                        return Some(vec![item.txn_id, combo[0], combo[1]]);
                    }
                }
            }
        }
    }

    // 4-item match: 2+2 approach
    if max_len >= 4 {
        for (sum_a, combos_a) in &sum2 {
            let remaining = target - sum_a;
            if let Some(combos_b) = sum2.get(&remaining) {
                for combo_a in combos_a {
                    for combo_b in combos_b {
                        if combo_a[0] != combo_b[0]
                            && combo_a[0] != combo_b[1]
                            && combo_a[1] != combo_b[0]
                            && combo_a[1] != combo_b[1]
                        {
                            return Some(vec![combo_a[0], combo_a[1], combo_b[0], combo_b[1]]);
                        }
                    }
                }
            }
        }
    }

    None
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
            metadata: HashMap::new(),
        }
    }

    // ── find_subset_sum ──

    #[test]
    fn find_subset_sum_exact_single_match() {
        // Single item matching target — not found (starts at len=2)
        let t1 = txn("00000000-0000-0000-0000-000000000001", 100);
        let items = [&t1];
        assert_eq!(find_subset_sum(&items, 100, 4), None);
    }

    #[test]
    fn find_subset_sum_two_item_match() {
        let t1 = txn("00000000-0000-0000-0000-000000000001", 60);
        let t2 = txn("00000000-0000-0000-0000-000000000002", 40);
        let items = [&t1, &t2];
        let result = find_subset_sum(&items, 100, 4);
        assert!(result.is_some());
        let ids = result.unwrap();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn find_subset_sum_three_item_match() {
        let t1 = txn("00000000-0000-0000-0000-000000000001", 50);
        let t2 = txn("00000000-0000-0000-0000-000000000002", 30);
        let t3 = txn("00000000-0000-0000-0000-000000000003", 20);
        let items = [&t1, &t2, &t3];
        let result = find_subset_sum(&items, 100, 4);
        assert!(result.is_some());
    }

    #[test]
    fn find_subset_sum_no_match() {
        let t1 = txn("00000000-0000-0000-0000-000000000001", 10);
        let t2 = txn("00000000-0000-0000-0000-000000000002", 20);
        let items = [&t1, &t2];
        assert_eq!(find_subset_sum(&items, 100, 4), None);
    }

    #[test]
    fn find_subset_sum_empty_items() {
        let items: Vec<&ReconciledTxn> = vec![];
        assert_eq!(find_subset_sum(&items, 50, 4), None);
    }

    #[test]
    fn find_subset_sum_respects_max_len() {
        // 5 items that sum to 100, but max_len=3 so no match
        let t1 = txn("00000000-0000-0000-0000-000000000001", 20);
        let t2 = txn("00000000-0000-0000-0000-000000000002", 20);
        let t3 = txn("00000000-0000-0000-0000-000000000003", 20);
        let t4 = txn("00000000-0000-0000-0000-000000000004", 20);
        let t5 = txn("00000000-0000-0000-0000-000000000005", 20);
        let items = [&t1, &t2, &t3, &t4, &t5];
        assert_eq!(find_subset_sum(&items, 100, 3), None);
    }

    #[test]
    fn find_subset_sum_four_item_match() {
        let t1 = txn("00000000-0000-0000-0000-000000000001", 10);
        let t2 = txn("00000000-0000-0000-0000-000000000002", 20);
        let t3 = txn("00000000-0000-0000-0000-000000000003", 30);
        let t4 = txn("00000000-0000-0000-0000-000000000004", 40);
        let items = [&t1, &t2, &t3, &t4];
        let result = find_subset_sum(&items, 100, 4);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 4);
    }

    #[test]
    fn find_subset_sum_large_set_performance() {
        // 200 items — the old O(2^n) algorithm would hang here
        let items: Vec<ReconciledTxn> = (0..200)
            .map(|i| {
                txn(
                    &format!("00000000-0000-0000-0000-{:012}", i),
                    (i as i64 % 97) + 1,
                )
            })
            .collect();
        let refs: Vec<&ReconciledTxn> = items.iter().collect();
        // Should complete quickly and not find a match (sums don't align)
        let result = find_subset_sum(&refs, 999999, 4);
        assert_eq!(result, None);
    }
}
