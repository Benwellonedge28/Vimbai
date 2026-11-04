#![forbid(unsafe_code)]
//! HTTP surface for the Vimbai calculation library.
//!
//! Every calculator in this crate is a pure function of its JSON payload, and every
//! one of them runs the same code the Python seam binds to. The point of the service
//! is to let the Python services retire: once a calculator answers here, the FastAPI
//! service in front of it holds nothing but routing.
//!
//! Money is carried as a decimal string wherever a figure is an amount, so no amount
//! is rounded on its way through the wire.

use axum::{
    extract::{Path, RawQuery, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

pub type Calculator = fn(&Value) -> Result<Value, String>;

/// The registered calculators, by the name clients call them.
pub fn calculators() -> BTreeMap<&'static str, Calculator> {
    let mut map: BTreeMap<&'static str, Calculator> = BTreeMap::new();
    map.insert(
        "internal-control-evaluation",
        (|payload| Ok(advisory::evaluate_controls(payload))) as Calculator,
    );
    map.insert("target-costing", cost_accounting::target_cost::evaluate);
    map.insert("divestiture", group_accounting::divestiture::evaluate);
    map.insert("eps", ratios_statements::eps_from_json);
    map.insert("croic", ratios_statements::croic_from_json);
    map.insert("amortization", assets::amortization_from_json);
    map.insert("depreciation", assets::depreciation_batch_from_json);
    map.insert("depreciation-method", assets::depreciation_method_from_json);
    map.insert("fixed-assets", assets::analyze_fixed_assets_from_json);
    map.insert("goodwill", assets::impairment_test_from_json);
    map.insert(
        "inventory-valuation",
        performance_measures::inventory_from_json,
    );
    map.insert("eva", ratios_statements::eva_from_json);
    // the endpoint's own payload: parent figures, nested subsidiary accounts, and
    // the presentation the service published around the consolidated core
    map.insert(
        "consolidation",
        group_accounting::consolidation::consolidate_statement_from_json,
    );
    map.insert(
        "options-pricing",
        market_risk::options::pricing_statement_from_json,
    );
    // the cost-accounting endpoints, each named after the service and the path the
    // service published it on
    map.insert(
        "activity-based-costing-calculate",
        cost_accounting::wire::abc_calculate_from_json,
    );
    map.insert(
        "throughput-accounting-analyze",
        cost_accounting::wire::throughput_accounting_from_json,
    );
    map.insert(
        "standard-cost-material-standard",
        cost_accounting::wire::standard_material_from_json,
    );
    map.insert(
        "standard-cost-labour-standard",
        cost_accounting::wire::standard_labour_from_json,
    );
    map.insert(
        "standard-cost-overhead-standard",
        cost_accounting::wire::standard_overhead_from_json,
    );
    map.insert(
        "standard-cost-total-standard",
        cost_accounting::wire::standard_total_from_json,
    );
    map.insert(
        "standard-cost-per-unit",
        cost_accounting::wire::standard_per_unit_from_json,
    );
    map.insert(
        "standard-cost-variance-analysis",
        cost_accounting::wire::standard_variance_from_json,
    );
    map.insert(
        "actual-cost-material-actual",
        cost_accounting::wire::actual_material_from_json,
    );
    map.insert(
        "actual-cost-labour-actual",
        cost_accounting::wire::actual_labour_from_json,
    );
    map.insert(
        "actual-cost-total-actual",
        cost_accounting::wire::actual_total_from_json,
    );
    map.insert(
        "actual-cost-compare-to-budget",
        cost_accounting::wire::actual_compare_budget_from_json,
    );
    map.insert(
        "actual-cost-variance-breakdown",
        cost_accounting::wire::actual_variance_from_json,
    );
    map.insert(
        "budgeted-cost-create",
        cost_accounting::wire::budget_create_from_json,
    );
    map.insert(
        "budgeted-cost-flexible-budget",
        cost_accounting::wire::budget_flexible_from_json,
    );
    map.insert(
        "budgeted-cost-total-budget",
        cost_accounting::wire::budget_total_from_json,
    );
    map.insert(
        "budgeted-cost-compare-output",
        cost_accounting::wire::budget_compare_output_from_json,
    );
    map.insert(
        "budgeted-cost-cost-breakdown",
        cost_accounting::wire::budget_breakdown_from_json,
    );
    map.insert(
        "prime-cost-calculate",
        cost_accounting::wire::prime_cost_from_json,
    );
    map.insert(
        "prime-cost-calculate-with-items",
        cost_accounting::wire::prime_cost_from_items_from_json,
    );
    map.insert(
        "cost-centre-summary-generate",
        cost_accounting::wire::cost_centre_summary_from_json,
    );
    map.insert(
        "overhead-apportionment-apportion",
        cost_accounting::wire::overhead_apportion_from_json,
    );
    map.insert(
        "limiting-factor-analyze",
        cost_accounting::wire::limiting_factor_analyze_from_json,
    );
    map.insert(
        "limiting-factor-compare-products",
        cost_accounting::wire::limiting_factor_compare_from_json,
    );
    map.insert(
        "order-acceptance-analyze",
        cost_accounting::wire::order_acceptance_from_json,
    );
    map.insert(
        "consolidation-eliminations",
        group_accounting::consolidation_views::eliminations_from_json,
    );
    map.insert(
        "consolidation-nci-calculation",
        group_accounting::consolidation_views::nci_from_json,
    );
    map.insert(
        "consolidation-goodwill-calculation",
        group_accounting::consolidation_views::goodwill_calculation_from_json,
    );
    map.insert(
        "consolidation-currency-translation",
        group_accounting::consolidation_views::currency_translation_from_json,
    );
    map.insert(
        "consolidation-validate",
        group_accounting::consolidation_views::validate_from_json,
    );
    map.insert(
        "corporate-tax-calculate",
        tax::statutory::statutory_from_json,
    );
    map.insert(
        "corporate-tax-compute",
        tax::statutory::compute_tax_from_json,
    );
    map.insert(
        "regulatory-ratio-calculate",
        bridge::http::regulatory_ratio_from_json,
    );
    map.insert("tax-risk-assess", bridge::http::tax_risk_from_json);
    map.insert("group-tax-consolidate", bridge::http::group_tax_from_json);
    map.insert(
        "vat-reporting-prepare-return",
        bridge::http::vat_return_from_json,
    );
    map.insert("government-grants-recognize", bridge::http::grant_from_json);
    map.insert(
        "balanced-scorecard-score",
        bridge::http::scorecard_from_json,
    );
    map.insert(
        "financial-distress-prediction-predict",
        bridge::http::distress_from_json,
    );
    map.insert(
        "fraud-risk-assessment-assess",
        bridge::http::fraud_risk_from_json,
    );
    map.insert(
        "pension-accounting-calculate",
        bridge::http::pension_from_json,
    );
    map.insert(
        "partnership-accounting-allocate",
        bridge::http::partnership_allocation_from_json,
    );
    map.insert(
        "partnership-sale-goodwill",
        bridge::http::partnership_goodwill_from_json,
    );
    map.insert(
        "partnership-sale-retirement",
        bridge::http::partner_buyout_from_json,
    );
    map.insert(
        "cost-accounting-job-cost",
        cost_accounting::wire::cost_accounting_job_cost_from_json,
    );
    map.insert(
        "cost-accounting-standards",
        cost_accounting::wire::cost_accounting_standards_from_json,
    );
    map.insert(
        "cost-of-capital-calculate",
        cost_accounting::wire::cost_of_capital_calculate_from_json,
    );
    map.insert(
        "cost-of-capital-from-percent",
        cost_accounting::wire::cost_of_capital_from_percent_from_json,
    );
    map.insert(
        "cost-of-capital-wacc",
        cost_accounting::wire::cost_of_capital_wacc_from_json,
    );
    map.insert(
        "cost-of-capital-simple-rate",
        cost_accounting::wire::cost_of_capital_simple_rate_from_json,
    );
    map.insert(
        "cost-of-capital-compound-rate",
        cost_accounting::wire::cost_of_capital_compound_rate_from_json,
    );
    map.insert(
        "cost-of-capital-list",
        cost_accounting::wire::cost_of_capital_list_from_json,
    );
    map.insert(
        "order-acceptance-one-time-order",
        cost_accounting::wire::one_time_order_from_json,
    );

    map.insert("du-pont", ratios_statements::du_pont_from_json);
    map.insert(
        "asset-turnover",
        ratios_statements::asset_turnover_from_json,
    );
    map.insert(
        "capital-expenditure-budget",
        budgeting::capex::prepare_from_json,
    );
    map.insert(
        "substantive-testing",
        advisory::substantive_testing::evaluate,
    );
    map.insert(
        "payment-optimization",
        treasury_fx::payment_optimization::evaluate,
    );
    map.insert(
        "net-realizable-value",
        performance_measures::realizable::net_realizable_value_from_json,
    );
    map.insert(
        "nrv-scrap-value",
        performance_measures::realizable::scrap_value_from_json,
    );
    map.insert(
        "nrv-residual-value",
        performance_measures::realizable::residual_value_from_json,
    );
    map.insert(
        "nrv-inventory",
        performance_measures::realizable::inventory_nrv_from_json,
    );
    map.insert(
        "nrv-compare-cost",
        performance_measures::realizable::compare_cost_nrv_from_json,
    );
    map.insert(
        "acquisition",
        (|p| via_seam(bridge::assurance::acquisition_json, p)) as Calculator,
    );
    map.insert(
        "capex-budget",
        (|p| via_seam(bridge::assurance::capex_budget_json, p)) as Calculator,
    );
    map.insert(
        "control-evaluation",
        (|p| via_seam(bridge::assurance::control_evaluation_json, p)) as Calculator,
    );
    map.insert(
        "controls",
        (|p| via_seam(bridge::assurance::controls_json, p)) as Calculator,
    );
    map.insert(
        "credit",
        (|p| via_seam(bridge::assurance::credit_json, p)) as Calculator,
    );
    map.insert(
        "credit-risk",
        (|p| via_seam(bridge::assurance::credit_risk_json, p)) as Calculator,
    );
    map.insert(
        "distress",
        (|p| via_seam(bridge::assurance::distress_json, p)) as Calculator,
    );
    map.insert(
        "fraud-risk",
        (|p| via_seam(bridge::assurance::fraud_risk_json, p)) as Calculator,
    );
    map.insert(
        "fx-convert",
        (|p| via_seam_today(bridge::assurance::fx_convert_json, p)) as Calculator,
    );
    map.insert(
        "fx-hedge",
        (|p| via_seam(bridge::assurance::fx_hedge_json, p)) as Calculator,
    );
    map.insert(
        "goodwill-impairment",
        (|p| via_seam(bridge::assurance::goodwill_impairment_json, p)) as Calculator,
    );
    map.insert(
        "grant",
        (|p| via_seam(bridge::assurance::grant_json, p)) as Calculator,
    );
    map.insert(
        "group-tax",
        (|p| via_seam(bridge::assurance::group_tax_json, p)) as Calculator,
    );
    map.insert(
        "inventory",
        (|p| via_seam(bridge::assurance::inventory_json, p)) as Calculator,
    );
    map.insert(
        "nrv",
        (|p| via_seam(bridge::assurance::nrv_json, p)) as Calculator,
    );
    map.insert(
        "nrv-compare",
        (|p| via_seam(bridge::assurance::nrv_compare_json, p)) as Calculator,
    );
    map.insert(
        "options-price",
        (|p| via_seam(bridge::assurance::options_price_json, p)) as Calculator,
    );
    map.insert(
        "partner-buyout",
        (|p| via_seam(bridge::assurance::partner_buyout_json, p)) as Calculator,
    );
    map.insert(
        "partnership-allocation",
        (|p| via_seam(bridge::assurance::partnership_allocation_json, p)) as Calculator,
    );
    map.insert(
        "partnership-goodwill",
        (|p| via_seam(bridge::assurance::partnership_goodwill_json, p)) as Calculator,
    );
    map.insert(
        "pd-historical",
        (|p| via_seam(bridge::assurance::pd_historical_json, p)) as Calculator,
    );
    map.insert(
        "pension",
        (|p| via_seam(bridge::assurance::pension_json, p)) as Calculator,
    );
    map.insert(
        "profit",
        (|p| via_seam(bridge::assurance::profit_json, p)) as Calculator,
    );
    map.insert(
        "regulatory-ratio",
        (|p| via_seam(bridge::assurance::regulatory_ratio_json, p)) as Calculator,
    );
    map.insert(
        "residual",
        (|p| via_seam(bridge::assurance::residual_json, p)) as Calculator,
    );
    map.insert(
        "scorecard",
        (|p| via_seam(bridge::assurance::scorecard_json, p)) as Calculator,
    );
    map.insert(
        "scrap",
        (|p| via_seam(bridge::assurance::scrap_json, p)) as Calculator,
    );
    map.insert(
        "tax-provision",
        (|p| via_seam(bridge::assurance::tax_provision_json, p)) as Calculator,
    );
    map.insert(
        "tax-risk",
        (|p| via_seam(bridge::assurance::tax_risk_json, p)) as Calculator,
    );
    map.insert(
        "var",
        (|p| via_seam(bridge::assurance::var_json, p)) as Calculator,
    );
    map.insert(
        "vat-return",
        (|p| via_seam(bridge::assurance::vat_return_json, p)) as Calculator,
    );
    map.insert(
        "xbrl",
        (|p| via_seam_today(bridge::assurance::xbrl_json, p)) as Calculator,
    );
    map.insert("margin-of-safety", (|p| margin_safety(p)) as Calculator);
    map.insert("arr-decision", (|p| arr_decision(p)) as Calculator);
    map.insert(
        "discounted-payback",
        (|p| discounted_payback(p)) as Calculator,
    );
    map.insert(
        "treasury-analytics",
        (|p| treasury_analytics(p)) as Calculator,
    );
    map
}

#[derive(Clone)]
pub struct AppState {
    pub calculators: Arc<BTreeMap<&'static str, Calculator>>,
}

pub fn router() -> Router {
    let state = AppState {
        calculators: Arc::new(calculators()),
    };
    Router::new()
        .route("/health", get(health))
        .route("/calculators", get(list))
        .route("/calc/:name", post(run))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "status": "healthy",
        "engine": "rust",
        "calculators": state.calculators.len(),
    }))
}

