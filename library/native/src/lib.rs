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
use vimbai_common::{Money, MoneyError};
use vimbai_ratios_statements::{
    asset_turnover, croic, du_pont_analysis, eps, eva, CroicRequest, DuPontRequest, EpsRequest,
    EvaRequest, TurnoverRequest,
};
use vimbai_tax::statutory::{statutory_corporate_tax, CorporateTaxInput};
use vimbai_tax::{calculate_tax, TaxBracket, TaxType};
use vimbai_treasury_fx::analytics::{analyze_treasury, TreasuryAnalyticsRequest};

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
