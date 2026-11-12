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
use budgeting_services::appropriation_control;
use common::Money;
use costing::{
    absorption, basis_apportionment, cost_calculations, cvp, job_costing, overhead_absorption,
    price_reduction,
};
use equity::equity_changes;
use finance::insurance_claims;
use serde_json::{json, Value};
use store::amounts::{amount, amount_or_zero, wire};
use store::{BookScope, Graph, StoreError};
use treasury::{cash_management, cash_optimization, treasury_analytics};

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
            "/cash-management/accounts",
            post(open_cash_account).get(cash_accounts),
        )
        .route(
            "/cash-management/transfers",
            post(move_cash).get(cash_transfers),
        )
        .route(
            "/cash-management/liquidity",
            post(measure_liquidity).get(liquidity_history),
        )
        .route(
            "/records/cash-management/accounts",
            post(open_cash_account).get(cash_accounts),
        )
        .route(
            "/records/cash-management/transfers",
            post(move_cash).get(cash_transfers),
        )
        .route(
            "/records/cash-management/liquidity",
            post(measure_liquidity).get(liquidity_history),
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
        .with_state(graph)
}

/// The drop-in paths, identical to what the Python services answered.
pub fn service_root_router(graph: GraphHandle) -> Router {
    Router::new()
        .route(
            "/absorption-costing/product-costs",
            get(product_costs).post(keep_product_cost),
        )
        .route(
            "/absorption-costing/product-costs/:product_id/latest",
            get(latest_product_cost),
        )
        .route("/absorption-costing/stock-valuation", get(stock_valuation))
        .route("/job-costing/jobs", post(open_job))
        .route("/job-costing/jobs/:company_id", get(company_jobs))
        .route(
            "/job-costing/jobs/:company_id/profitability",
            get(company_profitability),
        )
        .route("/job-costing/jobs/:job_id/costs", post(charge_cost))
        .route(
            "/fixed-assets-register/assets",
            get(fixed_assets).post(register_asset),
        )
        .route(
            "/fixed-assets-register/assets/:asset_id",
            get(one_fixed_asset),
        )
        .route(
            "/fixed-assets-register/assets/:asset_id/depreciate",
            post(depreciate_asset),
        )
        .route(
            "/fixed-assets-register/assets/:asset_id/depreciation",
            get(asset_depreciation),
        )
        .route(
            "/fixed-assets-register/assets/:asset_id/dispose",
            post(dispose_fixed_asset),
        )
        .route("/fixed-assets-register/summary", get(fixed_asset_summary))
        .route(
            "/fixed-assets-schedule/assets",
            get(schedule_asset_list).post(register_asset),
        )
        .route(
            "/fixed-assets-schedule/assets/:asset_id",
            get(one_fixed_asset).put(patch_fixed_asset),
        )
        .route(
            "/fixed-assets-schedule/schedule/generate",
            post(asset_schedule),
        )
        .route(
            "/fixed-assets-schedule/schedule/latest",
            get(asset_schedule),
        )
        .route("/fixed-assets-schedule/summary", get(schedule_summary))
        .route("/departmental-accounting/departments", post(create_department))
        .route("/departmental-accounting/departments", get(list_departments))
        .route(
            "/departmental-accounting/departments/:department_id",
            get(get_department),
        )
        .route(
            "/departmental-accounting/departments/:department_id",
            put(update_department),
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
            post(create_allocation_rule),
        )
        .route(
            "/departmental-accounting/allocation-rules",
            get(list_allocation_rules),
        )
        .route(
            "/departmental-accounting/allocation-rules/:rule_id",
            delete(deactivate_allocation_rule),
        )
        .route("/departmental-accounting/cost-pools", post(create_cost_pool))
        .route("/departmental-accounting/cost-pools", get(list_cost_pools))
        .route(
            "/departmental-accounting/cost-pools/:pool_id",
            get(get_cost_pool),
        )
        .route("/departmental-accounting/allocate", post(run_allocation))
        .route(
            "/departmental-accounting/allocations/:pool_id",
            get(allocation_results),
        )
        .route(
            "/departmental-accounting/inter-department-bills",
            post(create_inter_department_bill),
        )
        .route(
            "/departmental-accounting/inter-department-bills",
            get(list_inter_department_bills),
        )
        .route(
            "/departmental-accounting/inter-department-bills/:bill_id/approve",
            post(approve_inter_department_bill),
        )
        .route(
            "/departmental-accounting/reports/department-comparison",
            get(compare_departments).post(compare_departments),
        )
        .route(
            "/departmental-accounting/reports/cost-distribution",
            get(cost_distribution),
        )
        .route("/company-accounting/companies", post(create_company))
        .route("/company-accounting/companies", get(list_companies))
        .route("/company-accounting/companies/:company_id", get(get_company))
        .route("/company-accounting/companies/:company_id", put(update_company))
        .route(
            "/company-accounting/shareholders",
            post(register_company_shareholder),
        )
        .route(
            "/company-accounting/shareholders",
            get(list_company_shareholders),
        )
        .route("/company-accounting/share-capital", post(create_share_capital))
        .route("/company-accounting/share-capital", get(list_share_capital))
        .route(
            "/company-accounting/capital-transactions",
            post(record_capital_transaction),
        )
        .route(
            "/company-accounting/capital-transactions",
            get(list_capital_transactions),
        )
        .route("/company-accounting/dividends", post(declare_dividend))
        .route("/company-accounting/dividends", get(list_dividends))
        .route(
            "/company-accounting/dividends/:dividend_id/pay",
            put(pay_dividend),
        )
        .route(
            "/company-accounting/dividends/:dividend_id/payments",
            get(dividend_payments),
        )
        .route(
            "/company-accounting/retained-earnings",
            post(record_retained_earnings),
        )
        .route("/company-accounting/retained-earnings", get(retained_earnings))
        .route("/company-accounting/reserves", post(create_company_reserve))
        .route("/company-accounting/reserves", get(company_reserves))
        .route(
            "/company-accounting/reports/equity-statement/:company_id",
            get(equity_statement),
        )
        .route(
            "/company-accounting/reports/shareholder-register/:company_id",
            get(shareholder_register),
        )
        .route(
            "/company-accounting/reports/dividend-history/:company_id",
            get(dividend_history),
        )
        .route("/cashbook/accounts", post(create_cash_account))
        .route("/cashbook/accounts", get(list_cash_accounts))
        .route("/cashbook/accounts/:account_id", get(get_cash_account))
        .route("/cashbook/accounts/:account_id", put(update_cash_account))
        .route("/cashbook/entries", post(create_cash_entry))
        .route("/cashbook/entries", get(list_cash_entries))
        .route("/cashbook/entries/:entry_id", get(get_cash_entry))
        .route("/cashbook/entries/:entry_id/post", put(post_cash_entry))
        .route("/cashbook/entries/:entry_id/void", put(void_cash_entry))
        .route(
            "/cashbook/summary/:account_code",
            get(cashbook_summary),
        )
        .route(
            "/cashbook/reconciliations",
            post(create_cash_reconciliation),
        )
        .route(
            "/cashbook/reconciliations",
            get(list_cash_reconciliations),
        )
        .route(
            "/cashbook/reconciliations/:reconciliation_id",
            get(get_cash_reconciliation),
        )
        .route(
            "/cashbook/reconciliations/:reconciliation_id/complete",
            put(complete_reconciliation),
        )
        .route(
            "/cashbook/reconciliations/:reconciliation_id/outstanding",
            get(cash_reconciliation_outstanding),
        )
        .route("/cashbook/cash-flow", post(create_cash_flow_entry))
        .route("/cashbook/cash-flow", get(list_cash_flow))
        .route("/cashbook/cash-flow/summary", get(cash_flow_summary))
        .route("/cashbook/cash-position", get(cash_position))
        .route("/cashbook/cash-position/history", get(cash_position_history))
        .route("/ifrs-reporting/reports", post(create_ifrs_report))
        .route("/ifrs-reporting/reports", get(list_ifrs_reports))
        .route("/ifrs-reporting/reports/:report_id", get(get_ifrs_report))
        .route(
            "/ifrs-reporting/reports/:report_id/notes",
            post(add_ifrs_note),
        )
        .route(
            "/ifrs-reporting/reports/:report_id/notes",
            get(list_ifrs_notes),
        )
        .route(
            "/ifrs-reporting/reports/:report_id/approve",
            put(approve_ifrs_report),
        )
        .route("/intercompany/entities", post(create_intercompany_entity))
        .route("/intercompany/entities", get(list_intercompany_entities))
        .route(
            "/intercompany/transactions",
            post(create_intercompany_transaction),
        )
        .route(
            "/intercompany/transactions",
            get(list_intercompany_transactions),
        )
        .route(
            "/intercompany/transactions/match",
            post(match_intercompany),
        )
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
        .route(
            "/balance-sheet/history/:company_id",
            get(balance_sheet_history),
        )
        .route(
            "/balance-sheet/ratios/:company_id",
            get(balance_sheet_ratios),
        )
        .route("/fund-accounting/funds", post(create_fund))
        .route("/fund-accounting/funds/:company_id", get(company_funds))
        .route("/fund-accounting/transactions", post(record_fund_movement))
        .route(
            "/fund-accounting/transactions/:fund_id",
            get(fund_movements),
        )
        .route(
            "/revenue-recognition/contracts",
            post(create_revenue_contract),
        )
        .route(
            "/revenue-recognition/contracts/:contract_id/recognize",
            post(recognise_revenue),
        )
        .route(
            "/revenue-recognition/contracts/:company_id",
            get(company_revenue_contracts),
        )
        .route(
            "/revenue-recognition/summary/:company_id",
            get(company_revenue_summary),
        )
        .route(
            "/capital-reconstruction/reconstructions/create",
            post(create_reconstruction),
        )
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
        .route(
            "/capital-reconstruction/reconstructions",
            get(capital_reconstructions),
        )
        .route(
            "/capital-reconstruction/reconstructions/:reconstruction_id",
            get(reconstruction_scheme),
        )
        .route(
            "/capital-reconstruction/summary/:company_id",
            get(reconstruction_company_summary),
        )
        .route("/debentures/classes/create", post(create_debenture_class))
        .route(
            "/debentures/classes/:class_id/issue",
            post(issue_debentures),
        )
        .route(
            "/debentures/classes/:class_id/interest/accrue",
            post(accrue_debenture_interest),
        )
        .route(
            "/debentures/interest/:interest_id/pay",
            post(pay_debenture_interest),
        )
        .route(
            "/debentures/classes/:class_id/redeem",
            post(redeem_debentures),
        )
        .route("/debentures/classes", get(debenture_classes))
        .route("/debentures/issues", get(debenture_issues))
        .route("/debentures/interest", get(debenture_interest))
        .route("/issued-share-capital/issue", post(issue_share_capital))
        .route(
            "/issued-share-capital/shareholders/register",
            post(register_shareholder),
        )
        .route(
            "/issued-share-capital/shareholders/:company_id",
            get(company_shareholders),
        )
        .route(
            "/issued-share-capital/issues/:company_id",
            get(company_share_issues),
        )
        .route(
            "/issued-share-capital/summary/:company_id",
            get(issued_capital_summary),
        )
        .route("/initial-investment/create", post(create_investment))
        .route("/initial-investment/simple", post(simple_investment))
        .route(
            "/initial-investment/with-capital-allowance",
            post(investment_with_capital_allowance),
        )
        .route("/initial-investment/list", get(list_investments))
        .route(
            "/initial-investment/get/:investment_id",
            get(get_investment),
        )
        .route(
            "/share-redemption/redemptions/initiate",
            post(initiate_redemption),
        )
        .route(
            "/share-redemption/redemptions/:redemption_id/fresh-issue",
            post(record_fresh_issue),
        )
        .route(
            "/share-redemption/redemptions/:redemption_id/complete",
            post(complete_redemption),
        )
        .route("/share-redemption/redemptions", get(list_share_redemptions))
        .route(
            "/share-redemption/redemptions/:redemption_id",
            get(share_redemption_detail),
        )
        .route("/share-redemption/crr-requirements", get(crr_requirements))
        .route("/records/departmental-accounting/departments", post(create_department))
        .route("/records/departmental-accounting/departments", get(list_departments))
        .route(
            "/records/departmental-accounting/departments/:department_id",
            get(get_department),
        )
        .route(
            "/records/departmental-accounting/departments/:department_id",
            put(update_department),
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
            post(create_allocation_rule),
        )
        .route(
            "/records/departmental-accounting/allocation-rules",
            get(list_allocation_rules),
        )
        .route(
            "/records/departmental-accounting/allocation-rules/:rule_id",
            delete(deactivate_allocation_rule),
        )
        .route("/records/departmental-accounting/cost-pools", post(create_cost_pool))
        .route("/records/departmental-accounting/cost-pools", get(list_cost_pools))
        .route(
            "/records/departmental-accounting/cost-pools/:pool_id",
            get(get_cost_pool),
        )
        .route("/records/departmental-accounting/allocate", post(run_allocation))
        .route(
            "/records/departmental-accounting/allocations/:pool_id",
            get(allocation_results),
        )
        .route(
            "/records/departmental-accounting/inter-department-bills",
            post(create_inter_department_bill),
        )
        .route(
            "/records/departmental-accounting/inter-department-bills",
            get(list_inter_department_bills),
        )
        .route(
            "/records/departmental-accounting/inter-department-bills/:bill_id/approve",
            post(approve_inter_department_bill),
        )
        .route(
            "/records/departmental-accounting/reports/department-comparison",
            get(compare_departments).post(compare_departments),
        )
        .route(
            "/records/departmental-accounting/reports/cost-distribution",
            get(cost_distribution),
        )
        .route("/records/company-accounting/companies", post(create_company))
        .route("/records/company-accounting/companies", get(list_companies))
        .route("/records/company-accounting/companies/:company_id", get(get_company))
        .route("/records/company-accounting/companies/:company_id", put(update_company))
        .route(
            "/records/company-accounting/shareholders",
            post(register_company_shareholder),
        )
        .route(
            "/records/company-accounting/shareholders",
            get(list_company_shareholders),
        )
        .route("/records/company-accounting/share-capital", post(create_share_capital))
        .route("/records/company-accounting/share-capital", get(list_share_capital))
        .route(
            "/records/company-accounting/capital-transactions",
            post(record_capital_transaction),
        )
        .route(
            "/records/company-accounting/capital-transactions",
            get(list_capital_transactions),
        )
        .route("/records/company-accounting/dividends", post(declare_dividend))
        .route("/records/company-accounting/dividends", get(list_dividends))
        .route(
            "/records/company-accounting/dividends/:dividend_id/pay",
            put(pay_dividend),
        )
        .route(
            "/records/company-accounting/dividends/:dividend_id/payments",
            get(dividend_payments),
        )
        .route(
            "/records/company-accounting/retained-earnings",
            post(record_retained_earnings),
        )
        .route("/records/company-accounting/retained-earnings", get(retained_earnings))
        .route("/records/company-accounting/reserves", post(create_company_reserve))
        .route("/records/company-accounting/reserves", get(company_reserves))
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
        .route("/records/cashbook/accounts", post(create_cash_account))
        .route("/records/cashbook/accounts", get(list_cash_accounts))
        .route("/records/cashbook/accounts/:account_id", get(get_cash_account))
        .route("/records/cashbook/accounts/:account_id", put(update_cash_account))
        .route("/records/cashbook/entries", post(create_cash_entry))
        .route("/records/cashbook/entries", get(list_cash_entries))
        .route("/records/cashbook/entries/:entry_id", get(get_cash_entry))
        .route("/records/cashbook/entries/:entry_id/post", put(post_cash_entry))
        .route("/records/cashbook/entries/:entry_id/void", put(void_cash_entry))
        .route(
            "/records/cashbook/summary/:account_code",
            get(cashbook_summary),
        )
        .route(
            "/records/cashbook/reconciliations",
            post(create_cash_reconciliation),
        )
        .route(
            "/records/cashbook/reconciliations",
            get(list_cash_reconciliations),
        )
        .route(
            "/records/cashbook/reconciliations/:reconciliation_id",
            get(get_cash_reconciliation),
        )
        .route(
            "/records/cashbook/reconciliations/:reconciliation_id/complete",
            put(complete_reconciliation),
        )
        .route(
            "/records/cashbook/reconciliations/:reconciliation_id/outstanding",
            get(cash_reconciliation_outstanding),
        )
        .route("/records/cashbook/cash-flow", post(create_cash_flow_entry))
        .route("/records/cashbook/cash-flow", get(list_cash_flow))
        .route("/records/cashbook/cash-flow/summary", get(cash_flow_summary))
        .route("/records/cashbook/cash-position", get(cash_position))
        .route("/records/cashbook/cash-position/history", get(cash_position_history))
        .route("/records/ifrs-reporting/reports", post(create_ifrs_report))
        .route("/records/ifrs-reporting/reports", get(list_ifrs_reports))
        .route("/records/ifrs-reporting/reports/:report_id", get(get_ifrs_report))
        .route(
            "/records/ifrs-reporting/reports/:report_id/notes",
            post(add_ifrs_note),
        )
        .route(
            "/records/ifrs-reporting/reports/:report_id/notes",
            get(list_ifrs_notes),
        )
        .route(
            "/records/ifrs-reporting/reports/:report_id/approve",
            put(approve_ifrs_report),
        )
        .route("/records/intercompany/entities", post(create_intercompany_entity))
        .route("/records/intercompany/entities", get(list_intercompany_entities))
        .route(
            "/records/intercompany/transactions",
            post(create_intercompany_transaction),
        )
        .route(
            "/records/intercompany/transactions",
            get(list_intercompany_transactions),
        )
        .route(
            "/records/intercompany/transactions/match",
            post(match_intercompany),
        )
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
        .route(
            "/records/balance-sheet/history/:company_id",
            get(balance_sheet_history),
        )
        .route(
            "/records/balance-sheet/ratios/:company_id",
            get(balance_sheet_ratios),
        )
        .route("/records/fund-accounting/funds", post(create_fund))
        .route("/records/fund-accounting/funds/:company_id", get(company_funds))
        .route("/records/fund-accounting/transactions", post(record_fund_movement))
        .route(
            "/records/fund-accounting/transactions/:fund_id",
            get(fund_movements),
        )
        .route(
            "/records/revenue-recognition/contracts",
            post(create_revenue_contract),
        )
        .route(
            "/records/revenue-recognition/contracts/:contract_id/recognize",
            post(recognise_revenue),
        )
        .route(
            "/records/revenue-recognition/contracts/:company_id",
            get(company_revenue_contracts),
        )
        .route(
            "/records/revenue-recognition/summary/:company_id",
            get(company_revenue_summary),
        )
        .route(
            "/records/capital-reconstruction/reconstructions/create",
            post(create_reconstruction),
        )
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
        .route(
            "/records/capital-reconstruction/reconstructions",
            get(capital_reconstructions),
        )
        .route(
            "/records/capital-reconstruction/reconstructions/:reconstruction_id",
            get(reconstruction_scheme),
        )
        .route(
            "/records/capital-reconstruction/summary/:company_id",
            get(reconstruction_company_summary),
        )
        .route(
            "/records/debentures/classes/create",
            post(create_debenture_class),
        )
        .route(
            "/records/debentures/classes/:class_id/issue",
            post(issue_debentures),
        )
        .route(
            "/records/debentures/classes/:class_id/interest/accrue",
            post(accrue_debenture_interest),
        )
        .route(
            "/records/debentures/interest/:interest_id/pay",
            post(pay_debenture_interest),
        )
        .route(
            "/records/debentures/classes/:class_id/redeem",
            post(redeem_debentures),
        )
        .route("/records/debentures/classes", get(debenture_classes))
        .route("/records/debentures/issues", get(debenture_issues))
        .route("/records/debentures/interest", get(debenture_interest))
        .route(
            "/records/issued-share-capital/issue",
            post(issue_share_capital),
        )
        .route(
            "/records/issued-share-capital/shareholders/register",
            post(register_shareholder),
        )
        .route(
            "/records/issued-share-capital/shareholders/:company_id",
            get(company_shareholders),
        )
        .route(
            "/records/issued-share-capital/issues/:company_id",
            get(company_share_issues),
        )
        .route(
            "/records/issued-share-capital/summary/:company_id",
            get(issued_capital_summary),
        )
        .route(
            "/records/initial-investment/create",
            post(create_investment),
        )
        .route(
            "/records/initial-investment/simple",
            post(simple_investment),
        )
        .route(
            "/records/initial-investment/with-capital-allowance",
            post(investment_with_capital_allowance),
        )
        .route("/records/initial-investment/list", get(list_investments))
        .route(
            "/records/initial-investment/get/:investment_id",
            get(get_investment),
        )
        .route(
            "/records/share-redemption/redemptions/initiate",
            post(initiate_redemption),
        )
        .route(
            "/records/share-redemption/redemptions/:redemption_id/fresh-issue",
            post(record_fresh_issue),
        )
        .route(
            "/records/share-redemption/redemptions/:redemption_id/complete",
            post(complete_redemption),
        )
        .route(
            "/records/share-redemption/redemptions",
            get(list_share_redemptions),
        )
        .route(
            "/records/share-redemption/redemptions/:redemption_id",
            get(share_redemption_detail),
        )
        .route(
            "/records/share-redemption/crr-requirements",
            get(crr_requirements),
        )
        .route("/share-premium/entries/record", post(record_share_premium))
        .route("/share-premium/entries", get(share_premium_entries))
        .route(
            "/share-premium/utilizations/record",
            post(utilize_share_premium),
        )
        .route(
            "/share-premium/utilizations",
            get(share_premium_utilizations),
        )
        .route(
            "/share-premium/adjustments/create",
            post(adjust_share_premium),
        )
        .route(
            "/share-premium/summary/:company_id",
            get(share_premium_summary),
        )
        .route(
            "/capital-redemption-reserve/redemptions/record",
            post(record_redemption),
        )
        .route(
            "/capital-redemption-reserve/redemptions",
            get(list_redemptions),
        )
        .route(
            "/capital-redemption-reserve/creations/create",
            post(create_crr),
        )
        .route(
            "/capital-redemption-reserve/creations",
            get(list_crr_creations),
        )
        .route(
            "/capital-redemption-reserve/utilizations/record",
            post(utilize_crr),
        )
        .route(
            "/capital-redemption-reserve/utilizations",
            get(list_crr_utilizations),
        )
        .route(
            "/capital-redemption-reserve/summary/:company_id",
            get(crr_summary),
        )
        .route(
            "/general-reserve/reserves",
            get(reserves).post(create_reserve),
        )
        .route("/general-reserve/reserves/:reserve_id", get(one_reserve))
        .route(
            "/general-reserve/reserves/:reserve_id/allocate",
            post(allocate_reserve),
        )
        .route(
            "/general-reserve/reserves/:reserve_id/utilize",
            post(utilize_reserve),
        )
        .route(
            "/general-reserve/reserves/:reserve_id/history",
            get(reserve_history),
        )
        .route(
            "/retained-profits/appropriations",
            get(profit_appropriations).post(create_profit_appropriation),
        )
        .route(
            "/retained-profits/interim-dividends",
            get(interim_dividends).post(declare_interim_dividend),
        )
        .route(
            "/retained-profits/interim-dividends/:dividend_id/pay",
            post(pay_interim_dividend),
        )
        .route(
            "/retained-profits/adjustments",
            get(prior_year_adjustments).post(create_prior_year_adjustment),
        )
        .route(
            "/retained-profits/summary/:company_id",
            get(retained_earnings_summary),
        )
        .route("/bonus-shares/issue", post(issue_bonus_shares))
        .route("/bonus-shares/issues", get(bonus_issues))
        .route("/right-issues/offer", post(offer_rights))
        .route("/right-issues/right-issues", get(right_issues))
        .route("/right-issues/right-issues/:issue_id", get(one_right_issue))
        .route(
            "/right-issues/right-issues/:issue_id/accept",
            post(accept_rights),
        )
        .route(
            "/right-issues/right-issues/:issue_id/renounce",
            post(renounce_rights),
        )
        .route(
            "/ordinary-shares/dividends",
            get(ordinary_dividends).post(declare_ordinary_dividend),
        )
        .route(
            "/ordinary-shares/dividends/:dividend_id/pay",
            post(pay_ordinary_dividend),
        )
        .route(
            "/preference-shares/classes",
            get(preference_classes).post(create_preference_class),
        )
        .route(
            "/preference-shares/classes/:class_id/issue",
            post(issue_preference_shares),
        )
        .route(
            "/preference-shares/classes/:class_id/dividends/declare",
            post(declare_preference_dividend),
        )
        .route(
            "/preference-shares/classes/:class_id/dividends/:dividend_id/pay",
            post(pay_preference_dividend),
        )
        .route(
            "/preference-shares/classes/:class_id/redeem",
            post(redeem_preference_shares),
        )
        .route("/preference-shares/dividends", get(preference_dividends))
        .route(
            "/authorized-share-capital/share-classes",
            get(share_classes).post(create_share_class),
        )
        .route(
            "/authorized-share-capital/share-classes/:class_id",
            get(one_share_class),
        )
        .route(
            "/authorized-share-capital/share-classes/:class_id/issue",
            post(issue_class_shares),
        )
        .route(
            "/authorized-share-capital/share-classes/:class_id/issuances",
            get(class_issuances),
        )
        .route(
            "/authorized-share-capital/share-classes/:class_id/buyback",
            post(buyback_class_shares),
        )
        .route(
            "/authorized-share-capital/summary",
            get(share_capital_summary),
        )
        .route(
            "/disposal-group/groups",
            get(disposal_groups).post(create_disposal_group),
        )
        .route("/disposal-group/groups/:group_id", get(one_disposal_group))
        .route(
            "/disposal-group/groups/:group_id/assets",
            get(disposal_assets).post(add_disposal_asset),
        )
        .route(
            "/disposal-group/groups/:group_id/impairment-test",
            post(disposal_impairment_test),
        )
        .route(
            "/disposal-group/groups/:group_id/status",
            put(disposal_group_status),
        )
        .route(
            "/lease-termination/terminations",
            get(lease_terminations).post(create_lease_termination),
        )
        .route(
            "/lease-termination/terminations/:termination_id",
            get(one_lease_termination),
        )
        .route(
            "/lease-termination/terminations/:termination_id/settlement",
            post(lease_settlement),
        )
        .route(
            "/lease-termination/terminations/:termination_id/approve",
            put(approve_lease_termination),
        )
        .route(
            "/lease-termination/terminations/:termination_id/complete",
            put(complete_lease_termination),
        )
        .route(
            "/intangible-assets/assets",
            get(intangibles).post(create_intangible),
        )
        .route("/intangible-assets/assets/:asset_id", get(one_intangible))
        .route(
            "/intangible-assets/assets/:asset_id/amortize",
            post(amortize_intangible),
        )
        .route(
            "/intangible-assets/assets/:asset_id/amortization",
            get(intangible_amortization),
        )
        .route(
            "/revaluation-reserve/revaluations/record",
            post(record_asset_revaluation),
        )
        .route(
            "/revaluation-reserve/utilizations/record",
            post(record_reserve_utilization),
        )
        .route("/revaluation-reserve/revaluations", get(revaluations))
        .route(
            "/revaluation-reserve/cumulative/:asset_id",
            get(cumulative_revaluation),
        )
        .route(
            "/revaluation-reserve/summary/:company_id",
            get(revaluation_summary),
        )
        .route("/insurance-claims/file", post(file_claim))
        .route("/insurance-claims/claims", get(insurance_claims))
        .route(
            "/insurance-claims/claims/:claim_id/process",
            post(process_insurance_claim),
        )
        .route("/treasury-analytics/analyze", post(analyze_treasury))
        .route("/treasury-analytics/kpi/:company_id", get(treasury_kpi))
        .route(
            "/cash-flow-statement/generate",
            post(generate_cash_flow_statement),
        )
        .route(
            "/cash-flow-statement/latest/:company_id",
            get(latest_cash_flow_statement),
        )
        .route(
            "/cash-flow-statement/history/:company_id",
            get(cash_flow_statement_history),
        )
        .route(
            "/equity-changes/transactions",
            post(record_equity_transaction),
        )
        .route(
            "/equity-changes/transactions/:company_id",
            get(company_equity_transactions),
        )
        .route("/equity-changes/statement", post(generate_equity_statement))
        .route(
            "/equity-changes/statements/:company_id",
            get(company_equity_statements),
        )
        .route("/cash-optimization/accounts", post(add_cash_account))
        .route(
            "/cash-optimization/accounts/:company_id",
            get(company_cash_accounts),
        )
        .route(
            "/cash-optimization/optimize/:company_id",
            post(run_cash_optimization),
        )
        .route(
            "/cash-optimization/suggestions/:company_id",
            get(company_cash_suggestions),
        )
        .route("/trading-account/generate", post(generate_trading_account))
        .route("/trading-account/accounts", get(list_trading_accounts))
        .route(
            "/trading-account/accounts/:account_id",
            get(get_trading_account),
        )
        .route("/selling-price-reduction/analyze", post(price_analysis))
        .route(
            "/selling-price-reduction/discount-analysis",
            post(price_discount_analysis),
        )
        .route("/selling-price-reduction/analyses", get(price_analyses))
        .route("/cvp-analysis/analyze", post(cvp_analysis))
        .route("/cvp-analysis/quick-analysis", post(cvp_quick_analysis))
        .route("/cvp-analysis/contribution", post(cvp_contribution))
        .route("/cvp-analysis/target-profit", post(cvp_target_profit))
        .route("/cvp-analysis/multi-product", post(cvp_multi_product))
        .route("/cvp-analysis/analyses", get(cvp_analyses))
        .route(
            "/cvp-analysis/analyses/:analysis_id",
            get(cvp_analysis_by_id),
        )
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
        .route(
            "/overhead-absorption/blanket-rate",
            post(overhead_blanket_rate),
        )
        .route(
            "/overhead-absorption/absorbed-overhead",
            post(overhead_absorbed),
        )
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
        .route(
            "/process-costing/calculations/:company_id",
            get(company_cost_calculations),
        )
        .route(
            "/process-costing/breakdown/:company_id/:calc_id",
            get(cost_breakdown),
        )
        .route("/process-costing/summary/:company_id", get(cost_summary))
        .route("/product-costing/calculate", post(cost_calculation))
        .route(
            "/product-costing/calculations/:company_id",
            get(company_cost_calculations),
        )
        .route(
            "/product-costing/breakdown/:company_id/:calc_id",
            get(cost_breakdown),
        )
        .route("/product-costing/summary/:company_id", get(cost_summary))
        .route(
            "/appropriation-control/appropriations",
            post(create_appropriation),
        )
        .route(
            "/appropriation-control/appropriations/:company_id",
            get(company_appropriations),
        )
        .route(
            "/appropriation-control/transactions",
            post(appropriation_transaction),
        )
        .route(
            "/appropriation-control/check/:appropriation_id",
            get(appropriation_check),
        )
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
}