/// The request a calculator is given, from the body and the query string.
///
/// Most calculators take a JSON body, but the cost-accounting endpoints took their
/// arguments as query parameters (`POST /variance-analysis?standard_cost=900`), so a
/// client of those has no body to send. Rather than make the route a different
/// contract from the service it replaces, both are accepted: the body is the
/// request, and any query parameters fill in fields it does not mention.
fn merge_request(query: Option<&str>, body: Option<Value>) -> Value {
    let mut merged = match body {
        Some(Value::Object(map)) => map,
        Some(other) => return other,
        None => serde_json::Map::new(),
    };
    for (key, value) in url::form_urlencoded::parse(query.unwrap_or("").as_bytes()) {
        if merged.contains_key(key.as_ref()) {
            continue;
        }
        // query values arrive as text; give the calculator the number the service's
        // own type annotation would have given it, and plain text otherwise
        let filled = match value.parse::<f64>() {
            Ok(number) if value.trim() == number.to_string() => json!(number),
            _ => match value.as_ref() {
                "true" => json!(true),
                "false" => json!(false),
                _ => json!(value.as_ref()),
            },
        };
        merged.insert(key.into_owned(), filled);
    }
    Value::Object(merged)
}

async fn list(State(state): State<AppState>) -> impl IntoResponse {
    let names: Vec<&str> = state.calculators.keys().copied().collect();
    Json(json!({ "calculators": names }))
}

