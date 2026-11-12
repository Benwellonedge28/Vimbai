//! Treasury analytics: the KPI set a treasurer reads off the dashboard.
//!
//! Ratios are carried as [`Money`] rather than `f64` so that a report of a
//! nine-figure book is reproducible; the one place this differs visibly from
//! the Python service is float noise, which this module does not reproduce
//! (1.7500000000000002 is reported as 1.75).
use serde::{Deserialize, Serialize};

use vimbai_common::Money;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TreasuryAnalyticsRequest {
    pub company_id: String,
    pub total_cash: Money,
    pub monthly_inflow: Money,
    pub monthly_outflow: Money,
    pub short_term_debt: Money,
    pub total_debt: Money,
    pub investments: Money,
    pub fx_exposure: Money,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TreasuryKpi {
    pub name: String,
    pub value: Money,
    pub unit: String,
    pub benchmark: Money,
    pub status: String,
    pub description: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TreasuryAnalyticsResult {
    pub company_id: String,
    pub kpis: Vec<TreasuryKpi>,
    pub cash_adequacy_days: Money,
    pub debt_service_ratio: Money,
    pub investment_yield: Money,
    pub fx_risk_score: Money,
}

/// The runway reported when nothing is being spent (no burn to divide by).
const UNLIMITED_RUNWAY: &str = "999";
/// The flat annual yield the source assumes on invested cash. It ignores what
/// the investments actually earn.
const ASSUMED_YIELD: &str = "0.05";
/// Ratio scale. Python leaves these as unrounded floats; ten decimal places
/// keeps every meaningful figure and drops the float noise.
const RATIO_SCALE: i64 = 10;

fn one() -> Money {
    Money::from_i64(1)
}

fn money(text: &str) -> Money {
    Money::parse(text).expect("constant")
}

/// Python's `max(1, x)`: a denominator floor that also catches negatives.
fn at_least_one(value: &Money) -> Money {
    if *value < one() {
        one()
    } else {
        value.clone()
    }
}

fn status_above(value: &Money, good: &str, warn: &str) -> String {
    if value > &money(good) {
        "good".into()
    } else if value > &money(warn) {
        "warning".into()
    } else {
        "critical".into()
    }
}

fn status_below(value: &Money, good: &str, warn: &str) -> String {
    if value < &money(good) {
        "good".into()
    } else if value < &money(warn) {
        "warning".into()
    } else {
        "critical".into()
    }
}

/// `/analyze` on the treasury analytics service.
///
/// PYTHON QUIRKS, preserved:
/// - runway is 999 days whenever the outflow is zero or negative, rather than
///   an infinity or an error;
/// - the yield is a flat 5% of investments, never the instruments' own rate;
/// - the FX score is capped at 100 but has no floor, so a negative exposure
///   (a net receivable) produces a negative score and rates "good";
/// - utilisation is always "good" whatever it reports.
pub fn analyze_treasury(req: &TreasuryAnalyticsRequest) -> TreasuryAnalyticsResult {
    let net_flow = req.monthly_inflow.minus(&req.monthly_outflow);

    let cash_adequacy = if req.monthly_outflow > Money::zero() {
        req.total_cash
            .over(&at_least_one(&req.monthly_outflow))
            .times(&Money::from_i64(30))
            .round(RATIO_SCALE)
    } else {
        money(UNLIMITED_RUNWAY)
    };

    let debt_service = if req.monthly_inflow > Money::zero() {
        req.short_term_debt
            .over(&at_least_one(&req.monthly_inflow))
            .times(&Money::from_i64(100))
            .round(RATIO_SCALE)
    } else {
        Money::zero()
    };

    let yield_pct = if req.total_cash > Money::zero() {
        req.investments
            .times(&money(ASSUMED_YIELD))
            .over(&at_least_one(&req.total_cash))
            .times(&Money::from_i64(100))
            .round(RATIO_SCALE)
    } else {
        Money::zero()
    };

    let hundred = Money::from_i64(100);
    let fx_raw = req
        .fx_exposure
        .over(&at_least_one(&req.total_cash))
        .times(&hundred)
        .round(RATIO_SCALE);
    let fx_score = if fx_raw > hundred {
        hundred.clone()
    } else {
        fx_raw.clone()
    };

    let deployed = req.total_cash.plus(&req.investments);
    let utilization = if deployed > Money::zero() {
        one()
            .minus(&req.total_cash.over(&at_least_one(&deployed)))
            .times(&hundred)
            .round(RATIO_SCALE)
    } else {
        Money::zero()
    };

    let kpis = vec![
        TreasuryKpi {
            name: "Net Cash Flow".into(),
            value: net_flow.clone(),
            unit: "USD".into(),
            benchmark: Money::zero(),
            status: if net_flow > Money::zero() {
                "good".into()
            } else {
                "warning".into()
            },
            description: "Monthly net cash position".into(),
        },
        TreasuryKpi {
            name: "Cash Runway".into(),
            value: cash_adequacy.clone(),
            unit: "days".into(),
            benchmark: money("90"),
            status: status_above(&cash_adequacy, "90", "30"),
            description: "Days of cash available at current burn".into(),
        },
        TreasuryKpi {
            name: "Debt Service Ratio".into(),
            value: debt_service.clone(),
            unit: "%".into(),
            benchmark: money("30"),
            status: status_below(&debt_service, "30", "50"),
            description: "Short-term debt as % of monthly inflow".into(),
        },
        TreasuryKpi {
            name: "Investment Yield".into(),
            value: yield_pct.clone(),
            unit: "%".into(),
            benchmark: money("5"),
            status: if yield_pct >= money("5") {
                "good".into()
            } else {
                "warning".into()
            },
            description: "Estimated annual yield on investments".into(),
        },
        TreasuryKpi {
            name: "FX Risk Score".into(),
            value: fx_score.clone(),
            unit: "score".into(),
            benchmark: money("20"),
            status: status_below(&fx_score, "20", "50"),
            description: "Foreign exchange exposure risk".into(),
        },
        TreasuryKpi {
            name: "Cash Utilization".into(),
            value: utilization.clone(),
            unit: "%".into(),
            benchmark: money("70"),
            // always good, whatever the number says
            status: "good".into(),
            description: "Cash deployed in investments vs idle".into(),
        },
    ];

    TreasuryAnalyticsResult {
        company_id: req.company_id.clone(),
        kpis,
        cash_adequacy_days: cash_adequacy,
        debt_service_ratio: debt_service,
        investment_yield: yield_pct,
        fx_risk_score: fx_score,
    }
}
