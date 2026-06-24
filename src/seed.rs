use sqlx::SqlitePool;
use uuid::Uuid;

/// Seed the database with realistic family financial data.
/// Only runs if the portfolios table is empty.
pub async fn seed_if_empty(pool: &SqlitePool) -> anyhow::Result<()> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM portfolios")
        .fetch_one(pool)
        .await?;
    if count.0 > 0 {
        tracing::info!("Database already has data, skipping seed");
        return Ok(());
    }
    tracing::info!("Seeding database with sample family portfolio...");
    seed(pool).await
}

pub async fn seed(pool: &SqlitePool) -> anyhow::Result<()> {
    // ── Create demo user first (needed for FK) ──
    let seed_user = "demo";
    let hash = bcrypt::hash("demo", bcrypt::DEFAULT_COST)?;
    sqlx::query("INSERT OR IGNORE INTO users (username, password_hash) VALUES (?, ?)")
        .bind(seed_user)
        .bind(&hash)
        .execute(pool)
        .await?;

    // ── Portfolio: "Family Finances" ──
    let portfolio_id = Uuid::now_v7();
    sqlx::query("INSERT INTO portfolios (portfolio_id, name, user_id) VALUES (?, ?, ?)")
        .bind(portfolio_id.to_string())
        .bind("Family Finances")
        .bind(seed_user)
        .execute(pool)
        .await?;

    // ── Wealth Items ──
    // Assets
    let home_equity = insert_item(pool, portfolio_id, "Home Equity", "asset").await?;
    let savings = insert_item(pool, portfolio_id, "Savings Account", "asset").await?;
    let emergency_fund = insert_item(pool, portfolio_id, "Emergency Fund", "asset").await?;
    let car_value = insert_item(pool, portfolio_id, "Car Value", "asset").await?;

    // Investments
    let retirement_401k = insert_item(pool, portfolio_id, "401(k)", "investment").await?;
    let roth_ira = insert_item(pool, portfolio_id, "Roth IRA", "investment").await?;
    let brokerage = insert_item(pool, portfolio_id, "Brokerage", "investment").await?;
    let college_529 = insert_item(pool, portfolio_id, "529 College Fund", "investment").await?;

    // Debts
    let mortgage = insert_item(pool, portfolio_id, "Mortgage", "debt").await?;
    let student_loans = insert_item(pool, portfolio_id, "Student Loans", "debt").await?;
    let car_loan = insert_item(pool, portfolio_id, "Car Loan", "debt").await?;
    let credit_cards = insert_item(pool, portfolio_id, "Credit Cards", "debt").await?;

    // ── 2 years of monthly balance history ──
    // Realistic family trajectory:
    //   - Home equity rises slowly (appreciation ~3-4%/yr)
    //   - Car value declines (depreciation)
    //   - Savings & emergency fund grow steadily
    //   - Investments have a mid-2024 dip (market correction) then recovery
    //   - Debts decline (regular payments)
    //   - Credit cards spike around holidays (Nov/Dec) then paid down

    let data: Vec<(Uuid, &str, i64)> = vec![
        // ── 2024 ──

        // Jan 2024 — starting point
        (home_equity,    "2024-01-01", 175_000_00),
        (savings,        "2024-01-01",   8_200_00),
        (emergency_fund, "2024-01-01",  10_000_00),
        (car_value,      "2024-01-01",  24_000_00),
        (retirement_401k,"2024-01-01", 128_000_00),
        (roth_ira,       "2024-01-01",  33_000_00),
        (brokerage,      "2024-01-01",  22_500_00),
        (college_529,    "2024-01-01",  14_500_00),
        (mortgage,       "2024-01-01", 272_000_00),
        (student_loans,  "2024-01-01",  38_500_00),
        (car_loan,       "2024-01-01",  18_000_00),
        (credit_cards,   "2024-01-01",   4_500_00),

        // Feb 2024
        (home_equity,    "2024-02-01", 175_800_00),
        (savings,        "2024-02-01",   8_600_00),
        (emergency_fund, "2024-02-01",  10_300_00),
        (car_value,      "2024-02-01",  23_500_00),
        (retirement_401k,"2024-02-01", 130_500_00),
        (roth_ira,       "2024-02-01",  33_800_00),
        (brokerage,      "2024-02-01",  22_900_00),
        (college_529,    "2024-02-01",  14_800_00),
        (mortgage,       "2024-02-01", 271_200_00),
        (student_loans,  "2024-02-01",  38_000_00),
        (car_loan,       "2024-02-01",  17_300_00),
        (credit_cards,   "2024-02-01",   4_100_00),

        // Mar 2024
        (home_equity,    "2024-03-01", 176_800_00),
        (savings,        "2024-03-01",   9_000_00),
        (emergency_fund, "2024-03-01",  10_600_00),
        (car_value,      "2024-03-01",  23_000_00),
        (retirement_401k,"2024-03-01", 132_800_00),
        (roth_ira,       "2024-03-01",  34_500_00),
        (brokerage,      "2024-03-01",  23_400_00),
        (college_529,    "2024-03-01",  15_100_00),
        (mortgage,       "2024-03-01", 270_400_00),
        (student_loans,  "2024-03-01",  37_500_00),
        (car_loan,       "2024-03-01",  16_600_00),
        (credit_cards,   "2024-03-01",   3_700_00),

        // Apr 2024
        (home_equity,    "2024-04-01", 177_500_00),
        (savings,        "2024-04-01",   9_400_00),
        (emergency_fund, "2024-04-01",  11_000_00),
        (car_value,      "2024-04-01",  22_500_00),
        (retirement_401k,"2024-04-01", 134_200_00),
        (roth_ira,       "2024-04-01",  35_100_00),
        (brokerage,      "2024-04-01",  23_800_00),
        (college_529,    "2024-04-01",  15_400_00),
        (mortgage,       "2024-04-01", 269_600_00),
        (student_loans,  "2024-04-01",  37_000_00),
        (car_loan,       "2024-04-01",  15_900_00),
        (credit_cards,   "2024-04-01",   3_200_00),

        // May 2024
        (home_equity,    "2024-05-01", 178_500_00),
        (savings,        "2024-05-01",   9_800_00),
        (emergency_fund, "2024-05-01",  11_300_00),
        (car_value,      "2024-05-01",  22_000_00),
        (retirement_401k,"2024-05-01", 136_000_00),
        (roth_ira,       "2024-05-01",  35_800_00),
        (brokerage,      "2024-05-01",  24_500_00),
        (college_529,    "2024-05-01",  15_700_00),
        (mortgage,       "2024-05-01", 268_800_00),
        (student_loans,  "2024-05-01",  36_500_00),
        (car_loan,       "2024-05-01",  15_200_00),
        (credit_cards,   "2024-05-01",   2_800_00),

        // Jun 2024
        (home_equity,    "2024-06-01", 179_200_00),
        (savings,        "2024-06-01",  10_200_00),
        (emergency_fund, "2024-06-01",  11_600_00),
        (car_value,      "2024-06-01",  21_500_00),
        (retirement_401k,"2024-06-01", 137_500_00),
        (roth_ira,       "2024-06-01",  36_200_00),
        (brokerage,      "2024-06-01",  25_000_00),
        (college_529,    "2024-06-01",  16_100_00),
        (mortgage,       "2024-06-01", 268_000_00),
        (student_loans,  "2024-06-01",  36_000_00),
        (car_loan,       "2024-06-01",  14_500_00),
        (credit_cards,   "2024-06-01",   2_400_00),

        // Jul 2024 — market correction starts
        (home_equity,    "2024-07-01", 179_800_00),
        (savings,        "2024-07-01",  10_500_00),
        (emergency_fund, "2024-07-01",  12_000_00),
        (car_value,      "2024-07-01",  21_200_00),
        (retirement_401k,"2024-07-01", 133_000_00),  // dip
        (roth_ira,       "2024-07-01",  35_000_00),   // dip
        (brokerage,      "2024-07-01",  23_800_00),   // dip
        (college_529,    "2024-07-01",  15_800_00),   // dip
        (mortgage,       "2024-07-01", 267_200_00),
        (student_loans,  "2024-07-01",  35_500_00),
        (car_loan,       "2024-07-01",  13_800_00),
        (credit_cards,   "2024-07-01",   2_700_00),

        // Aug 2024 — correction bottom
        (home_equity,    "2024-08-01", 180_200_00),
        (savings,        "2024-08-01",  10_800_00),
        (emergency_fund, "2024-08-01",  12_300_00),
        (car_value,      "2024-08-01",  20_800_00),
        (retirement_401k,"2024-08-01", 131_500_00),  // lowest point
        (roth_ira,       "2024-08-01",  34_200_00),
        (brokerage,      "2024-08-01",  23_200_00),
        (college_529,    "2024-08-01",  15_500_00),
        (mortgage,       "2024-08-01", 266_400_00),
        (student_loans,  "2024-08-01",  35_000_00),
        (car_loan,       "2024-08-01",  13_100_00),
        (credit_cards,   "2024-08-01",   3_100_00),

        // Sep 2024 — recovery begins
        (home_equity,    "2024-09-01", 181_000_00),
        (savings,        "2024-09-01",  11_200_00),
        (emergency_fund, "2024-09-01",  12_800_00),
        (car_value,      "2024-09-01",  20_500_00),
        (retirement_401k,"2024-09-01", 135_000_00),
        (roth_ira,       "2024-09-01",  35_500_00),
        (brokerage,      "2024-09-01",  24_000_00),
        (college_529,    "2024-09-01",  16_000_00),
        (mortgage,       "2024-09-01", 265_600_00),
        (student_loans,  "2024-09-01",  34_500_00),
        (car_loan,       "2024-09-01",  12_400_00),
        (credit_cards,   "2024-09-01",   2_900_00),

        // Oct 2024
        (home_equity,    "2024-10-01", 181_800_00),
        (savings,        "2024-10-01",  11_500_00),
        (emergency_fund, "2024-10-01",  13_200_00),
        (car_value,      "2024-10-01",  20_200_00),
        (retirement_401k,"2024-10-01", 138_000_00),
        (roth_ira,       "2024-10-01",  36_500_00),
        (brokerage,      "2024-10-01",  24_800_00),
        (college_529,    "2024-10-01",  16_500_00),
        (mortgage,       "2024-10-01", 264_800_00),
        (student_loans,  "2024-10-01",  34_000_00),
        (car_loan,       "2024-10-01",  11_700_00),
        (credit_cards,   "2024-10-01",   3_400_00),

        // Nov 2024 — holiday spending starts
        (home_equity,    "2024-11-01", 182_500_00),
        (savings,        "2024-11-01",  11_200_00),   // dip (holiday shopping)
        (emergency_fund, "2024-11-01",  13_500_00),
        (car_value,      "2024-11-01",  19_800_00),
        (retirement_401k,"2024-11-01", 140_000_00),
        (roth_ira,       "2024-11-01",  37_200_00),
        (brokerage,      "2024-11-01",  25_500_00),
        (college_529,    "2024-11-01",  16_900_00),
        (mortgage,       "2024-11-01", 264_000_00),
        (student_loans,  "2024-11-01",  33_500_00),
        (car_loan,       "2024-11-01",  11_000_00),
        (credit_cards,   "2024-11-01",   4_800_00),   // spike

        // Dec 2024 — peak holiday debt
        (home_equity,    "2024-12-01", 183_200_00),
        (savings,        "2024-12-01",  10_800_00),   // low (gifts)
        (emergency_fund, "2024-12-01",  13_500_00),
        (car_value,      "2024-12-01",  19_500_00),
        (retirement_401k,"2024-12-01", 141_500_00),
        (roth_ira,       "2024-12-01",  37_800_00),
        (brokerage,      "2024-12-01",  26_000_00),
        (college_529,    "2024-12-01",  17_200_00),
        (mortgage,       "2024-12-01", 263_200_00),
        (student_loans,  "2024-12-01",  33_000_00),
        (car_loan,       "2024-12-01",  10_300_00),
        (credit_cards,   "2024-12-01",   6_200_00),   // peak

        // ── 2025 ──

        // Jan 2025 — paying down holiday debt
        (home_equity,    "2025-01-01", 183_800_00),
        (savings,        "2025-01-01",  11_200_00),
        (emergency_fund, "2025-01-01",  14_000_00),
        (car_value,      "2025-01-01",  19_300_00),
        (retirement_401k,"2025-01-01", 143_000_00),
        (roth_ira,       "2025-01-01",  38_500_00),
        (brokerage,      "2025-01-01",  26_500_00),
        (college_529,    "2025-01-01",  17_500_00),
        (mortgage,       "2025-01-01", 262_400_00),
        (student_loans,  "2025-01-01",  32_500_00),
        (car_loan,       "2025-01-01",   9_800_00),
        (credit_cards,   "2025-01-01",   5_100_00),   // paying down

        // Feb 2025
        (home_equity,    "2025-02-01", 184_500_00),
        (savings,        "2025-02-01",  11_800_00),
        (emergency_fund, "2025-02-01",  14_500_00),
        (car_value,      "2025-02-01",  19_000_00),
        (retirement_401k,"2025-02-01", 145_500_00),
        (roth_ira,       "2025-02-01",  39_500_00),
        (brokerage,      "2025-02-01",  27_200_00),
        (college_529,    "2025-02-01",  17_900_00),
        (mortgage,       "2025-02-01", 261_600_00),
        (student_loans,  "2025-02-01",  32_000_00),
        (car_loan,       "2025-02-01",   9_100_00),
        (credit_cards,   "2025-02-01",   3_800_00),

        // Mar 2025 — small market dip
        (home_equity,    "2025-03-01", 185_500_00),
        (savings,        "2025-03-01",  12_200_00),
        (emergency_fund, "2025-03-01",  15_000_00),
        (car_value,      "2025-03-01",  18_700_00),
        (retirement_401k,"2025-03-01", 142_000_00),  // dip
        (roth_ira,       "2025-03-01",  38_800_00),   // dip
        (brokerage,      "2025-03-01",  26_500_00),   // dip
        (college_529,    "2025-03-01",  17_800_00),
        (mortgage,       "2025-03-01", 260_800_00),
        (student_loans,  "2025-03-01",  31_500_00),
        (car_loan,       "2025-03-01",   8_400_00),
        (credit_cards,   "2025-03-01",   3_200_00),

        // Apr 2025 — recovery
        (home_equity,    "2025-04-01", 186_500_00),
        (savings,        "2025-04-01",  12_800_00),
        (emergency_fund, "2025-04-01",  15_500_00),
        (car_value,      "2025-04-01",  18_500_00),
        (retirement_401k,"2025-04-01", 148_000_00),  // recovery
        (roth_ira,       "2025-04-01",  40_500_00),
        (brokerage,      "2025-04-01",  28_000_00),
        (college_529,    "2025-04-01",  18_300_00),
        (mortgage,       "2025-04-01", 260_000_00),
        (student_loans,  "2025-04-01",  31_000_00),
        (car_loan,       "2025-04-01",   7_700_00),
        (credit_cards,   "2025-04-01",   2_600_00),

        // May 2025
        (home_equity,    "2025-05-01", 187_500_00),
        (savings,        "2025-05-01",  13_500_00),
        (emergency_fund, "2025-05-01",  16_000_00),
        (car_value,      "2025-05-01",  18_200_00),
        (retirement_401k,"2025-05-01", 152_000_00),
        (roth_ira,       "2025-05-01",  42_000_00),
        (brokerage,      "2025-05-01",  29_200_00),
        (college_529,    "2025-05-01",  18_800_00),
        (mortgage,       "2025-05-01", 259_200_00),
        (student_loans,  "2025-05-01",  30_500_00),
        (car_loan,       "2025-05-01",   7_000_00),
        (credit_cards,   "2025-05-01",   2_200_00),

        // Jun 2025 — car loan paid off this month!
        (home_equity,    "2025-06-01", 188_500_00),
        (savings,        "2025-06-01",  14_200_00),
        (emergency_fund, "2025-06-01",  16_500_00),
        (car_value,      "2025-06-01",  18_000_00),
        (retirement_401k,"2025-06-01", 155_000_00),
        (roth_ira,       "2025-06-01",  43_200_00),
        (brokerage,      "2025-06-01",  30_000_00),
        (college_529,    "2025-06-01",  19_200_00),
        (mortgage,       "2025-06-01", 258_400_00),
        (student_loans,  "2025-06-01",  30_000_00),
        (car_loan,       "2025-06-01",   6_300_00),
        (credit_cards,   "2025-06-01",   1_800_00),

        // Jul 2025
        (home_equity,    "2025-07-01", 189_200_00),
        (savings,        "2025-07-01",  14_800_00),
        (emergency_fund, "2025-07-01",  17_000_00),
        (car_value,      "2025-07-01",  17_800_00),
        (retirement_401k,"2025-07-01", 157_500_00),
        (roth_ira,       "2025-07-01",  44_000_00),
        (brokerage,      "2025-07-01",  30_800_00),
        (college_529,    "2025-07-01",  19_600_00),
        (mortgage,       "2025-07-01", 257_600_00),
        (student_loans,  "2025-07-01",  29_500_00),
        (car_loan,       "2025-07-01",   5_600_00),
        (credit_cards,   "2025-07-01",   1_500_00),

        // Aug 2025
        (home_equity,    "2025-08-01", 190_000_00),
        (savings,        "2025-08-01",  15_200_00),
        (emergency_fund, "2025-08-01",  17_500_00),
        (car_value,      "2025-08-01",  17_500_00),
        (retirement_401k,"2025-08-01", 159_000_00),
        (roth_ira,       "2025-08-01",  44_800_00),
        (brokerage,      "2025-08-01",  31_500_00),
        (college_529,    "2025-08-01",  20_000_00),
        (mortgage,       "2025-08-01", 256_800_00),
        (student_loans,  "2025-08-01",  29_000_00),
        (car_loan,       "2025-08-01",   4_900_00),
        (credit_cards,   "2025-08-01",   1_800_00),

        // Sep 2025
        (home_equity,    "2025-09-01", 191_000_00),
        (savings,        "2025-09-01",  15_800_00),
        (emergency_fund, "2025-09-01",  18_000_00),
        (car_value,      "2025-09-01",  17_200_00),
        (retirement_401k,"2025-09-01", 161_000_00),
        (roth_ira,       "2025-09-01",  45_500_00),
        (brokerage,      "2025-09-01",  32_200_00),
        (college_529,    "2025-09-01",  20_500_00),
        (mortgage,       "2025-09-01", 256_000_00),
        (student_loans,  "2025-09-01",  28_500_00),
        (car_loan,       "2025-09-01",   4_200_00),
        (credit_cards,   "2025-09-01",   2_000_00),

        // Oct 2025
        (home_equity,    "2025-10-01", 192_000_00),
        (savings,        "2025-10-01",  16_200_00),
        (emergency_fund, "2025-10-01",  18_500_00),
        (car_value,      "2025-10-01",  17_000_00),
        (retirement_401k,"2025-10-01", 163_000_00),
        (roth_ira,       "2025-10-01",  46_200_00),
        (brokerage,      "2025-10-01",  33_000_00),
        (college_529,    "2025-10-01",  21_000_00),
        (mortgage,       "2025-10-01", 255_200_00),
        (student_loans,  "2025-10-01",  28_000_00),
        (car_loan,       "2025-10-01",   3_500_00),
        (credit_cards,   "2025-10-01",   2_300_00),

        // Nov 2025 — holiday uptick again
        (home_equity,    "2025-11-01", 192_800_00),
        (savings,        "2025-11-01",  15_800_00),   // dip
        (emergency_fund, "2025-11-01",  18_800_00),
        (car_value,      "2025-11-01",  16_800_00),
        (retirement_401k,"2025-11-01", 165_000_00),
        (roth_ira,       "2025-11-01",  47_000_00),
        (brokerage,      "2025-11-01",  33_800_00),
        (college_529,    "2025-11-01",  21_400_00),
        (mortgage,       "2025-11-01", 254_400_00),
        (student_loans,  "2025-11-01",  27_500_00),
        (car_loan,       "2025-11-01",   2_800_00),
        (credit_cards,   "2025-11-01",   3_800_00),   // holiday spending

        // Dec 2025 — peak holiday debt again
        (home_equity,    "2025-12-01", 193_500_00),
        (savings,        "2025-12-01",  15_200_00),   // low
        (emergency_fund, "2025-12-01",  18_800_00),
        (car_value,      "2025-12-01",  16_500_00),
        (retirement_401k,"2025-12-01", 166_500_00),
        (roth_ira,       "2025-12-01",  47_500_00),
        (brokerage,      "2025-12-01",  34_200_00),
        (college_529,    "2025-12-01",  21_800_00),
        (mortgage,       "2025-12-01", 253_600_00),
        (student_loans,  "2025-12-01",  27_000_00),
        (car_loan,       "2025-12-01",   2_100_00),
        (credit_cards,   "2025-12-01",   5_200_00),   // peak
    ];

    for (item_id, date, value) in &data {
        let log_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO balance_logs (log_id, item_id, log_date, balance_value) VALUES (?, ?, ?, ?)",
        )
        .bind(log_id.to_string())
        .bind(item_id.to_string())
        .bind(*date)
        .bind(*value)
        .execute(pool)
        .await?;
    }

    tracing::info!(
        "Seeded {} balance entries across 12 items (24 months)",
        data.len()
    );

    // ── Seed Transactions ──
    let transactions = vec![
        // January 2024
        ("2024-01-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2024-01-05", 5200_00, "Income", "Salary", "income"),
        ("2024-01-10", -450_00, "Food & Dining", "Groceries", "expense"),
        ("2024-01-15", -120_00, "Utilities", "Electric bill", "expense"),
        ("2024-01-20", -85_00, "Transportation", "Gas", "expense"),
        ("2024-01-25", -60_00, "Entertainment", "Streaming subscriptions", "expense"),
        // February 2024
        ("2024-02-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2024-02-05", 5200_00, "Income", "Salary", "income"),
        ("2024-02-11", -420_00, "Food & Dining", "Groceries", "expense"),
        ("2024-02-14", -150_00, "Gifts", "Valentine's dinner", "expense"),
        ("2024-02-18", -110_00, "Utilities", "Gas bill", "expense"),
        ("2024-02-22", -75_00, "Personal Care", "Haircut", "expense"),
        // March 2024
        ("2024-03-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2024-03-05", 5200_00, "Income", "Salary", "income"),
        ("2024-03-08", -430_00, "Food & Dining", "Groceries", "expense"),
        ("2024-03-12", -100_00, "Insurance", "Car insurance quarterly", "expense"),
        ("2024-03-17", -90_00, "Utilities", "Water bill", "expense"),
        ("2024-03-25", -200_00, "Healthcare", "Dentist copay", "expense"),
        // April 2024
        ("2024-04-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2024-04-05", 5200_00, "Income", "Salary", "income"),
        ("2024-04-10", -400_00, "Food & Dining", "Groceries", "expense"),
        ("2024-04-15", -130_00, "Utilities", "Electric + internet", "expense"),
        ("2024-04-22", -250_00, "Shopping", "Spring clothes", "expense"),
        // May 2024
        ("2024-05-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2024-05-05", 5200_00, "Income", "Salary", "income"),
        ("2024-05-05", 300_00, "Income", "Freelance project", "income"),
        ("2024-05-10", -380_00, "Food & Dining", "Groceries", "expense"),
        ("2024-05-15", -115_00, "Utilities", "Electric bill", "expense"),
        ("2024-05-20", -90_00, "Transportation", "Gas + tolls", "expense"),
        // June 2024
        ("2024-06-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2024-06-05", 5200_00, "Income", "Salary", "income"),
        ("2024-06-10", -450_00, "Food & Dining", "Groceries", "expense"),
        ("2024-06-15", -120_00, "Utilities", "Electric bill", "expense"),
        ("2024-06-22", -180_00, "Entertainment", "Concert tickets", "expense"),
        ("2024-06-28", -350_00, "Shopping", "New running shoes", "expense"),
        // July 2024
        ("2024-07-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2024-07-05", 5200_00, "Income", "Salary", "income"),
        ("2024-07-10", -420_00, "Food & Dining", "Groceries", "expense"),
        ("2024-07-15", -125_00, "Utilities", "Electric (summer AC)", "expense"),
        ("2024-07-20", -150_00, "Transportation", "Car maintenance", "expense"),
        ("2024-07-25", -500_00, "Entertainment", "Weekend trip", "expense"),
        // August 2024
        ("2024-08-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2024-08-05", 5200_00, "Income", "Salary", "income"),
        ("2024-08-10", -410_00, "Food & Dining", "Groceries + dining out", "expense"),
        ("2024-08-15", -135_00, "Utilities", "Electric (summer)", "expense"),
        ("2024-08-22", -200_00, "Healthcare", "Annual physical copay", "expense"),
        // September 2024
        ("2024-09-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2024-09-05", 5200_00, "Income", "Salary", "income"),
        ("2024-09-10", -390_00, "Food & Dining", "Groceries", "expense"),
        ("2024-09-15", -110_00, "Utilities", "Electric bill", "expense"),
        ("2024-09-18", -100_00, "Insurance", "Car insurance quarterly", "expense"),
        ("2024-09-25", -175_00, "Shopping", "Fall wardrobe", "expense"),
        // October 2024
        ("2024-10-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2024-10-05", 5400_00, "Income", "Salary (raise)", "income"),
        ("2024-10-10", -440_00, "Food & Dining", "Groceries", "expense"),
        ("2024-10-15", -105_00, "Utilities", "Electric + water", "expense"),
        ("2024-10-25", -300_00, "Shopping", "Halloween supplies", "expense"),
        // November 2024
        ("2024-11-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2024-11-05", 5400_00, "Income", "Salary", "income"),
        ("2024-11-10", -480_00, "Food & Dining", "Groceries + Thanksgiving dinner", "expense"),
        ("2024-11-15", -120_00, "Utilities", "Heating bill", "expense"),
        ("2024-11-22", -600_00, "Gifts", "Holiday gifts", "expense"),
        ("2024-11-28", -250_00, "Entertainment", "Black Friday deals", "expense"),
        // December 2024
        ("2024-12-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2024-12-05", 5400_00, "Income", "Salary", "income"),
        ("2024-12-10", -500_00, "Food & Dining", "Holiday groceries", "expense"),
        ("2024-12-15", -140_00, "Utilities", "Heating bill", "expense"),
        ("2024-12-20", -800_00, "Gifts", "Christmas gifts", "expense"),
        ("2024-12-25", -300_00, "Entertainment", "Holiday activities", "expense"),
        // January 2025
        ("2025-01-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2025-01-05", 5400_00, "Income", "Salary", "income"),
        ("2025-01-10", -430_00, "Food & Dining", "Groceries", "expense"),
        ("2025-01-15", -130_00, "Utilities", "Heating bill", "expense"),
        ("2025-01-20", -200_00, "Healthcare", "New year health checkup", "expense"),
        // February 2025
        ("2025-02-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2025-02-05", 5400_00, "Income", "Salary", "income"),
        ("2025-02-10", -410_00, "Food & Dining", "Groceries", "expense"),
        ("2025-02-14", -200_00, "Gifts", "Valentine's Day", "expense"),
        ("2025-02-18", -110_00, "Utilities", "Electric bill", "expense"),
        // March 2025
        ("2025-03-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2025-03-05", 5400_00, "Income", "Salary", "income"),
        ("2025-03-10", -400_00, "Food & Dining", "Groceries", "expense"),
        ("2025-03-15", -100_00, "Insurance", "Car insurance quarterly", "expense"),
        ("2025-03-20", -115_00, "Utilities", "Water + electric", "expense"),
        // April 2025
        ("2025-04-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2025-04-05", 5400_00, "Income", "Salary", "income"),
        ("2025-04-10", -390_00, "Food & Dining", "Groceries", "expense"),
        ("2025-04-15", -120_00, "Utilities", "Electric bill", "expense"),
        ("2025-04-22", -275_00, "Shopping", "Spring shopping", "expense"),
        // May 2025
        ("2025-05-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2025-05-05", 5400_00, "Income", "Salary", "income"),
        ("2025-05-05", 500_00, "Income", "Bonus", "income"),
        ("2025-05-10", -420_00, "Food & Dining", "Groceries", "expense"),
        ("2025-05-15", -105_00, "Utilities", "Electric bill", "expense"),
        ("2025-05-20", -150_00, "Transportation", "Gas", "expense"),
        // June 2025
        ("2025-06-03", -2100_00, "Housing", "Rent payment", "expense"),
        ("2025-06-05", 5400_00, "Income", "Salary", "income"),
        ("2025-06-10", -450_00, "Food & Dining", "Groceries", "expense"),
        ("2025-06-15", -120_00, "Utilities", "Summer electric", "expense"),
        ("2025-06-20", -350_00, "Entertainment", "Summer concert", "expense"),
    ];

    let seed_user = "demo";

    for (date, amount, category, description, txn_type) in &transactions {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO transactions (txn_id, txn_date, amount, category, description, txn_type, user_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(*date)
        .bind(*amount)
        .bind(*category)
        .bind(*description)
        .bind(*txn_type)
        .bind(seed_user)
        .execute(pool)
        .await?;
    }

    // ── Seed Budgets (2025-06) ──
    let budgets = vec![
        ("Housing", "2025-06", 2200_00),
        ("Food & Dining", "2025-06", 500_00),
        ("Utilities", "2025-06", 150_00),
        ("Transportation", "2025-06", 200_00),
        ("Entertainment", "2025-06", 300_00),
        ("Shopping", "2025-06", 200_00),
        ("Healthcare", "2025-06", 150_00),
        ("Insurance", "2025-06", 100_00),
        ("Subscriptions", "2025-06", 80_00),
    ];

    for (category, month, planned) in &budgets {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO budgets (budget_id, category, month, planned_amount, user_id) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(*category)
        .bind(*month)
        .bind(*planned)
        .bind(seed_user)
        .execute(pool)
        .await?;
    }

    // ── Seed Savings Goals ──
    let goals = vec![
        ("Emergency Fund Top-Up", 15000_00, 8500_00, Some("2025-12-31"), "general"),
        ("New Car Down Payment", 25000_00, 7200_00, Some("2026-06-01"), "vehicle"),
        ("European Vacation", 5000_00, 1800_00, Some("2025-09-01"), "travel"),
        ("Laptop Upgrade", 2500_00, 1200_00, None, "tech"),
    ];

    for (name, target, current, target_date, category) in &goals {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO savings_goals (goal_id, name, target_amount, current_amount, target_date, category, user_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(*name)
        .bind(*target)
        .bind(*current)
        .bind(*target_date)
        .bind(*category)
        .bind(seed_user)
        .execute(pool)
        .await?;
    }

    // ── Seed Holidays ──
    let holidays = vec![
        ("Christmas 2024", "2024-12-20", "2024-12-31"),
        ("Spring Break 2025", "2025-03-15", "2025-03-23"),
        ("Summer Vacation 2025", "2025-06-28", "2025-07-07"),
    ];

    for (name, start, end) in &holidays {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO holidays (holiday_id, name, start_date, end_date, user_id) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(*name)
        .bind(*start)
        .bind(*end)
        .bind(seed_user)
        .execute(pool)
        .await?;
    }

    tracing::info!(
        "Seeded {} transactions, {} budgets, {} goals, {} holidays",
        transactions.len(),
        budgets.len(),
        goals.len(),
        holidays.len()
    );
    Ok(())
}

async fn insert_item(
    pool: &SqlitePool,
    portfolio_id: Uuid,
    name: &str,
    item_type: &str,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO wealth_items (item_id, portfolio_id, name, item_type) VALUES (?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(portfolio_id.to_string())
    .bind(name)
    .bind(item_type)
    .execute(pool)
    .await?;
    Ok(id)
}