async fn run(
    State(state): State<AppState>,
    Path(name): Path<String>,
    RawQuery(query): RawQuery,
    body: Option<Json<Value>>,
) -> Response {
    let payload = merge_request(query.as_deref(), body.map(|Json(value)| value));
    match state.calculators.get(name.as_str()) {
        Some(calculator) => match calculator(&payload) {
            Ok(answer) => (StatusCode::OK, Json(answer)).into_response(),
            Err(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": message })),
            )
                .into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": format!("no calculator named {name}"),
                "calculators": state.calculators.keys().copied().collect::<Vec<_>>(),
            })),
        )
            .into_response(),
    }
}

/// Some calculations only ever had a Python entry point: they take a JSON string and
/// return one. These adapters put the same maths behind the HTTP calculator registry,
/// so the two front ends cannot drift apart.
fn via_seam(
    calculate: fn(&str) -> Result<String, String>,
    payload: &Value,
) -> Result<Value, String> {
    let rendered = calculate(&payload.to_string())?;
    serde_json::from_str(&rendered).map_err(|error| error.to_string())
}

fn via_seam_today(
    calculate: fn(&str, &str) -> Result<String, String>,
    payload: &Value,
) -> Result<Value, String> {
    let rendered = calculate(&payload.to_string(), &today())?;
    serde_json::from_str(&rendered).map_err(|error| error.to_string())
}

