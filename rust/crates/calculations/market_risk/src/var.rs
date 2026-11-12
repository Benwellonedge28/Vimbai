//! Value at risk: weighted volatility, scaled by a diversification discount.

use serde::{Deserialize, Serialize};

use vimbai_common::numbers::{money, round_dp};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub instrument: String,
    pub exposure: f64,
    pub volatility: f64,
    pub correlation: f64,
}

impl Position {
    pub fn new(instrument: &str, exposure: f64, volatility: f64) -> Position {
        Position {
            instrument: instrument.to_string(),
            exposure,
            volatility,
            correlation: 1.0,
        }
    }

    pub fn correlated(mut self, correlation: f64) -> Position {
        self.correlation = correlation;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VarRequest {
    pub company_id: String,
    pub portfolio_name: String,
    pub positions: Vec<Position>,
    pub confidence_level: f64,
    pub holding_period_days: i32,
}

impl VarRequest {
    pub fn new(company_id: &str, portfolio_name: &str, positions: Vec<Position>) -> VarRequest {
        VarRequest {
            company_id: company_id.to_string(),
            portfolio_name: portfolio_name.to_string(),
            positions,
            confidence_level: 0.95,
            holding_period_days: 1,
        }
    }

    pub fn over_days(mut self, days: i32) -> VarRequest {
        self.holding_period_days = days;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VarResult {
    pub company_id: String,
    pub portfolio_name: String,
    pub total_exposure: f64,
    pub portfolio_volatility: f64,
    pub var_95: f64,
    pub var_99: f64,
    pub expected_shortfall: f64,
    pub stress_loss_2sd: f64,
    pub stress_loss_3sd: f64,
    pub risk_level: String,
}

/// Calculate VaR.
///
/// Preserved quirk, and the most important thing to know about this crate's risk
/// figures: the portfolio volatility is a weighted average discounted by a flat
/// 85%, so positions are only weakly diversified. `correlation` is accepted and
/// never used, and both VaR figures are returned whatever confidence level the
/// caller asked for, because 95 and 99 are hardcoded. A portfolio that really is
/// diversified will be over-reported here.
pub fn calculate(request: &VarRequest) -> VarResult {
    let total: f64 = request.positions.iter().map(|p| p.exposure).sum();
    let weighted_vol = if total != 0.0 {
        request
            .positions
            .iter()
            .map(|p| p.exposure * p.volatility)
            .sum::<f64>()
            / total
    } else {
        0.0
    };
    let portfolio_vol = weighted_vol * 0.85;

    let (z_95, z_99) = (1.645_f64, 2.326_f64);
    let scale = (request.holding_period_days as f64).sqrt();
    let var_95 = total * portfolio_vol * z_95 * scale;
    let var_99 = total * portfolio_vol * z_99 * scale;
    let es = total * portfolio_vol * 2.063 * scale;
    let stress_2sd = total * portfolio_vol * 2.0;
    let stress_3sd = total * portfolio_vol * 3.0;

    let ratio = if total != 0.0 { var_95 / total } else { 0.0 };
    let risk_level = if ratio > 0.20 {
        "high"
    } else if ratio > 0.10 {
        "medium"
    } else {
        "low"
    };

    VarResult {
        company_id: request.company_id.clone(),
        portfolio_name: request.portfolio_name.clone(),
        total_exposure: money(total),
        portfolio_volatility: round_dp(portfolio_vol, 4),
        var_95: money(var_95),
        var_99: money(var_99),
        expected_shortfall: money(es),
        stress_loss_2sd: money(stress_2sd),
        stress_loss_3sd: money(stress_3sd),
        risk_level: risk_level.to_string(),
    }
}
