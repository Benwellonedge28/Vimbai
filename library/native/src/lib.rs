//! Vimbai's Python seam.
//!
//! This is the only crate in the project that is not `#![forbid(unsafe_code)]`:
//! PyO3's extension machinery needs `unsafe` to hand values across the
//! interpreter boundary. No financial logic lives here. Every function below
//! is a thin adapter that parses strings into [`Money`], calls a safe library
//! function, and renders the result back as a string, so the numbers the
//! Python services report are produced by the same code the Rust tests cover.
// PyO3's #[pyfunction] glue converts PyErr into PyErr at every wrapper; the
// lint fires on the generated code rather than on anything we wrote.
#![allow(clippy::useless_conversion)]

mod assurance;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use serde_json::Value;

use vimbai_assets::{
    amortization, analyze_fixed_assets, depreciation_batch, depreciation_method, AmortizationAsset,
    DepreciationAsset, FixedAssetsRequest,
};
use vimbai_budgeting::forecasting::{forecast_metric, ForecastRequest as MetricForecastRequest};
use vimbai_capital_valuation::capital_budgeting::{analyse_capital_budget, CapitalBudgetRequest};
use vimbai_capital_valuation::capital_returns::{
    accounting_rate_of_return, annuity_factor, annuity_factor_percent, arr_decision,
    arr_from_annual_profits, average_capital, average_profit, compound_rate, cost_of_capital,
    cost_of_capital_percent, discount_factor, discount_factor_percent, discount_factor_table,
    discount_factor_table_range, discounted_payback, simple_rate, wacc,
};
use vimbai_common::{Money, MoneyError};
use vimbai_cost_accounting::absorption::{cost_plus_pricing, overhead_absorption, product_cost};
use vimbai_cost_accounting::activity_based::{calculate_abc, AbcRequest};
use vimbai_cost_accounting::actual_cost::{
    actual_labour_cost, actual_material_cost, compare_actual_to_budget, total_actual_cost,
    variance_breakdown,
};
use vimbai_cost_accounting::budgeted_cost::{
    compare_output, cost_breakdown, create_budgeted_cost, flexible_budget, total_budget, BudgetItem,
};
use vimbai_cost_accounting::cost_centre::{generate_summary, CostCentreCost as SummaryCost};
use vimbai_cost_accounting::job_costing::{add_cost, CostType, JobCosts};
use vimbai_cost_accounting::labour_variance::{analyse_labour_variance, LabourVarianceRequest};
use vimbai_cost_accounting::limiting_factor::{
    analyze_limiting_factor, compare_products_for_factor, ProductWithFactor,
};
use vimbai_cost_accounting::margin_safety::{
    margin_from_basic_data, margin_of_safety, sales_for_target_mos,
};
use vimbai_cost_accounting::marginal::{
    contribution_analysis, forecast_profit, marginal_cost_item, marginal_income_statement,
};
use vimbai_cost_accounting::material_variance::{
    material_cost_variance, multi_material_variance, simple_material_variance, MaterialInput,
};
use vimbai_cost_accounting::order_acceptance::{analyze_one_time_order, analyze_order_acceptance};
use vimbai_cost_accounting::overhead_apportionment::{
    apportion_overhead, batch_apportion_overheads, CostCentreDatum,
    OverheadItem as ApportionOverheadItem,
};
use vimbai_cost_accounting::prime_cost::{
    direct_cost_item, prime_cost, prime_cost_from_items, DirectCostItem,
};
use vimbai_cost_accounting::standard_cost::{
    labour_standard, material_standard, overhead_standard, standard_per_unit, total_standard,
    variance_analysis,
};
use vimbai_cost_accounting::throughput::{analyze_throughput, ThroughputRequest};
use vimbai_cost_accounting::total_production_cost::calculate_total_production_cost;
use vimbai_ifrs::interim::{
    effective_tax_rate as interim_tax_rate, prepare as interim_prepare,
    year_to_date as interim_ytd, InterimPeriod, InterimReportRequest,
};
use vimbai_ifrs::segments::{report_segments, SegmentReportRequest};
use vimbai_ratios_statements::comparative::{
    analyze as comparative_analyze, horizontal_analysis as comparative_horizontal, trend_series,
    variance_analysis as comparative_variance, ComparativeRequest, TrendSeries, VarianceReport,
};
use vimbai_ratios_statements::working_capital::{
    analyze_wc_finance, analyze_working_capital, calculate_factoring, optimize_working_capital,
    FactoringInput, WcAnalysisInput, WcFinanceInput, WcOptimizeInput,
};

use vimbai_reporting::annual::{prepare_annual_report, AnnualReportRequest};
use vimbai_reporting::financial::{generate_financial_report, FinancialReportingRequest};
use vimbai_reporting::management::{generate_management_report, ManagementReportingRequest};
use vimbai_reporting::regulatory::{generate_report, ReportRequest};
use vimbai_tax::statutory::{statutory_corporate_tax, CorporateTaxInput};
use vimbai_tax::{calculate_tax, TaxBracket, TaxType};
use vimbai_treasury_fx::analytics::{analyze_treasury, TreasuryAnalyticsRequest};
use vimbai_treasury_fx::cash_forecast::{forecast_cash_flow, CashFlowForecastRequest};
use vimbai_treasury_fx::credit_derivatives::{
    analyze_credit_derivatives, CreditDerivativesRequest,
};
use vimbai_treasury_fx::derivatives::{
    value_cross_currency_swaps, value_swaps, CrossCurrencySwapRequest, SwapValuationRequest,
};
use vimbai_treasury_fx::exotics::{price_derivative, DerivativeRequest};
use vimbai_treasury_fx::forwards::{analyze_forward_contracts, ForwardContractRequest};
use vimbai_treasury_fx::fx_hedging::{analyze_currency_hedge, CurrencyHedgingRequest};
use vimbai_treasury_fx::fx_translation::{translate_financials, TranslationRequest};
use vimbai_treasury_fx::hedging_analytics::{
    analyze_commodity_hedge, analyze_futures_hedge, analyze_interest_rate_hedge,
    CommodityHedgingRequest, FuturesHedgingRequest, InterestRateHedgingRequest,
};
use vimbai_treasury_fx::liquidity::{
    analyze_liquidity, analyze_liquidity_risk, forecast_liquidity, LiquidityAnalysisRequest,
    LiquidityForecastRequest, LiquidityRequest,
};
use vimbai_treasury_fx::multi_currency::{
    convert_fixed, create_hedge, process_transaction, CurrencyConversionRequest, HedgeRequest,
    TransactionRequest,
};

/// Anything the seam refuses, described in words the calling service can log.
#[derive(Debug)]
struct SeamError(String);

impl From<MoneyError> for SeamError {
    fn from(err: MoneyError) -> Self {
        SeamError(err.to_string())
    }
}

impl From<serde_json::Error> for SeamError {
    fn from(err: serde_json::Error) -> Self {
        SeamError(err.to_string())
    }
}

/// Named rather than routed through `From`: PyO3 already has a blanket
/// conversion, and clippy rightly treats a second identity path as noise.
fn seam_error(err: SeamError) -> PyErr {
    PyValueError::new_err(err.0)
}

fn parse_all(values: &[String]) -> Result<Vec<Money>, SeamError> {
    values
        .iter()
        .map(|v| Money::parse(v).map_err(SeamError::from))
        .collect()
}

/// Sum a list of amounts exactly, in the order given.
fn sum_exactly(values: &[String]) -> Result<String, SeamError> {
    let amounts = parse_all(values)?;
    Ok(Money::sum(amounts.iter()).to_string())
}

#[pyfunction]
fn sum_amounts(values: Vec<String>) -> PyResult<String> {
    sum_exactly(&values).map_err(seam_error)
}

