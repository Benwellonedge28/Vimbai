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
    extract::{Path, State},
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

async fn list(State(state): State<AppState>) -> impl IntoResponse {
    let names: Vec<&str> = state.calculators.keys().copied().collect();
    Json(json!({ "calculators": names }))
}

async fn run(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
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
