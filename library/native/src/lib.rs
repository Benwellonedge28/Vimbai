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

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use serde_json::Value;

use vimbai_assets::{
    amortization, analyze_fixed_assets, depreciation_batch, depreciation_method, AmortizationAsset,
    DepreciationAsset, FixedAssetsRequest,
};
use vimbai_capital_valuation::capital_returns::{
    accounting_rate_of_return, annuity_factor, annuity_factor_percent, arr_decision,
    arr_from_annual_profits, average_capital, average_profit, compound_rate, cost_of_capital,
    cost_of_capital_percent, discount_factor, discount_factor_percent, discount_factor_table,
    discount_factor_table_range, discounted_payback, simple_rate, wacc,
};
use vimbai_common::{Money, MoneyError};
use vimbai_ratios_statements::{
    asset_turnover, croic, du_pont_analysis, eps, eva, CroicRequest, DuPontRequest, EpsRequest,
    EvaRequest, TurnoverRequest,
};
use vimbai_tax::statutory::{statutory_corporate_tax, CorporateTaxInput};
use vimbai_tax::{calculate_tax, TaxBracket, TaxType};
use vimbai_treasury_fx::analytics::{analyze_treasury, TreasuryAnalyticsRequest};
use vimbai_treasury_fx::derivatives::{
    value_cross_currency_swaps, value_swaps, CrossCurrencySwapRequest, SwapValuationRequest,
};
use vimbai_treasury_fx::fx_hedging::{analyze_currency_hedge, CurrencyHedgingRequest};
use vimbai_treasury_fx::fx_translation::{translate_financials, TranslationRequest};
use vimbai_treasury_fx::hedging_analytics::{
    analyze_commodity_hedge, analyze_futures_hedge, analyze_interest_rate_hedge,
    CommodityHedgingRequest, FuturesHedgingRequest, InterestRateHedgingRequest,
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
    let output = depreciation_method(method, cost, salvage_value, useful_life);
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
    let request: DuPontRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let response = du_pont_analysis(&request);
    serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// asset_turnover-service, body in and body out.
#[pyfunction]
fn asset_turnover_json(payload: &str) -> PyResult<String> {
    let request: TurnoverRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let response = asset_turnover(&request);
    serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// croic-service, body in and body out.
#[pyfunction]
fn croic_json(payload: &str) -> PyResult<String> {
    let request: CroicRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let response = croic(&request);
    serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// eva-service, body in and body out.
#[pyfunction]
fn eva_json(payload: &str) -> PyResult<String> {
    let request: EvaRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let response = eva(&request);
    serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
}

/// eps-service, body in and body out.
#[pyfunction]
fn eps_json(payload: &str) -> PyResult<String> {
    let request: EpsRequest =
        serde_json::from_str(payload).map_err(|e| seam_error(SeamError::from(e)))?;
    let response = eps(&request);
    serde_json::to_string(&response).map_err(|e| seam_error(SeamError(e.to_string())))
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

#[pymodule]
fn vimbai_native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(sum_amounts, m)?)?;
    m.add_function(wrap_pyfunction!(allocate, m)?)?;
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
    m.add_function(wrap_pyfunction!(fx_cross_currency_swap_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_swap_valuation_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_futures_hedging_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_commodity_hedging_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_interest_rate_hedging_json, m)?)?;
    m.add_function(wrap_pyfunction!(fx_translation_json, m)?)?;

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