/// Split `total` across `weights` without losing a cent.
fn split_exactly(total: &str, weights: &[String], scale: i64) -> Result<Vec<String>, SeamError> {
    let total = Money::parse(total).map_err(SeamError::from)?;
    let weights = parse_all(weights)?;
    Ok(total
        .allocate(&weights, scale)
        .iter()
        .map(|m| m.to_string())
        .collect())
}

#[pyfunction]
#[pyo3(signature = (total, weights, scale = 2))]
fn allocate(total: &str, weights: Vec<String>, scale: i64) -> PyResult<Vec<String>> {
    split_exactly(total, &weights, scale).map_err(seam_error)
}

/// Run the treasury KPI set. Amounts travel as JSON strings in both directions.
fn analyze_exactly(payload: &str) -> Result<String, SeamError> {
    let request: TreasuryAnalyticsRequest = serde_json::from_str(payload)?;
    let result = analyze_treasury(&request);
    let rendered = serde_json::to_value(&result)?;
    Ok(render_amounts(rendered).to_string())
}

#[pyfunction]
fn analyze_treasury_json(payload: &str) -> PyResult<String> {
    analyze_exactly(payload).map_err(seam_error)
}

/// JSON numbers cannot hold a 40-digit balance, so every amount leaves as a
/// string even when serde chose a numeric rendering for it.
fn render_amounts(value: Value) -> Value {
    match value {
        Value::Number(n) => Value::String(n.to_string()),
        Value::Array(items) => Value::Array(items.into_iter().map(render_amounts).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, render_amounts(v)))
                .collect(),
        ),
        other => other,
    }
}

