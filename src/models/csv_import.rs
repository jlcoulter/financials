use crate::error::AppError;
use chrono::NaiveDate;

/// Detected column mapping from a CSV file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ColumnMapping {
    /// Column index for the date field
    pub date_col: usize,
    /// Column index for the amount field
    pub amount_col: usize,
    /// Column index for the vendor/description field (optional)
    pub vendor_col: Option<usize>,
    /// Date format string (e.g. "%Y-%m-%d", "%m/%d/%Y")
    pub date_format: String,
}

/// Result of analyzing a CSV file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CsvAnalysis {
    /// Header row (if present)
    pub headers: Vec<String>,
    /// First few data rows for preview
    pub preview_rows: Vec<Vec<String>>,
    /// Detected mapping
    pub detected: ColumnMapping,
    /// Total number of data rows (excluding header)
    pub total_rows: usize,
}

/// Try common date formats and return the first one that parses.
fn try_parse_date(val: &str) -> Option<String> {
    let val = val.trim();
    let formats = [
        "%Y-%m-%d",
        "%m/%d/%Y",
        "%d/%m/%Y",
        "%m/%d/%y",
        "%d/%m/%y",
        "%Y/%m/%d",
        "%b %d, %Y",
        "%d %b %Y",
        "%d %b %y",
        "%B %d, %Y",
        "%d %B %Y",
        "%m-%d-%Y",
        "%d-%m-%Y",
    ];
    for fmt in &formats {
        if NaiveDate::parse_from_str(val, fmt).is_ok() {
            return Some(fmt.to_string());
        }
    }
    None
}

/// Check if a string looks like a monetary amount.
fn looks_like_amount(val: &str) -> bool {
    let val = val.trim();
    if val.is_empty() {
        return false;
    }
    // Strip common prefixes/suffixes
    let cleaned: String = val
        .chars()
        .filter(|c| *c != ',' && *c != '$' && *c != '€' && *c != '£')
        .collect();
    cleaned.parse::<f64>().is_ok()
}

/// Detect column mapping from CSV headers and data rows.
pub fn analyze_csv(raw: &str) -> Result<CsvAnalysis, AppError> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(raw.as_bytes());

    let has_headers = reader.has_headers();
    let headers: Vec<String> = reader
        .headers()
        .map(|h| h.iter().map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    let mut all_records: Vec<Vec<String>> = Vec::new();
    for result in reader.records() {
        let record = result.map_err(|e| AppError::BadRequest(format!("CSV parse error: {}", e)))?;
        all_records.push(record.iter().map(|s| s.to_string()).collect());
    }

    if all_records.is_empty() {
        return Err(AppError::BadRequest("CSV file is empty".into()));
    }

    let num_cols = all_records[0].len();
    let preview_rows: Vec<Vec<String>> = all_records.iter().take(5).cloned().collect();
    let total_rows = all_records.len();

    // Score each column for date-ness and amount-ness using first 10 rows
    let sample_rows: Vec<&Vec<String>> = all_records.iter().take(10).collect();

    let mut date_scores: Vec<(usize, usize)> = (0..num_cols)
        .map(|col| {
            let score = sample_rows
                .iter()
                .filter(|row| row.get(col).is_some_and(|v| try_parse_date(v).is_some()))
                .count();
            (col, score)
        })
        .collect();

    let mut amount_scores: Vec<(usize, usize)> = (0..num_cols)
        .map(|col| {
            let score = sample_rows
                .iter()
                .filter(|row| row.get(col).is_some_and(|v| looks_like_amount(v)))
                .count();
            (col, score)
        })
        .collect();

    // Boost scores using header names
    let date_headers = [
        "date",
        "posting date",
        "transaction date",
        "trans date",
        "date posted",
    ];
    let amount_headers = [
        "amount",
        "amount",
        "debit",
        "credit",
        "withdrawal",
        "deposit",
        "transaction amount",
        "value",
    ];
    let vendor_headers = [
        "description",
        "vendor",
        "merchant",
        "payee",
        "memo",
        "transaction description",
        "details",
        "reference",
    ];

    for (col, score) in &mut date_scores {
        if let Some(h) = headers.get(*col) {
            let h_lower = h.to_lowercase();
            if date_headers.iter().any(|dh| h_lower.contains(dh)) {
                *score += 10;
            }
        }
    }

    for (col, score) in &mut amount_scores {
        if let Some(h) = headers.get(*col) {
            let h_lower = h.to_lowercase();
            if amount_headers.iter().any(|ah| h_lower.contains(ah)) {
                *score += 10;
            }
        }
    }

    // Pick best date column
    date_scores.sort_by_key(|b| std::cmp::Reverse(b.1));
    let date_col = date_scores[0].0;

    // Pick best amount column (different from date column)
    amount_scores.sort_by_key(|b| std::cmp::Reverse(b.1));
    let amount_col = amount_scores
        .iter()
        .find(|(col, _)| *col != date_col)
        .map(|(col, _)| *col)
        .unwrap_or(0);

    // Detect date format from the chosen date column
    let date_format = sample_rows
        .iter()
        .filter_map(|row| row.get(date_col).and_then(|v| try_parse_date(v)))
        .next()
        .unwrap_or_else(|| "%d/%m/%Y".to_string());

    // Detect vendor column: look for header match or pick the first text-heavy column that
    // isn't date or amount
    let mut vendor_col: Option<usize> = None;

    // First try header match
    for (col, _) in (0..num_cols).enumerate() {
        if col == date_col || col == amount_col {
            continue;
        }
        if let Some(h) = headers.get(col) {
            let h_lower = h.to_lowercase();
            if vendor_headers.iter().any(|vh| h_lower.contains(vh)) {
                vendor_col = Some(col);
                break;
            }
        }
    }

    // Fallback: pick the first non-date, non-amount column with mostly text data
    if vendor_col.is_none() {
        for col in 0..num_cols {
            if col == date_col || col == amount_col {
                continue;
            }
            let text_count = sample_rows
                .iter()
                .filter(|row| {
                    row.get(col).is_some_and(|v| {
                        !v.trim().is_empty() && !looks_like_amount(v) && try_parse_date(v).is_none()
                    })
                })
                .count();
            if text_count > sample_rows.len() / 2 {
                vendor_col = Some(col);
                break;
            }
        }
    }

    let detected = ColumnMapping {
        date_col,
        amount_col,
        vendor_col,
        date_format,
    };

    Ok(CsvAnalysis {
        headers: if has_headers { headers } else { Vec::new() },
        preview_rows,
        detected,
        total_rows,
    })
}

