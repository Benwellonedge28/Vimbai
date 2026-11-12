//! FX exposure hedging analytics: simple exposure cover, portfolio hedge
//! assessment, and the multi-currency service's fixed-table conversions.
use serde::{Deserialize, Serialize};

use crate::fx_rates::round_half_even;

/// Units of local currency per USD, as hard-coded in the FX services.
pub fn spot_to_usd(code: &str) -> Option<f64> {
    let rate = match code {
        "USD" => 1.0,
        "EUR" => 0.92,
        "GBP" => 0.79,
        "ZWL" => 15.5,
        "ZAR" => 18.5,
        "BWP" => 13.6,
        "KES" => 152.0,
        "NGN" => 1600.0,
        "GHS" => 15.2,
        "CNY" => 7.25,
        "JPY" => 149.5,
        _ => return None,
    };
    Some(rate)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FxExposureRequest {
    pub company_id: String,
    pub currency: String,
    pub exposure_amount: f64,
    pub hedge_ratio: f64,
    pub forward_rate: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FxHedgeResult {
    pub company_id: String,
    pub currency: String,
    pub exposure_amount: f64,
    pub hedged_amount: f64,
    pub unhedged_amount: f64,
    pub hedge_ratio: f64,
    pub forward_rate: f64,
    pub potential_loss_unhedged: f64,
    pub hedge_cost_estimate: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FxHedgeError {
    UnsupportedCurrency { code: String },
}

impl std::fmt::Display for FxHedgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FxHedgeError::UnsupportedCurrency { code } => write!(f, "Unsupported currency {code}"),
        }
    }
}

impl std::error::Error for FxHedgeError {}