/// corporate-tax-service's statutory liability, from and to the service's own
/// JSON envelope. The service owns validation; the seam only computes.
#[pyfunction]
fn corporate_tax_json(payload: &str) -> PyResult<String> {
    let input: CorporateTaxInput =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let result = statutory_corporate_tax(&input);
    serde_json::to_string(&result).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// tax-calculation-service's bracket engine. Brackets arrive as JSON so the
/// caller can pass the country's own ladder; omitted, the ZW fixture applies.
#[pyfunction]
#[pyo3(signature = (payload, brackets = None))]
fn tax_calculation_json(payload: &str, brackets: Option<&str>) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Request {
        company_id: String,
        tax_type: String,
        country: String,
        taxable_amount: f64,
        deductions: f64,
        fiscal_year: i64,
    }

    let req: Request = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let parsed_brackets: Option<Vec<TaxBracket>> = match brackets {
        Some(raw) => Some(serde_json::from_str(raw).map_err(|e| seam_error(SeamError::from(e)))?),
        None => None,
    };
    let tax_type = match req.tax_type.to_ascii_lowercase().as_str() {
        "vat" => TaxType::Vat,
        "withholding" => TaxType::Withholding,
        "capital_gains" => TaxType::CapitalGains,
        "paye" => TaxType::Paye,
        // "income_tax" is what the service calls the progressive ladder.
        "income_tax" | "corporate" => TaxType::Corporate,
        _ => TaxType::Corporate,
    };
    let result = calculate_tax(
        &req.company_id,
        tax_type,
        &req.country,
        req.taxable_amount,
        req.deductions,
        parsed_brackets.as_deref(),
        req.fiscal_year,
    );
    serde_json::to_string(&result).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// fixed-assets-service's composite register report.
#[pyfunction]
fn fixed_assets_json(payload: &str) -> PyResult<String> {
    let request: FixedAssetsRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let report =
        analyze_fixed_assets(&request).map_err(|e| seam_error(SeamError(e.to_string())))?;
    serde_json::to_string(&report).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// depreciation-service `/calculate`, including its placeholder schedule.
#[pyfunction]
#[pyo3(signature = (payload, method = "straight_line"))]
fn depreciation_json(payload: &str, method: &str) -> PyResult<String> {
    let assets: Vec<DepreciationAsset> =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let output = depreciation_batch(method, &assets);
    serde_json::to_string(&output).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// depreciation-service `/method`: one asset, one ladder.
#[pyfunction]
#[pyo3(signature = (cost, salvage_value, useful_life, method = "straight_line"))]
fn depreciation_method_json(
    cost: f64,
    salvage_value: f64,
    useful_life: i64,
    method: &str,
) -> PyResult<String> {
    let output = depreciation_method(
        method,
        &Money::parse(&cost.to_string()).unwrap_or_else(|_| Money::zero()),
        &Money::parse(&salvage_value.to_string()).unwrap_or_else(|_| Money::zero()),
        useful_life,
    );
    serde_json::to_string(&output).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// amortization-service `/calculate`.
#[pyfunction]
fn amortization_json(payload: &str) -> PyResult<String> {
    let intangibles: Vec<AmortizationAsset> =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let output = amortization(&intangibles);
    serde_json::to_string(&output).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// du_pont-service, body in and body out.
#[pyfunction]
fn du_pont_json(payload: &str) -> PyResult<String> {
    let request: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_ratios_statements::du_pont_from_json(&request)
        .map_err(|e| seam_error(SeamError(e.to_string())))?;
    serde_json::to_string(&answer).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// asset_turnover-service, body in and body out.
#[pyfunction]
fn asset_turnover_json(payload: &str) -> PyResult<String> {
    let request: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_ratios_statements::asset_turnover_from_json(&request)
        .map_err(|e| seam_error(SeamError(e.to_string())))?;
    serde_json::to_string(&answer).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// croic-service, body in and body out.
#[pyfunction]
fn croic_json(payload: &str) -> PyResult<String> {
    let request: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_ratios_statements::croic_from_json(&request)
        .map_err(|e| seam_error(SeamError(e.to_string())))?;
    serde_json::to_string(&answer).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// eva-service, body in and body out.
#[pyfunction]
fn eva_json(payload: &str) -> PyResult<String> {
    let request: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_ratios_statements::eva_from_json(&request)
        .map_err(|e| seam_error(SeamError(e.to_string())))?;
    serde_json::to_string(&answer).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// eps-service, body in and body out.
#[pyfunction]
fn eps_json(payload: &str) -> PyResult<String> {
    let request: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_ratios_statements::eps_from_json(&request)
        .map_err(|e| seam_error(SeamError(e.to_string())))?;
    // per-share figures become floats for the service; share counts are already
    // JSON integers and stay integers. `period` is numeric-looking text and stays text.
    let rendered = floats_from_strings_except(&answer, &["period"]);
    serde_json::to_string(&rendered).map_err(|e| seam_error(SeamError(e.to_string())))
}

#[pyfunction]
fn coc_calculate_json(interest_rate: f64, risk_premium: f64) -> PyResult<String> {
    let value = cost_of_capital(interest_rate, risk_premium);
    serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
}

#[pyfunction]
fn coc_from_percent_json(
    interest_rate_percent: f64,
    risk_premium_percent: f64,
) -> PyResult<String> {
    let value = cost_of_capital_percent(interest_rate_percent, risk_premium_percent);
    serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
}

#[pyfunction]
fn coc_wacc_json(
    equity_value: f64,
    cost_of_equity: f64,
    debt_value: f64,
    cost_of_debt: f64,
    tax_rate: f64,
) -> PyResult<String> {
    match wacc(
        equity_value,
        cost_of_equity,
        debt_value,
        cost_of_debt,
        tax_rate,
    ) {
        Ok(value) => {
            serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
        }
        Err(message) => Ok(serde_json::json!({"error": message}).to_string()),
    }
}

#[pyfunction]
fn coc_simple_rate_json(principal: f64, amount_after: f64, years: i64) -> PyResult<String> {
    match simple_rate(principal, amount_after, years) {
        Ok(value) => {
            serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
        }
        Err(message) => Ok(serde_json::json!({"error": message}).to_string()),
    }
}

#[pyfunction]
fn coc_compound_rate_json(present_value: f64, future_value: f64, years: i64) -> PyResult<String> {
    match compound_rate(present_value, future_value, years) {
        Ok(value) => {
            serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
        }
        Err(message) => Ok(serde_json::json!({"error": message}).to_string()),
    }
}

#[pyfunction]
fn df_calculate_json(rate: f64, years: i64) -> PyResult<String> {
    match discount_factor(rate, years) {
        Ok(value) => {
            serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
        }
        Err(message) => Ok(serde_json::json!({"error": message}).to_string()),
    }
}

#[pyfunction]
fn df_percent_json(rate_percent: f64, years: i64) -> PyResult<String> {
    let value = discount_factor_percent(rate_percent, years);
    serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
}

#[pyfunction]
fn df_annuity_json(rate: f64, years: i64) -> PyResult<String> {
    let value = annuity_factor(rate, years);
    serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
}

#[pyfunction]
fn df_annuity_percent_json(rate_percent: f64, years: i64) -> PyResult<String> {
    let value = annuity_factor_percent(rate_percent, years);
    serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
}

#[pyfunction]
fn df_table_json(base_rate: f64, years: Vec<i64>) -> PyResult<String> {
    let value = discount_factor_table(base_rate, &years);
    serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
}

#[pyfunction]
fn df_table_range_json(
    rate_start: f64,
    rate_end: f64,
    rate_step: f64,
    years: i64,
) -> PyResult<String> {
    match discount_factor_table_range(rate_start, rate_end, rate_step, years) {
        Ok(value) => {
            serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
        }
        Err(message) => Ok(serde_json::json!({"error": message}).to_string()),
    }
}

#[pyfunction]
fn discounted_payback_json(
    initial_investment: f64,
    cash_flows: Vec<f64>,
    discount_rate: f64,
) -> PyResult<String> {
    let value = discounted_payback(initial_investment, &cash_flows, discount_rate);
    serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
}

#[pyfunction]
fn arr_calculate_json(
    initial_investment: f64,
    residual_value: f64,
    total_profits: f64,
    years: i64,
) -> PyResult<String> {
    match accounting_rate_of_return(initial_investment, residual_value, total_profits, years) {
        Ok(value) => {
            serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
        }
        Err(message) => Ok(serde_json::json!({"error": message}).to_string()),
    }
}

#[pyfunction]
fn arr_from_profits_json(
    annual_profits: Vec<f64>,
    initial_investment: f64,
    residual_value: f64,
) -> PyResult<String> {
    match arr_from_annual_profits(&annual_profits, initial_investment, residual_value) {
        Ok(value) => {
            serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
        }
        Err(message) => Ok(serde_json::json!({"error": message}).to_string()),
    }
}

/// `/compare`: projects arrive as (name, initial, residual, profits) tuples.
#[pyfunction]
fn arr_compare_json(projects: Vec<(String, f64, f64, Vec<f64>)>) -> PyResult<String> {
    let value = vimbai_capital_valuation::capital_returns::arr_comparison(&projects);
    serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
}

#[pyfunction]
fn arr_decision_json(arr: f64, target_return: f64) -> PyResult<String> {
    let value = arr_decision(arr, target_return);
    serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
}

#[pyfunction]
fn arr_average_profit_json(total_profits: f64, years: i64) -> PyResult<String> {
    match average_profit(total_profits, years) {
        Ok(value) => {
            serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
        }
        Err(message) => Ok(serde_json::json!({"error": message}).to_string()),
    }
}

#[pyfunction]
fn arr_average_capital_json(initial_investment: f64, residual_value: f64) -> PyResult<String> {
    let value = average_capital(initial_investment, residual_value);
    serde_json::to_string(&value).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// `analyze_currency_hedge` with the service's own request and response bodies.
#[pyfunction]
fn fx_currency_hedging_json(payload: &str) -> PyResult<String> {
    let request: CurrencyHedgingRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let response = analyze_currency_hedge(&request);
    serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// `value_cross_currency_swaps` with the service's own request and response bodies.
#[pyfunction]
fn fx_cross_currency_swap_json(payload: &str) -> PyResult<String> {
    let request: CrossCurrencySwapRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let response = value_cross_currency_swaps(&request);
    serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// `value_swaps` with the service's own request and response bodies.
#[pyfunction]
fn fx_swap_valuation_json(payload: &str) -> PyResult<String> {
    let request: SwapValuationRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let response = value_swaps(&request);
    serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// `analyze_futures_hedge` with the service's own request and response bodies.
#[pyfunction]
fn fx_futures_hedging_json(payload: &str) -> PyResult<String> {
    let request: FuturesHedgingRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let response = analyze_futures_hedge(&request);
    serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// `analyze_commodity_hedge` with the service's own request and response bodies.
#[pyfunction]
fn fx_commodity_hedging_json(payload: &str) -> PyResult<String> {
    let request: CommodityHedgingRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let response = analyze_commodity_hedge(&request);
    serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// `analyze_interest_rate_hedge` with the service's own request and response bodies.
#[pyfunction]
fn fx_interest_rate_hedging_json(payload: &str) -> PyResult<String> {
    let request: InterestRateHedgingRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let response = analyze_interest_rate_hedge(&request);
    serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// `translate_financials` with the service's own request and response bodies.
#[pyfunction]
fn fx_translation_json(payload: &str) -> PyResult<String> {
    let request: TranslationRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let response = translate_financials(&request);
    serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// forward-contracts-service `/analyze`.
#[pyfunction]
fn fx_forward_contracts_json(payload: &str) -> PyResult<String> {
    let request: ForwardContractRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let response = analyze_forward_contracts(&request);
    serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// multi-currency-service `/convert`. The timestamp is supplied by the caller
/// so the seam stays pure.
#[pyfunction]
#[pyo3(signature = (payload, converted_at = ""))]
fn fx_multi_currency_convert_json(payload: &str, converted_at: &str) -> PyResult<String> {
    let request: CurrencyConversionRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let response = convert_fixed(&request, converted_at);
    serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// multi-currency-service `/transaction`.
#[pyfunction]
fn fx_multi_currency_transaction_json(payload: &str) -> PyResult<String> {
    let request: TransactionRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let response = process_transaction(&request);
    serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// multi-currency-service `/hedge`; the service mints the id, so it is passed in.
#[pyfunction]
#[pyo3(signature = (payload, hedge_id = ""))]
fn fx_multi_currency_hedge_json(payload: &str, hedge_id: &str) -> PyResult<String> {
    let request: HedgeRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let response = create_hedge(&request, hedge_id);
    serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// credit-derivatives-service `/analyze`.
#[pyfunction]
fn fx_credit_derivatives_json(payload: &str) -> PyResult<String> {
    let request: CreditDerivativesRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let response = analyze_credit_derivatives(&request);
    serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// exotic-derivatives-service `/price`. Degenerate inputs come back as an
/// error object, matching the shape the service returns for a bad request.
#[pyfunction]
fn fx_exotic_derivatives_json(payload: &str) -> PyResult<String> {
    let request: DerivativeRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    match price_derivative(&request) {
        Ok(response) => {
            serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
        }
        Err(err) => Ok(serde_json::json!({ "error": err.to_string() }).to_string()),
    }
}

fn json_out<T: serde::Serialize>(value: &T) -> PyResult<String> {
    serde_json::to_string(value).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// `/analyze` on the comparative statements service. Three to ten periods.
#[pyfunction]
fn comparative_analyze_json(payload: &str) -> PyResult<String> {
    let req: ComparativeRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    if !(3..=10).contains(&req.number_of_periods) {
        return Err(seam_error(SeamError(
            "number_of_periods must be between 3 and 10".to_string(),
        )));
    }
    json_out(&comparative_analyze(&req))
}

/// `/horizontal-analysis`: one item, current against prior.
#[pyfunction]
fn comparative_horizontal_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        current_value: f64,
        prior_value: f64,
        item_name: String,
    }
    let body: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&comparative_horizontal(
        body.current_value,
        body.prior_value,
        &body.item_name,
    ))
}

/// `/trend-analysis`: a series indexed to its first period. Fewer than two
/// periods is a 400.
#[pyfunction]
fn comparative_trend_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        values: Vec<f64>,
        periods: Vec<String>,
    }
    let body: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let series: TrendSeries = trend_series(&body.values, &body.periods)
        .ok_or_else(|| seam_error(SeamError("At least 2 periods required".to_string())))?;
    json_out(&series)
}

/// `/variance-analysis`: budget against actual, item by item.
#[pyfunction]
fn comparative_variance_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        budgeted: Vec<f64>,
        actual: Vec<f64>,
        items: Vec<String>,
    }
    let body: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let report: VarianceReport = comparative_variance(&body.budgeted, &body.actual, &body.items);
    json_out(&report)
}

/// `/analyze` on the capital budgeting service. The service mints a uuid per
/// request only through its response model, so nothing is passed in here.
#[pyfunction]
fn capital_budgeting_json(payload: &str) -> PyResult<String> {
    let request: CapitalBudgetRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&analyse_capital_budget(&request))
}

/// `/forecast` on the financial forecasting service: history in, projection out.
#[pyfunction]
fn financial_forecast_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        #[serde(flatten)]
        request: MetricForecastRequest,
        report_id: String,
    }
    let body: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&forecast_metric(&body.request, &body.report_id))
}

/// `/analyze` on the labour cost variance service. The service mints a uuid for
/// each result, so the id is an input here.
#[pyfunction]
fn labour_variance_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        #[serde(flatten)]
        request: LabourVarianceRequest,
        report_id: String,
    }
    let body: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&analyse_labour_variance(&body.request, &body.report_id))
}

/// `/calculate` on the material cost variance service: four figures in, the
/// total and its price and usage split out.
#[pyfunction]
fn material_cost_variance_json(payload: &str) -> PyResult<String> {
    let input: MaterialInput =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&material_cost_variance(&input))
}

/// `/simple` on the same service: two costs, no split.
#[pyfunction]
fn simple_material_variance_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        standard_cost: f64,
        actual_cost: f64,
    }
    let body: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&simple_material_variance(
        body.standard_cost,
        body.actual_cost,
    ))
}

/// `/multi-material`: a bill of materials in, one line per material out.
#[pyfunction]
fn multi_material_variance_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        materials: Vec<MaterialInput>,
    }
    let body: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&multi_material_variance(&body.materials))
}

/// `/generate` on the regulatory reporting service. The return id and the
/// submission deadline are inputs; the service draws one from a UUID generator
/// and counts thirty days from the clock.
#[pyfunction]
fn regulatory_report_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        #[serde(flatten)]
        request: ReportRequest,
        report_id: String,
        submission_deadline: String,
    }
    let body: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&generate_report(
        &body.request,
        &body.report_id,
        &body.submission_deadline,
    ))
}