/// Today as an ISO date, for the seams that date their own defaults.
fn today() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let (year, month, day) = civil_from_days((seconds / 86_400) as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}
/// These four calculations take typed arguments rather than a JSON payload, so they had
/// no way to answer an HTTP request. Rather than reimplement them, these adapters read
/// the fields the Python service always sent and call the same maths.
fn number(payload: &Value, key: &str) -> Result<f64, String> {
    match payload.get(key) {
        Some(Value::Number(value)) => value
            .as_f64()
            .ok_or_else(|| format!("{key} is not a number")),
        Some(Value::String(text)) => text
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("{key} is not a number")),
        Some(other) => Err(format!("{key} is {other}, expected a number")),
        None => Err(format!("{key} is required")),
    }
}

fn number_list(payload: &Value, key: &str) -> Result<Vec<f64>, String> {
    let items = payload
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{key} must be a list of numbers"))?;
    items
        .iter()
        .map(|item| match item {
            Value::Number(value) => value
                .as_f64()
                .ok_or_else(|| format!("{key} holds a non-number")),
            Value::String(text) => text
                .trim()
                .parse::<f64>()
                .map_err(|_| format!("{key} holds a non-number")),
            _ => Err(format!("{key} holds a non-number")),
        })
        .collect()
}

fn to_value<T: serde::Serialize>(output: &T) -> Result<Value, String> {
    serde_json::to_value(output).map_err(|error| error.to_string())
}

