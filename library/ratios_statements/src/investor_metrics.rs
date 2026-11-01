//! Investor-facing return metrics that were still Python-only: the DuPont
//! decomposition, asset-turnover profile, CROIC, EVA and EPS.
//!
//! Each function returns the service's response body verbatim, key for key,
//! so the seam can hand it straight back without reshaping. Thresholds and
//! benchmarks are the services' own fixed ones (industry ROE 15%, CROIC 12%,
//! turnover floor 1.5) rather than anything configurable, and the commentary
//! strings are reproduced because clients already display them.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vimbai_common::{py_round, py_round2};

// --- DuPont -----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuPontRequest {
    pub company_id: String,
    pub period: String,
    pub net_income: f64,
    pub revenue: f64,
    pub total_assets: f64,
    pub total_equity: f64,
    pub ebt: f64,
    pub ebit: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuPontMetrics {
    pub net_profit_margin: f64,
    pub asset_turnover: f64,
    pub financial_leverage: f64,
    pub tax_burden: f64,
    pub interest_burden: f64,
    pub return_on_equity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuPontResponse {
    pub company_id: String,
    pub period: String,
    pub du_pont_metrics: DuPontMetrics,
    pub three_factor_breakdown: BTreeMap<String, f64>,
    pub five_factor_breakdown: BTreeMap<String, f64>,
    pub roe_contribution: BTreeMap<String, f64>,
    pub peer_comparison: BTreeMap<String, f64>,
    pub conclusions: BTreeMap<String, String>,
}

/// Every ratio falls back to zero rather than an error when its denominator is
/// missing, which is how the service behaves for partly-populated statements.
fn safe_div(numerator: f64, denominator: f64) -> f64 {
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

pub fn du_pont_analysis(request: &DuPontRequest) -> DuPontResponse {
    let net_profit_margin = safe_div(request.net_income, request.revenue);
    let asset_turnover = safe_div(request.revenue, request.total_assets);
    let financial_leverage = safe_div(request.total_assets, request.total_equity);
    let tax_burden = safe_div(request.net_income, request.ebt);
    let interest_burden = safe_div(request.ebt, request.ebit);

    let three_factor_roe = net_profit_margin * asset_turnover * financial_leverage;
    let five_factor_roe = tax_burden
        * interest_burden
        * safe_div(request.ebit, request.revenue)
        * asset_turnover
        * financial_leverage;

    let mut roe_contribution = BTreeMap::new();
    roe_contribution.insert(
        "profit_margin_contribution".into(),
        py_round2(net_profit_margin * 100.0),
    );
    roe_contribution.insert(
        "asset_turnover_contribution".into(),
        py_round2(asset_turnover * 100.0),
    );
    roe_contribution.insert(
        "financial_leverage_contribution".into(),
        py_round2(financial_leverage * 100.0),
    );
    roe_contribution.insert("tax_efficiency".into(), py_round2(tax_burden * 100.0));
    roe_contribution.insert("interest_impact".into(), py_round2(interest_burden * 100.0));

    let mut peer_comparison = BTreeMap::new();
    peer_comparison.insert("industry_avg_roe".into(), 15.0);
    peer_comparison.insert("company_roe".into(), py_round2(three_factor_roe * 100.0));
    peer_comparison.insert(
        "vs_industry".into(),
        py_round2(three_factor_roe * 100.0 - 15.0),
    );

    let profitability = if net_profit_margin > 0.1 {
        "Strong"
    } else if net_profit_margin > 0.05 {
        "Moderate"
    } else {
        "Weak"
    };
    let efficiency = if asset_turnover > 1.5 {
        "High"
    } else if asset_turnover > 1.0 {
        "Moderate"
    } else {
        "Low"
    };
    let leverage = if financial_leverage < 2.0 {
        "Conservative"
    } else if financial_leverage < 3.0 {
        "Moderate"
    } else {
        "Aggressive"
    };

    let mut conclusions = BTreeMap::new();
    conclusions.insert("profitability".into(), profitability.to_string());
    conclusions.insert("efficiency".into(), efficiency.to_string());
    conclusions.insert("leverage".into(), leverage.to_string());

    let mut three_factor_breakdown = BTreeMap::new();
    three_factor_breakdown.insert("net_profit_margin".into(), py_round(net_profit_margin, 4));
    three_factor_breakdown.insert("asset_turnover".into(), py_round(asset_turnover, 4));
    three_factor_breakdown.insert("financial_leverage".into(), py_round(financial_leverage, 4));
    three_factor_breakdown.insert("calculated_roe".into(), py_round(three_factor_roe, 4));

    let mut five_factor_breakdown = BTreeMap::new();
    five_factor_breakdown.insert("tax_burden".into(), py_round(tax_burden, 4));
    five_factor_breakdown.insert("interest_burden".into(), py_round(interest_burden, 4));
    five_factor_breakdown.insert(
        "operating_margin".into(),
        if request.revenue == 0.0 {
            0.0
        } else {
            py_round(request.ebit / request.revenue, 4)
        },
    );
    five_factor_breakdown.insert("asset_turnover".into(), py_round(asset_turnover, 4));
    five_factor_breakdown.insert("financial_leverage".into(), py_round(financial_leverage, 4));
    five_factor_breakdown.insert("calculated_roe".into(), py_round(five_factor_roe, 4));

    DuPontResponse {
        company_id: request.company_id.clone(),
        period: request.period.clone(),
        du_pont_metrics: DuPontMetrics {
            net_profit_margin: py_round(net_profit_margin, 4),
            asset_turnover: py_round(asset_turnover, 4),
            financial_leverage: py_round(financial_leverage, 4),
            tax_burden: py_round(tax_burden, 4),
            interest_burden: py_round(interest_burden, 4),
            return_on_equity: py_round(three_factor_roe, 4),
        },
        three_factor_breakdown,
        five_factor_breakdown,
        roe_contribution,
        peer_comparison,
        conclusions,
    }
}

// --- asset turnover ---------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnoverRequest {
    pub company_id: String,
    pub period: String,
    pub revenue: f64,
    pub cost_of_goods_sold: f64,
    pub total_assets: f64,
    pub fixed_assets: f64,
    pub current_assets: f64,
    pub current_liabilities: f64,
    pub accounts_receivable: f64,
    pub inventory: f64,
    pub accounts_payable: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnoverMetrics {
    pub total_asset_turnover: f64,
    pub fixed_asset_turnover: f64,
    pub working_capital_turnover: f64,
    pub receivables_turnover: f64,
    pub inventory_turnover: f64,
    pub payables_turnover: f64,
    pub cash_conversion_cycle: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnoverResponse {
    pub company_id: String,
    pub period: String,
    pub turnover_metrics: TurnoverMetrics,
    pub industry_benchmark: BTreeMap<String, f64>,
    pub efficiency_assessment: String,
    pub improvement_areas: Vec<String>,
    pub recommendations: Vec<String>,
}

pub fn asset_turnover(request: &TurnoverRequest) -> TurnoverResponse {
    let total_turnover = safe_div(request.revenue, request.total_assets);
    let fixed_turnover = safe_div(request.revenue, request.fixed_assets);
    let working_capital = request.current_assets - request.current_liabilities;
    let wc_turnover = safe_div(request.revenue, working_capital);
    let receivables_turnover = safe_div(request.revenue, request.accounts_receivable);
    let inventory_turnover = safe_div(request.cost_of_goods_sold, request.inventory);
    let payables_turnover = safe_div(request.cost_of_goods_sold, request.accounts_payable);

    let dso = safe_div(request.accounts_receivable, request.revenue) * 365.0;
    let dio = safe_div(request.inventory, request.cost_of_goods_sold) * 365.0;
    let dpo = safe_div(request.accounts_payable, request.cost_of_goods_sold) * 365.0;
    // Truncated, not rounded: the service casts the sum to int.
    let ccc = (dso + dio - dpo) as i64;

    let mut benchmark = BTreeMap::new();
    benchmark.insert("total_asset_turnover".into(), 1.5);
    benchmark.insert("fixed_asset_turnover".into(), 3.0);
    benchmark.insert("receivables_turnover".into(), 8.0);
    benchmark.insert("inventory_turnover".into(), 6.0);

    let mut improvement_areas = Vec::new();
    if total_turnover < 1.5 {
        improvement_areas.push("Total asset utilization below industry average".to_string());
    }
    if inventory_turnover < 6.0 {
        improvement_areas.push("Inventory management could be improved".to_string());
    }
    if ccc > 90 {
        improvement_areas.push("Cash conversion cycle is lengthy".to_string());
    }

    let assessment = if total_turnover > 1.5 {
        "efficient"
    } else if total_turnover > 1.0 {
        "moderate"
    } else {
        "inefficient"
    };

    let recommendations = if assessment == "inefficient" {
        vec![
            "Optimize working capital management".to_string(),
            "Improve inventory turnover".to_string(),
            "Review credit policies for receivables".to_string(),
            "Negotiate better payment terms with suppliers".to_string(),
        ]
    } else {
        vec!["Continue monitoring efficiency metrics".to_string()]
    };

    if improvement_areas.is_empty() {
        improvement_areas.push("Asset utilization is in line with expectations".to_string());
    }

    TurnoverResponse {
        company_id: request.company_id.clone(),
        period: request.period.clone(),
        turnover_metrics: TurnoverMetrics {
            total_asset_turnover: py_round2(total_turnover),
            fixed_asset_turnover: py_round2(fixed_turnover),
            working_capital_turnover: py_round2(wc_turnover),
            receivables_turnover: py_round2(receivables_turnover),
            inventory_turnover: py_round2(inventory_turnover),
            payables_turnover: py_round2(payables_turnover),
            cash_conversion_cycle: ccc,
        },
        industry_benchmark: benchmark,
        efficiency_assessment: assessment.to_string(),
        improvement_areas,
        recommendations,
    }
}

// --- CROIC ------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CroicRequest {
    pub company_id: String,
    pub operating_cash_flow: f64,
    pub capital_expenditures: f64,
    pub total_assets: f64,
    pub current_liabilities: f64,
    #[serde(default)]
    pub periods: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CroicMetrics {
    pub free_cash_flow: f64,
    pub invested_capital: f64,
    pub cash_return_on_invested_capital: f64,
    pub croic_5_year_avg: f64,
    pub capital_efficiency_rating: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CroicPeriod {
    pub period: String,
    pub croic: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CroicResponse {
    pub company_id: String,
    pub current_croic: CroicMetrics,
    pub historical_croic: Vec<CroicPeriod>,
    pub industry_benchmark: f64,
    pub croic_vs_industry: String,
    pub recommendations: Vec<String>,
}

pub fn croic(request: &CroicRequest) -> CroicResponse {
    let fcf = request.operating_cash_flow - request.capital_expenditures;
    let invested_capital = request.total_assets - request.current_liabilities;
    let ratio = safe_div(fcf, invested_capital);

    let rating = if ratio > 0.15 {
        "excellent"
    } else if ratio > 0.10 {
        "good"
    } else if ratio > 0.05 {
        "fair"
    } else {
        "poor"
    };

    // Every historical period is the current ratio scaled by 0.9. It is a
    // stand-in the service has always published, not a real history.
    let historical_croic: Vec<CroicPeriod> = request
        .periods
        .iter()
        .map(|period| CroicPeriod {
            period: period.clone(),
            croic: py_round(ratio * 0.9, 4),
        })
        .collect();

    let avg_5yr = if historical_croic.is_empty() {
        ratio
    } else {
        historical_croic.iter().map(|row| row.croic).sum::<f64>() / historical_croic.len() as f64
    };

    let industry_benchmark = 0.12;
    let vs_industry = if ratio > industry_benchmark {
        "outperforming"
    } else {
        "underperforming"
    };

    let recommendations = if rating == "excellent" || rating == "good" {
        vec![
            "Maintain high CROIC through disciplined capital allocation".to_string(),
            "Reinvest in high-return projects".to_string(),
            "Return excess cash to shareholders if limited opportunities".to_string(),
        ]
    } else {
        vec![
            "Improve operational efficiency to boost FCF".to_string(),
            "Review capital structure".to_string(),
            "Divest underperforming assets".to_string(),
        ]
    };

    CroicResponse {
        company_id: request.company_id.clone(),
        current_croic: CroicMetrics {
            free_cash_flow: py_round2(fcf),
            invested_capital: py_round2(invested_capital),
            cash_return_on_invested_capital: py_round(ratio, 4),
            croic_5_year_avg: py_round(avg_5yr, 4),
            capital_efficiency_rating: rating.to_string(),
        },
        historical_croic,
        industry_benchmark,
        croic_vs_industry: vs_industry.to_string(),
        recommendations,
    }
}

// --- EVA --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaRequest {
    pub company_id: String,
    pub period: String,
    pub ebit: f64,
    pub tax_rate: f64,
    pub total_assets: f64,
    pub current_liabilities: f64,
    pub debt: f64,
    pub equity: f64,
    pub cost_of_debt: f64,
    pub cost_of_equity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaMetrics {
    pub invested_capital: f64,
    pub weighted_average_cost_of_capital: f64,
    pub net_operating_profit_after_tax: f64,
    pub capital_charge: f64,
    pub economic_value_added: f64,
    pub value_creation_index: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaResponse {
    pub company_id: String,
    pub period: String,
    pub eva_metrics: EvaMetrics,
    pub eva_by_segment: BTreeMap<String, f64>,
    pub eva_momentum: String,
    pub wealth_created: bool,
    pub recommendations: Vec<String>,
}

pub fn eva(request: &EvaRequest) -> EvaResponse {
    let capital = request.debt + request.equity;
    let debt_ratio = safe_div(request.debt, capital);
    let equity_ratio = safe_div(request.equity, capital);

    let wacc = debt_ratio * request.cost_of_debt * (1.0 - request.tax_rate)
        + equity_ratio * request.cost_of_equity;
    let invested_capital = request.total_assets - request.current_liabilities;
    let nopat = request.ebit * (1.0 - request.tax_rate);
    let capital_charge = invested_capital * wacc;
    let eva = nopat - capital_charge;
    let value_creation_index = safe_div(nopat, capital_charge);

    // Fixed split, not a real segment attribution: 70/20/10 of total EVA.
    let mut eva_by_segment = BTreeMap::new();
    eva_by_segment.insert("core_operations".into(), py_round2(eva * 0.7));
    eva_by_segment.insert("growth_investments".into(), py_round2(eva * 0.2));
    eva_by_segment.insert("non_operating".into(), py_round2(eva * 0.1));

    // The -1m floor is the service's own cutoff for calling EVA "declining".
    let momentum = if eva > 0.0 {
        "improving"
    } else if eva < -1_000_000.0 {
        "declining"
    } else {
        "stable"
    };
    let wealth_created = eva > 0.0;

    let recommendations = if wealth_created {
        vec![
            "Continue value-creating investments".to_string(),
            "Monitor WACC changes".to_string(),
            "Consider share buybacks if excess capital".to_string(),
        ]
    } else {
        vec![
            "Focus on capital efficiency to improve EVA".to_string(),
            "Review underperforming business segments".to_string(),
            "Optimize working capital management".to_string(),
        ]
    };

    EvaResponse {
        company_id: request.company_id.clone(),
        period: request.period.clone(),
        eva_metrics: EvaMetrics {
            invested_capital: py_round2(invested_capital),
            weighted_average_cost_of_capital: py_round(wacc, 4),
            net_operating_profit_after_tax: py_round2(nopat),
            capital_charge: py_round2(capital_charge),
            economic_value_added: py_round2(eva),
            value_creation_index: py_round2(value_creation_index),
        },
        eva_by_segment,
        eva_momentum: momentum.to_string(),
        wealth_created,
        recommendations,
    }
}

// --- EPS --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareOptionInput {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub exercise_price: f64,
    #[serde(default)]
    pub average_market_price: f64,
    #[serde(default)]
    pub number_of_options: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AntiDilutiveItem {
    #[serde(default)]
    pub anti_dilutive_shares: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpsRequest {
    pub company_id: String,
    pub period: String,
    pub net_profit_attributable: f64,
    pub weighted_average_shares: f64,
    #[serde(default)]
    pub discontinued_operations_profit: f64,
    #[serde(default)]
    pub potential_dilutive_shares: Vec<ShareOptionInput>,
    #[serde(default)]
    pub anti_dilutive_items: Vec<AntiDilutiveItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShareOption {
    pub option_id: String,
    pub number_of_options: f64,
    pub exercise_price: f64,
    pub average_market_price: f64,
    pub dilution_effect: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpsResponse {
    pub company_id: String,
    pub period: String,
    pub basic_eps: f64,
    pub diluted_eps: f64,
    pub eps_from_continuing_operations: f64,
    pub weighted_average_shares: f64,
    pub diluted_shares: f64,
    pub dilutive_effect: f64,
    pub anti_dilutive_effect: f64,
    pub potential_dilutive_instruments: Vec<ShareOption>,
}

pub fn eps(request: &EpsRequest) -> EpsResponse {
    let basic_eps = safe_div(
        request.net_profit_attributable,
        request.weighted_average_shares,
    );
    let continuing_profit =
        request.net_profit_attributable - request.discontinued_operations_profit;
    let eps_continuing = safe_div(continuing_profit, request.weighted_average_shares);

    let mut dilutive_effect = 0.0;
    let mut instruments = Vec::new();
    for option in &request.potential_dilutive_shares {
        if option.average_market_price > option.exercise_price {
            // Treasury-stock method, truncated to whole shares like the service.
            let treasury_shares = (option.number_of_options
                * (1.0 - option.exercise_price / option.average_market_price))
                as i64;
            let delta = treasury_shares as f64;
            dilutive_effect += delta;
            instruments.push(ShareOption {
                option_id: option.id.clone(),
                number_of_options: option.number_of_options,
                exercise_price: option.exercise_price,
                average_market_price: option.average_market_price,
                dilution_effect: delta,
            });
        }
    }

    let anti_total: f64 = request
        .anti_dilutive_items
        .iter()
        .map(|item| item.anti_dilutive_shares)
        .sum();
    dilutive_effect -= anti_total;

    let diluted_shares = request.weighted_average_shares + dilutive_effect.max(0.0);
    let diluted_eps = safe_div(request.net_profit_attributable, diluted_shares);

    EpsResponse {
        company_id: request.company_id.clone(),
        period: request.period.clone(),
        basic_eps: py_round(basic_eps, 4),
        diluted_eps: py_round(diluted_eps, 4),
        eps_from_continuing_operations: py_round(eps_continuing, 4),
        weighted_average_shares: request.weighted_average_shares,
        diluted_shares,
        dilutive_effect: dilutive_effect.max(0.0),
        anti_dilutive_effect: anti_total.abs().max(0.0),
        potential_dilutive_instruments: instruments,
    }
}