/// `/generate` on the management reporting service. The report date is passed
/// in rather than read from the clock, so a report can be reproduced.
#[pyfunction]
fn management_report_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        #[serde(flatten)]
        request: ManagementReportingRequest,
        report_date: String,
    }
    let body: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&generate_management_report(
        &body.request,
        &body.report_date,
    ))
}

/// `/generate` on the financial reporting service, same date convention.
#[pyfunction]
fn financial_report_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        #[serde(flatten)]
        request: FinancialReportingRequest,
        report_date: String,
    }
    let body: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&generate_financial_report(&body.request, &body.report_date))
}

/// `/prepare` on the annual report service.
#[pyfunction]
fn annual_report_json(payload: &str) -> PyResult<String> {
    let req: AnnualReportRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&prepare_annual_report(&req))
}

/// `/prepare` on the interim financial reporting service.
#[pyfunction]
fn interim_report_json(payload: &str) -> PyResult<String> {
    let req: InterimReportRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    if !(1..=4).contains(&req.period.period_number) {
        return Err(seam_error(SeamError(
            "period_number must be between 1 and 4".to_string(),
        )));
    }
    json_out(&interim_prepare(&req))
}

/// `/year-to-date` on the interim financial reporting service.
#[pyfunction]
fn interim_ytd_json(payload: &str) -> PyResult<String> {
    let period: InterimPeriod =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&interim_ytd(&period))
}

/// `/effective-tax-rate` on the interim financial reporting service.
#[pyfunction]
fn interim_tax_json(payload: &str) -> PyResult<String> {
    let period: InterimPeriod =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&interim_tax_rate(&period))
}

/// `/report` on the segment reporting service.
#[pyfunction]
fn segment_report_json(payload: &str) -> PyResult<String> {
    let req: SegmentReportRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&report_segments(&req))
}

/// The four working-capital endpoints. `/optimize` answers a zero denominator
/// with a 400 whose detail names the field, so it is raised as one.
#[pyfunction]
fn wc_analyze_json(payload: &str) -> PyResult<String> {
    let req: WcAnalysisInput =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&analyze_working_capital(&req))
}

#[pyfunction]
fn wc_optimize_json(payload: &str) -> PyResult<String> {
    let req: WcOptimizeInput =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    match optimize_working_capital(&req) {
        Ok(out) => json_out(&out),
        Err(err) => Err(seam_error(SeamError(format!(
            "{} must be greater than zero",
            err.field()
        )))),
    }
}

#[pyfunction]
fn wc_finance_json(payload: &str) -> PyResult<String> {
    let req: WcFinanceInput =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&analyze_wc_finance(&req))
}

#[pyfunction]
fn wc_factoring_json(payload: &str) -> PyResult<String> {
    let req: FactoringInput =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&calculate_factoring(&req))
}

/// `/forecast` on the cash-flow forecasting service. `forecast_date` is the
/// caller's clock reading, stamped by the service rather than the maths.
#[pyfunction]
#[pyo3(signature = (payload, forecast_date = ""))]
fn cash_flow_forecast_json(payload: &str, forecast_date: &str) -> PyResult<String> {
    let req: CashFlowForecastRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let out = forecast_cash_flow(&req);
    let mut value = serde_json::to_value(&out).map_err(|e| seam_error(SeamError::from(e)))?;
    if let Some(map) = value.as_object_mut() {
        map.insert(
            "forecast_date".to_string(),
            serde_json::Value::String(forecast_date.to_string()),
        );
        // The rollups name their period key differently.
        for (key, label) in [("weekly_summary", "week"), ("monthly_summary", "month")] {
            if let Some(rows) = map.get_mut(key).and_then(|v| v.as_array_mut()) {
                for row in rows {
                    if let Some(obj) = row.as_object_mut() {
                        if let Some(index) = obj.remove("index") {
                            obj.insert(label.to_string(), index);
                        }
                    }
                }
            }
        }
    }
    serde_json::to_string(&value).map_err(|e| seam_error(SeamError::from(e)))
}

#[pyfunction]
fn liquidity_forecast_json(payload: &str) -> PyResult<String> {
    let req: LiquidityForecastRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&forecast_liquidity(&req))
}

#[pyfunction]
fn liquidity_management_json(payload: &str) -> PyResult<String> {
    let req: LiquidityRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&analyze_liquidity(&req))
}

