//! Liquidity forecasting, coverage and regulatory ratio analytics.
use serde::{Deserialize, Serialize};

use crate::fx_rates::round_half_even;

// ------------------------------------------------------------ forecasting

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CashFlow {
    pub date: String,
    pub inflow: f64,
    pub outflow: f64,
    /// Taken from the request; never recomputed from inflow less outflow.
    pub net: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiquidityForecastRequest {
    pub company_id: String,
    pub starting_cash: f64,
    pub cash_flows: Vec<CashFlow>,
    pub credit_lines_available: f64,
    pub minimum_cash_requirement: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DailyForecast {
    pub date: String,
    pub inflow: f64,
    pub outflow: f64,
    pub net: f64,
    pub balance: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ForecastSummary {
    pub starting_cash: f64,
    pub ending_cash: f64,
    pub total_inflows: f64,
    pub total_outflows: f64,
    pub net_change: f64,
    pub min_balance: f64,
    pub credit_available: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiquidityGap {
    pub date: String,
    pub shortfall: f64,
    pub required_credit: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiquidityForecastResult {
    pub company_id: String,
    pub forecast_summary: ForecastSummary,
    pub daily_forecast: Vec<DailyForecast>,
    pub liquidity_gaps: Vec<LiquidityGap>,
    pub recommendations: Vec<String>,
}

/// `/forecast` on the liquidity forecast service.
///
/// PYTHON QUIRKS, preserved:
/// - the running balance advances by the `net` the caller supplied, which is
///   never checked against inflow less outflow, so a mis-stated net silently
///   drives the whole forecast;
/// - the flow rows are used in the order given and the dates are never
///   sorted, so an out-of-order request produces an out-of-order balance;
/// - the gap check compares the ROUNDED balance against the minimum, so a
///   balance a fraction below the floor can escape detection;
/// - `credit_lines_available` is reported but never applied to any shortfall,
///   and each gap asks for the full shortfall as new credit.
pub fn forecast_liquidity(req: &LiquidityForecastRequest) -> LiquidityForecastResult {
    let mut running_balance = req.starting_cash;
    let mut min_balance = req.starting_cash;
    let mut daily_forecast = Vec::new();

    for cf in &req.cash_flows {
        running_balance += cf.net;
        if running_balance < min_balance {
            min_balance = running_balance;
        }
        daily_forecast.push(DailyForecast {
            date: cf.date.clone(),
            inflow: cf.inflow,
            outflow: cf.outflow,
            net: cf.net,
            balance: round_half_even(running_balance, 2),
        });
    }

    let total_inflows: f64 = daily_forecast.iter().map(|d| d.inflow).sum();
    let total_outflows: f64 = daily_forecast.iter().map(|d| d.outflow).sum();

    let mut liquidity_gaps = Vec::new();
    for day in &daily_forecast {
        if day.balance < req.minimum_cash_requirement {
            let shortfall = round_half_even(req.minimum_cash_requirement - day.balance, 2);
            liquidity_gaps.push(LiquidityGap {
                date: day.date.clone(),
                shortfall,
                required_credit: shortfall,
            });
        }
    }

    let ending_cash = round_half_even(running_balance, 2);
    let mut recommendations = Vec::new();
    if min_balance < req.minimum_cash_requirement {
        recommendations.push("Liquidity gap identified - draw on credit facilities".to_string());
    }
    if ending_cash < req.starting_cash * 0.5 {
        recommendations.push("Significant cash depletion - monitor closely".to_string());
    }

    LiquidityForecastResult {
        company_id: req.company_id.clone(),
        forecast_summary: ForecastSummary {
            starting_cash: req.starting_cash,
            ending_cash,
            total_inflows: round_half_even(total_inflows, 2),
            total_outflows: round_half_even(total_outflows, 2),
            net_change: round_half_even(running_balance - req.starting_cash, 2),
            min_balance: round_half_even(min_balance, 2),
            credit_available: req.credit_lines_available,
        },
        daily_forecast,
        liquidity_gaps,
        recommendations,
    }
}

// ------------------------------------------------------- coverage analysis

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiquidityRequest {
    pub company_id: String,
    pub cash: f64,
    pub marketable_securities: f64,
    pub credit_facilities_available: f64,
    pub credit_facilities_total: f64,
    pub current_assets: f64,
    pub current_liabilities: f64,
    pub monthly_fixed_costs: f64,
    pub monthly_variable_costs: f64,
    pub monthly_min_revenue: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiquidityPosition {
    pub cash: f64,
    pub marketable_securities: f64,
    pub credit_facilities_available: f64,
    /// INCLUDES undrawn credit, so the "liquid assets" figure is not cash.
    pub total_liquid_assets: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiquidityRatios {
    pub current_ratio: f64,
    pub quick_ratio: f64,
    pub cash_ratio: f64,
    pub credit_utilization: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StressScenario {
    pub monthly_impact: f64,
    pub runway_days: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StressTest {
    pub revenue_drop_30: StressScenario,
    pub revenue_drop_50: StressScenario,
    pub payment_delay_60: StressScenario,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiquidityResult {
    pub company_id: String,
    pub position: LiquidityPosition,
    pub liquidity_ratios: LiquidityRatios,
    pub coverage_days: f64,
    pub stress_test: StressTest,
    pub recommendations: Vec<String>,
}

/// `/analyze` on the liquidity management service.
///
/// PYTHON QUIRKS, preserved:
/// - total liquid assets include AVAILABLE CREDIT, so coverage days and every
///   stress runway are flattered by facilities that are not yet drawn, and
///   the 30-day critical warning can never fire on a company with a large
///   revolver;
/// - the revenue stress impacts are computed from COSTS, not revenue, so a
///   30% revenue drop is charged as 30% of monthly cost;
/// - the payment delay scenario instead charges 60% of MINIMUM REVENUE,
///   mixing inflow and outflow bases between scenarios;
/// - runway divides the full liquid assets by the shortfall each month, so
///   it takes no account of the cash still being consumed in the base case.
pub fn analyze_liquidity(req: &LiquidityRequest) -> LiquidityResult {
    let total_liquid = req.cash + req.marketable_securities + req.credit_facilities_available;

    let current_ratio = if req.current_liabilities != 0.0 {
        req.current_assets / req.current_liabilities
    } else {
        0.0
    };
    let quick_ratio = if req.current_liabilities != 0.0 {
        (req.cash + req.marketable_securities) / req.current_liabilities
    } else {
        0.0
    };
    let cash_ratio = if req.current_liabilities != 0.0 {
        req.cash / req.current_liabilities
    } else {
        0.0
    };
    let utilization = if req.credit_facilities_total != 0.0 {
        (req.credit_facilities_total - req.credit_facilities_available)
            / req.credit_facilities_total
    } else {
        0.0
    };

    let monthly_costs = req.monthly_fixed_costs + req.monthly_variable_costs;
    let coverage_days = if monthly_costs != 0.0 {
        total_liquid / monthly_costs * 30.0
    } else {
        0.0
    };

    let stress = |impact_basis: f64, factor: f64| -> StressScenario {
        let impact = impact_basis * factor;
        StressScenario {
            monthly_impact: impact,
            runway_days: if impact != 0.0 {
                total_liquid / impact * 30.0
            } else {
                0.0
            },
        }
    };

    let mut recommendations = Vec::new();
    if coverage_days < 30.0 {
        recommendations.push(
            "CRITICAL: Less than 30 days liquidity coverage - immediate action required"
                .to_string(),
        );
    }
    if coverage_days < 60.0 {
        recommendations.push(
            "WARNING: Coverage below 60 days - consider drawing on credit facilities".to_string(),
        );
    }
    if utilization > 0.8 {
        recommendations
            .push("Credit facility utilization above 80% - preserve capacity".to_string());
    }
    if quick_ratio < 1.0 {
        recommendations
            .push("Quick ratio below 1.0 - focus on converting receivables to cash".to_string());
    }

    LiquidityResult {
        company_id: req.company_id.clone(),
        position: LiquidityPosition {
            cash: req.cash,
            marketable_securities: req.marketable_securities,
            credit_facilities_available: req.credit_facilities_available,
            total_liquid_assets: total_liquid,
        },
        liquidity_ratios: LiquidityRatios {
            current_ratio: round_half_even(current_ratio, 4),
            quick_ratio: round_half_even(quick_ratio, 4),
            cash_ratio: round_half_even(cash_ratio, 4),
            credit_utilization: round_half_even(utilization, 4),
        },
        coverage_days: round_half_even(coverage_days, 2),
        stress_test: StressTest {
            revenue_drop_30: stress(monthly_costs, 0.3),
            revenue_drop_50: stress(monthly_costs, 0.5),
            payment_delay_60: stress(req.monthly_min_revenue, 0.6),
        },
        recommendations,
    }
}

// --------------------------------------------------------- regulatory risk

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiquidityAsset {
    pub asset_id: String,
    pub asset_type: String,
    pub value: f64,
    pub haircut: f64,
    pub maturity_bucket: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Liability {
    pub liability_id: String,
    pub liability_type: String,
    pub value: f64,
    pub outflow_rate: f64,
    pub maturity_bucket: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiquidityAnalysisRequest {
    pub company_id: String,
    pub reporting_date: String,
    pub liquid_assets: Vec<LiquidityAsset>,
    pub liabilities: Vec<Liability>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MaturityMismatch {
    pub day_30: f64,
    pub day_90: f64,
    pub year_1: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiquidityAnalysisResult {
    pub company_id: String,
    pub lcr_ratio: f64,
    pub nsfr_ratio: f64,
    pub liquid_asset_buffer: f64,
    pub net_cash_position: f64,
    pub maturity_mismatch: MaturityMismatch,
    pub survival_horizon_days: i64,
    pub liquidity_risk_rating: String,
}

/// `/analyze` on the liquidity risk service.
///
/// PYTHON QUIRKS, all preserved:
/// - the LCR denominator subtracts three quarters of a "net inflow" that is
///   itself just 10% of total liabilities, a figure with no relationship to
///   the contracted outflows, and the ratio is capped at 500 and floored by
///   an infinite case when outflows do not exceed that inflow;
/// - the NSFR uses 95% of liabilities as available stable funding and half
///   of GROSS asset values as required stable funding, so haircuts are
///   ignored on that side while applied to the LCR numerator;
/// - the survival horizon is a plain truncation of buffer over
///   outflows-per-30-days, with no allowance for inflows;
/// - the maturity mismatch is a HARD-CODED three-bucket figure, identical for
///   every company, and the cash flow curve and bucketing on the request are
///   read and discarded;
/// - the rating is a step function of the capped LCR.
pub fn analyze_liquidity_risk(req: &LiquidityAnalysisRequest) -> LiquidityAnalysisResult {
    let liquid_assets: f64 = req
        .liquid_assets
        .iter()
        .map(|a| a.value * (1.0 - a.haircut))
        .sum();
    let total_outflows: f64 = req
        .liabilities
        .iter()
        .map(|l| l.value * l.outflow_rate)
        .sum();
    let total_liabilities: f64 = req.liabilities.iter().map(|l| l.value).sum();
    let net_inflows = total_liabilities * 0.1;

    let lcr_ratio = if total_outflows > net_inflows {
        let denominator = total_outflows - net_inflows * 0.75;
        if denominator == 0.0 {
            500.0
        } else {
            (liquid_assets / denominator * 100.0).min(500.0)
        }
    } else {
        // the source yields infinity here, which the cap turns into 500
        500.0
    };

    let asf = total_liabilities * 0.95;
    let rsf: f64 = req.liquid_assets.iter().map(|a| a.value).sum::<f64>() * 0.5;
    let nsfr = if rsf > 0.0 { asf / rsf * 100.0 } else { 0.0 };

    let survival = if total_outflows != 0.0 {
        (liquid_assets / (total_outflows / 30.0)) as i64
    } else {
        0
    };

    let rating = if lcr_ratio >= 150.0 {
        "LOW"
    } else if lcr_ratio >= 100.0 {
        "MEDIUM"
    } else {
        "HIGH"
    };

    LiquidityAnalysisResult {
        company_id: req.company_id.clone(),
        lcr_ratio,
        nsfr_ratio: nsfr,
        liquid_asset_buffer: liquid_assets,
        net_cash_position: liquid_assets - total_outflows,
        maturity_mismatch: MaturityMismatch {
            day_30: -50_000.0,
            day_90: -200_000.0,
            year_1: -100_000.0,
        },
        survival_horizon_days: survival,
        liquidity_risk_rating: rating.to_string(),
    }
}