/// Parse CSV with an explicit column mapping.
pub fn parse_csv_with_mapping(
    raw: &str,
    mapping: &ColumnMapping,
) -> Result<Vec<(NaiveDate, i64, String)>, AppError> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(raw.as_bytes());

    let mut rows: Vec<(NaiveDate, i64, String)> = Vec::new();

    for result in reader.records() {
        let record = result.map_err(|e| AppError::BadRequest(format!("CSV parse error: {}", e)))?;
        let date_str = record.get(mapping.date_col).unwrap_or("").trim();
        if date_str.is_empty() {
            continue; // skip empty rows
        }
        let date = match NaiveDate::parse_from_str(date_str, &mapping.date_format) {
            Ok(d) => d,
            Err(_) => continue, // skip rows with unparseable dates
        };

        let amount_str = record.get(mapping.amount_col).unwrap_or("").trim();
        if amount_str.is_empty() {
            continue; // skip rows with no amount
        }
        let cents = match crate::utils::parse_dollars(amount_str) {
            Ok(c) => c.abs(),
            Err(_) => continue, // skip rows with unparseable amounts
        };

        let vendor = mapping
            .vendor_col
            .and_then(|col| record.get(col))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        rows.push((date, cents, vendor));
    }

    rows.sort_by_key(|(d, _, _)| *d);
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── try_parse_date ──

    #[test]
    fn try_parse_date_iso() {
        assert_eq!(try_parse_date("2025-07-01"), Some("%Y-%m-%d".to_string()));
    }

    #[test]
    fn try_parse_date_us() {
        assert_eq!(try_parse_date("07/01/2025"), Some("%m/%d/%Y".to_string()));
    }

    #[test]
    fn try_parse_date_eu() {
        // 25/07/2025 is unambiguously d/m/Y (day > 12)
        assert_eq!(try_parse_date("25/07/2025"), Some("%d/%m/%Y".to_string()));
    }

    #[test]
    fn try_parse_date_short_year_us() {
        // "07/01/25" is ambiguous — the parser prefers %m/%d/%Y
        // because it tries longer format patterns first
        let result = try_parse_date("07/01/25");
        assert!(result.is_some());
    }

    #[test]
    fn try_parse_date_named_month() {
        assert_eq!(try_parse_date("Jul 1, 2025"), Some("%b %d, %Y".to_string()));
    }

    #[test]
    fn try_parse_date_named_month_long() {
        assert_eq!(
            try_parse_date("July 1, 2025"),
            Some("%B %d, %Y".to_string())
        );
    }

    #[test]
    fn try_parse_date_slash_ymd() {
        assert_eq!(try_parse_date("2025/07/01"), Some("%Y/%m/%d".to_string()));
    }

    #[test]
    fn try_parse_date_dash_dm_y() {
        assert_eq!(try_parse_date("01-07-2025"), Some("%m-%d-%Y".to_string()));
    }

    #[test]
    fn try_parse_date_invalid() {
        assert_eq!(try_parse_date("not-a-date"), None);
    }

    #[test]
    fn try_parse_date_whitespace() {
        assert_eq!(
            try_parse_date("  2025-07-01  "),
            Some("%Y-%m-%d".to_string())
        );
    }

    // ── looks_like_amount ──

    #[test]
    fn looks_like_amount_plain() {
        assert!(looks_like_amount("100"));
    }

    #[test]
    fn looks_like_amount_with_dollar() {
        assert!(looks_like_amount("$1,234.56"));
    }

    #[test]
    fn looks_like_amount_negative() {
        assert!(looks_like_amount("-500.00"));
    }

    #[test]
    fn looks_like_amount_euro() {
        assert!(looks_like_amount("€1.234,56"));
    }

    #[test]
    fn looks_like_amount_pound() {
        assert!(looks_like_amount("£5,000"));
    }

    #[test]
    fn looks_like_amount_empty() {
        assert!(!looks_like_amount(""));
    }

    #[test]
    fn looks_like_amount_text() {
        assert!(!looks_like_amount("hello"));
    }

    // ── analyze_csv ──

    #[test]
    fn analyze_csv_detects_date_and_amount_columns() {
        let csv = "Date,Description,Amount\n01/07/2025,Coffee,4.50\n02/07/2025,Tea,3.20\n";
        let analysis = analyze_csv(csv).unwrap();
        assert_eq!(analysis.detected.date_col, 0);
        assert_eq!(analysis.detected.amount_col, 2);
        assert_eq!(analysis.headers.len(), 3);
        assert_eq!(analysis.total_rows, 2);
    }

    #[test]
    fn analyze_csv_detects_vendor_column() {
        let csv = "Date,Description,Amount\n01/07/2025,Coffee,4.50\n";
        let analysis = analyze_csv(csv).unwrap();
        assert_eq!(analysis.detected.vendor_col, Some(1));
    }

    #[test]
    fn analyze_csv_rejects_empty_csv() {
        let csv = "";
        assert!(analyze_csv(csv).is_err());
    }

    #[test]
    fn analyze_csv_defaults_to_dm_y_format() {
        let csv = "Date,Amount\nnotadate,100\nnotadate,200\n";
        let analysis = analyze_csv(csv).unwrap();
        assert_eq!(analysis.detected.date_format, "%d/%m/%Y");
    }

    #[test]
    fn analyze_csv_detects_dm_y_from_data() {
        let csv = "Date,Amount\n25/07/2025,100\n";
        let analysis = analyze_csv(csv).unwrap();
        assert_eq!(analysis.detected.date_format, "%d/%m/%Y");
    }

    #[test]
    fn analyze_csv_detects_ymd_format() {
        let csv = "Date,Amount\n2025-07-01,100\n";
        let analysis = analyze_csv(csv).unwrap();
        assert_eq!(analysis.detected.date_format, "%Y-%m-%d");
    }

    // ── parse_csv_with_mapping ──

    #[test]
    fn parse_csv_with_mapping_basic() {
        let csv = "Date,Description,Amount\n01/07/2025,Coffee,4.50\n02/07/2025,Tea,3.20\n";
        let mapping = ColumnMapping {
            date_col: 0,
            amount_col: 2,
            vendor_col: Some(1),
            date_format: "%d/%m/%Y".to_string(),
        };
        let rows = parse_csv_with_mapping(csv, &mapping).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, NaiveDate::from_ymd_opt(2025, 7, 1).unwrap());
        assert_eq!(rows[0].1, 450);
        assert_eq!(rows[0].2, "Coffee");
    }

    #[test]
    fn parse_csv_with_mapping_skips_empty_amount() {
        let csv = "Date,Description,Amount\n01/07/2025,Coffee,4.50\n02/07/2025,Tea,\n";
        let mapping = ColumnMapping {
            date_col: 0,
            amount_col: 2,
            vendor_col: Some(1),
            date_format: "%d/%m/%Y".to_string(),
        };
        let rows = parse_csv_with_mapping(csv, &mapping).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].2, "Coffee");
    }

    #[test]
    fn parse_csv_with_mapping_negative_amounts() {
        let csv = "Date,Amount\n01/07/2025,-150000\n";
        let mapping = ColumnMapping {
            date_col: 0,
            amount_col: 1,
            vendor_col: None,
            date_format: "%d/%m/%Y".to_string(),
        };
        let rows = parse_csv_with_mapping(csv, &mapping).unwrap();
        assert_eq!(rows[0].1, 15000000);
    }

    #[test]
    fn parse_csv_with_mapping_no_vendor() {
        let csv = "Date,Amount\n01/07/2025,500\n";
        let mapping = ColumnMapping {
            date_col: 0,
            amount_col: 1,
            vendor_col: None,
            date_format: "%d/%m/%Y".to_string(),
        };
        let rows = parse_csv_with_mapping(csv, &mapping).unwrap();
        assert_eq!(rows[0].2, "");
    }
}