#[pyfunction]
fn liquidity_risk_json(payload: &str) -> PyResult<String> {
    let req: LiquidityAnalysisRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&analyze_liquidity_risk(&req))
}

// ---------------------------------------------------------------------------
// Cost accounting seam
//
// Absorption, marginal and target costing. Every function below is a plain
// adapter: parse the payload, call the safe library function, render the result.
// ---------------------------------------------------------------------------

/// `/product-costs/calculate` on the absorption costing service: the product
/// cost card, prime cost, total production cost and cost per unit.
#[pyfunction]
fn absorption_product_cost_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        #[serde(default)]
        product_id: String,
        #[serde(default)]
        product_name: String,
        #[serde(default)]
        period: String,
        direct_materials: f64,
        direct_labor: f64,
        #[serde(default)]
        direct_expenses: f64,
        #[serde(default)]
        manufacturing_overhead: f64,
        #[serde(default)]
        units_produced: f64,
        #[serde(default)]
        opening_stock: f64,
        #[serde(default)]
        closing_stock: f64,
    }
    let body: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&product_cost(
        &body.product_id,
        &body.product_name,
        &body.period,
        body.direct_materials,
        body.direct_labor,
        body.direct_expenses,
        body.manufacturing_overhead,
        body.units_produced,
        body.opening_stock,
        body.closing_stock,
    ))
}

/// `/overhead/absorption` on the absorption costing service: the absorption
/// rate and the overhead it absorbs over the base.
#[pyfunction]
fn absorption_overhead_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        #[serde(default)]
        product_id: String,
        #[serde(default)]
        period: String,
        overhead_cost: f64,
        #[serde(default)]
        absorption_base: String,
        absorption_base_units: f64,
    }
    let body: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&overhead_absorption(
        &body.product_id,
        &body.period,
        body.overhead_cost,
        &body.absorption_base,
        body.absorption_base_units,
    ))
}

/// `/cost-plus` on the absorption costing service: markup and selling price.
#[pyfunction]
fn absorption_cost_plus_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        product_cost: f64,
        markup_percentage: f64,
    }
    let body: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&cost_plus_pricing(
        body.product_cost,
        body.markup_percentage,
    ))
}

/// Turn decimal strings back into JSON numbers for the legacy float contract.
///
/// Used only where a Python service already publishes f64 and its clients read it
/// that way. The library itself keeps the digits.
pub(crate) fn floats_from_strings(value: &serde_json::Value) -> serde_json::Value {
    floats_from_strings_except(value, &[])
}

/// The same conversion, leaving named fields as text. A year such as "2026" parses as
/// a number, so a service that publishes it as a string keeps it a string.
pub(crate) fn floats_from_strings_except(
    value: &serde_json::Value,
    keep_text: &[&str],
) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => text
            .parse::<f64>()
            .map(|n| serde_json::Value::from(n))
            .unwrap_or_else(|_| value.clone()),
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|item| floats_from_strings_except(item, keep_text))
                .collect(),
        ),
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| {
                    if keep_text.contains(&k.as_str()) {
                        (k.clone(), v.clone())
                    } else {
                        (k.clone(), floats_from_strings_except(v, keep_text))
                    }
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// `/calculate` on the target costing service. `component_costs` is sent as
/// `[[name, cost], ...]` pairs so the component analysis keeps the caller's
/// insertion order, which the Python dict relied on.
#[pyfunction]
fn target_cost_json(payload: &str) -> PyResult<String> {
    let request: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let exact = vimbai_cost_accounting::target_cost::evaluate(&request)
        .map_err(PyValueError::new_err)?;
    // The library keeps amounts as decimal text; this binding is the legacy float
    // contract the Python service already publishes, so the loss happens here and
    // nowhere earlier.
    Ok(floats_from_strings(&exact).to_string())
}

/// `/costs/add` on the marginal costing service: a registered marginal cost item.
#[pyfunction]
fn marginal_cost_item_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        cost_name: String,
        #[serde(default)]
        cost_code: String,
        variable_cost: f64,
        #[serde(default)]
        fixed_cost: f64,
        #[serde(default)]
        units: f64,
        #[serde(default)]
        cost_driver: String,
    }
    let body: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&marginal_cost_item(
        &body.cost_name,
        &body.cost_code,
        body.variable_cost,
        body.fixed_cost,
        body.units,
        &body.cost_driver,
    ))
}

/// `/income-statement/generate` on the marginal costing service.
#[pyfunction]
fn marginal_income_statement_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        #[serde(default)]
        company_id: String,
        #[serde(default)]
        period: String,
        sales_revenue: f64,
        #[serde(default)]
        variable_costs: std::collections::BTreeMap<String, f64>,
        #[serde(default)]
        fixed_costs: f64,
    }
    let body: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&marginal_income_statement(
        &body.company_id,
        &body.period,
        body.sales_revenue,
        &body.variable_costs,
        body.fixed_costs,
    ))
}

/// `/contribution/analyze` on the marginal costing service.
#[pyfunction]
fn marginal_contribution_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        #[serde(default)]
        product_id: String,
        selling_price: f64,
        variable_cost_per_unit: f64,
        #[serde(default)]
        units_sold: f64,
        #[serde(default)]
        fixed_costs: f64,
    }
    let body: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&contribution_analysis(
        &body.product_id,
        body.selling_price,
        body.variable_cost_per_unit,
        body.units_sold,
        body.fixed_costs,
    ))
}

/// `/profit-forecast` on the marginal costing service.
#[pyfunction]
fn marginal_forecast_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        selling_price: f64,
        variable_cost_per_unit: f64,
        #[serde(default)]
        fixed_costs: f64,
        #[serde(default)]
        expected_units: f64,
    }
    let body: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&forecast_profit(
        body.selling_price,
        body.variable_cost_per_unit,
        body.fixed_costs,
        body.expected_units,
    ))
}

#[pyfunction]
fn prime_cost_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::prime_cost_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn prime_cost_from_items_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::prime_cost_from_items_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn throughput_accounting_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::throughput_accounting_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn limiting_factor_analyze_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::limiting_factor_analyze_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn limiting_factor_compare_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::limiting_factor_compare_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn order_acceptance_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::order_acceptance_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn one_time_order_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::one_time_order_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn standard_material_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::standard_material_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn standard_labour_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::standard_labour_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn standard_overhead_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::standard_overhead_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn standard_total_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::standard_total_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn standard_per_unit_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::standard_per_unit_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn standard_variance_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::standard_variance_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn abc_calculate_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::abc_calculate_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn budget_create_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::budget_create_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn budget_flexible_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::budget_flexible_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn budget_total_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::budget_total_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn budget_compare_output_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::budget_compare_output_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn budget_breakdown_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::budget_breakdown_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn actual_material_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::actual_material_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn actual_labour_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::actual_labour_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn actual_total_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::actual_total_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn actual_compare_budget_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::actual_compare_budget_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn actual_variance_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::actual_variance_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn job_add_cost_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::job_add_cost_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn production_total_cost_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::production_total_cost_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

/// `/apportion` on overhead-apportionment-service. One overhead, one period.
#[pyfunction]
fn overhead_apportion_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::overhead_apportion_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

