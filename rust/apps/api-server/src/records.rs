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

use ::npo::npo_scale;
use accounting::{cash_flow, trading_account};
use analytics::benchmarking;
use axum::{
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use banking;
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
use invoicing::invoices;
use ledger;
use multimodal::capture;
use npo::npo;
use partnerships::{agreement, changes, dissolution, revaluation as partnership_revaluation};
use payables;
use receivables;
use serde_json::{json, Value};
use store::amounts::{amount, amount_or_zero, wire};
use store::{BookScope, Graph, StoreError};
use tax_services as tax;
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
        StoreError::Forbidden(_) => StatusCode::FORBIDDEN,
        StoreError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
        StoreError::Conflict(_) => StatusCode::CONFLICT,
        StoreError::BadRequest(_) => StatusCode::BAD_REQUEST,
        StoreError::Graph(_) => StatusCode::BAD_GATEWAY,
    };
    (status, Json(json!({ "error": error.to_string() }))).into_response()
}

/// A request whose scalar fields arrive in the query string, overlaid by anything sent as a body.
fn merged_request(payload: Option<Json<Value>>, query: Option<&str>) -> Value {
    let merged = body_or_query(query);
    let Value::Object(mut into) = merged else {
        return merged;
    };
    if let Some(Json(Value::Object(given))) = payload {
        for (key, value) in given {
            into.insert(key, value);
        }
    }
    Value::Object(into)
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

// --- family-community-group-service: savings groups and what they collect ---

async fn create_community_group(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(partnerships::groups::create_group(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn community_groups(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(partnerships::groups::list_groups(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn community_group(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(group_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(partnerships::groups::get_group(
        graph.as_ref(),
        &scope,
        &group_id,
    ))
}

async fn add_community_group_member(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(partnerships::groups::add_member(
        graph.as_ref(),
        &scope,
        &group_id,
        &body,
    ))
}

async fn community_group_members(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(group_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(partnerships::groups::list_members(
        graph.as_ref(),
        &scope,
        &group_id,
    ))
}

async fn contribute_to_community_group(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(partnerships::groups::contribute(
        graph.as_ref(),
        &scope,
        &group_id,
        &body,
    ))
}

async fn community_group_contributions(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(partnerships::groups::list_contributions(
        graph.as_ref(),
        &scope,
        &group_id,
        &body,
    ))
}

async fn advance_community_group_cycle(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(group_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(partnerships::groups::advance_cycle(
        graph.as_ref(),
        &scope,
        &group_id,
    ))
}

async fn community_group_payouts(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(group_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(partnerships::groups::list_payouts(
        graph.as_ref(),
        &scope,
        &group_id,
    ))
}

/// A request's payload: the query string, with any JSON body laid over it.
///
/// These services took their arguments as query parameters, so a caller migrating to a body must not lose the query
/// it already sends, and a body must win where both name the same field.
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
        // a key given more than once is a LIST, which is how the shipped services took
        // `?documents=a&documents=b`: dropping the earlier values would quietly lose them
        match payload.get_mut(key.as_ref()) {
            Some(held) => match held {
                Value::Array(list) => list.push(parsed),
                Value::Null => *held = json!([parsed]),
                first => {
                    let earlier = first.take();
                    *first = json!([earlier, parsed]);
                }
            },
            None => {
                payload.insert(key.to_string(), parsed);
            }
        }
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

// --- npo-service: the non-profit's funds, what moves through them, and its statements ---

async fn create_npo_fund(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::create_npo_fund(graph.as_ref(), &scope, &body))
}

async fn npo_funds(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::list_npo_funds(graph.as_ref(), &scope, &body))
}

async fn create_npo_donation(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::create_donation(graph.as_ref(), &scope, &body))
}

async fn npo_donations(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo::list_donations(graph.as_ref(), &scope))
}

async fn create_npo_grant(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::create_grant(graph.as_ref(), &scope, &body))
}

async fn npo_grants(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::list_grants(graph.as_ref(), &scope, &body))
}

async fn create_npo_project(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::create_project(graph.as_ref(), &scope, &body))
}

async fn npo_projects(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::list_projects(graph.as_ref(), &scope, &body))
}

async fn create_npo_program(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::create_program(graph.as_ref(), &scope, &body))
}

async fn npo_programs(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo::list_programs(graph.as_ref(), &scope))
}

async fn create_npo_donor(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::create_donor(graph.as_ref(), &scope, &body))
}

async fn npo_donors(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo::list_donors(graph.as_ref(), &scope))
}

async fn create_npo_budget(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::create_npo_budget(graph.as_ref(), &scope, &body))
}

async fn npo_budgets(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::list_npo_budgets(graph.as_ref(), &scope, &body))
}

async fn create_npo_internal_control(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::create_internal_control(graph.as_ref(), &scope, &body))
}

async fn npo_internal_controls(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo::list_internal_controls(graph.as_ref(), &scope))
}

async fn create_npo_audit_report(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::create_audit_report(graph.as_ref(), &scope, &body))
}

async fn npo_audit_reports(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo::list_audit_reports(graph.as_ref(), &scope))
}

async fn create_npo_program_metric(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::create_program_metric(graph.as_ref(), &scope, &body))
}

async fn create_npo_impact_measurement(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::create_impact_measurement(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn create_npo_volunteer_record(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::create_volunteer_record(graph.as_ref(), &scope, &body))
}

async fn npo_volunteer_records(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::list_volunteer_records(graph.as_ref(), &scope, &body))
}

async fn create_npo_net_assets(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::create_net_assets(graph.as_ref(), &scope, &body))
}

async fn create_npo_statement_of_activities(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::create_statement_of_activities(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn npo_statement_of_activities(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::statement_of_activities(graph.as_ref(), &scope, &body))
}

async fn create_npo_statement_of_position(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::create_statement_of_position(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn describe_npo_asset(
    State(_graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let _scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::describe_asset(&body))
}

async fn npo_sroi_analysis(
    State(_graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let _scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::analyse_sroi(&body))
}

async fn npo_fund(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(fund_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo::get_npo_fund(graph.as_ref(), &scope, &fund_id))
}

async fn npo_fund_transactions(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(fund_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::fund_transactions(
        graph.as_ref(),
        &scope,
        &fund_id,
        &body,
    ))
}

async fn record_npo_fund_transaction(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(fund_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::record_fund_transaction(
        graph.as_ref(),
        &scope,
        &fund_id,
        &body,
    ))
}

async fn restrict_npo_fund(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(fund_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::restrict_fund(graph.as_ref(), &scope, &fund_id, &body))
}

async fn npo_net_assets_on(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(as_of_date): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo::net_assets_on(graph.as_ref(), &scope, &as_of_date))
}

async fn npo_grant(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(grant_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo::get_grant(graph.as_ref(), &scope, &grant_id))
}

async fn record_npo_drawdown(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(grant_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::record_drawdown(
        graph.as_ref(),
        &scope,
        &grant_id,
        &body,
    ))
}

async fn npo_project(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(project_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo::get_project(graph.as_ref(), &scope, &project_id))
}

async fn npo_donor(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(donor_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo::get_donor(graph.as_ref(), &scope, &donor_id))
}

async fn add_npo_budget_line(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(budget_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo::add_budget_line(
        graph.as_ref(),
        &scope,
        &budget_id,
        &body,
    ))
}

// --- npo-scale-service: the same non-profit accounting at multi-entity scale ---

async fn create_npo_scale_org(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo_scale::create_org(graph.as_ref(), &scope, &body))
}

async fn npo_scale_orgs(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::list_orgs(graph.as_ref(), &scope))
}

async fn npo_scale_org(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::get_org(graph.as_ref(), &scope, &org_id))
}

async fn update_npo_scale_org(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo_scale::update_org(
        graph.as_ref(),
        &scope,
        &org_id,
        &body,
    ))
}

async fn npo_scale_org_features(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::org_features(graph.as_ref(), &scope, &org_id))
}

async fn create_npo_scale_branch(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo_scale::create_branch(
        graph.as_ref(),
        &scope,
        &org_id,
        &body,
    ))
}

async fn npo_scale_branches(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::list_branches(graph.as_ref(), &scope, &org_id))
}

async fn add_npo_scale_vendor(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo_scale::add_vendor(
        graph.as_ref(),
        &scope,
        &org_id,
        &body,
    ))
}

async fn npo_scale_vendors(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::list_vendors(graph.as_ref(), &scope, &org_id))
}

async fn record_npo_scale_revenue(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo_scale::record_revenue(
        graph.as_ref(),
        &scope,
        &org_id,
        &body,
    ))
}

async fn npo_scale_revenues(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::list_revenues(graph.as_ref(), &scope, &org_id))
}

async fn record_npo_scale_purchase(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(npo_scale::record_purchase(
        graph.as_ref(),
        &scope,
        &org_id,
        &body,
    ))
}

async fn pay_npo_scale_purchase(
    State(graph): GraphState,
    headers: HeaderMap,
    Path((org_id, purchase_id)): Path<(String, String)>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    // the person settling a purchase is the one its expense names as approving it
    let payer = scope.user_id.clone();
    answer(npo_scale::pay_purchase(
        graph.as_ref(),
        &scope,
        &org_id,
        &purchase_id,
        &payer,
    ))
}

async fn npo_scale_creditors(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::report_creditors(graph.as_ref(), &scope, &org_id))
}

async fn create_npo_scale_donor(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let payload = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(npo_scale::create_donor(
        graph.as_ref(),
        &scope,
        &org_id,
        &payload,
    ))
}

async fn list_npo_scale_donors(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::list_donors(graph.as_ref(), &scope, &org_id))
}

async fn create_npo_scale_donation(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let payload = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(npo_scale::record_donation(
        graph.as_ref(),
        &scope,
        &org_id,
        &payload,
    ))
}

async fn list_npo_scale_donations(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let query = query_object(query.as_deref());
    let designation = query.get("designation").and_then(Value::as_str);
    answer(npo_scale::list_donations(
        graph.as_ref(),
        &scope,
        &org_id,
        designation,
    ))
}

async fn create_npo_scale_pledge(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let payload = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(npo_scale::create_pledge(
        graph.as_ref(),
        &scope,
        &org_id,
        &payload,
    ))
}

async fn run_npo_scale_pledges(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let payload = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(npo_scale::run_pledges(
        graph.as_ref(),
        &scope,
        &org_id,
        &payload,
    ))
}

async fn list_npo_scale_receipts(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::list_scale_receipts(
        graph.as_ref(),
        &scope,
        &org_id,
    ))
}

async fn create_npo_scale_expense(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let payload = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(npo_scale::create_expense(
        graph.as_ref(),
        &scope,
        &org_id,
        &payload,
    ))
}

async fn list_npo_scale_expenses(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::list_expenses(graph.as_ref(), &scope, &org_id))
}

async fn save_npo_scale_budget(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let payload = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(npo_scale::upsert_budget(
        graph.as_ref(),
        &scope,
        &org_id,
        &payload,
    ))
}

async fn create_npo_scale_compliance(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let payload = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(npo_scale::create_compliance(
        graph.as_ref(),
        &scope,
        &org_id,
        &payload,
    ))
}

async fn list_npo_scale_compliance(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::list_compliance(graph.as_ref(), &scope, &org_id))
}

async fn create_npo_scale_balance_item(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let payload = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(npo_scale::create_balance_item(
        graph.as_ref(),
        &scope,
        &org_id,
        &payload,
    ))
}

async fn add_npo_scale_shareholder(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let payload = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(npo_scale::add_shareholder(
        graph.as_ref(),
        &scope,
        &org_id,
        &payload,
    ))
}

async fn list_npo_scale_shareholders(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::list_shareholders(
        graph.as_ref(),
        &scope,
        &org_id,
    ))
}

async fn add_npo_scale_director(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let payload = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(npo_scale::add_director(
        graph.as_ref(),
        &scope,
        &org_id,
        &payload,
    ))
}

async fn list_npo_scale_directors(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::list_directors(graph.as_ref(), &scope, &org_id))
}

async fn declare_npo_scale_dividend(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let payload = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(npo_scale::declare_dividend(
        graph.as_ref(),
        &scope,
        &org_id,
        &payload,
    ))
}

async fn report_npo_scale_equity(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::report_equity(graph.as_ref(), &scope, &org_id))
}

async fn add_npo_scale_partner(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let payload = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(npo_scale::add_scale_partner(
        graph.as_ref(),
        &scope,
        &org_id,
        &payload,
    ))
}

async fn list_npo_scale_partners(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::list_partners(graph.as_ref(), &scope, &org_id))
}

async fn draw_npo_scale_partner(
    State(graph): GraphState,
    headers: HeaderMap,
    Path((org_id, partner_id)): Path<(String, String)>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let payload = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(npo_scale::partner_draw(
        graph.as_ref(),
        &scope,
        &org_id,
        &partner_id,
        &payload,
    ))
}

