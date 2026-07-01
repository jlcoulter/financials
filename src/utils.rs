pub fn parse_dollars(s: &str) -> Result<i64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Invalid amount: (empty)".into());
    }
    // Handle accounting-style parentheses for negatives: (1,234.56) → -1234.56
    let is_negative = if s.starts_with('(') && s.ends_with(')') {
        true
    } else {
        s.starts_with('-')
    };
    // Strip leading signs, currency symbols, and parentheses
    let cleaned: String = s
        .chars()
        .filter(|c| {
            !c.is_whitespace()
                && *c != ','
                && *c != '$'
                && *c != '€'
                && *c != '£'
                && *c != '('
                && *c != ')'
                && *c != '-'
        })
        .collect();
    let val: f64 = cleaned
        .parse()
        .map_err(|_| format!("Invalid amount: {}", s))?;
    let cents = (val * 100.0).round() as i64;
    Ok(if is_negative { -cents } else { cents })
}

pub fn format_cents(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.abs();
    let dollars = abs / 100;
    let remainder = abs % 100;
    format!("{}${}.{:02}", sign, format_with_commas(dollars), remainder)
}

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