/// `/batch-apportion` on overhead-apportionment-service. Every overhead for a period.
#[pyfunction]
fn overhead_batch_apportion_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        period: String,
        #[serde(default)]
        overheads: Vec<BatchOverheadRaw>,
        #[serde(default)]
        cost_centre_data: Vec<serde_json::Value>,
    }
    #[derive(serde::Deserialize)]
    struct BatchOverheadRaw {
        overhead_id: String,
        overhead_name: String,
        total_amount: f64,
        basis: String,
    }
    let b: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let overheads: Vec<ApportionOverheadItem> = b
        .overheads
        .into_iter()
        .map(|o| ApportionOverheadItem {
            overhead_id: o.overhead_id,
            overhead_name: o.overhead_name,
            total_amount: o.total_amount,
            basis: o.basis,
        })
        .collect();
    let centres: Vec<CostCentreDatum> = b
        .cost_centre_data
        .into_iter()
        .filter_map(|raw| serde_json::from_value::<CostCentreDatumRaw2>(raw).ok())
        .map(|c| CostCentreDatum {
            cost_centre_id: c.cost_centre_id,
            cost_centre_name: c.cost_centre_name,
            basis_values: c
                .basis_values
                .into_iter()
                .filter_map(|(k, v)| v.as_f64().map(|n| (k, n)))
                .collect(),
        })
        .collect();
    json_out(&batch_apportion_overheads(&overheads, &b.period, &centres))
}

#[derive(serde::Deserialize)]
struct CostCentreDatumRaw2 {
    cost_centre_id: String,
    cost_centre_name: String,
    #[serde(flatten)]
    basis_values: std::collections::BTreeMap<String, serde_json::Value>,
}

/// `/summary/generate` on cost-centre-service.
#[pyfunction]
fn cost_centre_summary_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::cost_centre_summary_from_json(&value).map_err(PyValueError::new_err)?;
    json_out(&answer)
}

/// `/calculate` on margin-safety-service.
#[pyfunction]
fn margin_of_safety_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        current_output: f64,
        current_revenue: f64,
        break_even_output: f64,
        break_even_revenue: f64,
    }
    let b: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&margin_of_safety(
        b.current_output,
        b.current_revenue,
        b.break_even_output,
        b.break_even_revenue,
    ))
}

/// `/calculate-from-data` on margin-safety-service.
#[pyfunction]
fn margin_from_data_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        selling_price: f64,
        fixed_costs: f64,
        variable_cost_per_unit: f64,
        expected_sales_units: f64,
    }
    let b: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    json_out(&margin_from_basic_data(
        b.selling_price,
        b.fixed_costs,
        b.variable_cost_per_unit,
        b.expected_sales_units,
    ))
}

/// `/target-mos` on margin-safety-service. A target of 100% or more is
/// infinity, which JSON cannot carry, so it is reported as null.
#[pyfunction]
fn target_mos_json(payload: &str) -> PyResult<String> {
    #[derive(serde::Deserialize)]
    struct Body {
        break_even_revenue: f64,
        target_mos_percentage: f64,
    }
    let b: Body = serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let required = sales_for_target_mos(b.break_even_revenue, b.target_mos_percentage);
    let mut out = serde_json::json!({
        "break_even_revenue": b.break_even_revenue,
        "target_mos_percentage": b.target_mos_percentage,
        "required_sales_revenue": required,
    });
    if !required.is_finite() {
        out["required_sales_revenue"] = serde_json::Value::Null;
    }
    json_out(&out)
}

// ---- assurance, group, performance and risk seam ----
/// JSON in, JSON out: the library does the work.
#[pyfunction]
fn tax_credit_json(payload: &str) -> PyResult<String> {
    assurance::credit_json(payload).map_err(PyValueError::new_err)
}
/// JSON in, JSON out: the library does the work.
#[pyfunction]
fn group_tax_json(payload: &str) -> PyResult<String> {
    assurance::group_tax_json(payload).map_err(PyValueError::new_err)
}
/// JSON in, JSON out: the library does the work.
#[pyfunction]
fn vat_return_json(payload: &str) -> PyResult<String> {
    assurance::vat_return_json(payload).map_err(PyValueError::new_err)
}
/// JSON in, JSON out: the library does the work.
#[pyfunction]
fn tax_provision_json(payload: &str) -> PyResult<String> {
    assurance::tax_provision_json(payload).map_err(PyValueError::new_err)
}
/// JSON in, JSON out: the library does the work.
#[pyfunction]
fn tax_risk_json(payload: &str) -> PyResult<String> {
    assurance::tax_risk_json(payload).map_err(PyValueError::new_err)
}
/// JSON in, JSON out: the library does the work.
#[pyfunction]
fn grant_json(payload: &str) -> PyResult<String> {
    assurance::grant_json(payload).map_err(PyValueError::new_err)
}
/// JSON in, JSON out: the library does the work.
#[pyfunction]
fn partnership_allocation_json(payload: &str) -> PyResult<String> {
    assurance::partnership_allocation_json(payload).map_err(PyValueError::new_err)
}
/// JSON in, JSON out: the library does the work.
#[pyfunction]
fn partner_buyout_json(payload: &str) -> PyResult<String> {
    assurance::partner_buyout_json(payload).map_err(PyValueError::new_err)
}
/// JSON in, JSON out: the library does the work.
#[pyfunction]
fn partnership_goodwill_json(payload: &str) -> PyResult<String> {
    assurance::partnership_goodwill_json(payload).map_err(PyValueError::new_err)
}
/// JSON in, JSON out: the library does the work.
#[pyfunction]
fn acquisition_json(payload: &str) -> PyResult<String> {
    assurance::acquisition_json(payload).map_err(PyValueError::new_err)
}
/// JSON in, JSON out: the library does the work.
#[pyfunction]
fn goodwill_impairment_json(payload: &str) -> PyResult<String> {
    assurance::goodwill_impairment_json(payload).map_err(PyValueError::new_err)
}
/// JSON in, JSON out: the library does the work.
#[pyfunction]
fn pension_json(payload: &str) -> PyResult<String> {
    assurance::pension_json(payload).map_err(PyValueError::new_err)
}
/// JSON in, JSON out: the library does the work.
#[pyfunction]
fn consolidation_json(payload: &str) -> PyResult<String> {
    assurance::consolidation_json(payload).map_err(PyValueError::new_err)
}
/// XBRL needs the generation timestamp, so the caller supplies it.
#[pyfunction]
fn xbrl_json(payload: &str, generated_at: &str) -> PyResult<String> {
    assurance::xbrl_json(payload, generated_at).map_err(PyValueError::new_err)
}

