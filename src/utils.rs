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

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_dollars ──

    #[test]
    fn parse_dollars_positive() {
        assert_eq!(parse_dollars("$1,234.56").unwrap(), 123456);
    }

    #[test]
    fn parse_dollars_negative() {
        assert_eq!(parse_dollars("-$1,234.56").unwrap(), -123456);
    }

    #[test]
    fn parse_dollars_accounting_parens() {
        assert_eq!(parse_dollars("($1,234.56)").unwrap(), -123456);
    }

    #[test]
    fn parse_dollars_plain_number() {
        assert_eq!(parse_dollars("10000").unwrap(), 1000000);
    }

    #[test]
    fn parse_dollars_negative_plain() {
        assert_eq!(parse_dollars("-150000").unwrap(), -15000000);
    }

    #[test]
    fn parse_dollars_empty() {
        assert!(parse_dollars("").is_err());
    }

    #[test]
    fn parse_dollars_zero() {
        assert_eq!(parse_dollars("$0.00").unwrap(), 0);
        assert_eq!(parse_dollars("0").unwrap(), 0);
    }

    #[test]
    fn parse_dollars_euro() {
        // Euro format is not specially handled; after stripping € and ,
        // "€1.234,56" becomes "1.23456" which is ~123 cents
        assert_eq!(parse_dollars("€1.234,56").unwrap(), 123);
    }

    #[test]
    fn parse_dollars_pound() {
        assert_eq!(parse_dollars("£5,000.00").unwrap(), 500000);
    }

    #[test]
    fn parse_dollars_whitespace() {
        assert_eq!(parse_dollars("  $1,234.56  ").unwrap(), 123456);
    }

    #[test]
    fn parse_dollars_just_currency_symbol() {
        assert!(parse_dollars("$").is_err());
        assert!(parse_dollars("€").is_err());
    }

    #[test]
    fn parse_dollars_large_number() {
        assert_eq!(parse_dollars("$1,000,000.00").unwrap(), 100000000);
    }

    // ── format_cents ──

    #[test]
    fn format_cents_positive() {
        assert_eq!(format_cents(123456), "$1,234.56");
    }

    #[test]
    fn format_cents_negative() {
        assert_eq!(format_cents(-123456), "-$1,234.56");
    }

    #[test]
    fn format_cents_zero() {
        assert_eq!(format_cents(0), "$0.00");
    }

    #[test]
    fn format_cents_millions() {
        assert_eq!(format_cents(123456789), "$1,234,567.89");
    }

    #[test]
    fn format_cents_small() {
        assert_eq!(format_cents(1), "$0.01");
        assert_eq!(format_cents(99), "$0.99");
    }

    #[test]
    fn format_cents_roundtrip() {
        for &cents in &[0, 1, 99, 100, 999, 123456, -100, -999999] {
            assert_eq!(parse_dollars(&format_cents(cents)).unwrap(), cents);
        }
    }

    // ── format_with_commas ──

    #[test]
    fn format_with_commas_zero() {
        assert_eq!(format_with_commas(0), "0");
    }

    #[test]
    fn format_with_commas_one() {
        assert_eq!(format_with_commas(1), "1");
    }

    #[test]
    fn format_with_commas_three_digits() {
        assert_eq!(format_with_commas(999), "999");
    }

    #[test]
    fn format_with_commas_thousands() {
        assert_eq!(format_with_commas(1000), "1,000");
    }

    #[test]
    fn format_with_commas_millions() {
        assert_eq!(format_with_commas(1000000), "1,000,000");
    }
}
