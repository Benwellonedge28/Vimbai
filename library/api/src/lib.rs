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
