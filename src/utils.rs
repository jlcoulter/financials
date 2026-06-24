/// Format an integer with thousands separators (commas).
fn format_with_commas(n: i64) -> String {
    let s = n.abs().to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format cents as a dollar string with sign and thousands separators: -$1,234.56 or $1,234.56
pub fn format_cents(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.abs();
    let dollars = abs / 100;
    let remainder = abs % 100;
    format!("{}${}.{:02}", sign, format_with_commas(dollars), remainder)
}

/// Format cents as dollars with thousands separators but without $ sign: -1,234.56 or 1,234.56
pub fn format_dollars(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.abs();
    let dollars = abs / 100;
    let remainder = abs % 100;
    format!("{}{}.{:02}", sign, format_with_commas(dollars), remainder)
}

/// Parse a dollar string like "1234.56" or "1,234.56" into cents (i64).
/// Returns an error if the string is not a valid number.
pub fn parse_dollars(s: &str) -> Result<i64, String> {
    let cleaned: String = s.chars().filter(|c| *c != ',').collect();
    let val: f64 = cleaned.parse().map_err(|_| format!("Invalid amount: {}", s))?;
    if !val.is_finite() {
        return Err(format!("Invalid amount: {}", s));
    }
    Ok((val * 100.0).round() as i64)
}

/// Validate a YYYY-MM date string
pub fn validate_month(month: &str) -> Result<(), String> {
    let parts: Vec<&str> = month.split('-').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid month format: {} (expected YYYY-MM)", month));
    }
    let year: i32 = parts[0].parse().map_err(|_| format!("Invalid year in: {}", month))?;
    let mon: u32 = parts[1].parse().map_err(|_| format!("Invalid month in: {}", month))?;
    if year < 2000 || year > 2100 || mon < 1 || mon > 12 {
        return Err(format!("Month out of range: {}", month));
    }
    Ok(())
}

/// Validate that an amount string is a valid positive number of dollars
pub fn validate_amount(amount: &str, field: &str) -> Result<i64, String> {
    let cents = parse_dollars(amount)?;
    if cents == 0 {
        return Err(format!("{} cannot be zero", field));
    }
    Ok(cents)
}

/// Validate a YYYY-MM-DD date string
pub fn validate_date(date: &str, field: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| format!("Invalid date for {}: {} (expected YYYY-MM-DD)", field, date))
}

/// Escape HTML special characters to prevent XSS in raw HTML strings.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}