//! Futures, commodity and interest rate hedge analytics.
use serde::{Deserialize, Serialize};

use crate::fx_rates::round_half_even;

// -------------------------------------------------------------- futures

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FuturePosition {
    pub future_id: String,
    pub contract_month: String,
    pub position_size: f64,
    pub entry_price: f64,
    pub current_price: f64,
    pub contract_size: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FuturesHedgingRequest {
    pub company_id: String,
    pub positions: Vec<FuturePosition>,
    pub spot_exposure: f64,
    pub target_hedge_ratio: f64,
    pub basis_risk: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PositionAnalysis {
    pub future_id: String,
    pub contract_month: String,
    pub position_size: f64,
    pub entry_price: f64,
    pub current_price: f64,
    pub contract_value: f64,
    pub unrealized_pnl: f64,
    pub pnl_percentage: f64,
    pub daily_var: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HedgeEffectiveness {
    pub current_ratio: f64,
    pub target_ratio: f64,
    pub effectiveness: f64,
    pub basis_risk: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FuturesHedgingResult {
    pub company_id: String,
    pub position_analysis: Vec<PositionAnalysis>,
    pub hedge_effectiveness: HedgeEffectiveness,
    pub optimal_hedge_ratio: f64,
    pub recommendations: Vec<String>,
}

/// `/analyze` on the futures hedging service.
///
/// PYTHON QUIRKS, preserved:
/// - the current hedge ratio is contract value over spot exposure, with no
///   regard to direction, so a short hedge reads as positive cover;
/// - effectiveness is simply `1 - basis_risk` presented as a percentage;
/// - the optimal ratio is capped at 95%, and the shortfall can be negative
///   (an over-hedge) but a recommendation is only emitted when it is
///   positive, so an over-hedged book is never told to sell;
/// - the deviation message renders the gap as a percentage with two
///   decimals, and that message is built from the raw ratio, not the
///   rounded one.
pub fn analyze_futures_hedge(req: &FuturesHedgingRequest) -> FuturesHedgingResult {
    let mut position_analysis = Vec::new();
    let mut total_exposure = 0.0;

    for pos in &req.positions {
        let contract_value = pos.position_size * pos.contract_size;
        let unrealized_pnl =
            (pos.current_price - pos.entry_price) * pos.position_size * pos.contract_size;
        let daily_var = pos.contract_size * pos.position_size * req.basis_risk;

        position_analysis.push(PositionAnalysis {
            future_id: pos.future_id.clone(),
            contract_month: pos.contract_month.clone(),
            position_size: pos.position_size,
            entry_price: pos.entry_price,
            current_price: pos.current_price,
            contract_value: round_half_even(contract_value, 2),
            unrealized_pnl: round_half_even(unrealized_pnl, 2),
            pnl_percentage: round_half_even(
                if contract_value != 0.0 {
                    unrealized_pnl / contract_value * 100.0
                } else {
                    0.0
                },
                4,
            ),
            daily_var: round_half_even(daily_var, 2),
        });
        total_exposure += contract_value;
    }

    let current_ratio = if req.spot_exposure != 0.0 {
        total_exposure / req.spot_exposure
    } else {
        0.0
    };
    let optimal = req.target_hedge_ratio.min(0.95);
    let shortfall = req.spot_exposure * (optimal - current_ratio);

    let mut recommendations = Vec::new();
    let deviation = (current_ratio - req.target_hedge_ratio).abs();
    if deviation > 0.1 {
        recommendations.push(format!(
            "Hedge ratio deviates from target by {:.2}%",
            deviation * 100.0
        ));
    }
    if req.basis_risk > 0.02 {
        recommendations.push("Basis risk is elevated - monitor spread closely".to_string());
    }
    if shortfall > 0.0 {
        recommendations.push(format!(
            "Add {shortfall:.0} notional to reach optimal hedge ratio"
        ));
    }

    FuturesHedgingResult {
        company_id: req.company_id.clone(),
        position_analysis,
        hedge_effectiveness: HedgeEffectiveness {
            current_ratio: round_half_even(current_ratio, 4),
            target_ratio: req.target_hedge_ratio,
            effectiveness: round_half_even((1.0 - req.basis_risk) * 100.0, 2),
            basis_risk: round_half_even(req.basis_risk * 100.0, 2),
        },
        optimal_hedge_ratio: round_half_even(optimal, 4),
        recommendations,
    }
}

// ------------------------------------------------------------ commodity

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommodityExposure {
    pub commodity: String,
    pub exposure_type: String,
    pub volume: f64,
    pub unit: String,
    pub current_price: f64,
    pub budget_price: f64,
    pub volatility: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommodityHedgingRequest {
    pub company_id: String,
    pub exposures: Vec<CommodityExposure>,
    pub hedge_ratio: f64,
    pub budget_rate: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommodityExposureAnalysis {
    pub commodity: String,
    pub exposure_type: String,
    pub volume: f64,
    pub unit: String,
    pub current_price: f64,
    pub exposure_value: f64,
    pub budget_price: f64,
    pub budget_value: f64,
    pub volatility: f64,
    pub upside_risk: f64,
    pub downside_risk: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommodityHedgePlan {
    pub commodity: String,
    pub hedge_type: String,
    pub hedge_volume: f64,
    pub hedge_percentage: f64,
    pub estimated_cost: f64,
    pub protection_level: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BudgetImpact {
    pub total_exposure: f64,
    pub budget_value: f64,
    pub variance: f64,
    pub variance_pct: f64,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommodityHedgingResult {
    pub company_id: String,
    pub exposure_analysis: Vec<CommodityExposureAnalysis>,
    pub hedge_plan: Vec<CommodityHedgePlan>,
    pub budget_impact: BudgetImpact,
    pub recommendations: Vec<String>,
}

/// `/analyze` on the commodity hedging service.
///
/// PYTHON QUIRKS, preserved:
/// - the budget value uses the `budget_rate` entry for that commodity when
///   one exists and otherwise the exposure's own `budget_price`, so the
///   two budget fields can disagree within one row;
/// - upside and downside risk both use the exposure's `budget_price`, not
///   the rate that priced the budget value;
/// - the instrument is chosen purely on volatility, futures above 25% and
///   options at or below;
/// - the protection level is a price, `current * (1 - volatility)`, and
///   goes NEGATIVE for a volatility above 100%;
/// - a zero variance reports "Under Budget".
pub fn analyze_commodity_hedge(req: &CommodityHedgingRequest) -> CommodityHedgingResult {
    let mut exposure_analysis = Vec::new();
    let mut hedge_plan = Vec::new();
    let mut total_exposure_value = 0.0;
    let mut total_budget_value = 0.0;

    for exp in &req.exposures {
        let exposure_value = exp.volume * exp.current_price;
        let budget_price = req
            .budget_rate
            .iter()
            .find(|(k, _)| k == &exp.commodity)
            .map(|(_, v)| *v)
            .unwrap_or(exp.budget_price);
        let budget_value = exp.volume * budget_price;
        let price_upside = (exp.current_price - exp.budget_price).max(0.0) * exp.volume;
        let price_downside = (exp.budget_price - exp.current_price).max(0.0) * exp.volume;

        exposure_analysis.push(CommodityExposureAnalysis {
            commodity: exp.commodity.clone(),
            exposure_type: exp.exposure_type.clone(),
            volume: exp.volume,
            unit: exp.unit.clone(),
            current_price: exp.current_price,
            exposure_value: round_half_even(exposure_value, 2),
            budget_price: exp.budget_price,
            budget_value: round_half_even(budget_value, 2),
            volatility: round_half_even(exp.volatility * 100.0, 2),
            upside_risk: round_half_even(price_upside, 2),
            downside_risk: round_half_even(price_downside, 2),
        });

        let hedged_volume = exp.volume * req.hedge_ratio;
        let hedge_cost = hedged_volume * exp.current_price * exp.volatility * 0.15;
        hedge_plan.push(CommodityHedgePlan {
            commodity: exp.commodity.clone(),
            hedge_type: if exp.volatility > 0.25 {
                "Futures".to_string()
            } else {
                "Options".to_string()
            },
            hedge_volume: round_half_even(hedged_volume, 2),
            hedge_percentage: round_half_even(req.hedge_ratio * 100.0, 2),
            estimated_cost: round_half_even(hedge_cost, 2),
            protection_level: round_half_even(exp.current_price * (1.0 - exp.volatility), 2),
        });

        total_exposure_value += exposure_value;
        total_budget_value += budget_value;
    }

    let variance = total_exposure_value - total_budget_value;
    let variance_pct = if total_budget_value != 0.0 {
        variance / total_budget_value * 100.0
    } else {
        0.0
    };

    let mut recommendations = Vec::new();
    if variance > total_budget_value * 0.1 {
        recommendations
            .push("Commodity costs significantly over budget - increase hedges".to_string());
    }
    if req.hedge_ratio < 0.5 {
        recommendations.push("Low hedge ratio - consider increasing coverage".to_string());
    }
    if exposure_analysis.iter().any(|e| e.volatility > 30.0) {
        recommendations.push("High volatility commodities need options-based hedging".to_string());
    }

    CommodityHedgingResult {
        company_id: req.company_id.clone(),
        budget_impact: BudgetImpact {
            total_exposure: round_half_even(total_exposure_value, 2),
            budget_value: round_half_even(total_budget_value, 2),
            variance: round_half_even(variance, 2),
            variance_pct: round_half_even(variance_pct, 2),
            status: if variance > 0.0 {
                "Over Budget".to_string()
            } else {
                "Under Budget".to_string()
            },
        },
        exposure_analysis,
        hedge_plan,
        recommendations,
    }
}

// -------------------------------------------------- interest rate risk

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DebtExposure {
    pub debt_id: String,
    pub principal: f64,
    pub current_rate: f64,
    pub rate_type: String,
    pub maturity_years: f64,
    pub reference_rate: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InterestRateHedgingRequest {
    pub company_id: String,
    pub exposures: Vec<DebtExposure>,
    pub total_debt: f64,
    pub variable_debt: f64,
    pub rate_shock_scenarios: Vec<f64>,
    pub yield_curve_slope: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExposureSummary {
    pub total_debt: f64,
    pub variable_debt: f64,
    pub fixed_debt: f64,
    pub variable_debt_pct: f64,
    pub fixed_debt_pct: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SensitivityRow {
    pub rate_shock_bp: f64,
    pub additional_annual_cost: f64,
    pub monthly_impact: f64,
    pub ebitda_impact_pct: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HedgeAction {
    pub action: String,
    pub amount: f64,
    pub instrument: String,
    pub target_rate: f64,
    pub tenor: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InterestRateHedgingResult {
    pub company_id: String,
    pub exposure_summary: ExposureSummary,
    pub sensitivity_analysis: Vec<SensitivityRow>,
    pub hedge_recommendations: Vec<HedgeAction>,
    pub recommendations: Vec<String>,
}

/// `/analyze` on the interest rate hedging service.
///
/// PYTHON QUIRKS, preserved:
/// - the sensitivity baseline is a hard-coded 5% on the variable debt, not
///   the loans' own rates, so every shock scenario is measured against a
///   rate nobody holds;
/// - the EBITDA impact divides by TOTAL debt rather than EBITDA, so it is
///   a percentage of the debt book and explodes as total debt shrinks;
/// - the fixed rate offered on a swap is 5% plus TWICE the curve slope,
///   twice the adjustment used for the bond in the maturity action;
/// - the fixed-debt percentage is derived as the complement of the variable
///   percentage, so it silently absorbs any debt that is neither;
/// - the weighted average maturity is principal-weighted and ignores the
///   rate type entirely.
pub fn analyze_interest_rate_hedge(req: &InterestRateHedgingRequest) -> InterestRateHedgingResult {
    let variable_debt_pct = if req.total_debt != 0.0 {
        req.variable_debt / req.total_debt
    } else {
        0.0
    };

    let mut sensitivity_analysis = Vec::new();
    for shock in &req.rate_shock_scenarios {
        let current_interest = req.variable_debt * 0.05;
        let shocked_interest = req.variable_debt * (0.05 + shock);
        let additional_cost = shocked_interest - current_interest;

        sensitivity_analysis.push(SensitivityRow {
            rate_shock_bp: round_half_even(shock * 10_000.0, 0),
            additional_annual_cost: round_half_even(additional_cost, 2),
            monthly_impact: round_half_even(additional_cost / 12.0, 2),
            ebitda_impact_pct: round_half_even(
                if req.total_debt != 0.0 {
                    additional_cost / req.total_debt * 100.0
                } else {
                    0.0
                },
                2,
            ),
        });
    }

    let principal_total: f64 = req.exposures.iter().map(|e| e.principal).sum();
    let weighted_avg_maturity = if req.exposures.is_empty() || principal_total == 0.0 {
        0.0
    } else {
        req.exposures
            .iter()
            .map(|e| e.principal * e.maturity_years)
            .sum::<f64>()
            / principal_total
    };

    let mut hedge_recommendations = Vec::new();
    if variable_debt_pct > 0.6 {
        hedge_recommendations.push(HedgeAction {
            action: "FIX".to_string(),
            amount: round_half_even(req.variable_debt * 0.3, 2),
            instrument: "Interest Rate Swap".to_string(),
            target_rate: round_half_even(0.05 + req.yield_curve_slope * 2.0, 4),
            tenor: "3 years".to_string(),
        });
    }
    if weighted_avg_maturity < 2.0 {
        hedge_recommendations.push(HedgeAction {
            action: "EXTEND".to_string(),
            amount: round_half_even(req.total_debt * 0.2, 2),
            instrument: "Fixed Rate Bond".to_string(),
            target_rate: round_half_even(0.05 + req.yield_curve_slope, 4),
            tenor: "5 years".to_string(),
        });
    }

    let mut recommendations = Vec::new();
    if variable_debt_pct > 0.7 {
        recommendations
            .push("High variable rate exposure - consider swapping to fixed".to_string());
    }
    if weighted_avg_maturity < 1.5 {
        recommendations.push("Short debt maturity - extend to reduce refinancing risk".to_string());
    }

    InterestRateHedgingResult {
        company_id: req.company_id.clone(),
        exposure_summary: ExposureSummary {
            total_debt: req.total_debt,
            variable_debt: req.variable_debt,
            fixed_debt: req.total_debt - req.variable_debt,
            variable_debt_pct: round_half_even(variable_debt_pct * 100.0, 2),
            fixed_debt_pct: round_half_even((1.0 - variable_debt_pct) * 100.0, 2),
        },
        sensitivity_analysis,
        hedge_recommendations,
        recommendations,
    }
}
