//! Treasury cash management: investment optimisation, bank reconciliation
//! and liquidity ratios.
//!
//! Amounts here are [`Money`] rather than `f64` on purpose. These are the
//! figures a treasury desk actually keys in, and a group balance can run to
//! dozens of digits in a hyperinflating currency; a decimal keeps every
//! unit of it. The `f64`-based modules elsewhere exist for parity with
//! Python services that already computed in float.
use serde::{Deserialize, Serialize};

use vimbai_common::Money;

// ------------------------------------------------------------ optimisation

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InvestmentOption {
    pub name: String,
    /// The source treats this as a CAP on the allocation, not a floor. `None`
    /// means the key was absent, which the source defaults to 10,000; an
    /// explicit zero caps the allocation at nothing.
    #[serde(default)]
    pub min_investment: Option<Money>,
    pub yield_rate: Money,
    pub maturity_days: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CashOptimizationRequest {
    pub company_id: String,
    /// Supplied by the caller. The source hard-coded a 750,000 cash balance
    /// because it had no input for it; here the real figure is used.
    pub current_cash: Money,
    pub target_cash_balance: Money,
    pub investment_options: Vec<InvestmentOption>,
    pub risk_tolerance: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecommendedInvestment {
    pub instrument: String,
    pub amount: Money,
    pub yield_rate: Money,
    pub maturity_days: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MaturityEntry {
    pub date: String,
    pub amount: Money,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CashOptimizationResult {
    pub company_id: String,
    pub current_cash: Money,
    pub target_cash: Money,
    pub excess_cash: Money,
    pub recommended_investments: Vec<RecommendedInvestment>,
    /// A flat 4.5% for the whole plan regardless of the instruments chosen.
    pub expected_yield: Money,
    pub maturity_schedule: Vec<MaturityEntry>,
}

/// `/cash-optimization` on the treasury cash service.
///
/// PYTHON QUIRKS, preserved:
/// - `min_investment` is applied as `min(excess * 0.3, min_investment)`, so a
///   HIGHER stated minimum produces a SMALLER investment and an instrument
///   with no minimum takes 30% of the excess;
/// - every instrument is offered the same 30% slice rather than sharing one
///   pot, so the recommendations can together exceed the excess cash;
/// - `expected_yield` is a fixed 4.5% and ignores the instruments' own
///   quoted yields entirely;
/// - `risk_tolerance` is accepted and never consulted.
///
/// The maturity schedule needs a calendar date, so the caller passes the
/// as-of date instead of the source reading the clock.
pub fn optimize_cash_flow(req: &CashOptimizationRequest, as_of: &str) -> CashOptimizationResult {
    let excess = req.current_cash.minus(&req.target_cash_balance);
    let excess_cash = if excess.is_negative() {
        Money::zero()
    } else {
        excess
    };
    let share = excess_cash.percentage(&Money::parse("0.3").expect("constant"), 2);

    let mut recommended_investments = Vec::new();
    for opt in &req.investment_options {
        let cap = opt
            .min_investment
            .clone()
            .unwrap_or_else(|| Money::parse("10000").expect("constant"));
        let allocation = if share < cap { share.clone() } else { cap };
        if allocation.is_zero() {
            continue;
        }
        recommended_investments.push(RecommendedInvestment {
            instrument: if opt.name.is_empty() {
                "T-Bill".to_string()
            } else {
                opt.name.clone()
            },
            amount: allocation.round(2),
            yield_rate: opt.yield_rate.clone(),
            maturity_days: opt.maturity_days,
        });
    }

    let maturity_schedule = recommended_investments
        .iter()
        .map(|inv| MaturityEntry {
            date: add_days(as_of, inv.maturity_days),
            amount: inv.amount.clone(),
        })
        .collect();

    CashOptimizationResult {
        company_id: req.company_id.clone(),
        current_cash: req.current_cash.round(2),
        target_cash: req.target_cash_balance.round(2),
        excess_cash: excess_cash.round(2),
        recommended_investments,
        expected_yield: Money::parse("0.045").expect("constant"),
        maturity_schedule,
    }
}

// ---------------------------------------------------------- reconciliation

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BankReconciliationRequest {
    pub company_id: String,
    pub bank_account_id: String,
    pub statement_date: String,
    pub book_balance: Money,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutstandingItem {
    pub reference: String,
    pub amount: Money,
    pub date: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReconciliationDifference {
    pub description: String,
    pub amount: Money,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BankReconciliationResult {
    pub company_id: String,
    pub bank_account_id: String,
    pub book_balance: Money,
    pub bank_balance: Money,
    pub adjusted_book_balance: Money,
    pub adjusted_bank_balance: Money,
    pub differences: Vec<ReconciliationDifference>,
    pub outstanding_checks: Vec<OutstandingItem>,
    pub deposits_in_transit: Vec<OutstandingItem>,
    pub reconciliation_status: String,
}

/// `/bank-reconciliation` on the treasury cash service.
///
/// PYTHON QUIRKS, preserved:
/// - the outstanding checks and the deposit in transit are FIXED CONSTANTS,
///   identical for every company and account, and the transactions actually
///   posted on the request are discarded;
/// - the bank balance is the book balance plus a flat 500, so it is derived
///   from the ledger rather than from any statement;
/// - the adjusted BANK balance is also built from the BOOK balance plus the
///   deposits, so the two adjustments share a single starting figure and the
///   reconciliation can never detect a genuine bank-side error;
/// - the status tolerance is an absolute one unit, with no materiality
///   scale relative to the balance.
pub fn reconcile_bank(req: &BankReconciliationRequest) -> BankReconciliationResult {
    let checks = vec![
        OutstandingItem {
            reference: "1001".to_string(),
            amount: Money::parse("5000").expect("constant"),
            date: "2024-01-15".to_string(),
        },
        OutstandingItem {
            reference: "1002".to_string(),
            amount: Money::parse("3500").expect("constant"),
            date: "2024-01-18".to_string(),
        },
    ];
    let deposits = vec![OutstandingItem {
        reference: "D001".to_string(),
        amount: Money::parse("12000").expect("constant"),
        date: "2024-01-28".to_string(),
    }];

    let total_checks = Money::sum(checks.iter().map(|c| &c.amount));
    let total_deposits = Money::sum(deposits.iter().map(|d| &d.amount));

    let bank_balance = req
        .book_balance
        .plus(&Money::parse("500").expect("constant"));
    let adjusted_book = req.book_balance.minus(&total_checks);
    let adjusted_bank = req.book_balance.plus(&total_deposits);
    let gap = adjusted_book.minus(&adjusted_bank).abs();

    BankReconciliationResult {
        company_id: req.company_id.clone(),
        bank_account_id: req.bank_account_id.clone(),
        book_balance: req.book_balance.round(2),
        bank_balance: bank_balance.round(2),
        adjusted_book_balance: adjusted_book.round(2),
        adjusted_bank_balance: adjusted_bank.round(2),
        differences: Vec::new(),
        outstanding_checks: checks,
        deposits_in_transit: deposits,
        reconciliation_status: if gap < Money::from_i64(1) {
            "balanced".to_string()
        } else {
            "review_needed".to_string()
        },
    }
}

// -------------------------------------------------------------- ratios

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiquidityRatioRequest {
    pub company_id: String,
    pub current_assets: Money,
    pub current_liabilities: Money,
    pub inventory: Money,
    pub cash_and_equivalents: Money,
    pub marketable_securities: Money,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiquidityRatioResult {
    pub company_id: String,
    pub current_ratio: Money,
    pub quick_ratio: Money,
    pub cash_ratio: Money,
    pub defensive_interval: Money,
    pub liquidity_index: Money,
    pub assessment: String,
}

/// `/liquidity-ratios` on the treasury cash service.
///
/// PYTHON QUIRKS, preserved:
/// - the defensive interval divides by a hard-coded 50,000 daily burn, so it
///   is a ratio of the balance to a constant rather than to real spending;
/// - the liquidity index blends the current, quick and cash ratios with
///   0.3/0.4/0.3 weights, which means it adds ratios that share a
///   denominator and is not itself a ratio;
/// - the assessment looks only at the current ratio, so a company can be
///   rated strong while its quick and cash ratios are collapsing;
/// - a zero denominator yields zero rather than an error or infinity.
pub fn calculate_liquidity_ratios(req: &LiquidityRatioRequest) -> LiquidityRatioResult {
    let current_ratio = req
        .current_assets
        .ratio_pct(&req.current_liabilities, 4)
        .over(&Money::from_i64(100));
    // inventory above current assets gives a NEGATIVE quick ratio, as in the
    // source, which never floors the numerator
    let quick_numerator = req.current_assets.minus(&req.inventory);
    let quick_ratio = quick_numerator
        .ratio_pct(&req.current_liabilities, 4)
        .over(&Money::from_i64(100));
    let cash_numerator = req.cash_and_equivalents.plus(&req.marketable_securities);
    let cash_ratio = cash_numerator
        .ratio_pct(&req.current_liabilities, 4)
        .over(&Money::from_i64(100));

    let daily_burn = Money::parse("50000").expect("constant");
    let defensive_interval = if daily_burn.is_zero() {
        Money::zero()
    } else {
        cash_numerator.over(&daily_burn)
    };

    let liquidity_index = current_ratio
        .times(&Money::parse("0.3").expect("constant"))
        .plus(&quick_ratio.times(&Money::parse("0.4").expect("constant")))
        .plus(&cash_ratio.times(&Money::parse("0.3").expect("constant")));

    let assessment = if current_ratio > Money::from_i64(2) {
        "strong"
    } else if current_ratio > Money::parse("1.5").expect("constant") {
        "adequate"
    } else {
        "weak"
    };

    LiquidityRatioResult {
        company_id: req.company_id.clone(),
        current_ratio: current_ratio.round(4),
        quick_ratio: quick_ratio.round(4),
        cash_ratio: cash_ratio.round(4),
        defensive_interval: defensive_interval.round(2),
        liquidity_index: liquidity_index.round(4),
        assessment: assessment.to_string(),
    }
}

// ------------------------------------------------------------- calendar

/// Add days to a `YYYY-MM-DD` date, returning the input unchanged when it
/// cannot be parsed. Uses the civil-from-days algorithm so the workspace
/// needs no date library.
pub fn add_days(iso: &str, days: i64) -> String {
    let parts: Vec<&str> = iso.split('-').collect();
    if parts.len() != 3 {
        return iso.to_string();
    }
    let (y, m, d) = match (
        parts[0].parse::<i64>(),
        parts[1].parse::<u32>(),
        parts[2].parse::<u32>(),
    ) {
        (Ok(y), Ok(m), Ok(d)) if (1..=12).contains(&m) && (1..=31).contains(&d) => (y, m, d),
        _ => return iso.to_string(),
    };
    let serial = days_from_civil(y, m, d) + days;
    let (ny, nm, nd) = civil_from_days(serial);
    format!("{ny:04}-{nm:02}-{nd:02}")
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = ((m + 9) % 12) as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