/// `/hedge` on the FX service.
///
/// PYTHON QUIRKS, both preserved:
/// - a forward rate of zero (the default) is replaced by the SPOT rate, so
///   an unhedged forward reports a zero loss rather than an error;
/// - `potential_loss_unhedged` is a PERCENTAGE of the spot rate, not a
///   currency amount, even though its name and neighbours are amounts;
/// - the hedge cost is a flat 0.2% of the hedged amount.
pub fn calculate_fx_hedge(req: &FxExposureRequest) -> Result<FxHedgeResult, FxHedgeError> {
    let spot = spot_to_usd(&req.currency).ok_or_else(|| FxHedgeError::UnsupportedCurrency {
        code: req.currency.clone(),
    })?;
    let hedged = req.exposure_amount * req.hedge_ratio;
    let unhedged = req.exposure_amount - hedged;
    let fwd_rate = if req.forward_rate != 0.0 {
        req.forward_rate
    } else {
        spot
    };
    let potential_loss = unhedged * (fwd_rate - spot).abs() / spot * 100.0;
    let hedge_cost = hedged * 0.002;

    Ok(FxHedgeResult {
        company_id: req.company_id.clone(),
        currency: req.currency.clone(),
        exposure_amount: round_half_even(req.exposure_amount, 2),
        hedged_amount: round_half_even(hedged, 2),
        unhedged_amount: round_half_even(unhedged, 2),
        hedge_ratio: req.hedge_ratio,
        forward_rate: round_half_even(fwd_rate, 6),
        potential_loss_unhedged: round_half_even(potential_loss, 2),
        hedge_cost_estimate: round_half_even(hedge_cost, 2),
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FxExposure {
    pub currency_pair: String,
    pub exposure_amount: f64,
    pub spot_rate: f64,
    pub expected_rate: f64,
    pub volatility: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CurrencyHedgingRequest {
    pub company_id: String,
    pub exposures: Vec<FxExposure>,
    pub current_hedge_ratio: f64,
    pub target_hedge_ratio: f64,
    pub risk_aversion: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExposureAnalysis {
    pub currency_pair: String,
    pub exposure_amount: f64,
    pub exposure_usd: f64,
    pub spot_rate: f64,
    pub volatility: f64,
    pub hedged_amount: f64,
    pub hedge_cost: f64,
    pub var_95: f64,
    pub hedged_var: f64,
    pub var_reduction: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HedgeRecommendation {
    pub currency_pair: String,
    pub action: String,
    pub amount: f64,
    pub instrument: String,
    pub estimated_cost: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PortfolioMetrics {
    pub total_exposure_usd: f64,
    pub hedged_value_usd: f64,
    pub current_hedge_ratio: f64,
    pub target_hedge_ratio: f64,
    pub var_portfolio_95: f64,
    pub hedged_var_95: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CurrencyHedgingResult {
    pub company_id: String,
    pub exposure_analysis: Vec<ExposureAnalysis>,
    pub hedge_recommendations: Vec<HedgeRecommendation>,
    pub portfolio_metrics: PortfolioMetrics,
    pub recommendations: Vec<String>,
}

/// `/analyze` on the currency-hedging service.
///
/// PYTHON QUIRKS, preserved:
/// - the local exposure is divided by spot to reach USD, so a higher spot
///   rate means a SMALLER USD exposure;
/// - the reported `volatility` is the fraction times 100 while the figures
///   beside it use the fraction, and `var_95` is a plain 1.65 sigma on the
///   whole exposure with no time scaling;
/// - recommendations are only produced when the TARGET exceeds the current
///   ratio, so an over-hedged book never gets a sell instruction;
/// - the portfolio VaR is summed from the ROUNDED per-exposure figures;
/// - `risk_aversion` is accepted and never used.
pub fn analyze_currency_hedge(req: &CurrencyHedgingRequest) -> CurrencyHedgingResult {
    let mut exposure_analysis = Vec::new();
    let mut total_exposure = 0.0;
    let mut total_hedged_value = 0.0;

    for exp in &req.exposures {
        let exposure_usd = if exp.spot_rate != 0.0 {
            exp.exposure_amount / exp.spot_rate
        } else {
            0.0
        };
        let hedged_amount = exposure_usd * req.current_hedge_ratio;
        let hedge_cost = hedged_amount * exp.volatility * 0.1;
        let var_95 = exposure_usd * exp.volatility * 1.65;
        let hedged_var = exposure_usd * (1.0 - req.current_hedge_ratio) * exp.volatility * 1.65;
        let var_reduction = if var_95 != 0.0 {
            (var_95 - hedged_var) / var_95 * 100.0
        } else {
            0.0
        };

        exposure_analysis.push(ExposureAnalysis {
            currency_pair: exp.currency_pair.clone(),
            exposure_amount: exp.exposure_amount,
            exposure_usd: round_half_even(exposure_usd, 2),
            spot_rate: exp.spot_rate,
            volatility: round_half_even(exp.volatility * 100.0, 2),
            hedged_amount: round_half_even(hedged_amount, 2),
            hedge_cost: round_half_even(hedge_cost, 2),
            var_95: round_half_even(var_95, 2),
            hedged_var: round_half_even(hedged_var, 2),
            var_reduction: round_half_even(var_reduction, 2),
        });
        total_exposure += exposure_usd;
        total_hedged_value += hedged_amount;
    }

    let mut hedge_recommendations = Vec::new();
    if req.target_hedge_ratio > req.current_hedge_ratio {
        for exp in &req.exposures {
            let base = if exp.spot_rate != 0.0 {
                exp.exposure_amount / exp.spot_rate
            } else {
                0.0
            };
            let target_hedge = base * req.target_hedge_ratio;
            let current_hedge = base * req.current_hedge_ratio;
            hedge_recommendations.push(HedgeRecommendation {
                currency_pair: exp.currency_pair.clone(),
                action: "BUY".to_string(),
                amount: round_half_even(target_hedge - current_hedge, 2),
                instrument: "Forward".to_string(),
                estimated_cost: round_half_even(
                    (target_hedge - current_hedge) * exp.volatility * 0.1,
                    2,
                ),
            });
        }
    }

    let var_portfolio: f64 = exposure_analysis.iter().map(|e| e.var_95).sum();
    let hedged_var: f64 = exposure_analysis.iter().map(|e| e.hedged_var).sum();
    let portfolio_metrics = PortfolioMetrics {
        total_exposure_usd: round_half_even(total_exposure, 2),
        hedged_value_usd: round_half_even(total_hedged_value, 2),
        current_hedge_ratio: round_half_even(req.current_hedge_ratio * 100.0, 2),
        target_hedge_ratio: round_half_even(req.target_hedge_ratio * 100.0, 2),
        var_portfolio_95: round_half_even(var_portfolio, 2),
        hedged_var_95: round_half_even(hedged_var, 2),
    };

    let mut recommendations = Vec::new();
    if req.current_hedge_ratio < req.target_hedge_ratio * 0.8 {
        recommendations.push("Under-hedged - increase hedge ratio to target level".to_string());
    }
    if portfolio_metrics.hedged_var_95 > portfolio_metrics.total_exposure_usd * 0.15 {
        recommendations.push("VaR exceeds comfort level - consider increasing hedges".to_string());
    }

    CurrencyHedgingResult {
        company_id: req.company_id.clone(),
        exposure_analysis,
        hedge_recommendations,
        portfolio_metrics,
        recommendations,
    }
}
