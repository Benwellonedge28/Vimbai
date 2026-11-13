//! The scoped request path: the endpoints that own records.
//!
//! Every other route in this crate is a pure calculation, so the caller is irrelevant to it. These
//! are not: they read and write a Book's records, which means the caller has to be known before a
//! statement runs. The gateway injects `X-User-Id` and `X-Book-ID`, exactly as it does for the
//! Python services these replace, so a client does not have to change to talk to them.
//!
//! Paths keep the service prefix (`/records/absorption-costing/product-costs`) so the gateway can
//! move one service at a time and both can answer during the changeover.

#![forbid(unsafe_code)]

use std::sync::Arc;

use accounting::{cash_flow, trading_account};
use axum::{
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use budgeting_services::{
    activity_based_budget as activity_budget, appropriation_control, master_budget,
    rolling_forecast, scenario_analysis, scenario_modeling, sensitivity_analysis,
    zero_based_budget,
};
use common::Money;
use costing::{
    absorption, basis_apportionment, cost_calculations, cvp, job_costing, overhead_absorption,
    price_reduction, statement,
};
use equity::equity_changes;
use finance::{
    continue_shutdown, insurance_claims, internal_rate_return, investment_monitoring,
    net_present_value, payback_period, personal_finance, personal_money, trade_finance,
};
use ledger;
use partnerships::{agreement, changes, dissolution, revaluation as partnership_revaluation};
use receivables;
use serde_json::{json, Value};
use store::amounts::{amount, amount_or_zero, wire};
use store::{BookScope, Graph, StoreError};
use treasury::{
    cash_management, cash_optimization, debt_management, foreign_exchange, petty_cash,
    sovereign_treasury, treasury_analytics, treasury_compliance, treasury_management,
    treasury_policy, treasury_reporting, treasury_risk,
};

/// A graph the API can use for the whole life of the process. The driver pools its own
/// connections, so one handle serves every request.
pub type GraphHandle = Arc<dyn Graph + Send + Sync>;

/// Where a request is allowed to read and write.
///
/// Both headers are required. A Book-less caller is not handled yet: the Python layer stored a
/// null book, and answering that case means a second statement shape in the store rather than
/// quietly treating "no Book" as a Book named nothing.
pub(crate) fn scope_of(headers: &HeaderMap) -> Result<BookScope, Box<Response>> {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    match (header("x-user-id"), header("x-book-id")) {
        (Some(user_id), Some(book_id)) => Ok(BookScope::new(user_id, book_id)),
        _ => Err(Box::new(
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "X-User-Id and X-Book-ID are both required"})),
            )
                .into_response(),
        )),
    }
}

fn fail(error: StoreError) -> Response {
    let status = match error {
        StoreError::NotFound => StatusCode::NOT_FOUND,
        StoreError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
        StoreError::Graph(_) => StatusCode::BAD_GATEWAY,
    };
    (status, Json(json!({ "error": error.to_string() }))).into_response()
}

pub(crate) fn answer(result: Result<Value, StoreError>) -> Response {
    match result {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(error) => fail(error),
    }
}

/// A required parameter the caller did not send.
fn refused(message: &str) -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({ "error": message })),
    )
        .into_response()
}

/// Every query parameter as an object of strings, for the services whose listings take several.
pub(crate) fn query_object(query: Option<&str>) -> Value {
    let mut object = serde_json::Map::new();
    for (key, value) in url::form_urlencoded::parse(query.unwrap_or("").as_bytes()) {
        object.insert(key.into_owned(), json!(value.into_owned()));
    }
    Value::Object(object)
}

pub(crate) fn query_param(query: Option<&str>, name: &str) -> Option<String> {
    url::form_urlencoded::parse(query.unwrap_or("").as_bytes())
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

/// The record routes when a database is configured, and nothing when it is not.
///
/// Connecting is lazy: the driver only opens a socket when the first statement runs, so a
/// deployment without records configured simply does not mount these routes rather than failing
/// to boot.
pub fn from_env() -> Option<GraphHandle> {
    match store::Neo4jGraph::from_env() {
        Ok(graph) => Some(Arc::new(graph) as GraphHandle),
        Err(error) => {
            eprintln!("record routes not mounted: {error}");
            None
        }
    }
}

pub(crate) type GraphState = State<GraphHandle>;

async fn product_costs(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let product_id = query_param(query.as_deref(), "product_id");
    let period = query_param(query.as_deref(), "period");
    answer(absorption::list_product_costs(
        graph.as_ref(),
        &scope,
        product_id.as_deref(),
        period.as_deref(),
    ))
}

async fn latest_product_cost(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(product_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(absorption::latest_product_cost(
        graph.as_ref(),
        &scope,
        &product_id,
    ))
}

async fn stock_valuation(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let Some(product_id) = query_param(query.as_deref(), "product_id") else {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "product_id is required"})),
        )
            .into_response();
    };
    let method = query_param(query.as_deref(), "valuation_method").unwrap_or_else(|| "fifo".into());
    answer(absorption::stock_valuation(
        graph.as_ref(),
        &scope,
        &product_id,
        &method,
    ))
}