fn margin_safety(payload: &Value) -> Result<Value, String> {
    let output = cost_accounting::margin_safety::margin_of_safety(
        number(payload, "current_output")?,
        number(payload, "current_revenue")?,
        number(payload, "break_even_output")?,
        number(payload, "break_even_revenue")?,
    );
    to_value(&output)
}

fn arr_decision(payload: &Value) -> Result<Value, String> {
    Ok(capital_valuation::capital_returns::arr_decision(
        number(payload, "arr")?,
        number(payload, "target_return")?,
    ))
}

fn discounted_payback(payload: &Value) -> Result<Value, String> {
    let flows = number_list(payload, "cash_flows")?;
    let output = capital_valuation::capital_returns::discounted_payback(
        number(payload, "initial_investment")?,
        &flows,
        number(payload, "discount_rate")?,
    );
    to_value(&output)
}

fn treasury_analytics(payload: &Value) -> Result<Value, String> {
    let request: treasury_fx::analytics::TreasuryAnalyticsRequest =
        serde_json::from_value(payload.clone())
            .map_err(|error| format!("invalid treasury analytics request: {error}"))?;
    to_value(&treasury_fx::analytics::analyze_treasury(&request))
}

#[cfg(test)]
mod typed_adapters {
    use super::*;
    use serde_json::json;

    fn calculate(name: &str, payload: Value) -> Value {
        let registry = calculators();
        let entry = registry
            .get(name)
            .unwrap_or_else(|| panic!("{name} is not registered"));
        entry(&payload).unwrap_or_else(|error| panic!("{name} failed: {error}"))
    }

    #[test]
    fn margin_of_safety_takes_the_service_field_names() {
        let out = calculate(
            "margin-of-safety",
            json!({
                "current_output": 1000.0,
                "current_revenue": 50000.0,
                "break_even_output": 800.0,
                "break_even_revenue": 40000.0
            }),
        );
        assert!(out.is_object());
        assert_eq!(out["margin_of_safety_units"], json!(200.0));
    }

    #[test]
    fn arr_decision_answers_with_a_verdict() {
        let out = calculate("arr-decision", json!({"arr": 18.0, "target_return": 12.0}));
        assert_eq!(out["decision"], json!("Accept"));
    }

    #[test]
    fn discounted_payback_walks_the_cash_flows() {
        let out = calculate(
            "discounted-payback",
            json!({
                "initial_investment": 1000.0,
                "cash_flows": [400.0, 400.0, 400.0],
                "discount_rate": 10.0
            }),
        );
        assert!(out.is_object());
    }

    #[test]
    fn treasury_analytics_parses_its_request() {
        let out = calculate(
            "treasury-analytics",
            json!({
                "company_id": "c-1",
                "total_cash": "250000",
                "monthly_inflow": "90000",
                "monthly_outflow": "60000",
                "short_term_debt": "40000",
                "total_debt": "120000",
                "investments": "50000",
                "fx_exposure": "10000"
            }),
        );
        assert!(out.is_object());
    }
}