// ---- performance, market and credit risk seam ----
#[pyfunction]
fn payment_optimization_json(payload: &str) -> PyResult<String> {
    assurance::payment_optimization_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn substantive_testing_json(payload: &str) -> PyResult<String> {
    assurance::substantive_testing_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn divestiture_json(payload: &str) -> PyResult<String> {
    assurance::divestiture_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn capex_budget_json(payload: &str) -> PyResult<String> {
    assurance::capex_budget_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn control_evaluation_json(payload: &str) -> PyResult<String> {
    assurance::control_evaluation_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn scorecard_json(payload: &str) -> PyResult<String> {
    assurance::scorecard_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn controls_test_json(payload: &str) -> PyResult<String> {
    assurance::controls_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn profit_json(payload: &str) -> PyResult<String> {
    assurance::profit_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn inventory_valuation_json(payload: &str) -> PyResult<String> {
    assurance::inventory_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn nrv_inventory_json(payload: &str) -> PyResult<String> {
    assurance::nrv_inventory_json(payload).map_err(PyValueError::new_err)
}

#[pyfunction]
fn nrv_compare_json(payload: &str) -> PyResult<String> {
    assurance::nrv_compare_json(payload).map_err(PyValueError::new_err)
}

#[pyfunction]
fn nrv_json(payload: &str) -> PyResult<String> {
    assurance::nrv_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn scrap_value_json(payload: &str) -> PyResult<String> {
    assurance::scrap_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn residual_value_json(payload: &str) -> PyResult<String> {
    assurance::residual_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn fx_hedge_json(payload: &str) -> PyResult<String> {
    assurance::fx_hedge_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn options_price_json(payload: &str) -> PyResult<String> {
    assurance::options_price_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn var_json(payload: &str) -> PyResult<String> {
    assurance::var_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn credit_risk_json(payload: &str) -> PyResult<String> {
    assurance::credit_risk_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn pd_historical_json(payload: &str) -> PyResult<String> {
    assurance::pd_historical_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn fraud_risk_json(payload: &str) -> PyResult<String> {
    assurance::fraud_risk_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn distress_json(payload: &str) -> PyResult<String> {
    assurance::distress_json(payload).map_err(PyValueError::new_err)
}
#[pyfunction]
fn regulatory_ratio_json(payload: &str) -> PyResult<String> {
    assurance::regulatory_ratio_json(payload).map_err(PyValueError::new_err)
}
/// FX conversion falls back to the caller's date when the payload has none.
#[pyfunction]
fn fx_convert_json(payload: &str, today: &str) -> PyResult<String> {
    assurance::fx_convert_json(payload, today).map_err(PyValueError::new_err)
}


/// cost-of-capital-service, answered by the library's own adapters.
#[pyfunction]
fn cost_of_capital_calculate_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::cost_of_capital_calculate_from_json(&value)
        .map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn cost_of_capital_from_percent_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::cost_of_capital_from_percent_from_json(&value)
        .map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn cost_of_capital_wacc_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::cost_of_capital_wacc_from_json(&value)
        .map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn cost_of_capital_simple_rate_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::cost_of_capital_simple_rate_from_json(&value)
        .map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn cost_of_capital_compound_rate_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::cost_of_capital_compound_rate_from_json(&value)
        .map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pyfunction]
fn cost_of_capital_list_json(payload: &str) -> PyResult<String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let answer = vimbai_cost_accounting::wire::cost_of_capital_list_from_json(&value)
        .map_err(PyValueError::new_err)?;
    json_out(&answer)
}

#[pymodule]
fn vimbai_native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(sum_amounts, m)?)?;
    m.add_function(wrap_pyfunction!(payment_optimization_json, m)?)?;
    m.add_function(wrap_pyfunction!(substantive_testing_json, m)?)?;
    m.add_function(wrap_pyfunction!(divestiture_json, m)?)?;
    m.add_function(wrap_pyfunction!(capex_budget_json, m)?)?;
    m.add_function(wrap_pyfunction!(control_evaluation_json, m)?)?;
    m.add_function(wrap_pyfunction!(scorecard_json, m)?)?;
    m.add_function(wrap_pyfunction!(controls_test_json, m)?)?;
    m.add_function(wrap_pyfunction!(profit_json, m)?)?;
    m.add_function(wrap_pyfunction!(inventory_valuation_json, m)?)?;
    m.add_function(wrap_pyfunction!(nrv_json, m)?)?;
    m.add_function(wrap_pyfunction!(cost_of_capital_calculate_json, m)?)?;
    m.add_function(wrap_pyfunction!(cost_of_capital_from_percent_json, m)?)?;
    m.add_function(wrap_pyfunction!(cost_of_capital_wacc_json, m)?)?;
    m.add_function(wrap_pyfunction!(cost_of_capital_simple_rate_json, m)?)?;
    m.add_function(wrap_pyfunction!(cost_of_capital_compound_rate_json, m)?)?;
    m.add_function(wrap_pyfunction!(cost_of_capital_list_json, m)?)?;

    m.add_function(wrap_pyfunction!(nrv_inventory_json, m)?)?;
    m.add_function(wrap_pyfunction!(nrv_compare_json, m)?)?;
    m.add_function(wrap_pyfunction!(scrap_value_json, m)?)?;
    m.add_function(wrap_pyfunction!(residual_value_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_hedge_json, m)?)?;
    m.add_function(wrap_pyfunction!(options_price_json, m)?)?;
    m.add_function(wrap_pyfunction!(var_json, m)?)?;
    m.add_function(wrap_pyfunction!(credit_risk_json, m)?)?;
    m.add_function(wrap_pyfunction!(pd_historical_json, m)?)?;
    m.add_function(wrap_pyfunction!(fraud_risk_json, m)?)?;
    m.add_function(wrap_pyfunction!(distress_json, m)?)?;
    m.add_function(wrap_pyfunction!(regulatory_ratio_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_convert_json, m)?)?;

    m.add_function(wrap_pyfunction!(tax_credit_json, m)?)?;
    m.add_function(wrap_pyfunction!(group_tax_json, m)?)?;
    m.add_function(wrap_pyfunction!(vat_return_json, m)?)?;
    m.add_function(wrap_pyfunction!(tax_provision_json, m)?)?;
    m.add_function(wrap_pyfunction!(tax_risk_json, m)?)?;
    m.add_function(wrap_pyfunction!(grant_json, m)?)?;
    m.add_function(wrap_pyfunction!(xbrl_json, m)?)?;
    m.add_function(wrap_pyfunction!(partnership_allocation_json, m)?)?;
    m.add_function(wrap_pyfunction!(partner_buyout_json, m)?)?;
    m.add_function(wrap_pyfunction!(partnership_goodwill_json, m)?)?;
    m.add_function(wrap_pyfunction!(acquisition_json, m)?)?;
    m.add_function(wrap_pyfunction!(goodwill_impairment_json, m)?)?;
    m.add_function(wrap_pyfunction!(pension_json, m)?)?;
    m.add_function(wrap_pyfunction!(consolidation_json, m)?)?;

    m.add_function(wrap_pyfunction!(allocate, m)?)?;
    m.add_function(wrap_pyfunction!(margin_of_safety_json, m)?)?;
    m.add_function(wrap_pyfunction!(margin_from_data_json, m)?)?;
    m.add_function(wrap_pyfunction!(target_mos_json, m)?)?;
    m.add_function(wrap_pyfunction!(cost_centre_summary_json, m)?)?;
    m.add_function(wrap_pyfunction!(overhead_apportion_json, m)?)?;
    m.add_function(wrap_pyfunction!(overhead_batch_apportion_json, m)?)?;
    m.add_function(wrap_pyfunction!(analyze_treasury_json, m)?)?;
    m.add_function(wrap_pyfunction!(corporate_tax_json, m)?)?;
    m.add_function(wrap_pyfunction!(tax_calculation_json, m)?)?;
    m.add_function(wrap_pyfunction!(fixed_assets_json, m)?)?;
    m.add_function(wrap_pyfunction!(depreciation_json, m)?)?;
    m.add_function(wrap_pyfunction!(depreciation_method_json, m)?)?;
    m.add_function(wrap_pyfunction!(amortization_json, m)?)?;
    m.add_function(wrap_pyfunction!(du_pont_json, m)?)?;
    m.add_function(wrap_pyfunction!(asset_turnover_json, m)?)?;
    m.add_function(wrap_pyfunction!(croic_json, m)?)?;
    m.add_function(wrap_pyfunction!(eva_json, m)?)?;
    m.add_function(wrap_pyfunction!(eps_json, m)?)?;
    m.add_function(wrap_pyfunction!(coc_calculate_json, m)?)?;
    m.add_function(wrap_pyfunction!(coc_from_percent_json, m)?)?;
    m.add_function(wrap_pyfunction!(coc_wacc_json, m)?)?;
    m.add_function(wrap_pyfunction!(coc_simple_rate_json, m)?)?;
    m.add_function(wrap_pyfunction!(coc_compound_rate_json, m)?)?;
    m.add_function(wrap_pyfunction!(df_calculate_json, m)?)?;
    m.add_function(wrap_pyfunction!(df_percent_json, m)?)?;
    m.add_function(wrap_pyfunction!(df_annuity_json, m)?)?;
    m.add_function(wrap_pyfunction!(df_annuity_percent_json, m)?)?;
    m.add_function(wrap_pyfunction!(df_table_json, m)?)?;
    m.add_function(wrap_pyfunction!(df_table_range_json, m)?)?;
    m.add_function(wrap_pyfunction!(discounted_payback_json, m)?)?;
    m.add_function(wrap_pyfunction!(arr_calculate_json, m)?)?;
    m.add_function(wrap_pyfunction!(arr_from_profits_json, m)?)?;
    m.add_function(wrap_pyfunction!(arr_compare_json, m)?)?;
    m.add_function(wrap_pyfunction!(arr_decision_json, m)?)?;
    m.add_function(wrap_pyfunction!(arr_average_profit_json, m)?)?;
    m.add_function(wrap_pyfunction!(arr_average_capital_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_currency_hedging_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_forward_contracts_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_credit_derivatives_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_exotic_derivatives_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_multi_currency_convert_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_multi_currency_transaction_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_multi_currency_hedge_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_cross_currency_swap_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_swap_valuation_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_futures_hedging_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_commodity_hedging_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_interest_rate_hedging_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_translation_json, m)?)?;

    m.add_function(wrap_pyfunction!(capital_budgeting_json, m)?)?;
    m.add_function(wrap_pyfunction!(financial_forecast_json, m)?)?;
    m.add_function(wrap_pyfunction!(absorption_product_cost_json, m)?)?;
    m.add_function(wrap_pyfunction!(absorption_overhead_json, m)?)?;
    m.add_function(wrap_pyfunction!(absorption_cost_plus_json, m)?)?;
    m.add_function(wrap_pyfunction!(target_cost_json, m)?)?;
    m.add_function(wrap_pyfunction!(marginal_cost_item_json, m)?)?;
    m.add_function(wrap_pyfunction!(marginal_income_statement_json, m)?)?;
    m.add_function(wrap_pyfunction!(marginal_contribution_json, m)?)?;
    m.add_function(wrap_pyfunction!(marginal_forecast_json, m)?)?;
    m.add_function(wrap_pyfunction!(labour_variance_json, m)?)?;
    m.add_function(wrap_pyfunction!(prime_cost_json, m)?)?;
    m.add_function(wrap_pyfunction!(prime_cost_from_items_json, m)?)?;
    m.add_function(wrap_pyfunction!(throughput_accounting_json, m)?)?;
    m.add_function(wrap_pyfunction!(limiting_factor_analyze_json, m)?)?;
    m.add_function(wrap_pyfunction!(limiting_factor_compare_json, m)?)?;
    m.add_function(wrap_pyfunction!(order_acceptance_json, m)?)?;
    m.add_function(wrap_pyfunction!(one_time_order_json, m)?)?;
    m.add_function(wrap_pyfunction!(standard_material_json, m)?)?;
    m.add_function(wrap_pyfunction!(standard_labour_json, m)?)?;
    m.add_function(wrap_pyfunction!(standard_overhead_json, m)?)?;
    m.add_function(wrap_pyfunction!(standard_total_json, m)?)?;
    m.add_function(wrap_pyfunction!(standard_per_unit_json, m)?)?;
    m.add_function(wrap_pyfunction!(standard_variance_json, m)?)?;
    m.add_function(wrap_pyfunction!(abc_calculate_json, m)?)?;
    m.add_function(wrap_pyfunction!(budget_create_json, m)?)?;
    m.add_function(wrap_pyfunction!(budget_flexible_json, m)?)?;
    m.add_function(wrap_pyfunction!(budget_total_json, m)?)?;
    m.add_function(wrap_pyfunction!(budget_compare_output_json, m)?)?;
    m.add_function(wrap_pyfunction!(budget_breakdown_json, m)?)?;
    m.add_function(wrap_pyfunction!(actual_material_json, m)?)?;
    m.add_function(wrap_pyfunction!(actual_labour_json, m)?)?;
    m.add_function(wrap_pyfunction!(actual_total_json, m)?)?;
    m.add_function(wrap_pyfunction!(actual_compare_budget_json, m)?)?;
    m.add_function(wrap_pyfunction!(actual_variance_json, m)?)?;
    m.add_function(wrap_pyfunction!(job_add_cost_json, m)?)?;
    m.add_function(wrap_pyfunction!(production_total_cost_json, m)?)?;
    m.add_function(wrap_pyfunction!(material_cost_variance_json, m)?)?;
    m.add_function(wrap_pyfunction!(simple_material_variance_json, m)?)?;
    m.add_function(wrap_pyfunction!(multi_material_variance_json, m)?)?;
    m.add_function(wrap_pyfunction!(regulatory_report_json, m)?)?;
    m.add_function(wrap_pyfunction!(management_report_json, m)?)?;
    m.add_function(wrap_pyfunction!(financial_report_json, m)?)?;
    m.add_function(wrap_pyfunction!(annual_report_json, m)?)?;
    m.add_function(wrap_pyfunction!(interim_report_json, m)?)?;
    m.add_function(wrap_pyfunction!(interim_ytd_json, m)?)?;
    m.add_function(wrap_pyfunction!(interim_tax_json, m)?)?;
    m.add_function(wrap_pyfunction!(comparative_analyze_json, m)?)?;
    m.add_function(wrap_pyfunction!(comparative_horizontal_json, m)?)?;
    m.add_function(wrap_pyfunction!(comparative_trend_json, m)?)?;
    m.add_function(wrap_pyfunction!(comparative_variance_json, m)?)?;
    m.add_function(wrap_pyfunction!(segment_report_json, m)?)?;
    m.add_function(wrap_pyfunction!(wc_analyze_json, m)?)?;
    m.add_function(wrap_pyfunction!(wc_optimize_json, m)?)?;
    m.add_function(wrap_pyfunction!(wc_finance_json, m)?)?;
    m.add_function(wrap_pyfunction!(wc_factoring_json, m)?)?;
    m.add_function(wrap_pyfunction!(cash_flow_forecast_json, m)?)?;
    m.add_function(wrap_pyfunction!(liquidity_forecast_json, m)?)?;
    m.add_function(wrap_pyfunction!(liquidity_management_json, m)?)?;
    m.add_function(wrap_pyfunction!(liquidity_risk_json, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summing_keeps_every_digit() {
        let total = sum_exactly(&[
            "0.1".to_string(),
            "0.2".to_string(),
            "123456789012345678901234567890.55".to_string(),
        ])
        .unwrap();
        assert_eq!(total, "123456789012345678901234567890.85");
    }

    #[test]
    fn allocation_never_loses_a_cent() {
        let parts = split_exactly("100.00", &["1".into(), "1".into(), "1".into()], 2).unwrap();
        assert_eq!(parts, vec!["33.34", "33.33", "33.33"]);
    }

    #[test]
    fn a_bad_amount_is_refused_rather_than_guessed() {
        assert!(sum_exactly(&["not money".to_string()]).is_err());
    }

    #[test]
    fn treasury_payload_round_trips_as_strings() {
        let payload = r#"{
            "company_id": "comp-1",
            "total_cash": "500000",
            "monthly_inflow": "200000",
            "monthly_outflow": "150000",
            "short_term_debt": "30000",
            "total_debt": "100000",
            "investments": "200000",
            "fx_exposure": "50000"
        }"#;
        let out: Value = serde_json::from_str(&analyze_exactly(payload).unwrap()).unwrap();
        // ratios carry their scale (100.0000000000), so compare values, not text
        assert_eq!(
            out["cash_adequacy_days"],
            Value::String("100.0000000000".into())
        );
        assert_eq!(
            out["debt_service_ratio"],
            Value::String("15.0000000000".into())
        );
        assert!(out["kpis"].as_array().unwrap().len() >= 5);
    }
}