async fn report_npo_scale_capital_accounts(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::report_capital_accounts(
        graph.as_ref(),
        &scope,
        &org_id,
    ))
}

async fn report_npo_scale_activities(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::report_activities(
        graph.as_ref(),
        &scope,
        &org_id,
    ))
}

async fn report_npo_scale_functional(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::report_functional(
        graph.as_ref(),
        &scope,
        &org_id,
    ))
}

async fn report_npo_scale_position(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::report_position(graph.as_ref(), &scope, &org_id))
}

async fn report_npo_scale_consolidated(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(npo_scale::report_consolidated(
        graph.as_ref(),
        &scope,
        &org_id,
    ))
}

async fn report_npo_scale_budget(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(org_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let query = query_object(query.as_deref());
    let year = query
        .get("fiscal_year")
        .and_then(Value::as_u64)
        .map(|year| year as u32);
    answer(npo_scale::report_budget(
        graph.as_ref(),
        &scope,
        &org_id,
        year,
    ))
}

/// `GET /npo-scale/receipts/verify/{token}`: the public check a receipt holder runs, so no Book is asked for.
async fn verify_npo_scale_receipt(State(graph): GraphState, Path(token): Path<String>) -> Response {
    answer(npo_scale::verify_receipt(graph.as_ref(), &token))
}

/// `GET /npo-scale/public/holdings/{verify_code}`: an investor's own holding, likewise without a Book.
async fn public_npo_scale_holdings(
    State(graph): GraphState,
    Path(verify_code): Path<String>,
) -> Response {
    answer(npo_scale::public_holdings(graph.as_ref(), &verify_code))
}

/// `GET /npo-scale/orgs/{org_id}/receipts/{receipt_id}/pdf`: the receipt itself, as a document to keep.
async fn npo_scale_receipt_pdf(
    State(graph): GraphState,
    headers: HeaderMap,
    Path((org_id, receipt_id)): Path<(String, String)>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    match npo_scale::receipt_pdf(graph.as_ref(), &scope, &org_id, &receipt_id) {
        Ok(pdf) => Response::builder()
            .header("content-type", "application/pdf")
            .header(
                "content-disposition",
                format!("attachment; filename=\"{receipt_id}.pdf\""),
            )
            .body(axum::body::Body::from(pdf))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(error) => fail(error),
    }
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
        // tax: rates and registration
        .route("/tax-rates", post(post_tax_rate).get(list_tax_rates))
        .route("/tax-rates/:rate_id", get(get_tax_rate).put(put_tax_rate))
        .route(
            "/tax-registrations",
            post(post_tax_registration).get(list_tax_registrations),
        )
        // tax: the tax worked out on a transaction, one at a time or as a batch
        .route("/calculate", post(post_tax_calculation))
        .route("/calculate-batch", post(post_tax_calculation_batch))
        // tax: returns and their filing and payment
        .route("/tax-returns", post(post_tax_return).get(list_tax_returns))
        .route("/tax-returns/:return_id/file", post(post_file_tax_return))
        .route("/tax-returns/:return_id/pay", post(post_pay_tax_return))
        // tax: withholding
        .route(
            "/withholding-tax",
            post(post_withholding_tax).get(list_withholding_tax),
        )
        // tax: the reports a tax manager reads
        .route("/reports/tax-summary", post(post_tax_summary))
        .route("/reports/vat-by-jurisdiction", get(get_vat_by_jurisdiction))
        .route("/deferred-tax", post(post_deferred_tax))
        .route("/liability-schedule", get(get_tax_liability_schedule))
        // tax: the obligations a company owes and the compliance summary over them
        .route(
            "/obligations",
            post(post_tax_obligation).get(list_tax_obligations),
        )
        .route(
            "/obligations/:obligation_id/file",
            post(post_file_tax_obligation),
        )
        // "summary" already answers the receivables control account, so the compliance score is named for
        // what it summarises rather than reusing the word
        .route("/compliance-summary", get(get_tax_compliance_summary))
        // tax: the strategies a planner submits and the plan they add up to
        .route(
            "/strategies",
            post(post_tax_strategy).get(list_tax_strategies),
        )
        .route("/plan", post(post_tax_plan))
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

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// subscription plans: the plans a Book sells on, the subscriptions taken against them, and what a change of plan
// would cost. The preview is pure - it prices the four tiers from its own table and reads no stored plan.
// ---------------------------------------------------------------------------

async fn post_plan(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(invoicing::billing::create_plan(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_plans(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(invoicing::billing::list_plans(graph.as_ref(), &scope))
}

async fn post_subscribe(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(invoicing::billing::subscribe(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_subscriptions(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(invoicing::billing::subscriptions_for_company(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn post_upgrade(RawQuery(query): RawQuery, payload: Option<Json<Value>>) -> Response {
    let asked = merged_request(payload, query.as_deref());
    answer(invoicing::billing::preview_upgrade(&asked))
}

// ---------------------------------------------------------------------------
// payroll benefits: the plans a Book offers, who is enrolled in them, and the leave each person has accrued.
// The plan collection answers at /benefit-plans rather than /plans, which the subscription catalogue already serves.
// ---------------------------------------------------------------------------

async fn post_benefit_plan(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(payroll::benefits::offer_benefit_plan(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_benefit_plans(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(None, query.as_deref());
    answer(payroll::benefits::benefit_plans_offered(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn post_enroll(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(payroll::benefits::enroll_employee(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_employee_enrollments(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(employee_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(payroll::benefits::list_employee_enrollments(
        graph.as_ref(),
        &scope,
        &employee_id,
    ))
}

async fn post_leave_accrual(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(payroll::benefits::accrue_leave(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_employee_leave(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(employee_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(None, query.as_deref());
    answer(payroll::benefits::list_employee_leave(
        graph.as_ref(),
        &scope,
        &employee_id,
        &asked,
    ))
}

// ---------------------------------------------------------------------------
// zero trust: the access policies a Book sets and every decision taken against them. The policy collection answers at
// /access-policies rather than /policies, which the zero-trust-data service's sibling register already holds.
// ---------------------------------------------------------------------------

async fn post_access_policy(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(security_services::zero_trust::set_access_policy(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_access_policies(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(security_services::zero_trust::access_policies_held(
        graph.as_ref(),
        &scope,
    ))
}

async fn put_access_policy(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(policy_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(security_services::zero_trust::replace_policy(
        graph.as_ref(),
        &scope,
        &policy_id,
        &asked,
    ))
}

async fn delete_access_policy(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(policy_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(security_services::zero_trust::delete_policy(
        graph.as_ref(),
        &scope,
        &policy_id,
    ))
}

async fn post_access_evaluation(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(security_services::zero_trust::evaluate_access(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_access_attempts(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(None, query.as_deref());
    answer(security_services::zero_trust::list_attempts(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_subject_attempts(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(None, query.as_deref());
    answer(security_services::zero_trust::attempts_for_user(
        graph.as_ref(),
        &scope,
        &user_id,
        &asked,
    ))
}

// ---------------------------------------------------------------------------
// documents: the files a Book holds, their checksums, the text taken off them and what they are linked to. The
// statistics answer at /document-statistics because /statistics already answers the receivables control account.
// ---------------------------------------------------------------------------

async fn post_document(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(documents::documents::upload_document(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn post_documents_batch(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(documents::documents::upload_documents(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn post_documents_search(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(documents::documents::search_documents(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_documents(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(None, query.as_deref());
    answer(documents::documents::list_documents(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_document_statistics(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(documents::documents::documents_held_counted(
        graph.as_ref(),
        &scope,
    ))
}

async fn get_document(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(document_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(documents::documents::document_held(
        graph.as_ref(),
        &scope,
        &document_id,
    ))
}

async fn get_document_download(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(document_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(documents::documents::download_document(
        graph.as_ref(),
        &scope,
        &document_id,
    ))
}

async fn get_document_preview(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(document_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(documents::documents::preview_document(
        graph.as_ref(),
        &scope,
        &document_id,
    ))
}

async fn put_document(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(document_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(documents::documents::update_document(
        graph.as_ref(),
        &scope,
        &document_id,
        &asked,
    ))
}

async fn delete_document(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(document_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(None, query.as_deref());
    answer(documents::documents::delete_document(
        graph.as_ref(),
        &scope,
        &document_id,
        &asked,
    ))
}

async fn post_document_ocr(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(document_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(documents::documents::process_ocr(
        graph.as_ref(),
        &scope,
        &document_id,
        &asked,
    ))
}

async fn post_document_link(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(document_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(documents::documents::link_document(
        graph.as_ref(),
        &scope,
        &document_id,
        &asked,
    ))
}

async fn post_documents_batch_link(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(documents::documents::link_documents(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

// ---------------------------------------------------------------------------
// alerts: the rules a Book watches and the alerts those rules raise. Evaluating answers at /alerts/evaluate because
// /evaluate already answers an access decision, and the statistics at /alerts/stats because /stats is taken too.
// ---------------------------------------------------------------------------

async fn post_alert_rule(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(alerts::alerts::set_alert_rule(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_alert_rules(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(None, query.as_deref());
    answer(alerts::alerts::alert_rules_held(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_alert_rule(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(alerts::alerts::rule_held(graph.as_ref(), &scope, &id))
}

async fn put_alert_rule(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(alerts::alerts::replace_rule(
        graph.as_ref(),
        &scope,
        &id,
        &asked,
    ))
}

async fn put_alert_rule_delete(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(alerts::alerts::remove_alert_rule(
        graph.as_ref(),
        &scope,
        &id,
    ))
}

async fn post_alert(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(alerts::alerts::create_alert(graph.as_ref(), &scope, &asked))
}

async fn get_alerts(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(None, query.as_deref());
    answer(alerts::alerts::list_alerts(graph.as_ref(), &scope, &asked))
}

async fn post_alerts_evaluate(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(alerts::alerts::evaluate_data(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_alert_stats(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(alerts::alerts::alerts_counted(graph.as_ref(), &scope))
}

async fn get_alert(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(alerts::alerts::alert_held(graph.as_ref(), &scope, &id))
}

async fn put_alert_acknowledge(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(alerts::alerts::acknowledge_alert(
        graph.as_ref(),
        &scope,
        &id,
        &asked,
    ))
}

async fn put_alert_resolve(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(alerts::alerts::resolve_alert(
        graph.as_ref(),
        &scope,
        &id,
        &asked,
    ))
}

async fn put_alert_dismiss(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(alerts::alerts::dismiss_alert(graph.as_ref(), &scope, &id))
}

async fn post_alert_trigger(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(alerts::alerts::trigger_rule(
        graph.as_ref(),
        &scope,
        &id,
        &asked,
    ))
}

// ---------------------------------------------------------------------------
// notifications: each person's own inbox, their preferences and the templates a notice is filled from. The templates
// answer under /notification-templates because /templates belongs to the report service, and the statistics under
// /notification-stats because /stats is the receivables control account.
// ---------------------------------------------------------------------------

fn asked_from(payload: Option<Json<Value>>, query: Option<&str>) -> Value {
    merged_request(payload, query)
}

async fn post_notification(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(notifications::notifications::create_notification(
        graph.as_ref(),
        &scope,
        &asked_from(payload, query.as_deref()),
    ))
}

async fn post_notifications_batch(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(notifications::notifications::create_notifications(
        graph.as_ref(),
        &scope,
        &asked_from(payload, query.as_deref()),
    ))
}

async fn get_inbox(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(notifications::notifications::inbox_of(
        graph.as_ref(),
        &scope,
        &user_id,
        &asked_from(None, query.as_deref()),
    ))
}

async fn delete_one_notification(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(notification_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(notifications::notifications::delete_notification(
        graph.as_ref(),
        &scope,
        &notification_id,
    ))
}

async fn put_notification_read(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(notification_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(notifications::notifications::mark_read(
        graph.as_ref(),
        &scope,
        &notification_id,
        &asked_from(payload, query.as_deref()),
    ))
}

async fn put_notifications_read_all(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(notifications::notifications::mark_all_read(
        graph.as_ref(),
        &scope,
        &user_id,
        &asked_from(payload, query.as_deref()),
    ))
}

async fn get_notification_preferences(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(notifications::notifications::preferences_of(
        graph.as_ref(),
        &scope,
        &user_id,
    ))
}

async fn put_notification_preferences(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(notifications::notifications::save_preferences(
        graph.as_ref(),
        &scope,
        &user_id,
        &asked_from(payload, query.as_deref()),
    ))
}

async fn get_notification_templates() -> Response {
    answer(Ok(notifications::notifications::notification_templates()))
}

async fn get_notification_template(Path(template_name): Path<String>) -> Response {
    answer(notifications::notifications::template_held(&template_name))
}

async fn post_notification_template_send(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(template_name): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(notifications::notifications::send_from_template(
        graph.as_ref(),
        &scope,
        &template_name,
        &asked_from(payload, query.as_deref()),
    ))
}

async fn post_workflow_notify(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(workflow_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(notifications::notifications::notify_workflow(
        graph.as_ref(),
        &scope,
        &workflow_id,
        &asked_from(payload, query.as_deref()),
    ))
}

async fn post_approval_notify(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(approval_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(notifications::notifications::notify_approval(
        graph.as_ref(),
        &scope,
        &approval_id,
        &asked_from(payload, query.as_deref()),
    ))
}

async fn get_notification_stats(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(notifications::notifications::notification_totals(
        graph.as_ref(),
        &scope,
    ))
}

// ---------------------------------------------------------------------------
// webhooks: the endpoints a Book subscribes, the events dispatched to them, and how each delivery went. This build has
// no TLS client, so an https endpoint is reported unreachable with the reason kept on the delivery, rather than a
// delivery being pretended.
// ---------------------------------------------------------------------------

async fn post_webhook_endpoint(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(webhooks::webhooks::register_endpoint(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_webhook_endpoints(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(webhooks::webhooks::endpoints_of(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn post_webhook_dispatch(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    let sender = crate::outbound::PlainHttp;
    answer(webhooks::webhooks::dispatch_event(
        graph.as_ref(),
        &scope,
        &sender,
        &company_id,
        &asked,
    ))
}

async fn get_webhook_deliveries(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(endpoint_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(webhooks::webhooks::deliveries_of(
        graph.as_ref(),
        &scope,
        &endpoint_id,
        &merged_request(None, query.as_deref()),
    ))
}

// ---------------------------------------------------------------------------
// cache: keyed values with a life, a size limit and the counts a cache keeps of itself. The clock comes from the
// request, so a cache can be reasoned about exactly.
// ---------------------------------------------------------------------------

async fn post_cache_entry(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cache::cache::write(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_cache_entry(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(key): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cache::cache::read(
        graph.as_ref(),
        &scope,
        &key,
        &merged_request(None, query.as_deref()),
    ))
}

async fn delete_cache_entry(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cache::cache::remove(graph.as_ref(), &scope, &key))
}

async fn delete_whole_cache(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cache::cache::clear(graph.as_ref(), &scope))
}

async fn get_cache_stats(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(cache::cache::cache_totals(
        graph.as_ref(),
        &scope,
        &merged_request(None, query.as_deref()),
    ))
}

// ---------------------------------------------------------------------------
// workflow: the definitions a step runs through and the instances that walk them. The steps themselves are held on the
// definition, and a walk raises one task per step it enters.
// ---------------------------------------------------------------------------

async fn post_workflow_definition(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(workflow::workflow::create_definition(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_workflow_definitions(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(workflow::workflow::definitions(graph.as_ref(), &scope))
}

async fn get_workflow_definition(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(definition_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(workflow::workflow::definition_of(
        graph.as_ref(),
        &scope,
        &definition_id,
    ))
}

async fn put_workflow_definition(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(definition_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(workflow::workflow::update_definition(
        graph.as_ref(),
        &scope,
        &definition_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn delete_workflow_definition(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(definition_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(workflow::workflow::delete_definition(
        graph.as_ref(),
        &scope,
        &definition_id,
    ))
}

async fn post_workflow_start(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(definition_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(workflow::workflow::start_instance(
        graph.as_ref(),
        &scope,
        &definition_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn post_workflow_instance(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(workflow::workflow::create_instance(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_workflow_instance(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(instance_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(workflow::workflow::instance_of(
        graph.as_ref(),
        &scope,
        &instance_id,
    ))
}

async fn put_workflow_instance(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(instance_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(workflow::workflow::update_instance(
        graph.as_ref(),
        &scope,
        &instance_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn delete_workflow_instance(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(instance_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(workflow::workflow::delete_instance(
        graph.as_ref(),
        &scope,
        &instance_id,
    ))
}

async fn post_workflow_complete(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(instance_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(workflow::workflow::complete_task(
        graph.as_ref(),
        &scope,
        &instance_id,
        &merged_request(payload, query.as_deref()),
    ))
}

// ---------------------------------------------------------------------------
// Multimodal capture: what a Book's inbox holds, and the corrections made to it.
// ---------------------------------------------------------------------------

async fn post_capture(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(capture::create_capture(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_captures(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(capture::captures(graph.as_ref(), &scope))
}

async fn get_capture(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(capture::capture(graph.as_ref(), &scope, &task_id))
}

async fn put_capture(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(capture::update_capture(
        graph.as_ref(),
        &scope,
        &task_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn delete_capture(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(capture::delete_capture(graph.as_ref(), &scope, &task_id))
}

async fn get_capture_inbox(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(capture::inbox(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_capture_summary(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(capture::inbox_summary(graph.as_ref(), &scope))
}

async fn post_capture_organize(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(capture::organize_capture(
        graph.as_ref(),
        &scope,
        &task_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn post_capture_correction(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(capture::submit_correction(
        graph.as_ref(),
        &scope,
        &task_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_capture_corrections(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(capture::corrections(graph.as_ref(), &scope, &task_id))
}

// ---------------------------------------------------------------------------
// Distributed workflow orchestration: multi-step workflows and the rollback of what ran.
// ---------------------------------------------------------------------------

async fn post_orchestration(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(workflow::orchestration::create_orchestration(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_orchestrations(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(workflow::orchestration::orchestrations(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_orchestration(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(workflow_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(workflow::orchestration::orchestration(
        graph.as_ref(),
        &scope,
        &workflow_id,
    ))
}

async fn post_orchestration_run(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(workflow_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(workflow::orchestration::run_orchestration(
        graph.as_ref(),
        &scope,
        &workflow_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn post_orchestration_rollback(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(workflow_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(workflow::orchestration::compensate_orchestration(
        graph.as_ref(),
        &scope,
        &workflow_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn delete_orchestration(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(workflow_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(workflow::orchestration::cancel_orchestration(
        graph.as_ref(),
        &scope,
        &workflow_id,
        &merged_request(payload, query.as_deref()),
    ))
}

// ---------------------------------------------------------------------------
// Invoicing: the customers a Book bills, the invoices it raises, and what has been paid.
// ---------------------------------------------------------------------------

async fn post_invoice_customer(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(invoices::create_invoice_customer(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_invoice_customers(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(invoices::invoice_customers(graph.as_ref(), &scope))
}

async fn get_invoice_customer(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(customer_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(invoices::invoice_customer(
        graph.as_ref(),
        &scope,
        &customer_id,
    ))
}

async fn put_invoice_customer(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(customer_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(invoices::update_invoice_customer(
        graph.as_ref(),
        &scope,
        &customer_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn delete_invoice_customer(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(customer_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(invoices::delete_invoice_customer(
        graph.as_ref(),
        &scope,
        &customer_id,
    ))
}

async fn post_invoice(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(invoices::create_invoice(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_invoices(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(invoices::invoices(graph.as_ref(), &scope))
}

async fn get_invoice(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(invoice_number): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(invoices::invoice(graph.as_ref(), &scope, &invoice_number))
}

async fn put_invoice(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(invoice_number): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(invoices::update_invoice(
        graph.as_ref(),
        &scope,
        &invoice_number,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn delete_invoice(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(invoice_number): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(invoices::delete_invoice(
        graph.as_ref(),
        &scope,
        &invoice_number,
    ))
}

async fn post_invoice_payment(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(invoice_number): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(invoices::record_invoice_payment(
        graph.as_ref(),
        &scope,
        &invoice_number,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_invoice_payments(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(invoice_number): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(invoices::invoice_payments(
        graph.as_ref(),
        &scope,
        &invoice_number,
    ))
}

// ---------------------------------------------------------------------------
// The automation engine: the rules a Book automates, and what happened when one ran.
// ---------------------------------------------------------------------------

async fn post_automation(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(automation::automations::create_automation_rule(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_automations(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(automation::automations::automations(
        graph.as_ref(),
        &scope,
        &merged_request(None, query.as_deref()),
    ))
}

async fn get_automation(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(rule_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(automation::automations::automation(
        graph.as_ref(),
        &scope,
        &rule_id,
    ))
}

async fn delete_automation(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(rule_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(automation::automations::delete_automation(
        graph.as_ref(),
        &scope,
        &rule_id,
    ))
}

async fn toggle_automation(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(rule_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(automation::automations::toggle_automation(
        graph.as_ref(),
        &scope,
        &rule_id,
    ))
}

async fn post_automation_run(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(rule_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(automation::automations::execute_automation(
        graph.as_ref(),
        &scope,
        &rule_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_automation_runs(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(automation::automations::executions(
        graph.as_ref(),
        &scope,
        &merged_request(None, query.as_deref()),
    ))
}

async fn get_automation_run(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(execution_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(automation::automations::execution(
        graph.as_ref(),
        &scope,
        &execution_id,
    ))
}

// ---------------------------------------------------------------------------
// Benchmarking: the metrics an industry is measured on, and where one Book falls against them.
// ---------------------------------------------------------------------------

async fn post_benchmark_metric(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(benchmarking::create_metric(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_benchmark_metrics(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(benchmarking::benchmark_metrics(
        graph.as_ref(),
        &scope,
        &merged_request(None, query.as_deref()),
    ))
}

async fn post_benchmark(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(benchmarking::benchmark(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_benchmark_results(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(benchmarking::benchmark_results(
        graph.as_ref(),
        &scope,
        &merged_request(None, query.as_deref()),
    ))
}

async fn post_peer_comparison(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(benchmarking::add_peer_comparison(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_peer_comparisons(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(metric_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(benchmarking::peer_comparisons(
        graph.as_ref(),
        &scope,
        &metric_id,
        &merged_request(None, query.as_deref()),
    ))
}

// ---------------------------------------------------------------------------
// The privacy register: the data subject requests a Book must answer, and the consent it must respect.
// ---------------------------------------------------------------------------

async fn post_privacy_request(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(compliance::privacy::open_request(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_privacy_requests(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(compliance::privacy::privacy_requests(
        graph.as_ref(),
        &scope,
        &merged_request(None, query.as_deref()),
    ))
}

async fn put_privacy_request(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(request_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(compliance::privacy::amend_request(
        graph.as_ref(),
        &scope,
        &request_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn delete_privacy_request(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(request_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(compliance::privacy::delete_request(
        graph.as_ref(),
        &scope,
        &request_id,
    ))
}

async fn post_consent(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(compliance::privacy::record_consent(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn delete_consent(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(consent_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(compliance::privacy::revoke_consent(
        graph.as_ref(),
        &scope,
        &consent_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_consent_for(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(subject): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(compliance::privacy::consents_for(
        graph.as_ref(),
        &scope,
        &subject,
    ))
}

async fn get_privacy_dashboard(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(compliance::privacy::privacy_dashboard(
        graph.as_ref(),
        &scope,
    ))
}

// ---------------------------------------------------------------------------
// Encrypted backups: the backups a Book took, the key each was sealed with, and the restores asked of them.
// ---------------------------------------------------------------------------

async fn post_backup(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::backup::take_backup(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_backups(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::backup::backups(
        graph.as_ref(),
        &scope,
        &merged_request(None, query.as_deref()),
    ))
}

async fn get_backup(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(backup_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::backup::backup(graph.as_ref(), &scope, &backup_id))
}

async fn post_backup_restore(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(backup_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::backup::restore(
        graph.as_ref(),
        &scope,
        &backup_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn delete_backup(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(backup_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::backup::delete_backup(
        graph.as_ref(),
        &scope,
        &backup_id,
    ))
}

async fn get_restores(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::backup::restores(graph.as_ref(), &scope))
}

// ---------------------------------------------------------------------------
// The Book's data shelf: the warehouse's dimensions, facts and queries, and the ETL pipelines that fill it.
// ---------------------------------------------------------------------------

async fn post_dimension(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::warehouse::create_dimension(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_dimensions(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::warehouse::dimensions(graph.as_ref(), &scope))
}

async fn post_fact(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::warehouse::create_fact(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_facts(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::warehouse::facts(graph.as_ref(), &scope))
}

async fn post_warehouse_query(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::warehouse::run_query(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn post_warehouse_load(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::warehouse::load(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_warehouse_loads(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::warehouse::loads(graph.as_ref(), &scope))
}

async fn post_pipeline(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::pipelines::create_pipeline(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_pipelines(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::pipelines::pipelines(
        graph.as_ref(),
        &scope,
        &merged_request(None, query.as_deref()),
    ))
}

async fn get_pipeline(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(pipeline_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::pipelines::pipeline(
        graph.as_ref(),
        &scope,
        &pipeline_id,
    ))
}

async fn post_pipeline_run(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(pipeline_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::pipelines::run_pipeline(
        graph.as_ref(),
        &scope,
        &pipeline_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn delete_pipeline(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(pipeline_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(data::pipelines::delete_pipeline(
        graph.as_ref(),
        &scope,
        &pipeline_id,
    ))
}

// ---------------------------------------------------------------------------
// The Book event bus: publishing, the subscriptions that listen, and every delivery attempt against one event.
// ---------------------------------------------------------------------------

fn bus_payload(payload: Option<Json<Value>>, query: Option<&str>) -> Value {
    merged_request(payload, query)
}

async fn post_bus_publish(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let sender = crate::outbound::PlainHttp;
    answer(events::events::publish(
        graph.as_ref(),
        &scope,
        &bus_payload(payload, query.as_deref()),
        &sender,
    ))
}

async fn post_bus_publish_type(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(event_type): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let sender = crate::outbound::PlainHttp;
    answer(events::events::publish_by_type(
        graph.as_ref(),
        &scope,
        &event_type,
        &bus_payload(payload, query.as_deref()),
        &sender,
    ))
}

async fn get_bus_events(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(events::events::bus_events(
        graph.as_ref(),
        &scope,
        &bus_payload(None, query.as_deref()),
    ))
}

async fn get_bus_event(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(event_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(events::events::bus_event(graph.as_ref(), &scope, &event_id))
}

async fn get_bus_event_deliveries(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(event_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(events::events::event_deliveries(
        graph.as_ref(),
        &scope,
        &event_id,
    ))
}

async fn post_bus_subscription(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(events::events::bus_subscribe(
        graph.as_ref(),
        &scope,
        &bus_payload(payload, query.as_deref()),
    ))
}

async fn get_bus_subscriptions(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(events::events::bus_subscriptions(graph.as_ref(), &scope))
}

async fn get_bus_subscription(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(subscription_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(events::events::get_subscription(
        graph.as_ref(),
        &scope,
        &subscription_id,
    ))
}

async fn delete_bus_subscription(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(subscription_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(events::events::delete_subscription(
        graph.as_ref(),
        &scope,
        &subscription_id,
    ))
}

async fn get_bus_event_types() -> Response {
    answer(Ok(events::events::event_types()))
}

async fn get_bus_metrics(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(events::events::metrics(graph.as_ref(), &scope))
}

async fn get_bus_health(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(events::events::health(graph.as_ref(), &scope))
}

async fn post_bus_trigger_journal(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let sender = crate::outbound::PlainHttp;
    answer(events::events::trigger_journal_entry(
        graph.as_ref(),
        &scope,
        &bus_payload(payload, query.as_deref()),
        &sender,
    ))
}

async fn post_bus_trigger_variance(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let sender = crate::outbound::PlainHttp;
    answer(events::events::trigger_budget_variance(
        graph.as_ref(),
        &scope,
        &bus_payload(payload, query.as_deref()),
        &sender,
    ))
}

async fn post_bus_trigger_flagged(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let sender = crate::outbound::PlainHttp;
    answer(events::events::trigger_transaction_flagged(
        graph.as_ref(),
        &scope,
        &bus_payload(payload, query.as_deref()),
        &sender,
    ))
}

async fn post_bus_trigger_approval(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let sender = crate::outbound::PlainHttp;
    answer(events::events::trigger_approval_requested(
        graph.as_ref(),
        &scope,
        &bus_payload(payload, query.as_deref()),
        &sender,
    ))
}

// ---------------------------------------------------------------------------
// Book sync: the shared Books a person belongs to. A Book here is identified by its own id - the caller's Book header
// is only used to scope the store, so these entry points work the same from inside any Book.
// ---------------------------------------------------------------------------

async fn post_book(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book_sync::sync::create_book(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_books(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book_sync::sync::list_books(graph.as_ref(), &scope))
}

async fn post_book_default(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book_sync::sync::default_book(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_book(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book_sync::sync::get_book(graph.as_ref(), &scope, &book_id))
}

async fn put_book(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book_sync::sync::update_book(
        graph.as_ref(),
        &scope,
        &book_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_book_shared(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    match book_sync::sync::membership_check(graph.as_ref(), &scope, &book_id) {
        Ok(_) => answer(
            book_sync::sync::is_shared(graph.as_ref(), &book_id)
                .map(|shared| json!({"shared": shared})),
        ),
        Err(error) => answer(Err(error)),
    }
}

async fn post_book_member(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book_sync::sync::invite_member(
        graph.as_ref(),
        &scope,
        &book_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_book_members(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book_sync::sync::book_members(
        graph.as_ref(),
        &scope,
        &book_id,
    ))
}

async fn post_book_member_accept(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book_sync::sync::accept_invite(
        graph.as_ref(),
        &scope,
        &book_id,
    ))
}

async fn put_book_member(
    State(graph): GraphState,
    headers: HeaderMap,
    Path((book_id, member_user_id)): Path<(String, String)>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book_sync::sync::update_member(
        graph.as_ref(),
        &scope,
        &book_id,
        &member_user_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn post_book_device(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book_sync::sync::register_device(
        graph.as_ref(),
        &scope,
        &book_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_book_devices(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book_sync::sync::list_devices(
        graph.as_ref(),
        &scope,
        &book_id,
    ))
}

async fn post_book_entries_push(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book_sync::sync::push_entries(
        graph.as_ref(),
        &scope,
        &book_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_book_entries(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(None, query.as_deref());
    answer(book_sync::sync::pull_entries(
        graph.as_ref(),
        &scope,
        &book_id,
        &asked,
    ))
}

async fn post_book_metadata(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book_sync::sync::upsert_metadata(
        graph.as_ref(),
        &scope,
        &book_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_book_metadata(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(None, query.as_deref());
    answer(book_sync::sync::pull_metadata(
        graph.as_ref(),
        &scope,
        &book_id,
        &asked,
    ))
}

async fn get_book_audit(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(None, query.as_deref());
    answer(book_sync::sync::audit_log(
        graph.as_ref(),
        &scope,
        &book_id,
        &asked,
    ))
}

async fn get_book_membership(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book_sync::sync::membership_check(
        graph.as_ref(),
        &scope,
        &book_id,
    ))
}

// ---------------------------------------------------------------------------
// MFA: the second factor a person carries. Self-service only, so the person is always the caller.
// ---------------------------------------------------------------------------

async fn post_mfa_setup(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(mfa::mfa::setup(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn post_mfa_verify(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(mfa::mfa::verify(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn post_mfa_challenge(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(mfa::mfa::raise_challenge(
        graph.as_ref(),
        &scope,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn post_mfa_challenge_verify(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(challenge_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(mfa::mfa::verify_challenge(
        graph.as_ref(),
        &scope,
        &challenge_id,
        &merged_request(payload, query.as_deref()),
    ))
}

async fn get_mfa_status(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(mfa::mfa::enrolment_status(graph.as_ref(), &scope))
}

async fn get_mfa_backup_codes(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(mfa::mfa::backup_remaining(graph.as_ref(), &scope))
}

// ---------------------------------------------------------------------------
// financial identity: the KYC profile a Book keeps for a person, and the verification done against it. A profile
// names a SUBJECT, which is not the caller, so it answers at /profiles and is read back by id or by subject.
// ---------------------------------------------------------------------------

async fn post_profile(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(identity::kyc::create_profile(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_profile(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(profile_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(identity::kyc::get_profile(
        graph.as_ref(),
        &scope,
        &profile_id,
    ))
}

async fn put_profile_verify(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(profile_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    // the documents arrive as repeated query parameters, which is how the shipped endpoint took them
    let asked = merged_request(payload, query.as_deref());
    answer(identity::kyc::verify_profile(
        graph.as_ref(),
        &scope,
        &profile_id,
        &asked,
    ))
}

async fn get_profile_for_subject(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(subject): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(identity::kyc::profile_for_subject(
        graph.as_ref(),
        &scope,
        &subject,
    ))
}

// ---------------------------------------------------------------------------
// regulatory compliance: the regulations a company is held to and the dashboard that adds them up. The listing
// answers at /regulations/{company} and the board at /regulation-dashboard, because a bare /dashboard would not say
// whose figures they are.
// ---------------------------------------------------------------------------

async fn post_regulation(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(compliance::regulation::create_regulation(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_regulations(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let mut asked = merged_request(None, query.as_deref());
    // the company is the one the path names
    asked["company_id"] = json!(company_id);
    answer(compliance::regulation::list_regulations(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn post_regulation_status(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(regulation_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(compliance::regulation::update_regulation_status(
        graph.as_ref(),
        &scope,
        &regulation_id,
        &asked,
    ))
}

async fn get_regulation_dashboard(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(None, query.as_deref());
    answer(compliance::regulation::compliance_dashboard(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

// ---------------------------------------------------------------------------
// audit compliance: the immutable trail, resource versions, lineage, chain integrity, and the planning, report,
// trail and compliance readings drawn from a caller's own figures. Every handler reads its arguments from the
// body or the query, the body winning, because the shipped endpoints were written as query parameters.
// ---------------------------------------------------------------------------

async fn post_audit_event(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(audit_services::compliance::create_event(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn list_audit_events(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(audit_services::compliance::list_events(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_audit_event(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(event_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(audit_services::compliance::get_event(
        graph.as_ref(),
        &scope,
        &event_id,
    ))
}

async fn post_version_snapshot(
    State(graph): GraphState,
    headers: HeaderMap,
    Path((resource_type, resource_id)): Path<(String, String)>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(audit_services::compliance::create_version(
        graph.as_ref(),
        &scope,
        &resource_type,
        &resource_id,
        &asked,
    ))
}

async fn get_version_history(
    State(graph): GraphState,
    headers: HeaderMap,
    Path((resource_type, resource_id)): Path<(String, String)>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(audit_services::compliance::version_history(
        graph.as_ref(),
        &scope,
        &resource_type,
        &resource_id,
    ))
}

async fn get_audit_lineage(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(resource_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(None, query.as_deref());
    answer(audit_services::compliance::lineage(
        graph.as_ref(),
        &scope,
        &resource_id,
        &asked,
    ))
}

async fn get_audit_chain_integrity(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(None, query.as_deref());
    answer(audit_services::compliance::verify_chain(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_audit_event_integrity(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(event_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(None, query.as_deref());
    answer(audit_services::compliance::verify_event(
        graph.as_ref(),
        &scope,
        &event_id,
        &asked,
    ))
}

async fn post_audit_plan(headers: HeaderMap, payload: Option<Json<Value>>) -> Response {
    if let Err(response) = scope_of(&headers) {
        return *response;
    }
    // a plan is answered from the caller's own figures and kept nowhere
    answer(Ok(audit_services::compliance::plan_audit(&merged_request(
        payload, None,
    ))))
}

async fn post_audit_report_generate(headers: HeaderMap, payload: Option<Json<Value>>) -> Response {
    if let Err(response) = scope_of(&headers) {
        return *response;
    }
    answer(Ok(audit_services::compliance::generate_audit_report(
        &merged_request(payload, None),
    )))
}

async fn post_audit_trails(headers: HeaderMap, payload: Option<Json<Value>>) -> Response {
    if let Err(response) = scope_of(&headers) {
        return *response;
    }
    answer(Ok(audit_services::compliance::analyse_trails(
        &merged_request(payload, None),
    )))
}

async fn post_compliance_monitor(headers: HeaderMap, payload: Option<Json<Value>>) -> Response {
    if let Err(response) = scope_of(&headers) {
        return *response;
    }
    answer(Ok(audit_services::compliance::monitor_compliance(
        &merged_request(payload, None),
    )))
}

async fn post_compliance_sox(headers: HeaderMap, payload: Option<Json<Value>>) -> Response {
    if let Err(response) = scope_of(&headers) {
        return *response;
    }
    answer(Ok(audit_services::compliance::assess_sox(&merged_request(
        payload, None,
    ))))
}

async fn post_compliance_gdpr(headers: HeaderMap, payload: Option<Json<Value>>) -> Response {
    if let Err(response) = scope_of(&headers) {
        return *response;
    }
    answer(Ok(audit_services::compliance::assess_gdpr(
        &merged_request(payload, None),
    )))
}

async fn post_compliance_report(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(audit_services::compliance::compliance_report(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_audit_analytics(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(None, query.as_deref());
    answer(audit_services::compliance::analytics(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_audit_search(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(None, query.as_deref());
    answer(audit_services::compliance::search(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

// audit: the engagements an auditor runs with their findings, and the sample it draws. Every handler reads its
// arguments from the body or the query, the body winning, because the shipped endpoints were query parameters.
// ---------------------------------------------------------------------------

async fn post_engagement(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(audit_services::engagement::create_engagement(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_audit_types(headers: HeaderMap) -> Response {
    if let Err(response) = scope_of(&headers) {
        return *response;
    }
    answer(Ok(audit_services::engagement::audit_types()))
}

async fn list_engagements(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(audit_services::engagement::list_engagements(
        graph.as_ref(),
        &scope,
        &company_id,
        &asked,
    ))
}

async fn put_engagement_status(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(engagement_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(audit_services::engagement::move_engagement(
        graph.as_ref(),
        &scope,
        &engagement_id,
        &asked,
    ))
}

async fn post_engagement_finding(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(engagement_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(audit_services::engagement::add_finding(
        graph.as_ref(),
        &scope,
        &engagement_id,
        &asked,
    ))
}

async fn put_finding_remediate(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(finding_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(audit_services::engagement::remediate_finding(
        graph.as_ref(),
        &scope,
        &finding_id,
    ))
}

async fn get_audit_report(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(engagement_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(audit_services::engagement::audit_report(
        graph.as_ref(),
        &scope,
        &engagement_id,
    ))
}

async fn post_audit_sample(headers: HeaderMap, payload: Option<Json<Value>>) -> Response {
    if let Err(response) = scope_of(&headers) {
        return *response;
    }
    // a sample is answered and forgotten, so it needs no store and no Book
    let asked = merged_request(payload, None);
    answer(audit_services::sampling::draw_sample(&asked).map_err(store::StoreError::Invalid))
}

// compliance: the SOX controls a Book tests and the risks it carries. The dashboards are named for their
// service (/sox-dashboard, /risk-dashboard/{company}) because a bare /dashboard would not say whose figures
// they are. Every handler reads its arguments from the body or the query, the body winning, because the
// shipped endpoints were written as query parameters.
// ---------------------------------------------------------------------------

async fn post_sox_control(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(compliance::sox::create_control(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn list_sox_controls(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(compliance::sox::list_controls(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn post_sox_control_test(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(control_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(compliance::sox::test_control(
        graph.as_ref(),
        &scope,
        &control_id,
        &asked,
    ))
}

async fn list_sox_control_tests(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(control_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(compliance::sox::list_tests(
        graph.as_ref(),
        &scope,
        &control_id,
    ))
}

async fn post_sox_deficiency(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(compliance::sox::create_deficiency(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn list_sox_deficiencies(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(compliance::sox::list_deficiencies(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn put_sox_deficiency(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(deficiency_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(compliance::sox::update_deficiency(
        graph.as_ref(),
        &scope,
        &deficiency_id,
        &asked,
    ))
}

async fn get_sox_dashboard(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(compliance::sox::sox_dashboard(graph.as_ref(), &scope))
}

async fn post_risk(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(compliance::risks::create_risk(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn list_risks(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(compliance::risks::list_risks(
        graph.as_ref(),
        &scope,
        &company_id,
        &asked,
    ))
}

async fn put_risk(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(risk_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(compliance::risks::update_risk(
        graph.as_ref(),
        &scope,
        &risk_id,
        &asked,
    ))
}

async fn delete_risk(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(risk_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(compliance::risks::close_risk(
        graph.as_ref(),
        &scope,
        &risk_id,
        &asked,
    ))
}

async fn get_risk_dashboard(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(compliance::risks::risk_dashboard(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

// reporting, automation and distribution: the templates a Book keeps, the schedules they run on, the reports they
// produced, the lists those go to and the record of what was sent. The listing of produced reports is called
// /generated-reports here because plain /reports already answers the receivables control account.
// ---------------------------------------------------------------------------

async fn post_automation_template(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(reporting_services::automation::create_template(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn list_automation_templates(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    answer(reporting_services::automation::list_templates(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn post_automation_schedule(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(reporting_services::automation::create_schedule(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn list_automation_schedules(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    answer(reporting_services::automation::list_schedules(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn post_automation_schedule_run(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(schedule_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    answer(reporting_services::automation::run_report(
        graph.as_ref(),
        &scope,
        &schedule_id,
        &asked,
    ))
}

async fn get_generated_reports(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    answer(reporting_services::automation::list_generated_reports(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn put_automation_schedule_pause(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(schedule_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(reporting_services::automation::pause_schedule(
        graph.as_ref(),
        &scope,
        &schedule_id,
    ))
}

async fn put_automation_schedule_resume(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(schedule_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(reporting_services::automation::resume_schedule(
        graph.as_ref(),
        &scope,
        &schedule_id,
    ))
}

async fn post_distribution_list(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(reporting_services::distribution::create_list(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn list_distribution_lists(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(reporting_services::distribution::list_lists(
        graph.as_ref(),
        &scope,
    ))
}

async fn post_distribution_subscription(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(reporting_services::distribution::create_subscription(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn list_distribution_subscriptions(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    answer(reporting_services::distribution::list_subscriptions(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn post_distribution_send(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(subscription_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    answer(reporting_services::distribution::distribute_report(
        graph.as_ref(),
        &scope,
        &subscription_id,
        &asked,
    ))
}

async fn list_distribution_logs(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    answer(reporting_services::distribution::list_logs(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

// ---------------------------------------------------------------------------
// reporting: the reports a Book asks for, and the dashboard layouts its figures are read on. Both are shapes
// rather than readings, and both say so, which is how they shipped.
// ---------------------------------------------------------------------------

async fn post_report_generate(Json(payload): Json<Value>) -> Response {
    answer(reporting_services::reports::generate_report(&payload))
}

async fn post_report_schedule(Json(payload): Json<Value>) -> Response {
    answer(reporting_services::reports::schedule_report(&payload))
}

async fn post_dashboard_load(Json(payload): Json<Value>) -> Response {
    answer(reporting_services::dashboards::load_dashboard(&payload))
}

async fn post_dashboard_custom(Json(payload): Json<Value>) -> Response {
    answer(reporting_services::dashboards::create_custom_dashboard(
        &payload,
    ))
}

// ---------------------------------------------------------------------------
// bank relationships and service quality, plus the plain connection door. Several paths carry a bank- prefix
// because /connections answers the feed's door and /bank-transactions/:id answers one feed transaction.
// ---------------------------------------------------------------------------

async fn post_relationship(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::relationship::create_relationship(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn get_relationships(
    State(graph): GraphState,
    headers: HeaderMap,
    // the same position as the update below, so the parameter carries one name: a GET names a company and a PUT
    // names a relationship
    Path(id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::relationship::list_relationships(
        graph.as_ref(),
        &scope,
        &id,
        &body_or_query(query.as_deref()),
    ))
}

async fn put_relationship(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(relationship_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(banking::relationship::update_relationship(
        graph.as_ref(),
        &scope,
        &relationship_id,
        &asked,
    ))
}

async fn post_quality_metric(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::relationship::add_quality_metric(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn get_quality_metrics(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(relationship_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::relationship::get_quality_metrics(
        graph.as_ref(),
        &scope,
        &relationship_id,
    ))
}

async fn get_bank_relationship_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::relationship::relationship_summary(
        graph.as_ref(),
        &scope,
        &company_id,
    ))
}

async fn post_connect_bank(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(banking::integration::connect(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_bank_account_connections(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::integration::list_connections(
        graph.as_ref(),
        &scope,
    ))
}

async fn delete_bank_account_connection(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(connection_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    answer(banking::integration::disconnect(
        graph.as_ref(),
        &scope,
        &connection_id,
        &store::read::label(&asked, "today"),
    ))
}

async fn post_bank_sync(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(connection_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(banking::integration::sync(
        graph.as_ref(),
        &scope,
        &connection_id,
        &asked,
    ))
}

async fn get_connection_transactions(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(connection_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::integration::list_connection_transactions(
        graph.as_ref(),
        &scope,
        &connection_id,
        &body_or_query(query.as_deref()),
    ))
}

async fn post_bank_reconcile(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(connection_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(banking::integration::reconcile_bank_transaction(
        graph.as_ref(),
        &scope,
        &connection_id,
        &asked,
    ))
}

async fn get_bank_account_balance(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(connection_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::integration::balance_of(
        graph.as_ref(),
        &scope,
        &connection_id,
    ))
}

// ---------------------------------------------------------------------------
// bank feed: the connections a Book holds, the transactions that arrive from them, and the rules that pick
// them out. The listing is named /bank-transactions because /transactions already answers the sales ledger.
// ---------------------------------------------------------------------------

async fn post_bank_connection(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    let organization = store::read::label(&asked, "organization_id");
    answer(banking::bank_feed::create_connection(
        graph.as_ref(),
        &scope,
        &asked,
        &organization,
    ))
}

async fn get_bank_connections(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    answer(banking::bank_feed::list_connections(
        graph.as_ref(),
        &scope,
        &store::read::label(&asked, "organization_id"),
    ))
}

async fn get_bank_connection(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(connection_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::bank_feed::get_connection(
        graph.as_ref(),
        &scope,
        &connection_id,
    ))
}

async fn delete_bank_connection(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(connection_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::bank_feed::delete_connection(
        graph.as_ref(),
        &scope,
        &connection_id,
    ))
}

async fn get_bank_balance(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(connection_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    answer(banking::bank_feed::account_balance(
        graph.as_ref(),
        &scope,
        &connection_id,
        &store::read::label(&asked, "today"),
    ))
}

async fn post_bank_transaction(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::bank_feed::import_transaction(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn post_bank_transactions_batch(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    // a batch arrives as a bare list, which FastAPI took as the whole body
    let lines = match payload {
        Some(Json(Value::Array(lines))) => lines,
        payload => merged_request(payload, query.as_deref())
            .get("transactions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    };
    answer(banking::bank_feed::import_batch(
        graph.as_ref(),
        &scope,
        &lines,
    ))
}

async fn post_bank_mt940(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(banking::bank_feed::import_mt940(
        graph.as_ref(),
        &scope,
        &store::read::label(&asked, "connection_id"),
        &store::read::label(&asked, "statement_data"),
        &store::read::label(&asked, "today"),
    ))
}

async fn get_bank_transactions(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::bank_feed::list_feed_transactions(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn get_bank_transaction(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(transaction_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::bank_feed::get_feed_transaction(
        graph.as_ref(),
        &scope,
        &transaction_id,
    ))
}

async fn put_bank_transaction_status(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(transaction_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(banking::bank_feed::update_transaction_status(
        graph.as_ref(),
        &scope,
        &transaction_id,
        &asked,
    ))
}

async fn post_bank_transaction_link(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(transaction_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(banking::bank_feed::link_transaction(
        graph.as_ref(),
        &scope,
        &transaction_id,
        &asked,
    ))
}

async fn post_bank_sync_start(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::bank_feed::start_sync(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn get_bank_sync_history(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::bank_feed::sync_history(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn get_bank_sync(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(sync_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::bank_feed::get_sync(
        graph.as_ref(),
        &scope,
        &sync_id,
    ))
}

async fn post_bank_rule(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::bank_feed::create_feed_rule(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn get_bank_rules(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::bank_feed::list_feed_rules(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn put_bank_rule(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(rule_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::bank_feed::update_feed_rule(
        graph.as_ref(),
        &scope,
        &rule_id,
        &payload,
    ))
}

async fn delete_bank_rule(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(rule_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::bank_feed::delete_feed_rule(
        graph.as_ref(),
        &scope,
        &rule_id,
    ))
}

async fn post_bank_rules_apply(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    // the list of transactions arrives as a bare list, which FastAPI took as the whole body, so it is read
    // before the query is merged - an array has no keys to merge into
    let ids: Vec<String> = match payload {
        Some(Json(Value::Array(lines))) => lines
            .iter()
            .filter_map(|line| line.as_str().map(str::to_owned))
            .collect(),
        payload => merged_request(payload, query.as_deref())
            .get("transaction_ids")
            .and_then(Value::as_array)
            .map(|lines| {
                lines
                    .iter()
                    .filter_map(|line| line.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
    };
    let asked = body_or_query(query.as_deref());
    answer(banking::bank_feed::apply_rules(
        graph.as_ref(),
        &scope,
        &ids,
        &store::read::label(&asked, "today"),
    ))
}

async fn post_bank_webhook(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(provider): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::bank_feed::handle_webhook(
        graph.as_ref(),
        &scope,
        &provider,
        &payload,
    ))
}

async fn get_bank_statistics(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    answer(banking::bank_feed::statistics(
        graph.as_ref(),
        &scope,
        &store::read::label(&asked, "organization_id"),
    ))
}

// ---------------------------------------------------------------------------
// currency: the currencies a Book deals in, the rates between them, and the conversions over them
// ---------------------------------------------------------------------------

/// The day a request was made. A port has no clock, so the caller states it, exactly as the ledger's own
/// endpoints take `today`. A request that states none seeds undated, which only affects a brand new Book.
fn today_of(request: &Value) -> String {
    store::read::label(request, "today")
}

async fn get_currencies(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::currency::list_currencies(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn get_currency(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(code): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    answer(banking::currency::get_currency(
        graph.as_ref(),
        &scope,
        &code,
        &today_of(&asked),
    ))
}

async fn post_currency(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::currency::create_currency(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn put_currency(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(code): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::currency::update_currency(
        graph.as_ref(),
        &scope,
        &code,
        &payload,
    ))
}

async fn delete_currency(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(code): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    answer(banking::currency::deactivate_currency(
        graph.as_ref(),
        &scope,
        &code,
        &today_of(&asked),
    ))
}

async fn get_rates(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::currency::list_exchange_rates(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn get_latest_rates(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    answer(banking::currency::get_latest_rates(
        graph.as_ref(),
        &scope,
        &today_of(&asked),
    ))
}

async fn get_one_rate(
    State(graph): GraphState,
    headers: HeaderMap,
    Path((from_currency, to_currency)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    answer(banking::currency::get_exchange_rate(
        graph.as_ref(),
        &scope,
        &from_currency,
        &to_currency,
        &today_of(&asked),
    ))
}

async fn post_rate(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::currency::create_exchange_rate(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn put_rate(
    State(graph): GraphState,
    headers: HeaderMap,
    Path((from_currency, to_currency)): Path<(String, String)>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::currency::update_exchange_rate(
        graph.as_ref(),
        &scope,
        &from_currency,
        &to_currency,
        &payload,
    ))
}

async fn post_convert(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::currency::convert_currency(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn post_convert_batch(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    // the batch was posted as a bare JSON list, which FastAPI took as the whole body, so the list is read
    // before the query is merged - an array has no keys to merge into
    let lines = match payload {
        Some(Json(Value::Array(lines))) => lines,
        payload => merged_request(payload, query.as_deref())
            .get("requests")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    };
    answer(banking::currency::convert_batch(
        graph.as_ref(),
        &scope,
        &lines,
        &today_of(&Value::Null),
    ))
}

async fn post_triangulate(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = merged_request(payload, query.as_deref());
    answer(banking::currency::triangulate(
        graph.as_ref(),
        &scope,
        &asked,
    ))
}

async fn get_rate_history(
    State(graph): GraphState,
    headers: HeaderMap,
    Path((from_currency, to_currency)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::currency::historical_rates(
        graph.as_ref(),
        &scope,
        &from_currency,
        &to_currency,
        &body_or_query(query.as_deref()),
    ))
}

async fn get_formatted_amount(
    State(graph): GraphState,
    headers: HeaderMap,
    Path((currency_code, amount)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    answer(banking::currency::format_amount(
        graph.as_ref(),
        &scope,
        &currency_code,
        &amount,
        &today_of(&asked),
    ))
}

async fn post_validate_transaction(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::currency::validate_transaction(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn post_rate_alert(RawQuery(query): RawQuery, payload: Option<Json<Value>>) -> Response {
    // the alert was never kept, so it needs neither a Book nor a record
    answer(banking::currency::create_rate_alert(&merged_request(
        payload,
        query.as_deref(),
    )))
}

async fn get_currency_stats(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    answer(banking::currency::currency_stats(
        graph.as_ref(),
        &scope,
        &today_of(&asked),
    ))
}

// ---------------------------------------------------------------------------
// banking: statements imported from the bank, the cash book, and the reconciliations
// ---------------------------------------------------------------------------

async fn post_bank_statement(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::reconciliation::import_statement(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn get_bank_statements(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::reconciliation::list_statements(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn get_bank_statement(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(statement_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::reconciliation::get_statement(
        graph.as_ref(),
        &scope,
        &statement_id,
    ))
}

async fn post_bank_cash_entry(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::reconciliation::add_cash_book_entry(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn get_bank_cash_book(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::reconciliation::list_cash_book(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn post_bank_reconciliation(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = merged_request(payload, query.as_deref());
    answer(banking::reconciliation::create_reconciliation(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn post_bank_auto_match(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(reconciliation_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = merged_request(payload, query.as_deref());
    let statement_id = body
        .get("statement_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if statement_id.is_empty() {
        return refused("statement_id is required");
    }
    answer(banking::reconciliation::auto_match(
        graph.as_ref(),
        &scope,
        &reconciliation_id,
        &statement_id,
    ))
}

async fn post_bank_adjustments(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(reconciliation_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = merged_request(payload, query.as_deref());
    let reconciliation = match banking::reconciliation::get_reconciliation(
        graph.as_ref(),
        &scope,
        &reconciliation_id,
    ) {
        Ok(found) => found,
        Err(error) => return answer(Err(error)),
    };
    let journal = match banking::reconciliation::adjustment_journal(&reconciliation) {
        Ok(journal) => journal,
        Err(error) => return answer(Err(error)),
    };
    let today = body
        .get("today")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            reconciliation
                .get("reconciliation_date")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default();

    // nothing to adjust: the reconciliation is complete and states no journal, which is what shipped
    let declares_a_line = journal
        .get("lines")
        .and_then(Value::as_array)
        .is_some_and(|lines| !lines.is_empty());
    if !declares_a_line {
        return answer(banking::reconciliation::complete(
            graph.as_ref(),
            &scope,
            &reconciliation_id,
            None,
            &today,
        ));
    }

    // a journal is posted by the ledger's own writer, so an adjustment obeys the same rules as any other
    // entry. A bank error alone is one line, and one-sided entries are exactly what this engine refuses:
    // the shipped service handed that single line to the accounting service, which is where the refusal
    // now comes from.
    let posted = match accounting::ledger_book::create_entry(graph.as_ref(), &scope, &journal) {
        Ok(posted) => posted,
        Err(error) => return answer(Err(error)),
    };
    let journal_id = posted.get("id").and_then(Value::as_str).map(str::to_owned);
    answer(banking::reconciliation::complete(
        graph.as_ref(),
        &scope,
        &reconciliation_id,
        journal_id.as_deref(),
        &today,
    ))
}

async fn get_bank_reconciliations(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::reconciliation::list_reconciliations(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn get_bank_reconciliation(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(reconciliation_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::reconciliation::get_reconciliation(
        graph.as_ref(),
        &scope,
        &reconciliation_id,
    ))
}

async fn get_bank_outstanding_items(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(banking::reconciliation::outstanding_items(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

// ---------------------------------------------------------------------------
// payables: the purchases ledger and its control account
// ---------------------------------------------------------------------------

async fn post_creditor_transaction(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(payables::purchases_ledger::record_transaction(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn get_purchases_control_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(payables::purchases_ledger::purchases_control_summary(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn get_creditors(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(payables::purchases_ledger::list_creditors(
        graph.as_ref(),
        &scope,
    ))
}

async fn reconcile_purchases_control_account(
    State(graph): GraphState,
    headers: HeaderMap,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(payables::purchases_ledger::purchases_reconcile(
        graph.as_ref(),
        &scope,
    ))
}

// ---------------------------------------------------------------------------
// payables: the expenses claimed against that ledger
// ---------------------------------------------------------------------------

async fn post_expense(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(payables::expenses::claim_expense(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn get_expenses(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(payables::expenses::list_expense_claims(
        graph.as_ref(),
        &scope,
        &company_id,
        &body_or_query(query.as_deref()),
    ))
}

async fn approve_expense(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(expense_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    let approver = asked.get("approver").and_then(Value::as_str).unwrap_or("");
    answer(payables::expenses::approve_expense(
        graph.as_ref(),
        &scope,
        &expense_id,
        approver,
    ))
}

async fn reject_expense(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(expense_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = body_or_query(query.as_deref());
    let reason = asked.get("reason").and_then(Value::as_str).unwrap_or("");
    answer(payables::expenses::reject_expense(
        graph.as_ref(),
        &scope,
        &expense_id,
        reason,
    ))
}

async fn get_expense_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(company_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(payables::expenses::expense_summary(
        graph.as_ref(),
        &scope,
        &company_id,
        &body_or_query(query.as_deref()),
    ))
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
        // subscription plans: the plan catalogue, subscriptions and the upgrade preview
        .route("/plans", post(post_plan).get(get_plans))
        .route("/subscribe", post(post_subscribe))
        .route("/subscriptions/:company", get(get_subscriptions))
        .route("/upgrade", post(post_upgrade))

        // payroll benefits: plans, enrolments and leave
        .route(
            "/benefit-plans",
            post(post_benefit_plan).get(get_benefit_plans),
        )
        .route("/enroll", post(post_enroll))
        .route(
            "/employee/:employee_id/enrollments",
            get(get_employee_enrollments),
        )
        .route("/leave/accrue", post(post_leave_accrual))
        .route("/employee/:employee_id/leave", get(get_employee_leave))

        // zero trust: access policies, the decision taken against them, and the decisions on record
        .route(
            "/access-policies",
            post(post_access_policy).get(get_access_policies),
        )
        .route(
            "/access-policies/:policy_id",
            put(put_access_policy).delete(delete_access_policy),
        )
        .route("/evaluate", post(post_access_evaluation))
        .route("/attempts", get(get_access_attempts))
        .route("/attempts/user/:user_id", get(get_subject_attempts))

        // documents: filing, searching, the text taken off, and what each one is linked to. The register answers
        // under /files because /documents is the financial document state machine, whose records are a different thing.
        .route("/document-statistics", get(get_document_statistics))
        .route("/files/search", post(post_documents_search))
        .route(
            "/files/batch",
            post(post_documents_batch),
        )
        .route("/files/batch/link", post(post_documents_batch_link))
        .route("/files", post(post_document).get(get_documents))
        .route(
            "/files/:document_id",
            get(get_document)
                .put(put_document)
                .delete(delete_document),
        )
        .route("/files/:document_id/download", get(get_document_download))
        .route("/files/:document_id/preview", get(get_document_preview))
        .route("/files/:document_id/ocr", post(post_document_ocr))
        .route("/files/:document_id/link", post(post_document_link))

        // alerts: the rules watched, the alerts they raise, and what became of each one
        .route("/alerts/evaluate", post(post_alerts_evaluate))
        .route("/alerts/stats", get(get_alert_stats))
        .route("/alerts", post(post_alert).get(get_alerts))
        .route("/alerts/:alert_id", get(get_alert))
        .route(
            "/alerts/:alert_id/acknowledge",
            put(put_alert_acknowledge),
        )
        .route("/alerts/:alert_id/resolve", put(put_alert_resolve))
        .route("/alerts/:alert_id/dismiss", put(put_alert_dismiss))
        .route("/rules", post(post_alert_rule).get(get_alert_rules))
        .route(
            "/rules/:rule_id",
            get(get_alert_rule)
                .put(put_alert_rule)
                .delete(put_alert_rule_delete),
        )
        .route("/trigger/:rule_id", post(post_alert_trigger))

        // notifications: one person's inbox, their preferences, the templates and the notices workflows raise
        .route("/notifications/batch", post(post_notifications_batch))
        .route("/notifications", post(post_notification))
        .route(
            "/notifications/:id",
            get(get_inbox).delete(delete_one_notification),
        )
        .route("/notifications/:id/read", put(put_notification_read))
        .route("/notifications/:id/read-all", put(put_notifications_read_all))
        .route(
            "/preferences/:user_id",
            get(get_notification_preferences).put(put_notification_preferences),
        )
        .route("/notification-templates", get(get_notification_templates))
        .route(
            "/notification-templates/:name",
            get(get_notification_template),
        )
        .route(
            "/notification-templates/:name/send",
            post(post_notification_template_send),
        )
        .route("/workflow/:workflow_id/notify", post(post_workflow_notify))
        .route("/approval/:approval_id/notify", post(post_approval_notify))
        .route("/notification-stats", get(get_notification_stats))

        // webhooks: the endpoints subscribed, an event dispatched to them and the trail of attempts
        .route("/endpoints", post(post_webhook_endpoint))
        .route("/endpoints/:company_id", get(get_webhook_endpoints))
        .route("/dispatch/:company_id", post(post_webhook_dispatch))
        .route("/deliveries/:endpoint_id", get(get_webhook_deliveries))

        // cache: keyed values with a life, a size limit and the cache's own counts
        .route("/cache", post(post_cache_entry).delete(delete_whole_cache))
        .route(
            "/cache/:key",
            get(get_cache_entry).delete(delete_cache_entry),
        )
        .route("/cache-stats", get(get_cache_stats))

        // workflow: the definitions and the instances that walk them
        .route(
            "/workflow-definitions",
            get(get_workflow_definitions).post(post_workflow_definition),
        )
        .route(
            "/workflow-definitions/:id",
            get(get_workflow_definition)
                .put(put_workflow_definition)
                .delete(delete_workflow_definition),
        )
        .route("/workflow-definitions/:id/start", post(post_workflow_start))
        .route("/workflow-instances", post(post_workflow_instance))
        .route(
            "/workflow-instances/:id",
            get(get_workflow_instance)
                .put(put_workflow_instance)
                .delete(delete_workflow_instance),
        )
        .route("/workflow-instances/:id/complete", post(post_workflow_complete))

        // Multimodal capture: the inbox of records taken in, and the corrections made to them
        .route("/tasks", get(get_captures).post(post_capture))
        .route(
            "/tasks/:id",
            get(get_capture).put(put_capture).delete(delete_capture),
        )
        .route(
            "/tasks/:id/corrections",
            get(get_capture_corrections).post(post_capture_correction),
        )
        .route("/inbox", get(get_capture_inbox))
        .route("/inbox/summary", get(get_capture_summary))
        .route("/inbox/:id/organize", post(post_capture_organize))

        // Distributed workflow orchestration: the steps a workflow walks and its rollback
        .route(
            "/workflows",
            get(get_orchestrations).post(post_orchestration),
        )
        .route("/workflows/:id", get(get_orchestration).delete(delete_orchestration))
        .route("/workflows/:id/execute", post(post_orchestration_run))
        .route(
            "/workflows/:id/compensate",
            post(post_orchestration_rollback),
        )

        // Invoicing: customers, their invoices, and what has been paid against them
        .route(
            "/customers",
            get(get_invoice_customers).post(post_invoice_customer),
        )
        .route(
            "/customers/:id",
            get(get_invoice_customer)
                .put(put_invoice_customer)
                .delete(delete_invoice_customer),
        )
        .route("/invoices", get(get_invoices).post(post_invoice))
        .route(
            "/invoices/:id",
            get(get_invoice).put(put_invoice).delete(delete_invoice),
        )
        .route(
            "/invoices/:id/record-payment",
            post(post_invoice_payment),
        )
        .route("/invoices/:id/payments", get(get_invoice_payments))

        // The automation engine: rules, their switches, and the runs behind them
        .route(
            "/automation-rules",
            get(get_automations).post(post_automation),
        )
        .route(
            "/automation-rules/:id",
            get(get_automation).delete(delete_automation),
        )
        .route("/automation-rules/:id/toggle", post(toggle_automation))
        .route("/automation-rules/:id/execute", post(post_automation_run))
        .route("/automation-executions", get(get_automation_runs))
        .route(
            "/automation-executions/:id",
            get(get_automation_run),
        )

        // Benchmarking: metrics, placements and peers
        .route(
            "/metrics",
            get(get_benchmark_metrics).post(post_benchmark_metric),
        )
        .route("/benchmark", post(post_benchmark))
        .route("/results", get(get_benchmark_results))
        .route("/peer-comparisons", post(post_peer_comparison))
        .route("/peer-comparisons/:metric_id", get(get_peer_comparisons))

        // The privacy register: subject requests and the consent behind them
        .route("/requests", get(get_privacy_requests).post(post_privacy_request))
        .route(
            "/requests/:id",
            put(put_privacy_request).delete(delete_privacy_request),
        )
        .route("/consent", post(post_consent))
        // the shipped service answered GET by email and DELETE by id under one path; here one key serves both
        .route(
            "/consent/:subject",
            get(get_consent_for).delete(delete_consent),
        )
        .route("/dashboard", get(get_privacy_dashboard))

        // Encrypted backups and the restores asked of them
        .route("/backup", post(post_backup))
        .route("/backups", get(get_backups))
        .route("/backups/:id", get(get_backup).delete(delete_backup))
        .route("/backups/:id/restore", post(post_backup_restore))
        .route("/restores", get(get_restores))

        // The Book's data shelf: warehouse tables and queries, and the ETL pipelines behind them
        .route("/dimensions", get(get_dimensions).post(post_dimension))
        .route("/facts", get(get_facts).post(post_fact))
        .route("/query", post(post_warehouse_query))
        .route("/etl", get(get_warehouse_loads).post(post_warehouse_load))
        .route("/pipelines", get(get_pipelines).post(post_pipeline))
        .route("/pipelines/:id", get(get_pipeline).delete(delete_pipeline))
        .route("/pipelines/:id/run", post(post_pipeline_run))

        // The Book event bus: publishing, subscriptions and delivery attempts
        .route("/bus/health", get(get_bus_health))
        .route("/bus/metrics", get(get_bus_metrics))
        .route("/event-types", get(get_bus_event_types))
        .route("/bus/events", get(get_bus_events))
        .route("/bus/events/publish", post(post_bus_publish))
        .route("/bus/events/:event_type/publish", post(post_bus_publish_type))
        .route("/bus/events/:id", get(get_bus_event))
        .route("/bus/events/:id/deliveries", get(get_bus_event_deliveries))
        .route(
            "/bus/event-subscriptions",
            get(get_bus_subscriptions).post(post_bus_subscription),
        )
        .route(
            "/bus/event-subscriptions/:id",
            get(get_bus_subscription).delete(delete_bus_subscription),
        )
        .route("/bus/trigger/journal-entry-created", post(post_bus_trigger_journal))
        .route("/bus/trigger/budget-variance-alert", post(post_bus_trigger_variance))
        .route("/bus/trigger/transaction-flagged", post(post_bus_trigger_flagged))
        .route("/bus/trigger/approval-requested", post(post_bus_trigger_approval))

        // Book sync: the shared Books a person belongs to, their members, devices and encrypted entries
        .route("/books", get(get_books).post(post_book))
        .route("/books/default", post(post_book_default))
        .route("/books/:id", get(get_book).put(put_book))
        .route("/books/:id/shared", get(get_book_shared))
        .route("/books/:id/members", get(get_book_members).post(post_book_member))
        .route("/books/:id/members/accept", post(post_book_member_accept))
        .route("/books/:id/members/:user", put(put_book_member))
        .route("/books/:id/devices", get(get_book_devices).post(post_book_device))
        .route("/books/:id/entries", get(get_book_entries))
        .route("/books/:id/entries/push", post(post_book_entries_push))
        .route("/books/:id/metadata", get(get_book_metadata).post(post_book_metadata))
        .route("/books/:id/audit", get(get_book_audit))
        .route("/books/:id/membership", get(get_book_membership))

        // MFA: the second factor a person carries, self-service only
        .route("/mfa/setup", post(post_mfa_setup))
        .route("/mfa/verify", post(post_mfa_verify))
        .route("/mfa/challenge", post(post_mfa_challenge))
        .route("/mfa/challenge/:id/verify", post(post_mfa_challenge_verify))
        .route("/mfa/status", get(get_mfa_status))
        .route("/mfa/backup-codes", get(get_mfa_backup_codes))

        // financial identity: the KYC profile register
        .route("/profiles", post(post_profile))
        .route("/profiles/by-subject/:user", get(get_profile_for_subject))
        .route("/profiles/:id", get(get_profile))
        .route("/profiles/:id/verify", put(put_profile_verify))

        // regulatory compliance: the regulation register and the board it adds up to
        .route("/regulations", post(post_regulation))
        .route("/regulations/:company", get(get_regulations))
        .route("/regulations/:id/update", post(post_regulation_status))
        .route("/regulation-dashboard", get(get_regulation_dashboard))

        // audit compliance: the trail, versions, lineage, integrity and the readings drawn from them
        .route("/events", post(post_audit_event).get(list_audit_events))
        .route("/events/:id", get(get_audit_event))
        .route(
            "/versions/:resource_type/:resource_id",
            post(post_version_snapshot).get(get_version_history),
        )
        .route("/lineage/:id", get(get_audit_lineage))
        .route("/integrity/verify", get(get_audit_chain_integrity))
        .route("/integrity/verify-event/:id", get(get_audit_event_integrity))
        .route("/planning/plan", post(post_audit_plan))
        .route("/reports/generate", post(post_audit_report_generate))
        .route("/trails/analyze", post(post_audit_trails))
        .route("/compliance/monitor", post(post_compliance_monitor))
        .route("/compliance/sox", post(post_compliance_sox))
        .route("/compliance/gdpr", post(post_compliance_gdpr))
        .route("/compliance/report", post(post_compliance_report))
        .route("/analytics/summary", get(get_audit_analytics))
        .route("/search", get(get_audit_search))

        // audit: the engagements an auditor runs and the sample it draws
        .route("/engagements", post(post_engagement))
        .route("/audit-types", get(get_audit_types))
        .route("/engagements/:company", get(list_engagements))
        .route("/engagements/:id/status", put(put_engagement_status))
        .route("/engagements/:id/findings", post(post_engagement_finding))
        .route("/findings/:id/remediate", put(put_finding_remediate))
        .route("/engagement-report/:id", get(get_audit_report))
        .route("/sample", post(post_audit_sample))

        // compliance: the SOX controls a Book tests and the risks it carries
        .route("/controls", post(post_sox_control).get(list_sox_controls))
        .route("/controls/:id/test", post(post_sox_control_test))
        .route("/controls/:id/tests", get(list_sox_control_tests))
        .route(
            "/deficiencies",
            post(post_sox_deficiency).get(list_sox_deficiencies),
        )
        .route("/deficiencies/:id", put(put_sox_deficiency))
        .route("/sox-dashboard", get(get_sox_dashboard))
        .route("/risks", post(post_risk))
        // the same shape answers the register and works one risk, as the shipped service did: one segment that
        // is a company on a read and a risk on a write
        .route("/risks/:id", get(list_risks).put(put_risk).delete(delete_risk))
        .route("/risk-dashboard/:company", get(get_risk_dashboard))

        // reporting, automation and distribution
        .route(
            "/templates",
            post(post_automation_template).get(list_automation_templates),
        )
        .route(
            "/schedules",
            post(post_automation_schedule).get(list_automation_schedules),
        )
        .route("/schedules/:id/run", post(post_automation_schedule_run))
        .route("/schedules/:id/pause", put(put_automation_schedule_pause))
        .route("/schedules/:id/resume", put(put_automation_schedule_resume))
        .route("/generated-reports", get(get_generated_reports))
        .route(
            "/lists",
            post(post_distribution_list).get(list_distribution_lists),
        )
        .route(
            "/subscriptions",
            post(post_distribution_subscription).get(list_distribution_subscriptions),
        )
        .route("/subscriptions/:id/distribute", post(post_distribution_send))
        .route("/logs", get(list_distribution_logs))

        // reporting: a report asked for, a schedule recorded, and the dashboard layouts
        .route("/generate", post(post_report_generate))
        .route("/schedule", post(post_report_schedule))
        .route("/load", post(post_dashboard_load))
        .route("/custom", post(post_dashboard_custom))

        // bank relationships and service quality, and the plain connection door. Several paths carry a
        // bank- prefix because /connections answers the feed's door.
        .route("/relationships", post(post_relationship))
        // one path, two methods: a GET names a company and a PUT names a relationship
        .route(
            "/relationships/:id",
            get(get_relationships).put(put_relationship),
        )
        .route("/quality-metrics", post(post_quality_metric))
        .route(
            "/quality-metrics/:relationship_id",
            get(get_quality_metrics),
        )
        .route("/bank-summary/:company_id", get(get_bank_relationship_summary))
        .route("/connect", post(post_connect_bank))
        .route("/bank-account-connections", get(get_bank_account_connections))
        .route(
            "/bank-account-connections/:id",
            delete(delete_bank_account_connection),
        )
        .route("/bank-sync/:connection_id", post(post_bank_sync))
        .route(
            "/bank-connection-transactions/:connection_id",
            get(get_connection_transactions),
        )
        .route("/bank-reconcile/:connection_id", post(post_bank_reconcile))
        .route(
            "/bank-account-balance/:connection_id",
            get(get_bank_account_balance),
        )

        // bank feed: connections, the transactions that arrive from them, and the rules that pick them out.
        // The listing is /bank-transactions because /transactions already answers the sales ledger.
        .route(
            "/connections",
            get(get_bank_connections).post(post_bank_connection),
        )
        .route(
            "/connections/:id",
            get(get_bank_connection).delete(delete_bank_connection),
        )
        .route("/connections/:id/balance", get(get_bank_balance))
        .route("/bank-transactions", get(get_bank_transactions))
        .route(
            "/bank-transactions/import",
            post(post_bank_transaction),
        )
        .route(
            "/bank-transactions/import-batch",
            post(post_bank_transactions_batch),
        )
        .route("/bank-transactions/import-mt940", post(post_bank_mt940))
        .route("/bank-transactions/:id", get(get_bank_transaction))
        .route(
            "/bank-transactions/:id/status",
            put(put_bank_transaction_status),
        )
        .route(
            "/bank-transactions/:id/link",
            post(post_bank_transaction_link),
        )
        .route("/sync/start", post(post_bank_sync_start))
        .route("/sync/history", get(get_bank_sync_history))
        .route("/sync/:id", get(get_bank_sync))
        .route("/bank-rules", get(get_bank_rules).post(post_bank_rule))
        .route(
            "/bank-rules/apply",
            post(post_bank_rules_apply),
        )
        .route(
            "/bank-rules/:id",
            put(put_bank_rule).delete(delete_bank_rule),
        )
        .route("/webhooks/:provider", post(post_bank_webhook))
        .route("/statistics", get(get_bank_statistics))

        // currency: the currencies a Book deals in, the rates between them and the conversions over them
        .route("/currencies", get(get_currencies).post(post_currency))
        .route(
            "/currencies/:code",
            get(get_currency).put(put_currency).delete(delete_currency),
        )
        .route("/rates", get(get_rates).post(post_rate))
        .route("/rates/latest", get(get_latest_rates))
        .route(
            "/rates/history/:from_currency/:to_currency",
            get(get_rate_history),
        )
        .route(
            "/rates/:from_currency/:to_currency",
            get(get_one_rate).put(put_rate),
        )
        .route("/convert", post(post_convert))
        .route("/convert/batch", post(post_convert_batch))
        .route("/triangulate", post(post_triangulate))
        .route("/format/:currency_code/:amount", get(get_formatted_amount))
        .route("/validate-transaction", post(post_validate_transaction))
        .route("/alerts/rate", post(post_rate_alert))
        .route("/currency-stats", get(get_currency_stats))

        // banking: statements imported from the bank, the cash book they are reconciled against, and the
        // reconciliations themselves. /reconcile already answers the sales ledger's control account.
        .route("/statements", post(post_bank_statement).get(get_bank_statements))
        .route("/statements/:statement_id", get(get_bank_statement))
        .route("/cash-book", post(post_bank_cash_entry).get(get_bank_cash_book))
        .route("/bank-reconcile", post(post_bank_reconciliation))
        .route(
            "/bank-reconcile/:reconciliation_id/auto-match",
            post(post_bank_auto_match),
        )
        .route(
            "/bank-reconcile/:reconciliation_id/post-adjustments",
            post(post_bank_adjustments),
        )
        .route("/reconciliations", get(get_bank_reconciliations))
        .route(
            "/reconciliations/:reconciliation_id",
            get(get_bank_reconciliation),
        )
        .route("/outstanding-items", get(get_bank_outstanding_items))

        // payables: the purchases ledger and its control account. The plain /transactions, /reconcile and
        // /control-account/summary already answer the sales ledger, so each of these is named for the side of
        // the ledger it answers.
        .route("/creditor-transactions", post(post_creditor_transaction))
        .route("/purchases-control-account/summary", get(get_purchases_control_summary))
        .route("/creditors", get(get_creditors))
        .route("/purchases-reconcile", post(reconcile_purchases_control_account))
        // payables: the expenses claimed against it
        .route("/expenses", post(post_expense))
        .route("/expenses/:company_id", get(get_expenses))
        .route("/expenses/:expense_id/approve", put(approve_expense))
        .route("/expenses/:expense_id/reject", put(reject_expense))
        .route("/expense-summary/:company_id", get(get_expense_summary))

        // absorption-costing: the stock valuation derived from the product costs
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

        // the non-profit service keeps its own trailing slash on the collection paths; both answer
        .route("/npo/funds/", get(npo_funds).post(create_npo_fund))
        .route("/npo/funds", get(npo_funds).post(create_npo_fund))
        .route("/npo/funds/:fund_id", get(npo_fund))
        .route(
            "/npo/funds/:fund_id/transactions/",
            get(npo_fund_transactions).post(record_npo_fund_transaction),
        )
        .route("/npo/funds/:fund_id/restrictions/", post(restrict_npo_fund))
        .route("/npo/net-assets/", post(create_npo_net_assets))
        .route("/npo/net-assets/:as_of_date", get(npo_net_assets_on))
        .route("/npo/donations/", get(npo_donations).post(create_npo_donation))
        .route("/npo/grants/", get(npo_grants).post(create_npo_grant))
        .route("/npo/grants/:grant_id", get(npo_grant))
        .route("/npo/grants/:grant_id/drawdowns/", post(record_npo_drawdown))
        .route("/npo/projects/", get(npo_projects).post(create_npo_project))
        .route("/npo/projects/:project_id", get(npo_project))
        .route("/npo/programs/", get(npo_programs).post(create_npo_program))
        .route("/npo/donors/", get(npo_donors).post(create_npo_donor))
        .route("/npo/donors/:donor_id", get(npo_donor))
        .route("/npo/budgets/", get(npo_budgets).post(create_npo_budget))
        .route("/npo/budgets/:budget_id/lines/", post(add_npo_budget_line))
        .route(
            "/npo/internal-controls/",
            get(npo_internal_controls).post(create_npo_internal_control),
        )
        .route("/npo/audit-reports/", get(npo_audit_reports).post(create_npo_audit_report))
        .route("/npo/program-metrics/", post(create_npo_program_metric))
        .route("/npo/impact-measurements/", post(create_npo_impact_measurement))
        .route("/npo/sroi-analyses/", post(npo_sroi_analysis))
        .route(
            "/npo/volunteer-records/",
            get(npo_volunteer_records).post(create_npo_volunteer_record),
        )
        .route(
            "/npo/statements/activities/",
            get(npo_statement_of_activities).post(create_npo_statement_of_activities),
        )
        .route("/npo/statements/financial-position/", post(create_npo_statement_of_position))
        .route("/npo/assets/", post(describe_npo_asset))
        .route(
            "/npo-scale/orgs",
            get(npo_scale_orgs).post(create_npo_scale_org),
        )
        .route(
            "/npo-scale/orgs/:org_id",
            get(npo_scale_org).patch(update_npo_scale_org),
        )
        .route("/npo-scale/orgs/:org_id/features", get(npo_scale_org_features))
        .route(
            "/npo-scale/orgs/:org_id/branches",
            get(npo_scale_branches).post(create_npo_scale_branch),
        )
        .route(
            "/npo-scale/orgs/:org_id/vendors",
            get(npo_scale_vendors).post(add_npo_scale_vendor),
        )
        .route(
            "/npo-scale/orgs/:org_id/revenues",
            get(npo_scale_revenues).post(record_npo_scale_revenue),
        )
        .route("/npo-scale/orgs/:org_id/purchases", post(record_npo_scale_purchase))
        .route(
            "/npo-scale/orgs/:org_id/purchases/:purchase_id/pay",
            post(pay_npo_scale_purchase),
        )
        .route(
            "/npo-scale/orgs/:org_id/reports/creditors",
            get(npo_scale_creditors),
        )
        .route(
            "/npo-scale/orgs/:org_id/donors",
            post(create_npo_scale_donor).get(list_npo_scale_donors),
        )
        .route(
            "/npo-scale/orgs/:org_id/donations",
            post(create_npo_scale_donation).get(list_npo_scale_donations),
        )
        .route(
            "/npo-scale/orgs/:org_id/pledges",
            post(create_npo_scale_pledge),
        )
        .route(
            "/npo-scale/orgs/:org_id/pledges/run",
            post(run_npo_scale_pledges),
        )
        .route(
            "/npo-scale/orgs/:org_id/receipts",
            get(list_npo_scale_receipts),
        )
        .route(
            "/npo-scale/orgs/:org_id/receipts/:receipt_id/pdf",
            get(npo_scale_receipt_pdf),
        )
        .route(
            "/npo-scale/receipts/verify/:token",
            get(verify_npo_scale_receipt),
        )
        .route(
            "/npo-scale/public/holdings/:verify_code",
            get(public_npo_scale_holdings),
        )
        .route(
            "/npo-scale/orgs/:org_id/expenses",
            post(create_npo_scale_expense).get(list_npo_scale_expenses),
        )
        .route("/npo-scale/orgs/:org_id/budgets", put(save_npo_scale_budget))
        .route(
            "/npo-scale/orgs/:org_id/compliance",
            post(create_npo_scale_compliance).get(list_npo_scale_compliance),
        )
        .route(
            "/npo-scale/orgs/:org_id/balance-items",
            post(create_npo_scale_balance_item),
        )
        .route(
            "/npo-scale/orgs/:org_id/shareholders",
            post(add_npo_scale_shareholder).get(list_npo_scale_shareholders),
        )
        .route(
            "/npo-scale/orgs/:org_id/directors",
            post(add_npo_scale_director).get(list_npo_scale_directors),
        )
        .route(
            "/npo-scale/orgs/:org_id/dividends",
            post(declare_npo_scale_dividend),
        )
        .route(
            "/npo-scale/orgs/:org_id/reports/equity",
            get(report_npo_scale_equity),
        )
        .route(
            "/npo-scale/orgs/:org_id/partners",
            post(add_npo_scale_partner).get(list_npo_scale_partners),
        )
        .route(
            "/npo-scale/orgs/:org_id/partners/:partner_id/draws",
            post(draw_npo_scale_partner),
        )
        .route(
            "/npo-scale/orgs/:org_id/reports/capital-accounts",
            get(report_npo_scale_capital_accounts),
        )
        .route(
            "/npo-scale/orgs/:org_id/reports/activities",
            get(report_npo_scale_activities),
        )
        .route(
            "/npo-scale/orgs/:org_id/reports/functional-expenses",
            get(report_npo_scale_functional),
        )
        .route(
            "/npo-scale/orgs/:org_id/reports/position",
            get(report_npo_scale_position),
        )
        .route(
            "/npo-scale/orgs/:org_id/reports/budget-vs-actual",
            get(report_npo_scale_budget),
        )
        .route(
            "/npo-scale/orgs/:org_id/reports/consolidated",
            get(report_npo_scale_consolidated),
        )

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
        .route(
            "/family-community-group/groups",
            get(community_groups).post(create_community_group),
        )
        .route("/family-community-group/groups/:group_id", get(community_group))
        .route(
            "/family-community-group/groups/:group_id/members",
            get(community_group_members).post(add_community_group_member),
        )
        .route(
            "/family-community-group/groups/:group_id/contribute",
            post(contribute_to_community_group),
        )
        .route(
            "/family-community-group/groups/:group_id/contributions",
            get(community_group_contributions),
        )
        .route(
            "/family-community-group/groups/:group_id/advance-cycle",
            post(advance_community_group_cycle),
        )
        .route(
            "/family-community-group/groups/:group_id/payouts",
            get(community_group_payouts),
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

// --- tax: rates, registration, calculation, returns, withholding and the reports ---

async fn post_tax_rate(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(tax::accounting::create_rate(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn list_tax_rates(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(tax::accounting::list_rates(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn get_tax_rate(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(rate_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(tax::accounting::get_rate(graph.as_ref(), &scope, &rate_id))
}

async fn put_tax_rate(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(rate_id): Path<String>,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(tax::accounting::update_rate(
        graph.as_ref(),
        &scope,
        &rate_id,
        &payload,
    ))
}

async fn post_tax_registration(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(tax::accounting::create_registration(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn list_tax_registrations(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(tax::accounting::list_registrations(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn post_tax_calculation(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), payload.map(|Json(value)| value));
    answer(tax::accounting::calculate(graph.as_ref(), &scope, &body))
}

async fn post_tax_calculation_batch(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = match payload {
        Some(Json(value)) => value,
        None => Value::Null,
    };
    answer(tax::accounting::calculate_batch(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn post_tax_return(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), payload.map(|Json(value)| value));
    answer(tax::accounting::create_return(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn list_tax_returns(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(tax::accounting::list_returns(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn post_file_tax_return(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(return_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = match payload {
        Some(Json(value)) => value,
        None => Value::Null,
    };
    answer(tax::accounting::file_return(
        graph.as_ref(),
        &scope,
        &return_id,
        &body,
    ))
}

async fn post_pay_tax_return(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(return_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = match payload {
        Some(Json(value)) => value,
        None => Value::Null,
    };
    answer(tax::accounting::pay_return(
        graph.as_ref(),
        &scope,
        &return_id,
        &body,
    ))
}

async fn post_withholding_tax(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(tax::accounting::record_withholding(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn list_withholding_tax(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(tax::accounting::list_withholding(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn post_tax_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), payload.map(|Json(value)| value));
    answer(tax::accounting::summary_report(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn get_vat_by_jurisdiction(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(tax::accounting::vat_by_jurisdiction(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

async fn post_deferred_tax(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(tax::accounting::deferred_tax(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn get_tax_liability_schedule(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(tax::accounting::liability_schedule(
        graph.as_ref(),
        &scope,
        &body_or_query(query.as_deref()),
    ))
}

// --- tax: obligations, compliance and planning ---

async fn post_tax_obligation(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(tax::compliance::create_obligation(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn list_tax_obligations(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(tax::compliance::list_obligations(
        graph.as_ref(),
        &scope,
        &body,
    ))
}

async fn post_file_tax_obligation(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(obligation_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = match payload {
        Some(Json(value)) => crate::merge_request(query.as_deref(), Some(value)),
        None => crate::merge_request(query.as_deref(), None),
    };
    answer(tax::compliance::file_obligation(
        graph.as_ref(),
        &scope,
        &obligation_id,
        &body,
    ))
}

async fn get_tax_compliance_summary(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = crate::merge_request(query.as_deref(), None);
    answer(tax::compliance::summary(graph.as_ref(), &scope, &body))
}

async fn post_tax_strategy(
    State(graph): GraphState,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(tax::planning::create_strategy(
        graph.as_ref(),
        &scope,
        &payload,
    ))
}

async fn list_tax_strategies(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(tax::planning::list_strategies(graph.as_ref(), &scope))
}

async fn post_tax_plan(Json(payload): Json<Value>) -> Response {
    answer(tax::planning::plan(&payload))
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