async fn keep_product_cost(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(absorption::record_product_cost(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn open_job(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(job_costing::create_job(graph.as_ref(), &scope, &payload))
}

async fn company_jobs(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let status = query_param(query.as_deref(), "status");
    answer(job_costing::jobs(
        graph.as_ref(),
        &scope,
        &company_id,
        status.as_deref(),
    ))
}

async fn charge_cost(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(job_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(job_costing::add_cost(
        graph.as_ref(),
        &scope,
        &job_id,
        &payload,
    ))
}

async fn company_profitability(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(job_costing::profitability(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn cost_calculation(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cost_calculations::create_calculation(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn company_cost_calculations(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let product = query_param(query.as_deref(), "product");
    answer(cost_calculations::calculations(
        graph.as_ref(),
        &scope,
        &company_id,
        product.as_deref(),
    ))
}

async fn cost_breakdown(
    State(graph): GraphState,
    headers: HeaderMap,
    Path((company_id, calc_id)): Path<(String, String)>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cost_calculations::breakdown(
        graph.as_ref(),
        &scope,
        &company_id,
        &calc_id,
    ))
}

async fn cost_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cost_calculations::summary(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn price_analysis(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(price_reduction::create_analysis(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn price_discount_analysis(Json(payload): Json<Value>) -> Response {
    answer(price_reduction::discount_analysis(&payload))
}

async fn price_analyses(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let product = query_param(query.as_deref(), "product_id");
    answer(price_reduction::analyses(
        graph.as_ref(),
        &scope,
        product.as_deref(),
    ))
}

async fn cvp_analysis(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cvp::create_analysis(graph.as_ref(), &scope, &payload))
}

async fn cvp_quick_analysis(Json(payload): Json<Value>) -> Response {
    answer(cvp::quick_analysis(&payload))
}

async fn cvp_contribution(Json(payload): Json<Value>) -> Response {
    answer(cvp::contribution(&payload))
}

async fn cvp_target_profit(Json(payload): Json<Value>) -> Response {
    answer(cvp::target_profit(&payload))
}

async fn cvp_multi_product(Json(payload): Json<Value>) -> Response {
    answer(cvp::multi_product(&payload))
}

async fn cvp_analyses(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let entity = query_param(query.as_deref(), "entity_id");
    answer(cvp::analyses(graph.as_ref(), &scope, entity.as_deref()))
}

async fn cvp_analysis_by_id(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(analysis_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cvp::analysis(graph.as_ref(), &scope, &analysis_id))
}

async fn overhead_rate(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(overhead_absorption::create_rate(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn overhead_base_rate(
    state: (GraphState, usize),
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let (State(graph), index) = state;
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(overhead_absorption::base_rate(
        graph.as_ref(),
        &scope,
        index,
        &payload,
    ))
}

async fn overhead_blanket_rate(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(overhead_absorption::blanket_rate(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn overhead_absorbed(Json(payload): Json<Value>) -> Response {
    answer(overhead_absorption::absorbed_overhead(&payload))
}

async fn overhead_rates(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let centre = query_param(query.as_deref(), "cost_centre_id");
    answer(overhead_absorption::rates(
        graph.as_ref(),
        &scope,
        centre.as_deref(),
    ))
}

async fn overhead_analyses(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(overhead_absorption::analyses(graph.as_ref(), &scope))
}

async fn apportionment_rule(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(basis_apportionment::create_rule(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn apportionment_rules(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(basis_apportionment::rules(graph.as_ref(), &scope))
}

async fn apportionment_by_basis(
    state: (GraphState, usize),
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let (State(graph), index) = state;
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(basis_apportionment::apportion(
        graph.as_ref(),
        &scope,
        index,
        &payload,
    ))
}

async fn apportionment_results(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let cost_type = query_param(query.as_deref(), "cost_type");
    answer(basis_apportionment::results(
        graph.as_ref(),
        &scope,
        cost_type.as_deref(),
    ))
}

/// The same routes on the paths the two services publish, so the gateway can point at this
/// service without the client noticing. `strip_prefix` is off in the gateway, so a request to
/// `/absorption-costing/product-costs` arrives here unchanged.
///
/// Kept beside the `/records` mount rather than replacing it: one path names the shape (records),
/// the other names the service being replaced.
async fn file_claim(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(insurance_claims::file_claim(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn insurance_claims(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company_id = match query_param(query.as_deref(), "company_id") {
        Some(company_id) => company_id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "company_id is required"})),
            )
                .into_response()
        }
    };
    let status = query_param(query.as_deref(), "status");
    answer(
        insurance_claims::claims(graph.as_ref(), &scope, &company_id, status.as_deref())
            .map(|claims| json!(claims)),
    )
}

async fn process_insurance_claim(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(claim_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company_id = match query_param(query.as_deref(), "company_id") {
        Some(company_id) => company_id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "company_id is required"})),
            )
                .into_response()
        }
    };
    answer(insurance_claims::process_claim(
        graph.as_ref(),
        &scope,
        &company_id,
        &claim_id,
    ))
}

async fn analyze_treasury(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_analytics::analyze(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn treasury_kpi(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_analytics::kpi(graph.as_ref(), &scope, &company_id))
}

async fn generate_cash_flow_statement(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cash_flow::generate(graph.as_ref(), &scope, &payload))
}

async fn latest_cash_flow_statement(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cash_flow::latest(graph.as_ref(), &scope, &company_id))
}

async fn cash_flow_statement_history(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cash_flow::history(graph.as_ref(), &scope, &company_id))
}

async fn record_equity_transaction(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity_changes::create_transaction(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn company_equity_transactions(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let tx_type = query_param(query.as_deref(), "tx_type");
    answer(equity_changes::transactions(
        graph.as_ref(),
        &scope,
        &company_id,
        tx_type.as_deref(),
    ))
}

async fn generate_equity_statement(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity_changes::generate_statement(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn company_equity_statements(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity_changes::statements(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn open_treasury_policy(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_policy::create_policy(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn treasury_policies(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_policy::list_policies(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn set_policy_limit(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(policy_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_policy::set_limit(
        graph.as_ref(),
        &scope,
        &policy_id,
        &payload,
    ))
}

async fn policy_limits(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(policy_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_policy::list_limits(
        graph.as_ref(),
        &scope,
        &policy_id,
    ))
}

async fn check_policy_limit(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(limit_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_policy::check_compliance(
        graph.as_ref(),
        &scope,
        &limit_id,
        &payload,
    ))
}

async fn policy_compliance_checks(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_policy::list_compliance_checks(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn open_risk_exposure(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_risk::create_exposure(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn risk_exposures(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_risk::list_exposures(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn calculate_risk_var(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_risk::calculate_var(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn risk_var_results(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_risk::list_var_results(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn open_stress_scenario(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_risk::create_scenario(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn stress_scenarios(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_risk::list_scenarios(graph.as_ref(), &scope))
}

async fn run_stress(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(scenario_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_risk::run_stress_test(
        graph.as_ref(),
        &scope,
        &scenario_id,
        &payload,
    ))
}

async fn stress_results(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_risk::list_stress_results(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn risk_dashboard(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_risk::dashboard(graph.as_ref(), &scope))
}

async fn open_debt_loan(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(debt_management::create_loan(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn company_debt_loans(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let Some(company_id) = query_param(query.as_deref(), "company_id") else {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "company_id is required"})),
        )
            .into_response();
    };
    answer(debt_management::list_loans(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn debt_loan_schedule(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(loan_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let mut body = crate::merge_request(query.as_deref(), None);
    if let Some(object) = body.as_object_mut() {
        object.insert("loan_id".into(), json!(loan_id));
    }
    answer(debt_management::schedule(graph.as_ref(), &scope, &body))
}

async fn company_debt_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    let company_id = body
        .get("company_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    // equity narrows the ratio only; a book with none has no ratio rather than a division by nothing
    let equity = match body.get("equity") {
        Some(value) => match store::amounts::amount(&json!({ "amount": value }), "amount") {
            Ok(money) => money,
            Err(refusal) => {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({"error": refusal.to_string()})),
                )
                    .into_response()
            }
        },
        None => Money::zero(),
    };
    answer(debt_management::summary(
        graph.as_ref(),
        &scope,
        &company_id,
        &equity,
    ))
}

async fn company_compliance_checks(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_compliance::list_checks(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn set_compliance_verdict(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(check_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let mut body = payload;
    if let Some(object) = body.as_object_mut() {
        object.insert("check_id".into(), json!(check_id));
    }
    answer(treasury_compliance::update_status(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn company_compliance_report(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_compliance::report(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn open_sovereign_account(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(sovereign_treasury::create_account(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn sovereign_accounts(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(country): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(sovereign_treasury::list_accounts(
        graph.as_ref(),
        &scope,
        &country,
    ))
}

async fn register_sovereign_debt(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(sovereign_treasury::register_debt(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn sovereign_debts(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(country): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(sovereign_treasury::list_debts(
        graph.as_ref(),
        &scope,
        &country,
    ))
}

async fn set_sovereign_fiscal_position(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(sovereign_treasury::set_fiscal_position(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn sovereign_fiscal_position(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(country): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(sovereign_treasury::get_fiscal_position(
        graph.as_ref(),
        &scope,
        &country,
    ))
}

async fn generate_cash_position_report(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_reporting::cash_position(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn generate_fx_exposure_report(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_reporting::fx_exposure(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn generate_debt_portfolio_report(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_reporting::debt_portfolio(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn treasury_reports(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_reporting::list_reports(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn one_treasury_report(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(report_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_reporting::get_report(
        graph.as_ref(),
        &scope,
        &report_id,
    ))
}

async fn record_treasury_flow(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_management::record_cashflow(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn company_treasury_flows(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_management::list_cashflows(
        graph.as_ref(),
        &scope,
        &company_id,
        &query_object(query.as_deref()),
    ))
}

async fn company_cash_position(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_management::get_cash_position(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn set_company_cash_position(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_management::update_cash_position(
        graph.as_ref(),
        &scope,
        &company_id,
        &payload,
    ))
}

async fn company_cash_forecast(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(treasury_management::cash_forecast(
        graph.as_ref(),
        &scope,
        &company_id,
        &query_object(query.as_deref()),
    ))
}

async fn treasury_investment_options() -> Response {
    answer(Ok(treasury_management::investment_options()))
}

async fn open_cash_account(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cash_management::create_account(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn cash_accounts(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cash_management::list_accounts(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn move_cash(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cash_management::create_transfer(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn cash_transfers(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cash_management::list_transfers(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn measure_liquidity(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cash_management::calculate_liquidity(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn fx_rates() -> Response {
    answer(Ok(foreign_exchange::rates()))
}

async fn fx_rate(Path(currency): Path<String>) -> Response {
    answer(foreign_exchange::rate_for(&currency))
}

async fn fx_convert(Json(payload): Json<Value>) -> Response {
    answer(foreign_exchange::convert(&payload))
}

async fn fx_hedge(Json(payload): Json<Value>) -> Response {
    answer(foreign_exchange::hedge_exposure(&payload))
}

async fn open_petty_cash_fund(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::create_fund(graph.as_ref(), &scope, &payload))
}

async fn petty_cash_funds(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::list_funds(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn petty_cash_fund(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(fund_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::get_fund(graph.as_ref(), &scope, &fund_id))
}

async fn change_petty_cash_fund(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(fund_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::update_fund(
        graph.as_ref(),
        &scope,
        &fund_id,
        &payload,
    ))
}

async fn close_petty_cash_fund(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(fund_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::close_fund(
        graph.as_ref(),
        &scope,
        &fund_id,
        &payload,
    ))
}

async fn petty_cash_fund_balance(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(fund_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::fund_balance(graph.as_ref(), &scope, &fund_id))
}

async fn petty_cash_fund_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(fund_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::fund_summary(graph.as_ref(), &scope, &fund_id))
}

async fn add_petty_cash_transaction(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::create_transaction(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn petty_cash_transactions(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::list_transactions(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn petty_cash_transaction(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(transaction_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::get_transaction(
        graph.as_ref(),
        &scope,
        &transaction_id,
    ))
}

async fn add_petty_cash_voucher(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::create_voucher(graph.as_ref(), &scope, &payload))
}

async fn petty_cash_vouchers(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::list_vouchers(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn approve_petty_cash_voucher(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(voucher_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::approve_voucher(
        graph.as_ref(),
        &scope,
        &voucher_id,
        &payload,
    ))
}

async fn add_replenishment(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::create_replenishment(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn list_replenishment_requests(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::list_replenishments(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn approve_replenishment_request(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(replenishment_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::approve_replenishment(
        graph.as_ref(),
        &scope,
        &replenishment_id,
        &payload,
    ))
}

async fn petty_cash_fund_report(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(fund_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::fund_report(
        graph.as_ref(),
        &scope,
        &fund_id,
        &query_object(query.as_deref()),
    ))
}

async fn petty_cash_position(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::cash_position(graph.as_ref(), &scope))
}

async fn petty_cash_category_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(petty_cash::category_summary(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn npv_calculate(Json(payload): Json<Value>) -> Response {
    answer(net_present_value::calculate(&payload))
}

async fn npv_calculate_percent(Json(payload): Json<Value>) -> Response {
    answer(net_present_value::calculate_percent(&payload))
}

async fn npv_compare(Json(payload): Json<Value>) -> Response {
    answer(net_present_value::compare(&payload))
}

async fn npv_profitability_index(Json(payload): Json<Value>) -> Response {
    answer(net_present_value::profitability_index(&payload))
}

async fn irr_calculate(Json(payload): Json<Value>) -> Response {
    answer(internal_rate_return::calculate(&payload))
}

async fn irr_calculate_bisection(Json(payload): Json<Value>) -> Response {
    answer(internal_rate_return::calculate_bisection(&payload))
}

async fn irr_compare(Json(payload): Json<Value>) -> Response {
    answer(internal_rate_return::compare(&payload))
}

async fn irr_decision(Json(payload): Json<Value>) -> Response {
    answer(internal_rate_return::decision(&payload))
}

async fn payback_calculate(Json(payload): Json<Value>) -> Response {
    answer(payback_period::calculate(&payload))
}

async fn payback_compare(Json(payload): Json<Value>) -> Response {
    answer(payback_period::compare(&payload))
}

async fn payback_average_cash_flow(Json(payload): Json<Value>) -> Response {
    answer(payback_period::average_cash_flow(&payload))
}

async fn analyze_continue_shutdown(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(continue_shutdown::analyze(graph.as_ref(), &scope, &payload))
}

async fn quick_continue_shutdown(Json(payload): Json<Value>) -> Response {
    answer(continue_shutdown::quick_decision(&payload))
}

async fn continue_shutdown_decisions(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(continue_shutdown::list(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn continue_shutdown_decision(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(decision_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(continue_shutdown::get(graph.as_ref(), &scope, &decision_id))
}

async fn open_portfolio(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(investment_monitoring::create_portfolio(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn portfolios(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(investment_monitoring::list_portfolios(
        graph.as_ref(),
        &scope,
    ))
}

async fn add_portfolio_holding(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(portfolio_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(investment_monitoring::add_holding(
        graph.as_ref(),
        &scope,
        &portfolio_id,
        &payload,
    ))
}

async fn portfolio_holdings(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(portfolio_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(investment_monitoring::list_holdings(
        graph.as_ref(),
        &scope,
        &portfolio_id,
    ))
}

async fn measure_portfolio(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(portfolio_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(investment_monitoring::record_performance(
        graph.as_ref(),
        &scope,
        &portfolio_id,
        &payload,
    ))
}

async fn portfolio_performance(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(portfolio_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(investment_monitoring::list_performance(
        graph.as_ref(),
        &scope,
        &portfolio_id,
        &query_object(query.as_deref()),
    ))
}

async fn reprice_holding(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(holding_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(investment_monitoring::update_price(
        graph.as_ref(),
        &scope,
        &holding_id,
        &payload,
    ))
}

async fn open_trade_instrument(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(trade_finance::create_instrument(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn trade_instruments(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(trade_finance::list_instruments(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn present_trade_documents(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(instrument_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(trade_finance::present_documents(
        graph.as_ref(),
        &scope,
        &instrument_id,
        &query_object(query.as_deref()),
    ))
}

async fn settle_trade_instrument(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(instrument_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(trade_finance::settle_instrument(
        graph.as_ref(),
        &scope,
        &instrument_id,
        &query_object(query.as_deref()),
    ))
}

/// One handler shape for the many personal-finance writes: scope, body, and a named function.
macro_rules! personal_write {
    ($name:ident, $call:path) => {
        async fn $name(
            State(graph): GraphState,
            headers: HeaderMap,
            Json(payload): Json<Value>,
        ) -> Response {
            let scope = match scope_of(&headers) {
                Ok(scope) => scope,
                Err(response) => return *response,
            };
            answer($call(graph.as_ref(), &scope, &payload))
        }
    };
}

personal_write!(open_recurring, personal_finance::create_recurring);
personal_write!(open_debt, personal_finance::create_debt);
personal_write!(open_investment, personal_finance::create_investment);
personal_write!(open_goal, personal_money::create_goal);
personal_write!(add_income_source, personal_money::add_income);
personal_write!(add_legacy_debt, personal_money::create_legacy_debt);
personal_write!(replace_tax_bands, personal_money::replace_bands);

async fn recurring_items(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(personal_finance::list_recurring(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn stop_recurring(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(recurring_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(personal_finance::deactivate_recurring(
        graph.as_ref(),
        &scope,
        &recurring_id,
    ))
}

async fn run_recurring_now(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(recurring_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(personal_finance::run_recurring(
        graph.as_ref(),
        &scope,
        &recurring_id,
        &query_object(query.as_deref()),
    ))
}

async fn debts(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(personal_finance::list_debts(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn one_debt(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(debt_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(personal_finance::get_debt(
        graph.as_ref(),
        &scope,
        &debt_id,
        &query_object(query.as_deref()),
    ))
}

async fn pay_debt(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(debt_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(personal_finance::add_payment(
        graph.as_ref(),
        &scope,
        &debt_id,
        &payload,
    ))
}

async fn debt_projection(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(debt_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(personal_finance::debt_schedule(
        graph.as_ref(),
        &scope,
        &debt_id,
        &query_object(query.as_deref()),
    ))
}

async fn investments(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(personal_finance::list_investments(graph.as_ref(), &scope))
}

async fn trade_investment(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(investment_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(personal_finance::add_trade(
        graph.as_ref(),
        &scope,
        &investment_id,
        &payload,
    ))
}

async fn reprice_investment(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(investment_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(personal_finance::set_investment_price(
        graph.as_ref(),
        &scope,
        &investment_id,
        &payload,
    ))
}

async fn tax_bands(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(personal_money::list_bands(graph.as_ref(), &scope))
}

async fn estimate_tax(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(personal_money::estimate_tax(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn goals(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(personal_money::list_goals(graph.as_ref(), &scope))
}

async fn income_sources(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(personal_money::list_income(graph.as_ref(), &scope))
}

async fn personal_overview(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(personal_money::overview(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn budget_prepare(Json(payload): Json<Value>) -> Response {
    answer(master_budget::prepare(&payload))
}

async fn budget_create(Json(payload): Json<Value>) -> Response {
    answer(master_budget::create_budget(&payload))
}

async fn budget_monitor(Json(payload): Json<Value>) -> Response {
    answer(master_budget::monitor(&payload))
}

async fn budget_analyze(Json(payload): Json<Value>) -> Response {
    answer(master_budget::analyze(&payload))
}

async fn budget_variance(Json(payload): Json<Value>) -> Response {
    answer(master_budget::detailed_variance(&payload))
}

async fn budget_forecast(Json(payload): Json<Value>) -> Response {
    answer(master_budget::forecast(&payload))
}

async fn budget_rolling_forecast(Json(payload): Json<Value>) -> Response {
    answer(master_budget::rolling_forecast(&payload))
}

async fn open_budget_package(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(zero_based_budget::create_package(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn budget_packages(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let mut asked = query_object(query.as_deref());
    if let Some(object) = asked.as_object_mut() {
        object.insert("company_id".to_string(), json!(company_id));
    }
    answer(zero_based_budget::list_packages(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn add_budget_item(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(package_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(zero_based_budget::add_item(
        graph.as_ref(),
        &scope,
        &package_id,
        &payload,
    ))
}

async fn budget_package_status(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(package_id): Path<String>,
    RawQuery(query): RawQuery,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(zero_based_budget::update_status(
        graph.as_ref(),
        &scope,
        &package_id,
        &query_object(query.as_deref()),
        &payload,
    ))
}

async fn budget_item_priority(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(zero_based_budget::set_item_priority(
        graph.as_ref(),
        &scope,
        &item_id,
        &query_object(query.as_deref()),
    ))
}

async fn budget_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(zero_based_budget::summary(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn open_rolling_forecast(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(rolling_forecast::create(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
        &payload,
    ))
}

async fn rolling_forecasts(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(rolling_forecast::list_forecasts(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn one_rolling_forecast(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(forecast_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(rolling_forecast::forecast_by_id(
        graph.as_ref(),
        &scope,
        &forecast_id,
    ))
}

async fn record_forecast_actual(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(forecast_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let payload = payload
        .map(|Json(payload)| payload)
        .unwrap_or_else(|| json!({}));
    answer(rolling_forecast::update_actual(
        graph.as_ref(),
        &scope,
        &forecast_id,
        &query_object(query.as_deref()),
        &payload,
    ))
}

async fn roll_forecast(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(forecast_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(rolling_forecast::roll(
        graph.as_ref(),
        &scope,
        &forecast_id,
        &payload,
    ))
}

async fn analyze_scenarios(Json(payload): Json<Value>) -> Response {
    answer(scenario_analysis::analyze(&payload))
}

async fn open_scenario(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(scenario_analysis::open_scenario(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn scenarios_for_company(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(scenario_analysis::scenarios_for_company(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn compare_scenarios(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(scenario_analysis::compare_assumptions(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn run_sensitivity(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(sensitivity_analysis::analyze(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn sensitivity_analyses(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(sensitivity_analysis::list_analyses(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn open_model_scenario(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(scenario_modeling::open_model_scenario(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn model_scenarios(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(scenario_modeling::list_model_scenarios(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn one_model_scenario(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(scenario_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(scenario_modeling::get_scenario(
        graph.as_ref(),
        &scope,
        &scenario_id,
    ))
}

async fn change_model_scenario(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(scenario_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(scenario_modeling::update_scenario(
        graph.as_ref(),
        &scope,
        &scenario_id,
        &payload,
    ))
}

async fn drop_model_scenario(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(scenario_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(scenario_modeling::delete_scenario(
        graph.as_ref(),
        &scope,
        &scenario_id,
    ))
}

async fn open_rule(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(scenario_modeling::create_rule(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn model_rules(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(scenario_modeling::list_rules(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn one_rule(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(rule_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(scenario_modeling::get_rule(
        graph.as_ref(),
        &scope,
        &rule_id,
    ))
}

async fn change_rule(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(rule_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(scenario_modeling::update_rule(
        graph.as_ref(),
        &scope,
        &rule_id,
        &payload,
    ))
}

async fn drop_rule(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(rule_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(scenario_modeling::delete_rule(
        graph.as_ref(),
        &scope,
        &rule_id,
    ))
}

async fn run_what_if(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(scenario_modeling::what_if(graph.as_ref(), &scope, &payload))
}

async fn one_what_if(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(analysis_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(scenario_modeling::get_what_if(
        graph.as_ref(),
        &scope,
        &analysis_id,
    ))
}

async fn what_if_for_scenario(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(scenario_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(scenario_modeling::list_what_if(
        graph.as_ref(),
        &scope,
        &scenario_id,
    ))
}

async fn run_model_sweep(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(scenario_modeling::sweep(graph.as_ref(), &scope, &payload))
}

async fn compare_model_scenarios(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    // the service took a bare list of ids; a body keyed `scenario_ids` is accepted too
    let asked = query_object(query.as_deref());
    let ids: Vec<String> = match &payload {
        Value::Array(items) => items
            .iter()
            .map(|id| id.to_string().trim_matches('"').to_string())
            .collect(),
        other => other
            .get("scenario_ids")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|id| id.to_string().trim_matches('"').to_string())
            .collect(),
    };
    let today = {
        let asked_today = store::read::text(&asked, "today");
        if asked_today.is_empty() {
            store::read::text(&payload, "today")
        } else {
            asked_today
        }
    };
    answer(scenario_modeling::compare_model_scenarios(
        graph.as_ref(),
        &scope,
        &ids,
        &today,
    ))
}

async fn liquidity_history(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cash_management::list_liquidity_positions(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn add_cash_account(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cash_optimization::add_account(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn company_cash_accounts(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cash_optimization::accounts(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn run_cash_optimization(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cash_optimization::optimize(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn company_cash_suggestions(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cash_optimization::suggestions(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn generate_trading_account(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(trading_account::generate(graph.as_ref(), &scope, &payload))
}

async fn list_trading_accounts(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let period = query_param(query.as_deref(), "period");
    answer(trading_account::list(
        graph.as_ref(),
        &scope,
        period.as_deref(),
    ))
}

async fn get_trading_account(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(account_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(trading_account::get(graph.as_ref(), &scope, &account_id))
}

async fn create_appropriation(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(appropriation_control::create(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn company_appropriations(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let department = query_param(query.as_deref(), "department").unwrap_or_default();
    answer(appropriation_control::list(
        graph.as_ref(),
        &scope,
        &company_id,
        &department,
    ))
}

async fn appropriation_transaction(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(appropriation_control::transact(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn appropriation_check(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(appropriation_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    // `amount` arrives as a query parameter, as the service declared it.
    let amount = match query_param(query.as_deref(), "amount") {
        Some(text) => serde_json::from_str::<Value>(&text).unwrap_or(Value::Null),
        None => Value::Null,
    };
    answer(appropriation_control::check(
        graph.as_ref(),
        &scope,
        &appropriation_id,
        &amount,
    ))
}

async fn register_asset(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::create_asset(graph.as_ref(), &scope, &payload))
}

async fn fixed_assets(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let category = query_param(query.as_deref(), "category");
    let status = query_param(query.as_deref(), "status");
    let department = query_param(query.as_deref(), "department");
    answer(assets::list_assets(
        graph.as_ref(),
        &scope,
        category.as_deref(),
        status.as_deref(),
        department.as_deref(),
    ))
}

async fn one_fixed_asset(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(asset_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::get_asset(graph.as_ref(), &scope, &asset_id))
}

async fn depreciate_asset(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(asset_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    // `period` arrives as a query parameter, as the service declared it.
    let period = query_param(query.as_deref(), "period").unwrap_or_default();
    answer(assets::depreciate(
        graph.as_ref(),
        &scope,
        &asset_id,
        &period,
    ))
}

async fn asset_depreciation(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(asset_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::list_entries(graph.as_ref(), &scope, &asset_id))
}

async fn dispose_fixed_asset(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(asset_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    // the service took the disposal as query parameters
    let mut payload = serde_json::Map::new();
    for key in [
        "disposal_date",
        "disposal_value",
        "disposal_method",
        "notes",
    ] {
        if let Some(value) = query_param(query.as_deref(), key) {
            let parsed = serde_json::from_str::<Value>(&value).unwrap_or(Value::String(value));
            payload.insert(key.to_string(), parsed);
        }
    }
    answer(assets::dispose_registered_asset(
        graph.as_ref(),
        &scope,
        &asset_id,
        &Value::Object(payload),
    ))
}

async fn fixed_asset_summary(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::asset_register_summary(graph.as_ref(), &scope))
}

async fn schedule_asset_list(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let category = query_param(query.as_deref(), "category");
    let status = query_param(query.as_deref(), "status");
    answer(assets::list_register_with_count(
        graph.as_ref(),
        &scope,
        category.as_deref(),
        status.as_deref(),
    ))
}

async fn patch_fixed_asset(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(asset_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::update_registered_asset(
        graph.as_ref(),
        &scope,
        &asset_id,
        &payload,
    ))
}

async fn asset_schedule(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    // `as_of_date` arrives as a query parameter, as the service declared it. `/schedule/latest`
    // answers for today, and regenerates rather than reading the last stored schedule, which is
    // what the service did.
    let as_of_date = query_param(query.as_deref(), "as_of_date")
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
    answer(assets::generate_schedule(
        graph.as_ref(),
        &scope,
        &as_of_date,
    ))
}

async fn schedule_summary(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::schedule_summary(graph.as_ref(), &scope))
}

async fn create_intangible(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::create_intangible(graph.as_ref(), &scope, &payload))
}

async fn intangibles(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asset_type = query_param(query.as_deref(), "asset_type");
    let status = query_param(query.as_deref(), "status");
    answer(assets::list_intangibles(
        graph.as_ref(),
        &scope,
        asset_type.as_deref(),
        status.as_deref(),
    ))
}

async fn one_intangible(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(asset_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::get_intangible(graph.as_ref(), &scope, &asset_id))
}

async fn amortize_intangible(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(asset_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let period = query_param(query.as_deref(), "period").unwrap_or_default();
    answer(assets::amortize(graph.as_ref(), &scope, &asset_id, &period))
}

async fn intangible_amortization(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(asset_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::list_amortization_entries(
        graph.as_ref(),
        &scope,
        &asset_id,
    ))
}

async fn create_disposal_group(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::create_disposal_group(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn disposal_groups(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let status = query_param(query.as_deref(), "status");
    answer(assets::list_disposal_groups(
        graph.as_ref(),
        &scope,
        status.as_deref(),
    ))
}

async fn one_disposal_group(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(group_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::get_disposal_group(
        graph.as_ref(),
        &scope,
        &group_id,
    ))
}

async fn add_disposal_asset(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = body_or_query(query.as_deref());
    let body = overlay(body, payload.map(|Json(value)| value));
    answer(assets::add_disposal_asset(
        graph.as_ref(),
        &scope,
        &group_id,
        &body,
    ))
}

async fn disposal_assets(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(group_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::list_disposal_assets(
        graph.as_ref(),
        &scope,
        &group_id,
    ))
}

async fn disposal_impairment_test(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = body_or_query(query.as_deref());
    let body = overlay(body, payload.map(|Json(value)| value));
    answer(assets::disposal_impairment_test(
        graph.as_ref(),
        &scope,
        &group_id,
        &body,
    ))
}

async fn disposal_group_status(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let status = query_param(query.as_deref(), "status").unwrap_or_default();
    answer(assets::update_disposal_status(
        graph.as_ref(),
        &scope,
        &group_id,
        &status,
    ))
}

async fn create_lease_termination(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = body_or_query(query.as_deref());
    let body = overlay(body, payload.map(|Json(value)| value));
    answer(assets::create_termination(graph.as_ref(), &scope, &body))
}

async fn lease_terminations(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let status = query_param(query.as_deref(), "status");
    answer(assets::list_terminations(
        graph.as_ref(),
        &scope,
        status.as_deref(),
    ))
}

async fn one_lease_termination(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(termination_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::get_termination(
        graph.as_ref(),
        &scope,
        &termination_id,
    ))
}

async fn lease_settlement(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(termination_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = body_or_query(query.as_deref());
    let body = overlay(body, payload.map(|Json(value)| value));
    answer(assets::calculate_settlement(
        graph.as_ref(),
        &scope,
        &termination_id,
        &body,
    ))
}

async fn approve_lease_termination(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(termination_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = body_or_query(query.as_deref());
    answer(assets::approve_termination(
        graph.as_ref(),
        &scope,
        &termination_id,
        &body,
    ))
}

async fn complete_lease_termination(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(termination_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::complete_termination(
        graph.as_ref(),
        &scope,
        &termination_id,
    ))
}

/// The query string as a payload, with a JSON body laid over it.
///
/// These services took their arguments as query parameters, so a caller migrating to a body must not
/// lose the query it already sends, and a body must win where both name the same field.
pub(crate) fn overlay(query: Value, body: Option<Value>) -> Value {
    let mut merged = query.as_object().cloned().unwrap_or_default();
    if let Some(Value::Object(fields)) = body {
        for (key, value) in fields {
            merged.insert(key, value);
        }
    }
    Value::Object(merged)
}

async fn record_share_premium(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::record_share_premium(graph.as_ref(), &scope, &body))
}

async fn utilize_share_premium(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::utilize_premium(graph.as_ref(), &scope, &body))
}

async fn adjust_share_premium(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::adjust_premium(graph.as_ref(), &scope, &body))
}

fn company_filter(query: Option<&str>) -> Option<String> {
    query_param(query, "company_id")
}

async fn share_premium_entries(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = company_filter(query.as_deref());
    answer(equity::list_premium_entries(
        graph.as_ref(),
        &scope,
        company.as_deref(),
    ))
}

async fn share_premium_utilizations(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = company_filter(query.as_deref());
    answer(equity::list_premium_utilizations(
        graph.as_ref(),
        &scope,
        company.as_deref(),
    ))
}

async fn share_premium_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::premium_summary(graph.as_ref(), &scope, &company_id))
}

async fn record_redemption(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::record_redemption(graph.as_ref(), &scope, &body))
}

async fn create_crr(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::create_crr(graph.as_ref(), &scope, &body))
}

async fn utilize_crr(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::utilize_crr(graph.as_ref(), &scope, &body))
}

async fn list_redemptions(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = company_filter(query.as_deref());
    answer(equity::list_crr_redemptions(
        graph.as_ref(),
        &scope,
        company.as_deref(),
    ))
}

async fn list_crr_creations(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = company_filter(query.as_deref());
    answer(equity::list_crr_creations(
        graph.as_ref(),
        &scope,
        company.as_deref(),
    ))
}

async fn list_crr_utilizations(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = company_filter(query.as_deref());
    answer(equity::list_crr_utilizations(
        graph.as_ref(),
        &scope,
        company.as_deref(),
    ))
}

async fn crr_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::crr_summary(graph.as_ref(), &scope, &company_id))
}
async fn initiate_redemption(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::initiate_redemption(graph.as_ref(), &scope, &body))
}

async fn record_fresh_issue(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(redemption_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::record_fresh_issue(
        graph.as_ref(),
        &scope,
        &redemption_id,
        &body,
    ))
}

async fn complete_redemption(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(redemption_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::complete_redemption(
        graph.as_ref(),
        &scope,
        &redemption_id,
        &body,
    ))
}

async fn list_share_redemptions(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = company_filter(query.as_deref());
    let status = query_param(query.as_deref(), "status");
    answer(equity::list_share_redemptions(
        graph.as_ref(),
        &scope,
        company.as_deref(),
        status.as_deref(),
    ))
}

async fn share_redemption_detail(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(redemption_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::redemption_detail(
        graph.as_ref(),
        &scope,
        &redemption_id,
    ))
}

async fn crr_requirements(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let status = query_param(query.as_deref(), "status");
    answer(equity::list_crr_requirements(
        graph.as_ref(),
        &scope,
        status.as_deref(),
    ))
}
async fn create_department(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::create_department(graph.as_ref(), &scope, &body))
}

async fn list_departments(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::list_departments(
        graph.as_ref(),
        &scope,
        &filters,
    ))
}

async fn get_department(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::get_department(graph.as_ref(), &scope, &item_id))
}

async fn update_department(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::update_department(
        graph.as_ref(),
        &scope,
        &item_id,
        &body,
    ))
}

async fn department_hierarchy(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::department_hierarchy(
        graph.as_ref(),
        &scope,
        &item_id,
    ))
}

async fn create_allocation_rule(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::create_allocation_rule(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn list_allocation_rules(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::list_allocation_rules(
        graph.as_ref(),
        &scope,
        &filters,
    ))
}

async fn create_cost_pool(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::create_cost_pool(graph.as_ref(), &scope, &body))
}

async fn list_cost_pools(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::list_cost_pools(
        graph.as_ref(),
        &scope,
        &filters,
    ))
}

async fn get_cost_pool(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::get_cost_pool(graph.as_ref(), &scope, &item_id))
}

async fn allocation_results(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::allocation_results(
        graph.as_ref(),
        &scope,
        &item_id,
    ))
}

async fn create_inter_department_bill(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::create_inter_department_bill(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn list_inter_department_bills(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::list_inter_department_bills(
        graph.as_ref(),
        &scope,
        &filters,
    ))
}

async fn deactivate_allocation_rule(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::deactivate_allocation_rule(
        graph.as_ref(),
        &scope,
        &item_id,
    ))
}

async fn run_allocation(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    // the pool and its period arrive as query parameters, the revenues as a body
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(accounting::run_allocation(graph.as_ref(), &scope, &body))
}

async fn approve_inter_department_bill(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(accounting::approve_inter_department_bill(
        graph.as_ref(),
        &scope,
        &item_id,
        &body,
    ))
}

async fn department_financials(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::department_financials(
        graph.as_ref(),
        &scope,
        &item_id,
        &filters,
    ))
}

async fn performance_report(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::performance_report(
        graph.as_ref(),
        &scope,
        &item_id,
        &filters,
    ))
}

async fn compare_departments(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    // the GET alias takes its department list in a body as well as in the query
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(accounting::compare_departments(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn cost_distribution(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::cost_distribution(
        graph.as_ref(),
        &scope,
        &filters,
    ))
}

async fn create_company(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::create_company(graph.as_ref(), &scope, &body))
}

async fn list_companies(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::list_companies(graph.as_ref(), &scope, &filters))
}

async fn get_company(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::get_company(graph.as_ref(), &scope, &item_id))
}

async fn update_company(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::update_company(
        graph.as_ref(),
        &scope,
        &item_id,
        &body,
    ))
}

async fn register_company_shareholder(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::register_shareholder(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn list_company_shareholders(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::list_shareholders(
        graph.as_ref(),
        &scope,
        &filters,
    ))
}

async fn create_share_capital(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::create_share_capital(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn list_share_capital(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::list_share_capital(
        graph.as_ref(),
        &scope,
        &filters,
    ))
}

async fn record_capital_transaction(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::record_capital_transaction(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn list_capital_transactions(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::list_capital_transactions(
        graph.as_ref(),
        &scope,
        &filters,
    ))
}

async fn declare_dividend(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::declare_dividend(graph.as_ref(), &scope, &body))
}

async fn list_dividends(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::list_dividends(graph.as_ref(), &scope, &filters))
}

async fn pay_dividend(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::pay_dividend(
        graph.as_ref(),
        &scope,
        &item_id,
        &body,
    ))
}

async fn dividend_payments(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::dividend_payments(
        graph.as_ref(),
        &scope,
        &item_id,
    ))
}

async fn record_retained_earnings(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::record_retained_earnings(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn retained_earnings(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let Some(company_id) = query_param(query.as_deref(), "company_id") else {
        return refused("company_id is required");
    };
    answer(accounting::retained_earnings(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn company_reserves(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let Some(company_id) = query_param(query.as_deref(), "company_id") else {
        return refused("company_id is required");
    };
    answer(accounting::company_reserves(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn create_company_reserve(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::create_company_reserve(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn equity_statement(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::equity_statement(
        graph.as_ref(),
        &scope,
        &company_id,
        &filters,
    ))
}

async fn shareholder_register(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::shareholder_register(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn dividend_history(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::dividend_history(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn create_cash_account(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::create_cash_account(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn list_cash_accounts(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::list_cash_accounts(
        graph.as_ref(),
        &scope,
        &filters,
    ))
}

async fn get_cash_account(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::get_cash_account(
        graph.as_ref(),
        &scope,
        &item_id,
    ))
}

async fn update_cash_account(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::update_cash_account(
        graph.as_ref(),
        &scope,
        &item_id,
        &body,
    ))
}

async fn create_cash_entry(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::create_cash_entry(graph.as_ref(), &scope, &body))
}

async fn list_cash_entries(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::list_cash_entries(
        graph.as_ref(),
        &scope,
        &filters,
    ))
}

async fn get_cash_entry(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::get_cash_entry(graph.as_ref(), &scope, &item_id))
}

async fn post_cash_entry(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::post_cash_entry(
        graph.as_ref(),
        &scope,
        &item_id,
        &body,
    ))
}

async fn void_cash_entry(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::void_cash_entry(
        graph.as_ref(),
        &scope,
        &item_id,
        &body,
    ))
}

async fn cashbook_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::cashbook_summary(
        graph.as_ref(),
        &scope,
        &item_id,
        &filters,
    ))
}

async fn create_cash_reconciliation(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::create_cash_reconciliation(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn list_cash_reconciliations(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::list_cash_reconciliations(
        graph.as_ref(),
        &scope,
        &filters,
    ))
}

async fn get_cash_reconciliation(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::get_cash_reconciliation(
        graph.as_ref(),
        &scope,
        &item_id,
    ))
}

async fn complete_reconciliation(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::complete_reconciliation(
        graph.as_ref(),
        &scope,
        &item_id,
        &body,
    ))
}

async fn cash_reconciliation_outstanding(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(item_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::cash_reconciliation_outstanding(
        graph.as_ref(),
        &scope,
        &item_id,
    ))
}

async fn create_cash_flow_entry(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::create_cash_flow_entry(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn list_cash_flow(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::list_cash_flow(graph.as_ref(), &scope, &filters))
}

async fn cash_flow_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::cash_flow_summary(
        graph.as_ref(),
        &scope,
        &filters,
    ))
}

async fn cash_position(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::cash_position(graph.as_ref(), &scope, &filters))
}

async fn cash_position_history(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(accounting::cash_position_history(
        graph.as_ref(),
        &scope,
        &filters,
    ))
}

async fn create_ifrs_report(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::create_ifrs_report(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn list_ifrs_reports(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = json!({
        "report_type": query_param(query.as_deref(), "report_type"),
        "ifrs_standard": query_param(query.as_deref(), "ifrs_standard"),
        "status": query_param(query.as_deref(), "status"),
    });
    answer(accounting::list_ifrs_reports(
        graph.as_ref(),
        &scope,
        &filters,
    ))
}

async fn get_ifrs_report(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(report_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::get_ifrs_report(
        graph.as_ref(),
        &scope,
        &report_id,
    ))
}

async fn add_ifrs_note(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(report_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::add_ifrs_note(
        graph.as_ref(),
        &scope,
        &report_id,
        &body,
    ))
}

async fn list_ifrs_notes(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(report_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::list_ifrs_notes(
        graph.as_ref(),
        &scope,
        &report_id,
    ))
}

async fn approve_ifrs_report(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(report_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::approve_ifrs_report(
        graph.as_ref(),
        &scope,
        &report_id,
        &body,
    ))
}

async fn create_intercompany_entity(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::create_intercompany_entity(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn list_intercompany_entities(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::list_intercompany_entities(
        graph.as_ref(),
        &scope,
    ))
}

async fn create_intercompany_transaction(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::create_intercompany_transaction(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn list_intercompany_transactions(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = json!({
        "from_entity": query_param(query.as_deref(), "from_entity"),
        "to_entity": query_param(query.as_deref(), "to_entity"),
        "status": query_param(query.as_deref(), "status"),
    });
    answer(accounting::list_intercompany_transactions(
        graph.as_ref(),
        &scope,
        &filters,
    ))
}

async fn match_intercompany(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::match_intercompany_transactions(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn list_eliminations(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::list_eliminations(graph.as_ref(), &scope))
}

async fn create_cash_flow(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::create_cash_flow_schedule(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn cash_flow_inflows(payload: Option<Json<Value>>) -> Response {
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::cash_flow_total(&body, "inflows"))
}

async fn cash_flow_outflows(payload: Option<Json<Value>>) -> Response {
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::cash_flow_total(&body, "outflows"))
}

async fn cash_flow_net(payload: Option<Json<Value>>) -> Response {
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::net_flows(&body))
}

async fn cash_flow_cumulative(payload: Option<Json<Value>>) -> Response {
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::cumulative_flows(&body))
}

async fn list_cash_flows(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::list_cash_flow_schedules(graph.as_ref(), &scope))
}

async fn get_cash_flow(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(cash_flow_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::get_cash_flow_schedule(
        graph.as_ref(),
        &scope,
        &cash_flow_id,
    ))
}

async fn generate_balance_sheet(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::generate_balance_sheet(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn latest_balance_sheet(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::latest_sheet(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn balance_sheet_history(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::balance_sheet_history(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn balance_sheet_ratios(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::balance_sheet_ratios(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn create_fund(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::create_fund(graph.as_ref(), &scope, &body))
}

async fn company_funds(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::list_funds(graph.as_ref(), &scope, &company_id))
}

async fn record_fund_movement(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::record_fund_transaction(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn fund_movements(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(fund_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::list_fund_transactions(
        graph.as_ref(),
        &scope,
        &fund_id,
    ))
}

async fn create_revenue_contract(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(accounting::create_revenue_contract(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn recognise_revenue(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(contract_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let obligation = query_param(query.as_deref(), "obligation_id").unwrap_or_default();
    // no amount means "recognise this obligation in full", so the zero is deliberate
    let asked = match query_param(query.as_deref(), "amount") {
        Some(raw) => match amount(&json!({ "amount": raw }), "amount") {
            Ok(amount) => amount,
            Err(error) => return fail(error),
        },
        None => Money::zero(),
    };
    answer(accounting::recognise_revenue(
        graph.as_ref(),
        &scope,
        &contract_id,
        &obligation,
        &asked,
    ))
}

async fn company_revenue_contracts(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::list_revenue_contracts(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn company_revenue_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(accounting::revenue_summary(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn create_reconstruction(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::create_reconstruction(graph.as_ref(), &scope, &body))
}

async fn add_reconstruction_adjustment(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(reconstruction_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::add_adjustment(
        graph.as_ref(),
        &scope,
        &reconstruction_id,
        &body,
    ))
}

async fn add_reconstruction_conversion(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(reconstruction_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::add_reserve_conversion(
        graph.as_ref(),
        &scope,
        &reconstruction_id,
        &body,
    ))
}

async fn approve_reconstruction(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(reconstruction_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::approve_reconstruction(
        graph.as_ref(),
        &scope,
        &reconstruction_id,
        &body,
    ))
}

async fn capital_reconstructions(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = query_param(query.as_deref(), "company_id");
    let status = query_param(query.as_deref(), "status");
    answer(equity::list_capital_reconstructions(
        graph.as_ref(),
        &scope,
        company.as_deref(),
        status.as_deref(),
    ))
}

async fn reconstruction_scheme(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(reconstruction_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::reconstruction_detail(
        graph.as_ref(),
        &scope,
        &reconstruction_id,
    ))
}

async fn reconstruction_company_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::reconstruction_summary(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn create_debenture_class(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::create_debenture_class(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn issue_debentures(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(class_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::issue_debentures(
        graph.as_ref(),
        &scope,
        &class_id,
        &body,
    ))
}

async fn accrue_debenture_interest(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(class_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::accrue_interest(
        graph.as_ref(),
        &scope,
        &class_id,
        &body,
    ))
}

async fn pay_debenture_interest(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(interest_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::pay_interest(
        graph.as_ref(),
        &scope,
        &interest_id,
        &body,
    ))
}

async fn redeem_debentures(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(class_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::redeem(graph.as_ref(), &scope, &class_id, &body))
}

async fn debenture_classes(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = query_param(query.as_deref(), "company_id");
    answer(equity::list_debenture_classes(
        graph.as_ref(),
        &scope,
        company.as_deref(),
    ))
}

async fn debenture_issues(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = query_param(query.as_deref(), "company_id");
    answer(equity::list_debenture_issues(
        graph.as_ref(),
        &scope,
        company.as_deref(),
    ))
}

async fn debenture_interest(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = query_param(query.as_deref(), "company_id");
    answer(equity::list_debenture_interest(
        graph.as_ref(),
        &scope,
        company.as_deref(),
    ))
}

async fn issue_share_capital(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::record_issue(graph.as_ref(), &scope, &body))
}

async fn register_shareholder(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    // one register: this also recalculates each holding's share of the company
    answer(accounting::register_shareholder(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn company_shareholders(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::list_shareholders(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn company_share_issues(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::list_issues(graph.as_ref(), &scope, &company_id))
}

async fn issued_capital_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::capital_summary(graph.as_ref(), &scope, &company_id))
}

async fn create_investment(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::create_investment(graph.as_ref(), &scope, &body))
}

async fn simple_investment(
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    // pure arithmetic, but still a Book's caller: the scope is checked and then unused
    if let Err(response) = scope_of(&headers) {
        return *response;
    }
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    let amount = |key: &str| amount_or_zero(&body, key);
    let (asset, scrap, sold, tax) = (
        amount("asset_cost"),
        amount("scrap_value"),
        amount("selling_of_old_asset"),
        amount("tax_on_sale"),
    );
    match (asset, scrap, sold, tax) {
        (Ok(asset), Ok(scrap), Ok(sold), Ok(tax)) => {
            let (net, outlay) = equity::simple_outlay(&asset, &sold, &tax);
            answer(Ok(json!({
                "asset_cost": wire(&asset),
                "scrap_value": wire(&scrap),
                "selling_of_old_asset": wire(&sold),
                "tax_on_sale": wire(&tax),
                "net_proceeds_from_sale": wire(&net),
                "initial_cash_outlay": wire(&outlay),
            })))
        }
        (Err(error), _, _, _)
        | (_, Err(error), _, _)
        | (_, _, Err(error), _)
        | (_, _, _, Err(error)) => answer(Err(error)),
    }
}

async fn investment_with_capital_allowance(
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    // pure arithmetic, but still a Book's caller: the scope is checked and then unused
    if let Err(response) = scope_of(&headers) {
        return *response;
    }
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    let amount = |key: &str| amount_or_zero(&body, key);
    match (
        amount("asset_cost"),
        amount("scrap_value"),
        amount("capital_allowance"),
        amount("tax_rate"),
    ) {
        (Ok(asset), Ok(scrap), Ok(allowance), Ok(rate)) => {
            let saving = equity::capital_allowance_saving(&allowance, &rate);
            answer(Ok(json!({
                "asset_cost": wire(&asset),
                "scrap_value": wire(&scrap),
                "capital_allowance": wire(&allowance),
                "tax_rate": wire(&rate),
                "tax_saving": wire(&saving),
                "adjusted_investment_cost": wire(&asset.minus(&saving)),
            })))
        }
        (Err(error), _, _, _)
        | (_, Err(error), _, _)
        | (_, _, Err(error), _)
        | (_, _, _, Err(error)) => answer(Err(error)),
    }
}

async fn list_investments(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::list_investments(graph.as_ref(), &scope))
}

async fn get_investment(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(investment_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::get_investment(
        graph.as_ref(),
        &scope,
        &investment_id,
    ))
}

async fn create_reserve(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::create_reserve(graph.as_ref(), &scope, &body))
}

async fn reserves(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = query_param(query.as_deref(), "company_id");
    answer(equity::list_reserves(
        graph.as_ref(),
        &scope,
        company.as_deref(),
    ))
}

async fn one_reserve(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(reserve_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::get_reserve(graph.as_ref(), &scope, &reserve_id))
}

async fn allocate_reserve(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(reserve_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::allocate_to_reserve(
        graph.as_ref(),
        &scope,
        &reserve_id,
        &body,
    ))
}

async fn utilize_reserve(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(reserve_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::utilize_reserve(
        graph.as_ref(),
        &scope,
        &reserve_id,
        &body,
    ))
}

async fn reserve_history(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(reserve_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::reserve_history(graph.as_ref(), &scope, &reserve_id))
}

async fn create_profit_appropriation(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::create_appropriation(graph.as_ref(), &scope, &body))
}

async fn profit_appropriations(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = query_param(query.as_deref(), "company_id");
    let year = query_param(query.as_deref(), "financial_year");
    answer(equity::list_appropriations(
        graph.as_ref(),
        &scope,
        company.as_deref(),
        year.as_deref(),
    ))
}

async fn declare_interim_dividend(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::declare_interim_dividend(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn pay_interim_dividend(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(dividend_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::pay_interim_dividend(
        graph.as_ref(),
        &scope,
        &dividend_id,
        &body,
    ))
}

async fn interim_dividends(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = query_param(query.as_deref(), "company_id");
    answer(equity::list_interim_dividends(
        graph.as_ref(),
        &scope,
        company.as_deref(),
    ))
}

async fn create_prior_year_adjustment(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::create_prior_year_adjustment(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn prior_year_adjustments(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = query_param(query.as_deref(), "company_id");
    answer(equity::list_adjustments(
        graph.as_ref(),
        &scope,
        company.as_deref(),
    ))
}

async fn retained_earnings_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::retained_earnings_summary(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn issue_bonus_shares(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::issue_bonus_shares(graph.as_ref(), &scope, &body))
}

async fn bonus_issues(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = query_param(query.as_deref(), "company_id");
    answer(equity::list_bonus_issues(
        graph.as_ref(),
        &scope,
        company.as_deref(),
    ))
}

async fn offer_rights(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::offer_rights(graph.as_ref(), &scope, &body))
}

async fn accept_rights(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(issue_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::accept_rights(
        graph.as_ref(),
        &scope,
        &issue_id,
        &body,
    ))
}

async fn renounce_rights(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(issue_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::renounce_rights(
        graph.as_ref(),
        &scope,
        &issue_id,
        &body,
    ))
}

async fn right_issues(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = query_param(query.as_deref(), "company_id");
    answer(equity::list_right_issues(
        graph.as_ref(),
        &scope,
        company.as_deref(),
    ))
}

async fn one_right_issue(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(issue_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::get_right_issue(graph.as_ref(), &scope, &issue_id))
}

async fn declare_ordinary_dividend(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::declare_ordinary_dividend(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn pay_ordinary_dividend(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(dividend_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::pay_ordinary_dividend(
        graph.as_ref(),
        &scope,
        &dividend_id,
        &body,
    ))
}

async fn ordinary_dividends(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = query_param(query.as_deref(), "company_id");
    answer(equity::list_ordinary_dividends(
        graph.as_ref(),
        &scope,
        company.as_deref(),
    ))
}

async fn create_preference_class(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::create_preference_class(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn preference_classes(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = query_param(query.as_deref(), "company_id");
    answer(equity::list_preference_classes(
        graph.as_ref(),
        &scope,
        company.as_deref(),
    ))
}

async fn issue_preference_shares(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(class_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::issue_preference_shares(
        graph.as_ref(),
        &scope,
        &class_id,
        &body,
    ))
}

async fn declare_preference_dividend(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(class_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::declare_preference_dividend(
        graph.as_ref(),
        &scope,
        &class_id,
        &body,
    ))
}

async fn pay_preference_dividend(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(dividend_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::pay_preference_dividend(
        graph.as_ref(),
        &scope,
        &dividend_id,
        &body,
    ))
}

async fn redeem_preference_shares(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(class_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::redeem_preference_shares(
        graph.as_ref(),
        &scope,
        &class_id,
        &body,
    ))
}

async fn preference_dividends(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company = query_param(query.as_deref(), "company_id");
    answer(equity::list_preference_dividends(
        graph.as_ref(),
        &scope,
        company.as_deref(),
    ))
}

async fn create_share_class(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::create_share_class(graph.as_ref(), &scope, &body))
}

async fn share_classes(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::list_share_classes(graph.as_ref(), &scope))
}

async fn one_share_class(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(class_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::get_share_class(graph.as_ref(), &scope, &class_id))
}

async fn issue_class_shares(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(class_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::issue_shares(
        graph.as_ref(),
        &scope,
        &class_id,
        &body,
    ))
}

async fn class_issuances(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(class_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::list_share_issuances(
        graph.as_ref(),
        &scope,
        &class_id,
    ))
}

async fn buyback_class_shares(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(class_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(equity::buyback_shares(
        graph.as_ref(),
        &scope,
        &class_id,
        &body,
    ))
}

async fn share_capital_summary(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(equity::share_capital_summary(graph.as_ref(), &scope))
}

pub(crate) fn body_or_query(query: Option<&str>) -> Value {
    let mut payload = serde_json::Map::new();
    for (key, value) in url::form_urlencoded::parse(query.unwrap_or("").as_bytes()) {
        let parsed =
            serde_json::from_str::<Value>(&value).unwrap_or(Value::String(value.to_string()));
        payload.insert(key.to_string(), parsed);
    }
    Value::Object(payload)
}

async fn record_asset_revaluation(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::record_revaluation(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn record_reserve_utilization(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::record_revaluation_utilization(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn revaluations(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let company_id = query_param(query.as_deref(), "company_id");
    let asset_id = query_param(query.as_deref(), "asset_id");
    answer(assets::list_revaluations(
        graph.as_ref(),
        &scope,
        company_id.as_deref(),
        asset_id.as_deref(),
    ))
}

async fn cumulative_revaluation(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(asset_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::cumulative_revaluation(
        graph.as_ref(),
        &scope,
        &asset_id,
    ))
}

async fn revaluation_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(assets::revaluation_summary(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

// --- absorption-costing-statement: the trading account and the production cost statement ---

async fn generate_statement_trading_account(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(statement::generate_trading_statement(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn add_statement_expenses(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(statement_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(statement::add_statement_expenses(
        graph.as_ref(),
        &scope,
        &statement_id,
        &body_or_query(query.as_deref()),
    ))
}

async fn generate_production_cost_statement(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(statement::generate_production_statement(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn statement_trading_accounts(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(statement::list_trading_statements(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn one_statement_trading_account(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(statement_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(statement::get_trading_statement(
        graph.as_ref(),
        &scope,
        &statement_id,
    ))
}

async fn statement_production_costs(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(statement::list_production_statements(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

// --- activity-based budget: activities, budgets and their approval ---

async fn define_activity(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(activity_budget::define_activity(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn activities(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(activity_budget::list_activities(graph.as_ref(), &scope))
}

async fn create_activity_budget(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(activity_budget::create_budget(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn activity_budgets(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(activity_budget::list_budgets(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn one_activity_budget(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(budget_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(activity_budget::get_budget(
        graph.as_ref(),
        &scope,
        &budget_id,
    ))
}

async fn approve_activity_budget(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(budget_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(activity_budget::approve_budget(
        graph.as_ref(),
        &scope,
        &budget_id,
    ))
}

// --- revaluation model: a revaluation and the four questions that follow one ---

fn stateless(result: Result<Value, String>) -> Response {
    match result {
        Ok(answer) => Json(answer).into_response(),
        Err(message) => (StatusCode::BAD_REQUEST, Json(json!({"detail": message}))).into_response(),
    }
}

async fn perform_revaluation(Json(payload): Json<Value>) -> Response {
    stateless(assets::revalue_from_json(&payload))
}

async fn revaluation_surplus(RawQuery(query): RawQuery) -> Response {
    stateless(assets::surplus_from_json(&body_or_query(query.as_deref())))
}

async fn depreciation_after_revaluation(RawQuery(query): RawQuery) -> Response {
    stateless(assets::depreciation_after_from_json(&body_or_query(
        query.as_deref(),
    )))
}

async fn impairment_after_revaluation(RawQuery(query): RawQuery) -> Response {
    stateless(assets::impairment_after_from_json(&body_or_query(
        query.as_deref(),
    )))
}

async fn indexation_allowance(RawQuery(query): RawQuery) -> Response {
    stateless(assets::indexation_from_json(&body_or_query(
        query.as_deref(),
    )))
}

// --- partnerships: agreements, changes, dissolution and revaluation ---

async fn create_partnership_agreement(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(agreement::create_agreement(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn get_partnership_agreement(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(agreement_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(agreement::get_agreement(
        graph.as_ref(),
        &scope,
        &agreement_id,
    ))
}

async fn list_partnership_agreements(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(agreement::list_agreements(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn update_partnership_agreement(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(agreement_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(agreement::update_agreement(
        graph.as_ref(),
        &scope,
        &agreement_id,
        &payload,
    ))
}

async fn add_partnership_partner(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(agreement_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(agreement::add_partner(
        graph.as_ref(),
        &scope,
        &agreement_id,
        &payload,
    ))
}

async fn partnership_agreement_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(agreement_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(agreement::agreement_summary(
        graph.as_ref(),
        &scope,
        &agreement_id,
    ))
}

async fn admit_partner(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(changes::admit(graph.as_ref(), &scope, &payload))
}

async fn retire_partner(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(changes::retire(graph.as_ref(), &scope, &payload))
}

async fn record_partner_death(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(changes::record_death(graph.as_ref(), &scope, &payload))
}

async fn partnership_changes(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(changes::list_changes(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn settle_partner_change(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(change_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(changes::settle_change(graph.as_ref(), &scope, &change_id))
}

async fn dissolve_partnership(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(dissolution::dissolve(graph.as_ref(), &scope, &payload))
}

async fn partnership_dissolutions(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(dissolution::list_dissolutions(graph.as_ref(), &scope))
}

async fn partnership_dissolution(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(dissolution_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(dissolution::get_dissolution(
        graph.as_ref(),
        &scope,
        &dissolution_id,
    ))
}

async fn revalue_partnership(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(partnership_revaluation::revalue(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn partnership_revaluations(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(partnership_revaluation::list_revaluations(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn partnership_revaluation(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(revaluation_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(partnership_revaluation::get_revaluation(
        graph.as_ref(),
        &scope,
        &revaluation_id,
    ))
}

pub fn router(graph: GraphHandle) -> Router {
    records_router(graph.clone())
        .merge(service_root_router(graph.clone()))
        .merge(crate::accounting_standards::router(graph.clone()))
        .merge(crate::accounting_book::router(graph))
}

fn records_router(graph: GraphHandle) -> Router {
    Router::new()
        .route(
            "/records/absorption-costing/product-costs",
            get(product_costs).post(keep_product_cost),
        )
        .route(
            "/records/absorption-costing/product-costs/:product_id/latest",
            get(latest_product_cost),
        )
        .route(
            "/records/absorption-costing/stock-valuation",
            get(stock_valuation),
        )
        .route("/records/job-costing/jobs", post(open_job))
        .route("/records/job-costing/jobs/:company_id", get(company_jobs))
        .route(
            "/records/job-costing/jobs/:company_id/profitability",
            get(company_profitability),
        )
        .route("/records/job-costing/jobs/:job_id/costs", post(charge_cost))
        .route(
            "/records/fixed-assets-register/assets",
            get(fixed_assets).post(register_asset),
        )
        .route(
            "/records/fixed-assets-register/assets/:asset_id",
            get(one_fixed_asset),
        )
        .route(
            "/records/fixed-assets-register/assets/:asset_id/depreciate",
            post(depreciate_asset),
        )
        .route(
            "/records/fixed-assets-register/assets/:asset_id/depreciation",
            get(asset_depreciation),
        )
        .route(
            "/records/fixed-assets-register/assets/:asset_id/dispose",
            post(dispose_fixed_asset),
        )
        .route(
            "/records/fixed-assets-register/summary",
            get(fixed_asset_summary),
        )
        .route(
            "/records/fixed-assets-schedule/assets",
            get(schedule_asset_list).post(register_asset),
        )
        .route(
            "/records/fixed-assets-schedule/assets/:asset_id",
            get(one_fixed_asset).put(patch_fixed_asset),
        )
        .route(
            "/records/fixed-assets-schedule/schedule/generate",
            post(asset_schedule),
        )
        .route(
            "/records/fixed-assets-schedule/schedule/latest",
            get(asset_schedule),
        )
        .route(
            "/records/fixed-assets-schedule/summary",
            get(schedule_summary),
        )
        .route(
            "/records/share-premium/entries/record",
            post(record_share_premium),
        )
        .route("/records/share-premium/entries", get(share_premium_entries))
        .route(
            "/records/share-premium/utilizations/record",
            post(utilize_share_premium),
        )
        .route(
            "/records/share-premium/utilizations",
            get(share_premium_utilizations),
        )
        .route(
            "/records/share-premium/adjustments/create",
            post(adjust_share_premium),
        )
        .route(
            "/records/share-premium/summary/:company_id",
            get(share_premium_summary),
        )
        .route(
            "/records/capital-redemption-reserve/redemptions/record",
            post(record_redemption),
        )
        .route(
            "/records/capital-redemption-reserve/redemptions",
            get(list_redemptions),
        )
        .route(
            "/records/capital-redemption-reserve/creations/create",
            post(create_crr),
        )
        .route(
            "/records/capital-redemption-reserve/creations",
            get(list_crr_creations),
        )
        .route(
            "/records/capital-redemption-reserve/utilizations/record",
            post(utilize_crr),
        )
        .route(
            "/records/capital-redemption-reserve/utilizations",
            get(list_crr_utilizations),
        )
        .route(
            "/records/capital-redemption-reserve/summary/:company_id",
            get(crr_summary),
        )
        .route(
            "/records/general-reserve/reserves",
            get(reserves).post(create_reserve),
        )
        .route(
            "/records/general-reserve/reserves/:reserve_id",
            get(one_reserve),
        )
        .route(
            "/records/general-reserve/reserves/:reserve_id/allocate",
            post(allocate_reserve),
        )
        .route(
            "/records/general-reserve/reserves/:reserve_id/utilize",
            post(utilize_reserve),
        )
        .route(
            "/records/general-reserve/reserves/:reserve_id/history",
            get(reserve_history),
        )
        .route(
            "/records/retained-profits/appropriations",
            get(profit_appropriations).post(create_profit_appropriation),
        )
        .route(
            "/records/retained-profits/interim-dividends",
            get(interim_dividends).post(declare_interim_dividend),
        )
        .route(
            "/records/retained-profits/interim-dividends/:dividend_id/pay",
            post(pay_interim_dividend),
        )
        .route(
            "/records/retained-profits/adjustments",
            get(prior_year_adjustments).post(create_prior_year_adjustment),
        )
        .route(
            "/records/retained-profits/summary/:company_id",
            get(retained_earnings_summary),
        )
        .route("/records/bonus-shares/issue", post(issue_bonus_shares))
        .route("/records/bonus-shares/issues", get(bonus_issues))
        .route(
            "/records/right-issues/right-issues",
            get(right_issues).post(offer_rights),
        )
        .route(
            "/records/right-issues/right-issues/:issue_id",
            get(one_right_issue),
        )
        .route(
            "/records/right-issues/right-issues/:issue_id/accept",
            post(accept_rights),
        )
        .route(
            "/records/right-issues/right-issues/:issue_id/renounce",
            post(renounce_rights),
        )
        .route(
            "/records/ordinary-shares/dividends",
            get(ordinary_dividends).post(declare_ordinary_dividend),
        )
        .route(
            "/records/ordinary-shares/dividends/:dividend_id/pay",
            post(pay_ordinary_dividend),
        )
        .route(
            "/records/preference-shares/classes",
            get(preference_classes).post(create_preference_class),
        )
        .route(
            "/records/preference-shares/classes/:class_id/issue",
            post(issue_preference_shares),
        )
        .route(
            "/records/preference-shares/classes/:class_id/dividends",
            post(declare_preference_dividend),
        )
        .route(
            "/records/preference-shares/dividends/:dividend_id/pay",
            post(pay_preference_dividend),
        )
        .route(
            "/records/preference-shares/classes/:class_id/redeem",
            post(redeem_preference_shares),
        )
        .route(
            "/records/preference-shares/dividends",
            get(preference_dividends),
        )
        .route(
            "/records/authorized-share-capital/share-classes",
            get(share_classes).post(create_share_class),
        )
        .route(
            "/records/authorized-share-capital/share-classes/:class_id",
            get(one_share_class),
        )
        .route(
            "/records/authorized-share-capital/share-classes/:class_id/issue",
            post(issue_class_shares),
        )
        .route(
            "/records/authorized-share-capital/share-classes/:class_id/issuances",
            get(class_issuances),
        )
        .route(
            "/records/authorized-share-capital/share-classes/:class_id/buyback",
            post(buyback_class_shares),
        )
        .route(
            "/records/authorized-share-capital/summary",
            get(share_capital_summary),
        )
        .route(
            "/records/disposal-group/groups",
            get(disposal_groups).post(create_disposal_group),
        )
        .route(
            "/records/disposal-group/groups/:group_id",
            get(one_disposal_group),
        )
        .route(
            "/records/disposal-group/groups/:group_id/assets",
            get(disposal_assets).post(add_disposal_asset),
        )
        .route(
            "/records/disposal-group/groups/:group_id/impairment-test",
            post(disposal_impairment_test),
        )
        .route(
            "/records/disposal-group/groups/:group_id/status",
            put(disposal_group_status),
        )
        .route(
            "/records/lease-termination/terminations",
            get(lease_terminations).post(create_lease_termination),
        )
        .route(
            "/records/lease-termination/terminations/:termination_id",
            get(one_lease_termination),
        )
        .route(
            "/records/lease-termination/terminations/:termination_id/settlement",
            post(lease_settlement),
        )
        .route(
            "/records/lease-termination/terminations/:termination_id/approve",
            put(approve_lease_termination),
        )
        .route(
            "/records/lease-termination/terminations/:termination_id/complete",
            put(complete_lease_termination),
        )
        .route(
            "/records/intangible-assets/assets",
            get(intangibles).post(create_intangible),
        )
        .route(
            "/records/intangible-assets/assets/:asset_id",
            get(one_intangible),
        )
        .route(
            "/records/intangible-assets/assets/:asset_id/amortize",
            post(amortize_intangible),
        )
        .route(
            "/records/intangible-assets/assets/:asset_id/amortization",
            get(intangible_amortization),
        )
        .route(
            "/records/revaluation-reserve/revaluations/record",
            post(record_asset_revaluation),
        )
        .route(
            "/records/revaluation-reserve/utilizations/record",
            post(record_reserve_utilization),
        )
        .route(
            "/records/revaluation-reserve/revaluations",
            get(revaluations),
        )
        .route(
            "/records/revaluation-reserve/cumulative/:asset_id",
            get(cumulative_revaluation),
        )
        .route(
            "/records/revaluation-reserve/summary/:company_id",
            get(revaluation_summary),
        )
        .route("/records/insurance-claims/file", post(file_claim))
        .route("/records/insurance-claims/claims", get(insurance_claims))
        .route(
            "/records/insurance-claims/claims/:claim_id/process",
            post(process_insurance_claim),
        )
        .route(
            "/records/treasury-analytics/analyze",
            post(analyze_treasury),
        )
        .route(
            "/records/treasury-analytics/kpi/:company_id",
            get(treasury_kpi),
        )
        .route(
            "/records/cash-flow-statement/generate",
            post(generate_cash_flow_statement),
        )
        .route(
            "/records/cash-flow-statement/latest/:company_id",
            get(latest_cash_flow_statement),
        )
        .route(
            "/records/cash-flow-statement/history/:company_id",
            get(cash_flow_statement_history),
        )
        .route(
            "/records/equity-changes/transactions",
            post(record_equity_transaction),
        )
        .route(
            "/records/equity-changes/transactions/:company_id",
            get(company_equity_transactions),
        )
        .route(
            "/records/equity-changes/statement",
            post(generate_equity_statement),
        )
        .route(
            "/records/equity-changes/statements/:company_id",
            get(company_equity_statements),
        )
        .route(
            "/treasury-risk/exposures",
            get(risk_exposures).post(open_risk_exposure),
        )
        .route(
            "/treasury-risk/var",
            get(risk_var_results).post(calculate_risk_var),
        )
        .route(
            "/treasury-risk/scenarios",
            get(stress_scenarios).post(open_stress_scenario),
        )
        .route(
            "/treasury-risk/scenarios/:scenario_id/run",
            post(run_stress),
        )
        .route("/treasury-risk/stress-results", get(stress_results))
        .route("/treasury-risk/dashboard", get(risk_dashboard))
        .route(
            "/records/treasury-risk/exposures",
            get(risk_exposures).post(open_risk_exposure),
        )
        .route(
            "/records/treasury-risk/var",
            get(risk_var_results).post(calculate_risk_var),
        )
        .route(
            "/records/treasury-risk/scenarios",
            get(stress_scenarios).post(open_stress_scenario),
        )
        .route(
            "/records/treasury-risk/scenarios/:scenario_id/run",
            post(run_stress),
        )
        .route("/records/treasury-risk/stress-results", get(stress_results))
        .route("/records/treasury-risk/dashboard", get(risk_dashboard))
        .route(
            "/treasury-policy/policies",
            get(treasury_policies).post(open_treasury_policy),
        )
        .route(
            "/treasury-policy/policies/:policy_id/limits",
            get(policy_limits).post(set_policy_limit),
        )
        .route(
            "/treasury-policy/limits/:limit_id/check",
            post(check_policy_limit),
        )
        .route("/treasury-policy/compliance", get(policy_compliance_checks))
        .route(
            "/records/treasury-policy/policies",
            get(treasury_policies).post(open_treasury_policy),
        )
        .route(
            "/records/treasury-policy/policies/:policy_id/limits",
            get(policy_limits).post(set_policy_limit),
        )
        .route(
            "/records/treasury-policy/limits/:limit_id/check",
            post(check_policy_limit),
        )
        .route(
            "/records/treasury-policy/compliance",
            get(policy_compliance_checks),
        )
        .route(
            "/debt-management/loans",
            get(company_debt_loans).post(open_debt_loan),
        )
        .route(
            "/debt-management/loans/:loan_id/schedule",
            post(debt_loan_schedule),
        )
        .route("/debt-management/summary", get(company_debt_summary))
        .route(
            "/records/debt-management/loans",
            get(company_debt_loans).post(open_debt_loan),
        )
        .route(
            "/records/debt-management/loans/:loan_id/schedule",
            post(debt_loan_schedule),
        )
        .route(
            "/records/debt-management/summary",
            get(company_debt_summary),
        )
        .route(
            "/treasury-compliance/checks/:company_id",
            get(company_compliance_checks),
        )
        .route(
            "/treasury-compliance/checks/:check_id/status",
            put(set_compliance_verdict),
        )
        .route(
            "/treasury-compliance/report/:company_id",
            get(company_compliance_report),
        )
        .route(
            "/records/treasury-compliance/checks/:company_id",
            get(company_compliance_checks),
        )
        .route(
            "/records/treasury-compliance/checks/:check_id/status",
            put(set_compliance_verdict),
        )
        .route(
            "/records/treasury-compliance/report/:company_id",
            get(company_compliance_report),
        )
        .route("/sovereign-treasury/accounts", post(open_sovereign_account))
        .route(
            "/sovereign-treasury/accounts/:country",
            get(sovereign_accounts),
        )
        .route("/sovereign-treasury/debt", post(register_sovereign_debt))
        .route("/sovereign-treasury/debt/:country", get(sovereign_debts))
        .route(
            "/sovereign-treasury/fiscal-position",
            post(set_sovereign_fiscal_position),
        )
        .route(
            "/sovereign-treasury/fiscal-position/:country",
            get(sovereign_fiscal_position),
        )
        .route(
            "/records/sovereign-treasury/accounts",
            post(open_sovereign_account),
        )
        .route(
            "/records/sovereign-treasury/accounts/:country",
            get(sovereign_accounts),
        )
        .route(
            "/records/sovereign-treasury/debt",
            post(register_sovereign_debt),
        )
        .route(
            "/records/sovereign-treasury/debt/:country",
            get(sovereign_debts),
        )
        .route(
            "/records/sovereign-treasury/fiscal-position",
            post(set_sovereign_fiscal_position),
        )
        .route(
            "/records/sovereign-treasury/fiscal-position/:country",
            get(sovereign_fiscal_position),
        )
        .route(
            "/treasury-reporting/reports/cash-position",
            post(generate_cash_position_report),
        )
        .route(
            "/treasury-reporting/reports/fx-exposure",
            post(generate_fx_exposure_report),
        )
        .route(
            "/treasury-reporting/reports/debt-portfolio",
            post(generate_debt_portfolio_report),
        )
        .route("/treasury-reporting/reports", get(treasury_reports))
        .route(
            "/treasury-reporting/reports/:report_id",
            get(one_treasury_report),
        )
        .route(
            "/records/treasury-reporting/reports/cash-position",
            post(generate_cash_position_report),
        )
        .route(
            "/records/treasury-reporting/reports/fx-exposure",
            post(generate_fx_exposure_report),
        )
        .route(
            "/records/treasury-reporting/reports/debt-portfolio",
            post(generate_debt_portfolio_report),
        )
        .route("/records/treasury-reporting/reports", get(treasury_reports))
        .route(
            "/records/treasury-reporting/reports/:report_id",
            get(one_treasury_report),
        )
        .route("/treasury-management/cashflows", post(record_treasury_flow))
        .route(
            "/treasury-management/cashflows/:company_id",
            get(company_treasury_flows),
        )
        .route(
            "/treasury-management/position/:company_id",
            get(company_cash_position).put(set_company_cash_position),
        )
        .route(
            "/treasury-management/forecast/:company_id",
            post(company_cash_forecast),
        )
        .route(
            "/treasury-management/investment-options",
            get(treasury_investment_options),
        )
        .route(
            "/records/treasury-management/cashflows",
            post(record_treasury_flow),
        )
        .route(
            "/records/treasury-management/cashflows/:company_id",
            get(company_treasury_flows),
        )
        .route(
            "/records/treasury-management/position/:company_id",
            get(company_cash_position).put(set_company_cash_position),
        )
        .route(
            "/records/treasury-management/forecast/:company_id",
            post(company_cash_forecast),
        )
        .route(
            "/records/treasury-management/investment-options",
            get(treasury_investment_options),
        )
        .route(
            "/cash-management/accounts",
            get(cash_accounts).post(open_cash_account),
        )
        .route(
            "/cash-management/transfers",
            get(cash_transfers).post(move_cash),
        )
        .route(
            "/cash-management/liquidity",
            get(liquidity_history).post(measure_liquidity),
        )
        .route("/foreign-exchange/rates", get(fx_rates))
        .route("/foreign-exchange/rates/:currency", get(fx_rate))
        .route("/foreign-exchange/convert", post(fx_convert))
        .route("/foreign-exchange/hedge", post(fx_hedge))
        .route(
            "/petty-cash/funds",
            get(petty_cash_funds).post(open_petty_cash_fund),
        )
        .route(
            "/petty-cash/funds/:fund_id",
            get(petty_cash_fund).put(change_petty_cash_fund),
        )
        .route(
            "/petty-cash/funds/:fund_id/close",
            post(close_petty_cash_fund),
        )
        .route(
            "/petty-cash/funds/:fund_id/balance",
            get(petty_cash_fund_balance),
        )
        .route(
            "/petty-cash/funds/:fund_id/summary",
            get(petty_cash_fund_summary),
        )
        .route(
            "/petty-cash/transactions",
            get(petty_cash_transactions).post(add_petty_cash_transaction),
        )
        .route(
            "/petty-cash/transactions/:transaction_id",
            get(petty_cash_transaction),
        )
        .route(
            "/petty-cash/vouchers",
            get(petty_cash_vouchers).post(add_petty_cash_voucher),
        )
        .route(
            "/petty-cash/vouchers/:voucher_id/approve",
            put(approve_petty_cash_voucher),
        )
        .route(
            "/petty-cash/replenishments",
            get(list_replenishment_requests).post(add_replenishment),
        )
        .route(
            "/petty-cash/replenishments/:replenishment_id/approve",
            put(approve_replenishment_request),
        )
        .route(
            "/petty-cash/reports/fund-report/:fund_id",
            get(petty_cash_fund_report),
        )
        .route(
            "/petty-cash/reports/cash-position",
            get(petty_cash_position),
        )
        .route(
            "/petty-cash/reports/category-summary",
            get(petty_cash_category_summary),
        )
        .route(
            "/investment-monitoring/portfolios",
            get(portfolios).post(open_portfolio),
        )
        .route(
            "/investment-monitoring/portfolios/:portfolio_id/holdings",
            get(portfolio_holdings).post(add_portfolio_holding),
        )
        .route(
            "/investment-monitoring/portfolios/:portfolio_id/performance",
            get(portfolio_performance).post(measure_portfolio),
        )
        .route(
            "/investment-monitoring/holdings/:holding_id/update-price",
            post(reprice_holding),
        )
        .route(
            "/trade-finance/instruments",
            get(trade_instruments).post(open_trade_instrument),
        )
        .route(
            "/trade-finance/instruments/:instrument_id/present",
            post(present_trade_documents),
        )
        .route(
            "/trade-finance/instruments/:instrument_id/settle",
            post(settle_trade_instrument),
        )
        .route(
            "/records/investment-monitoring/portfolios",
            get(portfolios).post(open_portfolio),
        )
        .route(
            "/records/investment-monitoring/portfolios/:portfolio_id/holdings",
            get(portfolio_holdings).post(add_portfolio_holding),
        )
        .route(
            "/records/investment-monitoring/portfolios/:portfolio_id/performance",
            get(portfolio_performance).post(measure_portfolio),
        )
        .route(
            "/records/investment-monitoring/holdings/:holding_id/update-price",
            post(reprice_holding),
        )
        .route(
            "/records/trade-finance/instruments",
            get(trade_instruments).post(open_trade_instrument),
        )
        .route(
            "/records/trade-finance/instruments/:instrument_id/present",
            post(present_trade_documents),
        )
        .route(
            "/records/trade-finance/instruments/:instrument_id/settle",
            post(settle_trade_instrument),
        )
        .route(
            "/personal-finance/recurring",
            get(recurring_items).post(open_recurring),
        )
        .route(
            "/personal-finance/recurring/:recurring_id",
            delete(stop_recurring),
        )
        .route(
            "/personal-finance/recurring/:recurring_id/run",
            post(run_recurring_now),
        )
        .route("/personal-finance/debts", get(debts).post(open_debt))
        .route("/personal-finance/debts/:debt_id", get(one_debt))
        .route("/personal-finance/debts/:debt_id/payments", post(pay_debt))
        .route(
            "/personal-finance/debts/:debt_id/schedule",
            get(debt_projection),
        )
        .route(
            "/personal-finance/investments",
            get(investments).post(open_investment),
        )
        .route(
            "/personal-finance/investments/:investment_id/trades",
            post(trade_investment),
        )
        .route(
            "/personal-finance/investments/:investment_id/price",
            post(reprice_investment),
        )
        .route(
            "/personal-finance/tax/brackets",
            get(tax_bands).put(replace_tax_bands),
        )
        .route("/personal-finance/tax/estimate", post(estimate_tax))
        .route("/personal-finance/goals", get(goals).post(open_goal))
        .route(
            "/personal-finance/income",
            get(income_sources).post(add_income_source),
        )
        .route("/personal-finance/legacy-debts", post(add_legacy_debt))
        .route("/personal-finance/overview", get(personal_overview))
        .route(
            "/records/personal-finance/recurring",
            get(recurring_items).post(open_recurring),
        )
        .route(
            "/records/personal-finance/recurring/:recurring_id",
            delete(stop_recurring),
        )
        .route(
            "/records/personal-finance/recurring/:recurring_id/run",
            post(run_recurring_now),
        )
        .route(
            "/records/personal-finance/debts",
            get(debts).post(open_debt),
        )
        .route("/records/personal-finance/debts/:debt_id", get(one_debt))
        .route(
            "/records/personal-finance/debts/:debt_id/payments",
            post(pay_debt),
        )
        .route(
            "/records/personal-finance/debts/:debt_id/schedule",
            get(debt_projection),
        )
        .route(
            "/records/personal-finance/investments",
            get(investments).post(open_investment),
        )
        .route(
            "/records/personal-finance/investments/:investment_id/trades",
            post(trade_investment),
        )
        .route(
            "/records/personal-finance/investments/:investment_id/price",
            post(reprice_investment),
        )
        .route(
            "/records/personal-finance/tax/brackets",
            get(tax_bands).put(replace_tax_bands),
        )
        .route("/records/personal-finance/tax/estimate", post(estimate_tax))
        .route(
            "/records/personal-finance/goals",
            get(goals).post(open_goal),
        )
        .route(
            "/records/personal-finance/income",
            get(income_sources).post(add_income_source),
        )
        .route(
            "/records/personal-finance/legacy-debts",
            post(add_legacy_debt),
        )
        .route("/records/personal-finance/overview", get(personal_overview))
        .route("/budgeting/prepare", post(budget_prepare))
        .route("/budgeting/budget", post(budget_create))
        .route("/budgeting/monitor", post(budget_monitor))
        .route("/budgeting/analyze", post(budget_analyze))
        .route("/budgeting/variance-analysis", post(budget_variance))
        .route("/budgeting/forecast", post(budget_forecast))
        .route("/budgeting/rolling-forecast", post(budget_rolling_forecast))
        .route("/zero-based-budgeting/packages", post(open_budget_package))
        .route(
            "/zero-based-budgeting/packages/:company_id",
            get(budget_packages),
        )
        .route(
            "/zero-based-budgeting/packages/:package_id/items",
            post(add_budget_item),
        )
        .route(
            "/zero-based-budgeting/packages/:package_id/status",
            put(budget_package_status),
        )
        .route(
            "/zero-based-budgeting/items/:item_id/priority",
            put(budget_item_priority),
        )
        .route(
            "/zero-based-budgeting/summary/:company_id",
            get(budget_summary),
        )
        .route("/records/budgeting/prepare", post(budget_prepare))
        .route("/records/budgeting/budget", post(budget_create))
        .route("/records/budgeting/monitor", post(budget_monitor))
        .route("/records/budgeting/analyze", post(budget_analyze))
        .route(
            "/records/budgeting/variance-analysis",
            post(budget_variance),
        )
        .route("/records/budgeting/forecast", post(budget_forecast))
        .route(
            "/records/budgeting/rolling-forecast",
            post(budget_rolling_forecast),
        )
        .route(
            "/records/zero-based-budgeting/packages",
            post(open_budget_package),
        )
        .route(
            "/records/zero-based-budgeting/packages/:company_id",
            get(budget_packages),
        )
        .route(
            "/records/zero-based-budgeting/packages/:package_id/items",
            post(add_budget_item),
        )
        .route(
            "/records/zero-based-budgeting/packages/:package_id/status",
            put(budget_package_status),
        )
        .route(
            "/records/zero-based-budgeting/items/:item_id/priority",
            put(budget_item_priority),
        )
        .route(
            "/records/zero-based-budgeting/summary/:company_id",
            get(budget_summary),
        )
        .route("/net-present-value/calculate", post(npv_calculate))
        .route(
            "/net-present-value/calculate-percent",
            post(npv_calculate_percent),
        )
        .route("/net-present-value/compare", post(npv_compare))
        .route(
            "/net-present-value/profitability-index",
            post(npv_profitability_index),
        )
        .route("/records/net-present-value/calculate", post(npv_calculate))
        .route(
            "/records/net-present-value/calculate-percent",
            post(npv_calculate_percent),
        )
        .route("/records/net-present-value/compare", post(npv_compare))
        .route(
            "/records/net-present-value/profitability-index",
            post(npv_profitability_index),
        )
        .route("/internal-rate-return/calculate", post(irr_calculate))
        .route(
            "/internal-rate-return/calculate-bisection",
            post(irr_calculate_bisection),
        )
        .route("/internal-rate-return/compare", post(irr_compare))
        .route("/internal-rate-return/decision", post(irr_decision))
        .route(
            "/records/internal-rate-return/calculate",
            post(irr_calculate),
        )
        .route(
            "/records/internal-rate-return/calculate-bisection",
            post(irr_calculate_bisection),
        )
        .route("/records/internal-rate-return/compare", post(irr_compare))
        .route("/records/internal-rate-return/decision", post(irr_decision))
        .route("/payback-period/calculate", post(payback_calculate))
        .route("/payback-period/compare", post(payback_compare))
        .route(
            "/payback-period/average-cash-flow",
            post(payback_average_cash_flow),
        )
        .route("/records/payback-period/calculate", post(payback_calculate))
        .route("/records/payback-period/compare", post(payback_compare))
        .route(
            "/records/payback-period/average-cash-flow",
            post(payback_average_cash_flow),
        )
        .route(
            "/continue-shutdown/analyze",
            post(analyze_continue_shutdown),
        )
        .route(
            "/continue-shutdown/quick-analysis",
            post(quick_continue_shutdown),
        )
        .route(
            "/continue-shutdown/decisions",
            get(continue_shutdown_decisions),
        )
        .route(
            "/continue-shutdown/decisions/:decision_id",
            get(continue_shutdown_decision),
        )
        .route(
            "/records/continue-shutdown/analyze",
            post(analyze_continue_shutdown),
        )
        .route(
            "/records/continue-shutdown/quick-analysis",
            post(quick_continue_shutdown),
        )
        .route(
            "/records/continue-shutdown/decisions",
            get(continue_shutdown_decisions),
        )
        .route(
            "/records/continue-shutdown/decisions/:decision_id",
            get(continue_shutdown_decision),
        )
        .route("/records/foreign-exchange/rates", get(fx_rates))
        .route("/records/foreign-exchange/rates/:currency", get(fx_rate))
        .route("/records/foreign-exchange/convert", post(fx_convert))
        .route("/records/foreign-exchange/hedge", post(fx_hedge))
        .route(
            "/records/petty-cash/funds",
            get(petty_cash_funds).post(open_petty_cash_fund),
        )
        .route(
            "/records/petty-cash/funds/:fund_id",
            get(petty_cash_fund).put(change_petty_cash_fund),
        )
        .route(
            "/records/petty-cash/funds/:fund_id/close",
            post(close_petty_cash_fund),
        )
        .route(
            "/records/petty-cash/funds/:fund_id/balance",
            get(petty_cash_fund_balance),
        )
        .route(
            "/records/petty-cash/funds/:fund_id/summary",
            get(petty_cash_fund_summary),
        )
        .route(
            "/records/petty-cash/transactions",
            get(petty_cash_transactions).post(add_petty_cash_transaction),
        )
        .route(
            "/records/petty-cash/transactions/:transaction_id",
            get(petty_cash_transaction),
        )
        .route(
            "/records/petty-cash/vouchers",
            get(petty_cash_vouchers).post(add_petty_cash_voucher),
        )
        .route(
            "/records/petty-cash/vouchers/:voucher_id/approve",
            put(approve_petty_cash_voucher),
        )
        .route(
            "/records/petty-cash/replenishments",
            get(list_replenishment_requests).post(add_replenishment),
        )
        .route(
            "/records/petty-cash/replenishments/:replenishment_id/approve",
            put(approve_replenishment_request),
        )
        .route(
            "/records/petty-cash/reports/fund-report/:fund_id",
            get(petty_cash_fund_report),
        )
        .route(
            "/records/petty-cash/reports/cash-position",
            get(petty_cash_position),
        )
        .route(
            "/records/petty-cash/reports/category-summary",
            get(petty_cash_category_summary),
        )
        .route(
            "/records/cash-management/accounts",
            get(cash_accounts).post(open_cash_account),
        )
        .route(
            "/records/cash-management/transfers",
            get(cash_transfers).post(move_cash),
        )
        .route(
            "/records/cash-management/liquidity",
            get(liquidity_history).post(measure_liquidity),
        )
        .route(
            "/records/cash-optimization/accounts",
            post(add_cash_account),
        )
        .route(
            "/records/cash-optimization/accounts/:company_id",
            get(company_cash_accounts),
        )
        .route(
            "/records/cash-optimization/optimize/:company_id",
            post(run_cash_optimization),
        )
        .route(
            "/records/cash-optimization/suggestions/:company_id",
            get(company_cash_suggestions),
        )
        .route(
            "/records/trading-account/generate",
            post(generate_trading_account),
        )
        .route(
            "/records/trading-account/accounts",
            get(list_trading_accounts),
        )
        .route(
            "/records/trading-account/accounts/:account_id",
            get(get_trading_account),
        )
        .route(
            "/records/selling-price-reduction/analyze",
            post(price_analysis),
        )
        .route(
            "/records/selling-price-reduction/discount-analysis",
            post(price_discount_analysis),
        )
        .route(
            "/records/selling-price-reduction/analyses",
            get(price_analyses),
        )
        .route("/records/cvp-analysis/analyze", post(cvp_analysis))
        .route(
            "/records/cvp-analysis/quick-analysis",
            post(cvp_quick_analysis),
        )
        .route("/records/cvp-analysis/contribution", post(cvp_contribution))
        .route(
            "/records/cvp-analysis/target-profit",
            post(cvp_target_profit),
        )
        .route(
            "/records/cvp-analysis/multi-product",
            post(cvp_multi_product),
        )
        .route("/records/cvp-analysis/analyses", get(cvp_analyses))
        .route(
            "/records/cvp-analysis/analyses/:analysis_id",
            get(cvp_analysis_by_id),
        )
        .route(
            "/records/overhead-absorption/calculate",
            post(overhead_rate),
        )
        .route(
            "/records/overhead-absorption/machine-hours-rate",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    overhead_base_rate((state, 0), headers, body)
                },
            ),
        )
        .route(
            "/records/overhead-absorption/labour-hours-rate",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    overhead_base_rate((state, 1), headers, body)
                },
            ),
        )
        .route(
            "/records/overhead-absorption/labour-cost-rate",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    overhead_base_rate((state, 2), headers, body)
                },
            ),
        )
        .route(
            "/records/overhead-absorption/material-cost-rate",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    overhead_base_rate((state, 3), headers, body)
                },
            ),
        )
        .route(
            "/records/overhead-absorption/prime-cost-rate",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    overhead_base_rate((state, 4), headers, body)
                },
            ),
        )
        .route(
            "/records/overhead-absorption/unit-rate",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    overhead_base_rate((state, 5), headers, body)
                },
            ),
        )
        .route(
            "/records/overhead-absorption/blanket-rate",
            post(overhead_blanket_rate),
        )
        .route(
            "/records/overhead-absorption/absorbed-overhead",
            post(overhead_absorbed),
        )
        .route("/records/overhead-absorption/rates", get(overhead_rates))
        .route(
            "/records/overhead-absorption/analyses",
            get(overhead_analyses),
        )
        .route(
            "/records/apportionment/rules/create",
            post(apportionment_rule),
        )
        .route("/records/apportionment/rules", get(apportionment_rules))
        .route(
            "/records/apportionment/apportion/floor-area",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    apportionment_by_basis((state, 0), headers, body)
                },
            ),
        )
        .route(
            "/records/apportionment/apportion/volume",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    apportionment_by_basis((state, 1), headers, body)
                },
            ),
        )
        .route(
            "/records/apportionment/apportion/cost",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    apportionment_by_basis((state, 2), headers, body)
                },
            ),
        )
        .route(
            "/records/apportionment/apportion/net-book-value",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    apportionment_by_basis((state, 3), headers, body)
                },
            ),
        )
        .route(
            "/records/apportionment/apportion/replacement-value",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    apportionment_by_basis((state, 4), headers, body)
                },
            ),
        )
        .route(
            "/records/apportionment/apportion/personnel",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    apportionment_by_basis((state, 5), headers, body)
                },
            ),
        )
        .route(
            "/records/apportionment/apportion/requisitions",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    apportionment_by_basis((state, 6), headers, body)
                },
            ),
        )
        .route("/records/apportionment/results", get(apportionment_results))
        .route("/records/process-costing/calculate", post(cost_calculation))
        .route(
            "/records/process-costing/calculations/:company_id",
            get(company_cost_calculations),
        )
        .route(
            "/records/process-costing/breakdown/:company_id/:calc_id",
            get(cost_breakdown),
        )
        .route(
            "/records/process-costing/summary/:company_id",
            get(cost_summary),
        )
        .route("/records/product-costing/calculate", post(cost_calculation))
        .route(
            "/records/product-costing/calculations/:company_id",
            get(company_cost_calculations),
        )
        .route(
            "/records/product-costing/breakdown/:company_id/:calc_id",
            get(cost_breakdown),
        )
        .route(
            "/records/product-costing/summary/:company_id",
            get(cost_summary),
        )
        .route(
            "/records/appropriation-control/appropriations",
            post(create_appropriation),
        )
        .route(
            "/records/appropriation-control/appropriations/:company_id",
            get(company_appropriations),
        )
        .route(
            "/records/appropriation-control/transactions",
            post(appropriation_transaction),
        )
        .route(
            "/records/appropriation-control/check/:appropriation_id",
            get(appropriation_check),
        )
        .route(
            "/records/forecasts",
            get(rolling_forecasts).post(open_rolling_forecast),
        )
        .route("/records/forecasts/:forecast_id", get(one_rolling_forecast))
        .route(
            "/records/forecasts/:forecast_id/update-actual",
            post(record_forecast_actual),
        )
        .route("/records/forecasts/:forecast_id/roll", post(roll_forecast))
        .route(
            "/records/scenario-analysis/analyze",
            post(analyze_scenarios),
        )
        .route("/records/scenario-analysis/scenarios", post(open_scenario))
        .route(
            "/records/scenario-analysis/scenarios/:company_id",
            get(scenarios_for_company),
        )
        .route(
            "/records/scenario-analysis/compare/:company_id",
            get(compare_scenarios),
        )
        .route(
            "/records/sensitivity-analysis/analyze",
            post(run_sensitivity),
        )
        .route(
            "/records/sensitivity-analysis/analyses/:company_id",
            get(sensitivity_analyses),
        )
        .route(
            "/records/scenario-modeling/scenarios",
            get(model_scenarios).post(open_model_scenario),
        )
        .route(
            "/records/scenario-modeling/scenarios/:scenario_id",
            get(one_model_scenario)
                .put(change_model_scenario)
                .delete(drop_model_scenario),
        )
        .route(
            "/records/scenario-modeling/rules",
            get(model_rules).post(open_rule),
        )
        .route(
            "/records/scenario-modeling/rules/:rule_id",
            get(one_rule).put(change_rule).delete(drop_rule),
        )
        .route("/records/scenario-modeling/what-if", post(run_what_if))
        .route(
            "/records/scenario-modeling/what-if/:analysis_id",
            get(one_what_if),
        )
        .route(
            "/records/scenario-modeling/what-if/scenario/:scenario_id",
            get(what_if_for_scenario),
        )
        .route(
            "/records/scenario-modeling/sensitivity",
            post(run_model_sweep),
        )
        .route(
            "/records/scenario-modeling/compare",
            post(compare_model_scenarios),
        )
        .with_state(graph)
}

// --- receivables: the sales ledger, bad debts and the two provisions ---

async fn post_debtor_transaction(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::sales_ledger::add_debtor_transaction(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn get_control_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::sales_ledger::control_summary(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn get_debtor_balance(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(debtor_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::sales_ledger::debtor_balance(
        graph.as_ref(),
        &scope,
        &debtor_id,
    ))
}

async fn list_sales_ledger_debtors(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::sales_ledger::list_debtors(
        graph.as_ref(),
        &scope,
    ))
}

async fn reconcile_control_account(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::sales_ledger::reconcile(graph.as_ref(), &scope))
}

async fn register_bad_debt(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::bad_debts::register_debt(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn recover_bad_debt(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(debt_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::bad_debts::record_recovery(
        graph.as_ref(),
        &scope,
        &debt_id,
        &payload,
    ))
}

async fn list_bad_debts(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::bad_debts::list_bad_debts(
        graph.as_ref(),
        &scope,
    ))
}

async fn get_bad_debt(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(debt_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::bad_debts::get_bad_debt(
        graph.as_ref(),
        &scope,
        &debt_id,
    ))
}

async fn get_recovery_summary(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::bad_debts::recovery_summary(
        graph.as_ref(),
        &scope,
    ))
}

async fn create_provision_policy(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::provision::create_provision_policy(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn list_provision_policies(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::provision::list_provision_policies(
        graph.as_ref(),
        &scope,
    ))
}

async fn add_aged_debtor(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::provision::add_debtor(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn calculate_aged_provision(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::provision::calculate_provision(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn create_aged_provision_journal(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::provision::create_journal(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn list_aged_provision_history(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::provision::provision_history(
        graph.as_ref(),
        &scope,
    ))
}

async fn add_doubtful_debtor(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::doubtful::add_debtor(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn assess_doubtful_debtor(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(debtor_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::doubtful::assess_debtor(
        graph.as_ref(),
        &scope,
        &debtor_id,
        &payload,
    ))
}

async fn calculate_doubtful_provision(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::doubtful::calculate_provision(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn create_doubtful_provision(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::doubtful::create_provision(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn list_doubtful_provisions(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::doubtful::list_provisions(
        graph.as_ref(),
        &scope,
    ))
}

async fn list_doubtful_debtors(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(receivables::doubtful::list_debtors(graph.as_ref(), &scope))
}

// --- the ledger kernel: double entry rules, control account breaks, integrity checks, documents ---

async fn validate_journal_payload(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let _ = (&graph, &headers);
    answer(ledger::double_entry::validate_entry(&payload))
}

async fn calculate_debit_credit(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let _ = (&graph, &headers);
    answer(ledger::double_entry::calculate_entry(
        &crate::merge_request(query.as_deref(), None),
    ))
}

async fn account_rules(Path(account_type): Path<String>) -> Response {
    answer(ledger::double_entry::account_rules(&account_type))
}

async fn account_type_catalogue() -> Response {
    answer(Ok(ledger::double_entry::account_types()))
}

/// The shipped route was a POST that took no body at all: the entry type is the path and the template is
/// the answer, so a POST with an empty body still gets the template.
async fn journal_entry_template(Path(entry_type): Path<String>) -> Response {
    answer(ledger::double_entry::journal_template(&entry_type))
}

async fn analyze_transaction_description(
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let _ = &headers;
    answer(ledger::double_entry::analyze_transaction(
        &crate::merge_request(query.as_deref(), None),
    ))
}

async fn accounting_equation() -> Response {
    answer(Ok(ledger::double_entry::accounting_equation()))
}

async fn report_reconciliation_error(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(ledger::control_account::report_error(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn list_reconciliation_errors(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let status = query_param(query.as_deref(), "status").unwrap_or_default();
    let error_type = query_param(query.as_deref(), "error_type").unwrap_or_default();
    answer(ledger::control_account::list_errors(
        graph.as_ref(),
        &scope,
        &status,
        &error_type,
    ))
}

async fn resolve_reconciliation_error(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(error_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(ledger::control_account::resolve_error(
        graph.as_ref(),
        &scope,
        &error_id,
        &body,
    ))
}

async fn reconcile_ledger_control_account(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(account_type): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(ledger::control_account::reconcile_control_account(
        graph.as_ref(),
        &scope,
        &account_type,
        &body,
    ))
}

async fn list_reconciliation_reports(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let limit = query_param(query.as_deref(), "limit")
        .and_then(|text| text.parse::<usize>().ok())
        .unwrap_or(10);
    answer(ledger::control_account::list_reports(
        graph.as_ref(),
        &scope,
        limit,
    ))
}

async fn reconciliation_summary(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(ledger::control_account::reconciliation_summary(
        graph.as_ref(),
        &scope,
    ))
}

async fn integrity_balance_check(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(ledger::integrity::check_balance(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn integrity_hash_check(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(ledger::integrity::check_hash(graph.as_ref(), &scope, &body))
}

async fn integrity_completeness_check(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(ledger::integrity::check_completeness(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn integrity_report(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(ledger::integrity::integrity_report(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn create_financial_document(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(ledger::state_machine::create_document(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn get_financial_document(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(doc_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(ledger::state_machine::get_document(
        graph.as_ref(),
        &scope,
        &doc_id,
    ))
}

async fn transition_financial_document(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(doc_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(ledger::state_machine::transition(
        graph.as_ref(),
        &scope,
        &doc_id,
        &body,
    ))
}

async fn financial_document_history(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(doc_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(ledger::state_machine::history(
        graph.as_ref(),
        &scope,
        &doc_id,
    ))
}

async fn document_state_catalogue() -> Response {
    answer(Ok(ledger::state_machine::catalogue()))
}

pub fn service_root_router(graph: GraphHandle) -> Router {
    Router::new()
        .route("/absorption-costing/product-costs", get(product_costs).post(keep_product_cost))

        // the ledger kernel: double entry rules (nothing stored)
        .route("/validate-journal-entry", post(validate_journal_payload))
        .route("/calculate-entry", post(calculate_debit_credit))
        .route("/account-rules/:account_type", get(account_rules))
        .route("/account-types", get(account_type_catalogue))
        .route("/journal-entries/:entry_type", post(journal_entry_template))
        .route("/transaction-analyzer", post(analyze_transaction_description))
        .route("/accounting-equation", get(accounting_equation))
        // the ledger kernel: control account breaks and their reconciliation reports
        .route("/errors/report", post(report_reconciliation_error))
        .route("/errors", get(list_reconciliation_errors))
        .route("/errors/:error_id/resolve", post(resolve_reconciliation_error))
        .route(
            "/reconcile/:account_type",
            post(reconcile_ledger_control_account),
        )
        .route("/reports", get(list_reconciliation_reports))
        .route("/summary", get(reconciliation_summary))
        // the ledger kernel: the three integrity checks and the report they add up to
        .route("/check/balance", post(integrity_balance_check))
        .route("/check/hash", post(integrity_hash_check))
        .route("/check/completeness", post(integrity_completeness_check))
        .route("/report/:company_id", get(integrity_report))
        // the ledger kernel: the document state machine
        .route("/documents", post(create_financial_document))
        .route("/documents/:doc_id", get(get_financial_document))
        .route("/documents/:doc_id/transition", post(transition_financial_document))
        .route("/documents/:doc_id/history", get(financial_document_history))
        .route("/states", get(document_state_catalogue))

        // receivables: the sales ledger and its control account
        .route("/transactions", post(post_debtor_transaction))
        .route("/control-account/summary", get(get_control_summary))
        .route("/debtors/:debtor_id/balance", get(get_debtor_balance))
        .route("/reconcile", post(reconcile_control_account))
        // receivables: bad debts written off and recovered
        .route("/bad-debts/register", post(register_bad_debt))
        .route("/bad-debts/:debt_id/recover", post(recover_bad_debt))
        .route("/bad-debts/:debt_id", get(get_bad_debt))
        .route("/bad-debts", get(list_bad_debts))
        .route("/recovery-summary", get(get_recovery_summary))
        // receivables: the provision aged from a rule ladder
        .route("/policies", get(list_provision_policies).post(create_provision_policy))
        .route("/debtors", get(list_sales_ledger_debtors))
        .route("/journal/create", post(create_aged_provision_journal))
        .route("/provision-history", get(list_aged_provision_history))
        // receivables: provision-bad-debts, whose paths differ from the doubtful service only by prefix
        .route("/provision-bad-debts/debtors/add", post(add_aged_debtor))
        .route("/provision-bad-debts/calculate-provision", post(calculate_aged_provision))
        // receivables: the provision graded per debtor
        .route("/doubtful/debtors/add", post(add_doubtful_debtor))
        .route("/doubtful/debtors/:debtor_id/assess", post(assess_doubtful_debtor))
        .route("/doubtful/debtors", get(list_doubtful_debtors))
        .route("/doubtful/calculate-provision", post(calculate_doubtful_provision))
        .route("/doubtful/provision/create", post(create_doubtful_provision))
        .route("/doubtful/provisions", get(list_doubtful_provisions))
        .route("/absorption-costing/product-costs/:product_id/latest", get(latest_product_cost))
        .route("/absorption-costing/stock-valuation", get(stock_valuation))
        .route("/job-costing/jobs", post(open_job))
        .route("/job-costing/jobs/:company_id", get(company_jobs))
        .route("/job-costing/jobs/:company_id/profitability", get(company_profitability))
        .route("/job-costing/jobs/:job_id/costs", post(charge_cost))
        .route("/fixed-assets-register/assets", get(fixed_assets).post(register_asset))
        .route("/fixed-assets-register/assets/:asset_id", get(one_fixed_asset))
        .route("/fixed-assets-register/assets/:asset_id/depreciate", post(depreciate_asset))
        .route("/fixed-assets-register/assets/:asset_id/depreciation", get(asset_depreciation))
        .route("/fixed-assets-register/assets/:asset_id/dispose", post(dispose_fixed_asset))
        .route("/fixed-assets-register/summary", get(fixed_asset_summary))
        .route("/fixed-assets-schedule/assets", get(schedule_asset_list).post(register_asset))
        .route("/fixed-assets-schedule/assets/:asset_id", get(one_fixed_asset).put(patch_fixed_asset))
        .route("/fixed-assets-schedule/schedule/generate", post(asset_schedule))
        .route("/fixed-assets-schedule/schedule/latest", get(asset_schedule))
        .route("/fixed-assets-schedule/summary", get(schedule_summary))
        .route("/departmental-accounting/departments", get(list_departments).post(create_department))
        .route(
            "/departmental-accounting/departments/:department_id",
            get(get_department).put(update_department),
        )
        .route(
            "/departmental-accounting/departments/:department_id/hierarchy",
            get(department_hierarchy),
        )
        .route(
            "/departmental-accounting/departments/:department_id/financials",
            get(department_financials),
        )
        .route(
            "/departmental-accounting/departments/:department_id/performance-report",
            get(performance_report),
        )
        .route(
            "/departmental-accounting/allocation-rules",
            get(list_allocation_rules).post(create_allocation_rule),
        )
        .route("/departmental-accounting/allocation-rules/:rule_id", delete(deactivate_allocation_rule))
        .route("/departmental-accounting/cost-pools", get(list_cost_pools).post(create_cost_pool))
        .route("/departmental-accounting/cost-pools/:pool_id", get(get_cost_pool))
        .route("/departmental-accounting/allocate", post(run_allocation))
        .route("/departmental-accounting/allocations/:pool_id", get(allocation_results))
        .route(
            "/departmental-accounting/inter-department-bills",
            get(list_inter_department_bills).post(create_inter_department_bill),
        )
        .route(
            "/departmental-accounting/inter-department-bills/:bill_id/approve",
            post(approve_inter_department_bill),
        )
        .route(
            "/departmental-accounting/reports/department-comparison",
            get(compare_departments).post(compare_departments),
        )
        .route("/departmental-accounting/reports/cost-distribution", get(cost_distribution))
        .route("/company-accounting/companies", get(list_companies).post(create_company))
        .route("/company-accounting/companies/:company_id", get(get_company).put(update_company))
        .route(
            "/company-accounting/shareholders",
            get(list_company_shareholders).post(register_company_shareholder),
        )
        .route("/company-accounting/share-capital", get(list_share_capital).post(create_share_capital))
        .route(
            "/company-accounting/capital-transactions",
            get(list_capital_transactions).post(record_capital_transaction),
        )
        .route("/company-accounting/dividends", get(list_dividends).post(declare_dividend))
        .route("/company-accounting/dividends/:dividend_id/pay", put(pay_dividend))
        .route("/company-accounting/dividends/:dividend_id/payments", get(dividend_payments))
        .route(
            "/company-accounting/retained-earnings",
            get(retained_earnings).post(record_retained_earnings),
        )
        .route("/company-accounting/reserves", get(company_reserves).post(create_company_reserve))
        .route("/company-accounting/reports/equity-statement/:company_id", get(equity_statement))
        .route(
            "/company-accounting/reports/shareholder-register/:company_id",
            get(shareholder_register),
        )
        .route("/company-accounting/reports/dividend-history/:company_id", get(dividend_history))
        .route("/cashbook/accounts", get(list_cash_accounts).post(create_cash_account))
        .route("/cashbook/accounts/:account_id", get(get_cash_account).put(update_cash_account))
        .route("/cashbook/entries", get(list_cash_entries).post(create_cash_entry))
        .route("/cashbook/entries/:entry_id", get(get_cash_entry))
        .route("/cashbook/entries/:entry_id/post", put(post_cash_entry))
        .route("/cashbook/entries/:entry_id/void", put(void_cash_entry))
        .route("/cashbook/summary/:account_code", get(cashbook_summary))
        .route(
            "/cashbook/reconciliations",
            get(list_cash_reconciliations).post(create_cash_reconciliation),
        )
        .route("/cashbook/reconciliations/:reconciliation_id", get(get_cash_reconciliation))
        .route("/cashbook/reconciliations/:reconciliation_id/complete", put(complete_reconciliation))
        .route(
            "/cashbook/reconciliations/:reconciliation_id/outstanding",
            get(cash_reconciliation_outstanding),
        )
        .route("/cashbook/cash-flow", get(list_cash_flow).post(create_cash_flow_entry))
        .route("/cashbook/cash-flow/summary", get(cash_flow_summary))
        .route("/cashbook/cash-position", get(cash_position))
        .route("/cashbook/cash-position/history", get(cash_position_history))
        .route("/ifrs-reporting/reports", get(list_ifrs_reports).post(create_ifrs_report))
        .route("/ifrs-reporting/reports/:report_id", get(get_ifrs_report))
        .route("/ifrs-reporting/reports/:report_id/notes", get(list_ifrs_notes).post(add_ifrs_note))
        .route("/ifrs-reporting/reports/:report_id/approve", put(approve_ifrs_report))
        .route(
            "/intercompany/entities",
            get(list_intercompany_entities).post(create_intercompany_entity),
        )
        .route(
            "/intercompany/transactions",
            get(list_intercompany_transactions).post(create_intercompany_transaction),
        )
        .route("/intercompany/transactions/match", post(match_intercompany))
        .route("/intercompany/eliminations", get(list_eliminations))
        .route("/cash-flow/create", post(create_cash_flow))
        .route("/cash-flow/inflows", post(cash_flow_inflows))
        .route("/cash-flow/outflows", post(cash_flow_outflows))
        .route("/cash-flow/net-flows", post(cash_flow_net))
        .route("/cash-flow/cumulative-flows", post(cash_flow_cumulative))
        .route("/cash-flow/list", get(list_cash_flows))
        .route("/cash-flow/get/:cash_flow_id", get(get_cash_flow))
        .route("/balance-sheet/generate", post(generate_balance_sheet))
        .route("/balance-sheet/latest/:company_id", get(latest_balance_sheet))
        .route("/balance-sheet/history/:company_id", get(balance_sheet_history))
        .route("/balance-sheet/ratios/:company_id", get(balance_sheet_ratios))
        .route("/fund-accounting/funds", post(create_fund))
        .route("/fund-accounting/funds/:company_id", get(company_funds))
        .route("/fund-accounting/transactions", post(record_fund_movement))
        .route("/fund-accounting/transactions/:fund_id", get(fund_movements))
        .route("/revenue-recognition/contracts", post(create_revenue_contract))
        .route("/revenue-recognition/contracts/:contract_id/recognize", post(recognise_revenue))
        .route("/revenue-recognition/contracts/:company_id", get(company_revenue_contracts))
        .route("/revenue-recognition/summary/:company_id", get(company_revenue_summary))
        .route("/capital-reconstruction/reconstructions/create", post(create_reconstruction))
        .route(
            "/capital-reconstruction/reconstructions/:reconstruction_id/adjustments/add",
            post(add_reconstruction_adjustment),
        )
        .route(
            "/capital-reconstruction/reconstructions/:reconstruction_id/reserve-conversions/add",
            post(add_reconstruction_conversion),
        )
        .route(
            "/capital-reconstruction/reconstructions/:reconstruction_id/approve",
            post(approve_reconstruction),
        )
        .route("/capital-reconstruction/reconstructions", get(capital_reconstructions))
        .route("/capital-reconstruction/reconstructions/:reconstruction_id", get(reconstruction_scheme))
        .route("/capital-reconstruction/summary/:company_id", get(reconstruction_company_summary))
        .route("/debentures/classes/create", post(create_debenture_class))
        .route("/debentures/classes/:class_id/issue", post(issue_debentures))
        .route("/debentures/classes/:class_id/interest/accrue", post(accrue_debenture_interest))
        .route("/debentures/interest/:interest_id/pay", post(pay_debenture_interest))
        .route("/debentures/classes/:class_id/redeem", post(redeem_debentures))
        .route("/debentures/classes", get(debenture_classes))
        .route("/debentures/issues", get(debenture_issues))
        .route("/debentures/interest", get(debenture_interest))
        .route("/issued-share-capital/issue", post(issue_share_capital))
        .route("/issued-share-capital/shareholders/register", post(register_shareholder))
        .route("/issued-share-capital/shareholders/:company_id", get(company_shareholders))
        .route("/issued-share-capital/issues/:company_id", get(company_share_issues))
        .route("/issued-share-capital/summary/:company_id", get(issued_capital_summary))
        .route("/initial-investment/create", post(create_investment))
        .route("/initial-investment/simple", post(simple_investment))
        .route("/initial-investment/with-capital-allowance", post(investment_with_capital_allowance))
        .route("/initial-investment/list", get(list_investments))
        .route("/initial-investment/get/:investment_id", get(get_investment))
        .route("/share-redemption/redemptions/initiate", post(initiate_redemption))
        .route("/share-redemption/redemptions/:redemption_id/fresh-issue", post(record_fresh_issue))
        .route("/share-redemption/redemptions/:redemption_id/complete", post(complete_redemption))
        .route("/share-redemption/redemptions", get(list_share_redemptions))
        .route("/share-redemption/redemptions/:redemption_id", get(share_redemption_detail))
        .route("/share-redemption/crr-requirements", get(crr_requirements))
        .route(
            "/records/departmental-accounting/departments",
            get(list_departments).post(create_department),
        )
        .route(
            "/records/departmental-accounting/departments/:department_id",
            get(get_department).put(update_department),
        )
        .route(
            "/records/departmental-accounting/departments/:department_id/hierarchy",
            get(department_hierarchy),
        )
        .route(
            "/records/departmental-accounting/departments/:department_id/financials",
            get(department_financials),
        )
        .route(
            "/records/departmental-accounting/departments/:department_id/performance-report",
            get(performance_report),
        )
        .route(
            "/records/departmental-accounting/allocation-rules",
            get(list_allocation_rules).post(create_allocation_rule),
        )
        .route(
            "/records/departmental-accounting/allocation-rules/:rule_id",
            delete(deactivate_allocation_rule),
        )
        .route(
            "/records/departmental-accounting/cost-pools",
            get(list_cost_pools).post(create_cost_pool),
        )
        .route("/records/departmental-accounting/cost-pools/:pool_id", get(get_cost_pool))
        .route("/records/departmental-accounting/allocate", post(run_allocation))
        .route("/records/departmental-accounting/allocations/:pool_id", get(allocation_results))
        .route(
            "/records/departmental-accounting/inter-department-bills",
            get(list_inter_department_bills).post(create_inter_department_bill),
        )
        .route(
            "/records/departmental-accounting/inter-department-bills/:bill_id/approve",
            post(approve_inter_department_bill),
        )
        .route(
            "/records/departmental-accounting/reports/department-comparison",
            get(compare_departments).post(compare_departments),
        )
        .route("/records/departmental-accounting/reports/cost-distribution", get(cost_distribution))
        .route("/records/company-accounting/companies", get(list_companies).post(create_company))
        .route(
            "/records/company-accounting/companies/:company_id",
            get(get_company).put(update_company),
        )
        .route(
            "/records/company-accounting/shareholders",
            get(list_company_shareholders).post(register_company_shareholder),
        )
        .route(
            "/records/company-accounting/share-capital",
            get(list_share_capital).post(create_share_capital),
        )
        .route(
            "/records/company-accounting/capital-transactions",
            get(list_capital_transactions).post(record_capital_transaction),
        )
        .route("/records/company-accounting/dividends", get(list_dividends).post(declare_dividend))
        .route("/records/company-accounting/dividends/:dividend_id/pay", put(pay_dividend))
        .route("/records/company-accounting/dividends/:dividend_id/payments", get(dividend_payments))
        .route(
            "/records/company-accounting/retained-earnings",
            get(retained_earnings).post(record_retained_earnings),
        )
        .route(
            "/records/company-accounting/reserves",
            get(company_reserves).post(create_company_reserve),
        )
        .route(
            "/records/company-accounting/reports/equity-statement/:company_id",
            get(equity_statement),
        )
        .route(
            "/records/company-accounting/reports/shareholder-register/:company_id",
            get(shareholder_register),
        )
        .route(
            "/records/company-accounting/reports/dividend-history/:company_id",
            get(dividend_history),
        )
        .route("/records/cashbook/accounts", get(list_cash_accounts).post(create_cash_account))
        .route("/records/cashbook/accounts/:account_id", get(get_cash_account).put(update_cash_account))
        .route("/records/cashbook/entries", get(list_cash_entries).post(create_cash_entry))
        .route("/records/cashbook/entries/:entry_id", get(get_cash_entry))
        .route("/records/cashbook/entries/:entry_id/post", put(post_cash_entry))
        .route("/records/cashbook/entries/:entry_id/void", put(void_cash_entry))
        .route("/records/cashbook/summary/:account_code", get(cashbook_summary))
        .route(
            "/records/cashbook/reconciliations",
            get(list_cash_reconciliations).post(create_cash_reconciliation),
        )
        .route("/records/cashbook/reconciliations/:reconciliation_id", get(get_cash_reconciliation))
        .route(
            "/records/cashbook/reconciliations/:reconciliation_id/complete",
            put(complete_reconciliation),
        )
        .route(
            "/records/cashbook/reconciliations/:reconciliation_id/outstanding",
            get(cash_reconciliation_outstanding),
        )
        .route("/records/cashbook/cash-flow", get(list_cash_flow).post(create_cash_flow_entry))
        .route("/records/cashbook/cash-flow/summary", get(cash_flow_summary))
        .route("/records/cashbook/cash-position", get(cash_position))
        .route("/records/cashbook/cash-position/history", get(cash_position_history))
        .route("/records/ifrs-reporting/reports", get(list_ifrs_reports).post(create_ifrs_report))
        .route("/records/ifrs-reporting/reports/:report_id", get(get_ifrs_report))
        .route(
            "/records/ifrs-reporting/reports/:report_id/notes",
            get(list_ifrs_notes).post(add_ifrs_note),
        )
        .route("/records/ifrs-reporting/reports/:report_id/approve", put(approve_ifrs_report))
        .route(
            "/records/intercompany/entities",
            get(list_intercompany_entities).post(create_intercompany_entity),
        )
        .route(
            "/records/intercompany/transactions",
            get(list_intercompany_transactions).post(create_intercompany_transaction),
        )
        .route("/records/intercompany/transactions/match", post(match_intercompany))
        .route("/records/intercompany/eliminations", get(list_eliminations))
        .route("/records/cash-flow/create", post(create_cash_flow))
        .route("/records/cash-flow/inflows", post(cash_flow_inflows))
        .route("/records/cash-flow/outflows", post(cash_flow_outflows))
        .route("/records/cash-flow/net-flows", post(cash_flow_net))
        .route("/records/cash-flow/cumulative-flows", post(cash_flow_cumulative))
        .route("/records/cash-flow/list", get(list_cash_flows))
        .route("/records/cash-flow/get/:cash_flow_id", get(get_cash_flow))
        .route("/records/balance-sheet/generate", post(generate_balance_sheet))
        .route("/records/balance-sheet/latest/:company_id", get(latest_balance_sheet))
        .route("/records/balance-sheet/history/:company_id", get(balance_sheet_history))
        .route("/records/balance-sheet/ratios/:company_id", get(balance_sheet_ratios))
        .route("/records/fund-accounting/funds", post(create_fund))
        .route("/records/fund-accounting/funds/:company_id", get(company_funds))
        .route("/records/fund-accounting/transactions", post(record_fund_movement))
        .route("/records/fund-accounting/transactions/:fund_id", get(fund_movements))
        .route("/records/revenue-recognition/contracts", post(create_revenue_contract))
        .route("/records/revenue-recognition/contracts/:contract_id/recognize", post(recognise_revenue))
        .route("/records/revenue-recognition/contracts/:company_id", get(company_revenue_contracts))
        .route("/records/revenue-recognition/summary/:company_id", get(company_revenue_summary))
        .route("/records/capital-reconstruction/reconstructions/create", post(create_reconstruction))
        .route(
            "/records/capital-reconstruction/reconstructions/:reconstruction_id/adjustments/add",
            post(add_reconstruction_adjustment),
        )
        .route(
            "/records/capital-reconstruction/reconstructions/:reconstruction_id/reserve-conversions/add",
            post(add_reconstruction_conversion),
        )
        .route(
            "/records/capital-reconstruction/reconstructions/:reconstruction_id/approve",
            post(approve_reconstruction),
        )
        .route("/records/capital-reconstruction/reconstructions", get(capital_reconstructions))
        .route(
            "/records/capital-reconstruction/reconstructions/:reconstruction_id",
            get(reconstruction_scheme),
        )
        .route(
            "/records/capital-reconstruction/summary/:company_id",
            get(reconstruction_company_summary),
        )
        .route("/records/debentures/classes/create", post(create_debenture_class))
        .route("/records/debentures/classes/:class_id/issue", post(issue_debentures))
        .route("/records/debentures/classes/:class_id/interest/accrue", post(accrue_debenture_interest))
        .route("/records/debentures/interest/:interest_id/pay", post(pay_debenture_interest))
        .route("/records/debentures/classes/:class_id/redeem", post(redeem_debentures))
        .route("/records/debentures/classes", get(debenture_classes))
        .route("/records/debentures/issues", get(debenture_issues))
        .route("/records/debentures/interest", get(debenture_interest))
        .route("/records/issued-share-capital/issue", post(issue_share_capital))
        .route("/records/issued-share-capital/shareholders/register", post(register_shareholder))
        .route("/records/issued-share-capital/shareholders/:company_id", get(company_shareholders))
        .route("/records/issued-share-capital/issues/:company_id", get(company_share_issues))
        .route("/records/issued-share-capital/summary/:company_id", get(issued_capital_summary))
        .route("/records/initial-investment/create", post(create_investment))
        .route("/records/initial-investment/simple", post(simple_investment))
        .route(
            "/records/initial-investment/with-capital-allowance",
            post(investment_with_capital_allowance),
        )
        .route("/records/initial-investment/list", get(list_investments))
        .route("/records/initial-investment/get/:investment_id", get(get_investment))
        .route("/records/share-redemption/redemptions/initiate", post(initiate_redemption))
        .route(
            "/records/share-redemption/redemptions/:redemption_id/fresh-issue",
            post(record_fresh_issue),
        )
        .route(
            "/records/share-redemption/redemptions/:redemption_id/complete",
            post(complete_redemption),
        )
        .route("/records/share-redemption/redemptions", get(list_share_redemptions))
        .route("/records/share-redemption/redemptions/:redemption_id", get(share_redemption_detail))
        .route("/records/share-redemption/crr-requirements", get(crr_requirements))
        .route("/share-premium/entries/record", post(record_share_premium))
        .route("/share-premium/entries", get(share_premium_entries))
        .route("/share-premium/utilizations/record", post(utilize_share_premium))
        .route("/share-premium/utilizations", get(share_premium_utilizations))
        .route("/share-premium/adjustments/create", post(adjust_share_premium))
        .route("/share-premium/summary/:company_id", get(share_premium_summary))
        .route("/capital-redemption-reserve/redemptions/record", post(record_redemption))
        .route("/capital-redemption-reserve/redemptions", get(list_redemptions))
        .route("/capital-redemption-reserve/creations/create", post(create_crr))
        .route("/capital-redemption-reserve/creations", get(list_crr_creations))
        .route("/capital-redemption-reserve/utilizations/record", post(utilize_crr))
        .route("/capital-redemption-reserve/utilizations", get(list_crr_utilizations))
        .route("/capital-redemption-reserve/summary/:company_id", get(crr_summary))
        .route("/general-reserve/reserves", get(reserves).post(create_reserve))
        .route("/general-reserve/reserves/:reserve_id", get(one_reserve))
        .route("/general-reserve/reserves/:reserve_id/allocate", post(allocate_reserve))
        .route("/general-reserve/reserves/:reserve_id/utilize", post(utilize_reserve))
        .route("/general-reserve/reserves/:reserve_id/history", get(reserve_history))
        .route(
            "/retained-profits/appropriations",
            get(profit_appropriations).post(create_profit_appropriation),
        )
        .route(
            "/retained-profits/interim-dividends",
            get(interim_dividends).post(declare_interim_dividend),
        )
        .route("/retained-profits/interim-dividends/:dividend_id/pay", post(pay_interim_dividend))
        .route(
            "/retained-profits/adjustments",
            get(prior_year_adjustments).post(create_prior_year_adjustment),
        )
        .route("/retained-profits/summary/:company_id", get(retained_earnings_summary))
        .route("/bonus-shares/issue", post(issue_bonus_shares))
        .route("/bonus-shares/issues", get(bonus_issues))
        .route("/right-issues/offer", post(offer_rights))
        .route("/right-issues/right-issues", get(right_issues))
        .route("/right-issues/right-issues/:issue_id", get(one_right_issue))
        .route("/right-issues/right-issues/:issue_id/accept", post(accept_rights))
        .route("/right-issues/right-issues/:issue_id/renounce", post(renounce_rights))
        .route("/ordinary-shares/dividends", get(ordinary_dividends).post(declare_ordinary_dividend))
        .route("/ordinary-shares/dividends/:dividend_id/pay", post(pay_ordinary_dividend))
        .route("/preference-shares/classes", get(preference_classes).post(create_preference_class))
        .route("/preference-shares/classes/:class_id/issue", post(issue_preference_shares))
        .route(
            "/preference-shares/classes/:class_id/dividends/declare",
            post(declare_preference_dividend),
        )
        .route(
            "/preference-shares/classes/:class_id/dividends/:dividend_id/pay",
            post(pay_preference_dividend),
        )
        .route("/preference-shares/classes/:class_id/redeem", post(redeem_preference_shares))
        .route("/preference-shares/dividends", get(preference_dividends))
        .route("/authorized-share-capital/share-classes", get(share_classes).post(create_share_class))
        .route("/authorized-share-capital/share-classes/:class_id", get(one_share_class))
        .route("/authorized-share-capital/share-classes/:class_id/issue", post(issue_class_shares))
        .route("/authorized-share-capital/share-classes/:class_id/issuances", get(class_issuances))
        .route("/authorized-share-capital/share-classes/:class_id/buyback", post(buyback_class_shares))
        .route("/authorized-share-capital/summary", get(share_capital_summary))
        .route("/disposal-group/groups", get(disposal_groups).post(create_disposal_group))
        .route("/disposal-group/groups/:group_id", get(one_disposal_group))
        .route("/disposal-group/groups/:group_id/assets", get(disposal_assets).post(add_disposal_asset))
        .route("/disposal-group/groups/:group_id/impairment-test", post(disposal_impairment_test))
        .route("/disposal-group/groups/:group_id/status", put(disposal_group_status))
        .route(
            "/lease-termination/terminations",
            get(lease_terminations).post(create_lease_termination),
        )
        .route("/lease-termination/terminations/:termination_id", get(one_lease_termination))
        .route("/lease-termination/terminations/:termination_id/settlement", post(lease_settlement))
        .route(
            "/lease-termination/terminations/:termination_id/approve",
            put(approve_lease_termination),
        )
        .route(
            "/lease-termination/terminations/:termination_id/complete",
            put(complete_lease_termination),
        )
        .route("/intangible-assets/assets", get(intangibles).post(create_intangible))
        .route("/intangible-assets/assets/:asset_id", get(one_intangible))
        .route("/intangible-assets/assets/:asset_id/amortize", post(amortize_intangible))
        .route("/intangible-assets/assets/:asset_id/amortization", get(intangible_amortization))
        .route("/revaluation-reserve/revaluations/record", post(record_asset_revaluation))
        .route("/revaluation-reserve/utilizations/record", post(record_reserve_utilization))
        .route("/revaluation-reserve/revaluations", get(revaluations))
        .route("/revaluation-reserve/cumulative/:asset_id", get(cumulative_revaluation))
        .route("/revaluation-reserve/summary/:company_id", get(revaluation_summary))
        .route("/insurance-claims/file", post(file_claim))
        .route("/insurance-claims/claims", get(insurance_claims))
        .route("/insurance-claims/claims/:claim_id/process", post(process_insurance_claim))
        .route("/treasury-analytics/analyze", post(analyze_treasury))
        .route("/treasury-analytics/kpi/:company_id", get(treasury_kpi))
        .route("/cash-flow-statement/generate", post(generate_cash_flow_statement))
        .route("/cash-flow-statement/latest/:company_id", get(latest_cash_flow_statement))
        .route("/cash-flow-statement/history/:company_id", get(cash_flow_statement_history))
        .route("/equity-changes/transactions", post(record_equity_transaction))
        .route("/equity-changes/transactions/:company_id", get(company_equity_transactions))
        .route("/equity-changes/statement", post(generate_equity_statement))
        .route("/equity-changes/statements/:company_id", get(company_equity_statements))
        .route(
            "/partnership-agreement/agreements",
            get(list_partnership_agreements).post(create_partnership_agreement),
        )
        .route(
            "/partnership-agreement/agreements/:agreement_id",
            get(get_partnership_agreement).put(update_partnership_agreement),
        )
        .route(
            "/partnership-agreement/agreements/:agreement_id/partners/add",
            post(add_partnership_partner),
        )
        .route(
            "/partnership-agreement/agreements/:agreement_id/summary",
            get(partnership_agreement_summary),
        )
        .route("/partnership-changes/changes/admission", post(admit_partner))
        .route("/partnership-changes/changes/retirement", post(retire_partner))
        .route("/partnership-changes/changes/death", post(record_partner_death))
        .route("/partnership-changes/changes", get(partnership_changes))
        .route(
            "/partnership-changes/changes/:change_id/settle",
            post(settle_partner_change),
        )
        .route("/partnership-dissolution/dissolve", post(dissolve_partnership))
        .route("/partnership-dissolution/dissolutions", get(partnership_dissolutions))
        .route(
            "/partnership-dissolution/dissolutions/:dissolution_id",
            get(partnership_dissolution),
        )
        .route("/partnership-revaluation/revalue", post(revalue_partnership))
        .route("/partnership-revaluation/revaluations", get(partnership_revaluations))
        .route(
            "/partnership-revaluation/revaluations/:revaluation_id",
            get(partnership_revaluation),
        )
        .route("/cash-optimization/accounts", post(add_cash_account))
        .route("/cash-optimization/accounts/:company_id", get(company_cash_accounts))
        .route("/cash-optimization/optimize/:company_id", post(run_cash_optimization))
        .route("/cash-optimization/suggestions/:company_id", get(company_cash_suggestions))
        .route(
            "/absorption-costing-statement/trading-account/generate",
            post(generate_statement_trading_account),
        )
        .route(
            "/absorption-costing-statement/trading-account",
            get(statement_trading_accounts),
        )
        .route(
            "/absorption-costing-statement/trading-account/:statement_id",
            get(one_statement_trading_account),
        )
        .route(
            "/absorption-costing-statement/trading-account/:statement_id/add-expenses",
            post(add_statement_expenses),
        )
        .route(
            "/absorption-costing-statement/production-cost/generate",
            post(generate_production_cost_statement),
        )
        .route(
            "/absorption-costing-statement/production-cost",
            get(statement_production_costs),
        )
        .route(
            "/activity-based-budget/activities",
            get(activities).post(define_activity),
        )
        .route(
            "/activity-based-budget/budgets",
            get(activity_budgets).post(create_activity_budget),
        )
        .route(
            "/activity-based-budget/budgets/:budget_id",
            get(one_activity_budget),
        )
        .route(
            "/activity-based-budget/budgets/:budget_id/approve",
            put(approve_activity_budget),
        )
        .route("/revaluation-model/revalue", post(perform_revaluation))
        .route(
            "/revaluation-model/revaluation-surplus",
            post(revaluation_surplus),
        )
        .route(
            "/revaluation-model/depreciation-after-revaluation",
            post(depreciation_after_revaluation),
        )
        .route(
            "/revaluation-model/impairment-after-revaluation",
            post(impairment_after_revaluation),
        )
        .route(
            "/revaluation-model/indexation-allowance",
            post(indexation_allowance),
        )
        .route("/trading-account/generate", post(generate_trading_account))
        .route("/trading-account/accounts", get(list_trading_accounts))
        .route("/trading-account/accounts/:account_id", get(get_trading_account))
        .route("/selling-price-reduction/analyze", post(price_analysis))
        .route("/selling-price-reduction/discount-analysis", post(price_discount_analysis))
        .route("/selling-price-reduction/analyses", get(price_analyses))
        .route("/cvp-analysis/analyze", post(cvp_analysis))
        .route("/cvp-analysis/quick-analysis", post(cvp_quick_analysis))
        .route("/cvp-analysis/contribution", post(cvp_contribution))
        .route("/cvp-analysis/target-profit", post(cvp_target_profit))
        .route("/cvp-analysis/multi-product", post(cvp_multi_product))
        .route("/cvp-analysis/analyses", get(cvp_analyses))
        .route("/cvp-analysis/analyses/:analysis_id", get(cvp_analysis_by_id))
        .route("/overhead-absorption/calculate", post(overhead_rate))
        .route(
            "/overhead-absorption/machine-hours-rate",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    overhead_base_rate((state, 0), headers, body)
                },
            ),
        )
        .route(
            "/overhead-absorption/labour-hours-rate",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    overhead_base_rate((state, 1), headers, body)
                },
            ),
        )
        .route(
            "/overhead-absorption/labour-cost-rate",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    overhead_base_rate((state, 2), headers, body)
                },
            ),
        )
        .route(
            "/overhead-absorption/material-cost-rate",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    overhead_base_rate((state, 3), headers, body)
                },
            ),
        )
        .route(
            "/overhead-absorption/prime-cost-rate",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    overhead_base_rate((state, 4), headers, body)
                },
            ),
        )
        .route(
            "/overhead-absorption/unit-rate",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    overhead_base_rate((state, 5), headers, body)
                },
            ),
        )
        .route("/overhead-absorption/blanket-rate", post(overhead_blanket_rate))
        .route("/overhead-absorption/absorbed-overhead", post(overhead_absorbed))
        .route("/overhead-absorption/rates", get(overhead_rates))
        .route("/overhead-absorption/analyses", get(overhead_analyses))
        .route("/apportionment/rules/create", post(apportionment_rule))
        .route("/apportionment/rules", get(apportionment_rules))
        .route(
            "/apportionment/apportion/floor-area",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    apportionment_by_basis((state, 0), headers, body)
                },
            ),
        )
        .route(
            "/apportionment/apportion/volume",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    apportionment_by_basis((state, 1), headers, body)
                },
            ),
        )
        .route(
            "/apportionment/apportion/cost",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    apportionment_by_basis((state, 2), headers, body)
                },
            ),
        )
        .route(
            "/apportionment/apportion/net-book-value",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    apportionment_by_basis((state, 3), headers, body)
                },
            ),
        )
        .route(
            "/apportionment/apportion/replacement-value",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    apportionment_by_basis((state, 4), headers, body)
                },
            ),
        )
        .route(
            "/apportionment/apportion/personnel",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    apportionment_by_basis((state, 5), headers, body)
                },
            ),
        )
        .route(
            "/apportionment/apportion/requisitions",
            post(
                move |state: GraphState, headers: HeaderMap, body: Json<Value>| {
                    apportionment_by_basis((state, 6), headers, body)
                },
            ),
        )
        .route("/apportionment/results", get(apportionment_results))
        .route("/process-costing/calculate", post(cost_calculation))
        .route("/process-costing/calculations/:company_id", get(company_cost_calculations))
        .route("/process-costing/breakdown/:company_id/:calc_id", get(cost_breakdown))
        .route("/process-costing/summary/:company_id", get(cost_summary))
        .route("/product-costing/calculate", post(cost_calculation))
        .route("/product-costing/calculations/:company_id", get(company_cost_calculations))
        .route("/product-costing/breakdown/:company_id/:calc_id", get(cost_breakdown))
        .route("/product-costing/summary/:company_id", get(cost_summary))
        .route("/appropriation-control/appropriations", post(create_appropriation))
        .route("/appropriation-control/appropriations/:company_id", get(company_appropriations))
        .route("/appropriation-control/transactions", post(appropriation_transaction))
        .route("/appropriation-control/check/:appropriation_id", get(appropriation_check))
        .route("/forecasts", get(rolling_forecasts).post(open_rolling_forecast))
        .route("/forecasts/:forecast_id", get(one_rolling_forecast))
        .route("/forecasts/:forecast_id/update-actual", post(record_forecast_actual))
        .route("/forecasts/:forecast_id/roll", post(roll_forecast))
        .route("/scenario-analysis/analyze", post(analyze_scenarios))
        .route("/scenario-analysis/scenarios", post(open_scenario))
        .route("/scenario-analysis/scenarios/:company_id", get(scenarios_for_company))
        .route("/scenario-analysis/compare/:company_id", get(compare_scenarios))
        .route("/sensitivity-analysis/analyze", post(run_sensitivity))
        .route("/sensitivity-analysis/analyses/:company_id", get(sensitivity_analyses))
        .route("/scenario-modeling/scenarios", get(model_scenarios).post(open_model_scenario))
        .route(
            "/scenario-modeling/scenarios/:scenario_id",
            get(one_model_scenario).put(change_model_scenario).delete(drop_model_scenario),
        )
        .route("/scenario-modeling/rules", get(model_rules).post(open_rule))
        .route("/scenario-modeling/rules/:rule_id", get(one_rule).put(change_rule).delete(drop_rule))
        .route("/scenario-modeling/what-if", post(run_what_if))
        .route("/scenario-modeling/what-if/:analysis_id", get(one_what_if))
        .route("/scenario-modeling/what-if/scenario/:scenario_id", get(what_if_for_scenario))
        .route("/scenario-modeling/sensitivity", post(run_model_sweep))
        .route("/scenario-modeling/compare", post(compare_model_scenarios))
        .with_state(graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use std::sync::Mutex;
    use tower::ServiceExt;

    /// Answers every statement with the same rows, and remembers what it was asked.
    struct Fake {
        rows: Vec<Value>,
        seen: Mutex<Vec<(String, Value)>>,
    }

    impl Graph for Fake {
        fn run(&self, cypher: &str, params: Value) -> Result<Vec<Value>, StoreError> {
            self.seen.lock().unwrap().push((cypher.to_owned(), params));
            Ok(self.rows.clone())
        }
    }

    fn app(rows: Vec<Value>) -> (Router, Arc<Fake>) {
        let fake = Arc::new(Fake {
            rows,
            seen: Mutex::new(Vec::new()),
        });
        (router(fake.clone() as GraphHandle), fake)
    }

    fn get_request(uri: &str, user: &str, book: &str) -> Request<Body> {
        Request::builder()
            .uri(uri)
            .header("x-user-id", user)
            .header("x-book-id", book)
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn a_request_without_a_caller_is_refused_before_anything_runs() {
        let (app, fake) = app(vec![]);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/records/absorption-costing/product-costs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(fake.seen.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn the_service_paths_answer_from_the_same_handlers() {
        let (app, fake) = app(vec![json!({"x": {"product_id": "p-1"}})]);
        let response = app
            .oneshot(get_request(
                "/absorption-costing/product-costs",
                "user-1",
                "book-1",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(fake.seen.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_scoped_list_reaches_the_graph_with_the_callers_book() {
        let (app, fake) = app(vec![json!({"x": {"product_id": "p-1"}})]);
        let response = app
            .oneshot(get_request(
                "/records/absorption-costing/product-costs?product_id=p-1",
                "user-1",
                "book-1",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let (_, params) = fake.seen.lock().unwrap().last().cloned().unwrap();
        assert_eq!(params["book_id"], json!("book-1"));
        assert_eq!(params["user_id"], json!("user-1"));
    }

    #[tokio::test]
    async fn a_missing_record_is_a_404_not_an_empty_200() {
        let (app, _) = app(vec![]);
        let response = app
            .oneshot(get_request(
                "/records/absorption-costing/product-costs/p-9/latest",
                "user-1",
                "book-1",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_graph_that_is_down_is_a_502() {
        struct Down;
        impl Graph for Down {
            fn run(&self, _cypher: &str, _params: Value) -> Result<Vec<Value>, StoreError> {
                Err(StoreError::Graph("connection refused".into()))
            }
        }
        let response = router(Arc::new(Down) as GraphHandle)
            .oneshot(get_request(
                "/records/job-costing/jobs/c-1",
                "user-1",
                "book-1",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn opening_a_job_needs_a_contract_value() {
        let (app, fake) = app(vec![]);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/records/job-costing/jobs")
                    .header("x-user-id", "user-1")
                    .header("x-book-id", "book-1")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"company_id":"c-1"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            fake.seen.lock().unwrap().len(),
            0,
            "nothing should be written"
        );
    }

    #[tokio::test]
    async fn the_valuation_method_survives_the_query_string() {
        let (app, _) = app(vec![json!({"x": {
            "product_id": "p-1", "cost_per_unit": "10.00", "closing_stock": "5"
        }})]);
        let response = app
            .oneshot(get_request(
                "/records/absorption-costing/stock-valuation?product_id=p-1&valuation_method=weighted_average",
                "user-1",
                "book-1",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["valuation_method"], json!("weighted_average"));
        assert_eq!(value["closing_stock_value"], json!("50.000000"));
    }
    #[tokio::test]
    async fn a_partnership_agreement_is_created_with_the_capital_its_partners_bring() {
        let (app, _) = app(vec![
            json!({"id": "ag-1", "capital_amount": "12500.000000"}),
        ]);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/partnership-agreement/agreements")
                    .header("x-user-id", "user-1")
                    .header("x-book-id", "book-1")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"partnership_name":"Two Hands","agreement_number":"A-1","business_nature":"retail","start_date":"2026-01-01","capital_amount":999999,"partners":[{"name":"Ana","contribution":5000},{"name":"Bongani","contribution":7500}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["capital_amount"], json!("12500.000000"));
    }

    #[tokio::test]
    async fn a_partnership_agreement_the_caller_does_not_hold_is_not_found_over_http() {
        let (app, _) = app(vec![]);
        let response = app
            .oneshot(get_request(
                "/partnership-agreement/agreements/ag-9",
                "user-1",
                "book-1",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_partner_admission_answers_over_http() {
        let (app, _) = app(vec![json!({"id": "ch-1"})]);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/partnership-changes/changes/admission")
                    .header("x-user-id", "user-1")
                    .header("x-book-id", "book-1")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"new_partner_id":"abcdef123","new_partner_name":"Chen","admission_date":"2026-04-01","capital_contribution":4000,"goodwill_paid":600}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["journal_entries"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_partner_retirement_answers_over_http() {
        let (app, _) = app(vec![json!({"id": "ch-2"})]);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/partnership-changes/changes/retirement")
                    .header("x-user-id", "user-1")
                    .header("x-book-id", "book-1")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"partnership_id":"p-1","partner_id":"p-9","partner_name":"Ana","effective_date":"2026-02-01","capital_balance":12000,"current_account_balance":500,"goodwill_amount":1500}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["total_payable"], json!("14000.000000"));
    }

    #[tokio::test]
    async fn a_partnership_dissolution_answers_over_http() {
        let (app, _) = app(vec![json!({"id": "d-1"})]);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/partnership-dissolution/dissolve")
                    .header("x-user-id", "user-1")
                    .header("x-book-id", "book-1")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"partnership_id":"p-1","dissolution_date":"2026-05-01","reason":"insolvency","assets":[{"asset_id":"a-1","asset_name":"Van","book_value":10000,"sale_proceeds":11500}],"creditors":[],"partners":[]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["realization_profit"], json!("1500.000000"));
    }

    #[tokio::test]
    async fn a_partnership_revaluation_answers_over_http() {
        let (app, _) = app(vec![json!({"id": "r-1"})]);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/partnership-revaluation/revalue")
                    .header("x-user-id", "user-1")
                    .header("x-book-id", "book-1")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"partnership_id":"p-1","revaluation_date":"2026-06-01","goodwill_treatment":"write_off","asset_revaluations":[{"asset_id":"a-1","asset_name":"Premises","asset_code":"P1","account_code":"1100","old_value":100000,"new_value":130000}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["total_increase"], json!("30000.000000"));
    }
}
