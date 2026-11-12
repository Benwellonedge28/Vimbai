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
    routing::{get, post, put},
    Json, Router,
};
use budgeting_services::appropriation_control;
use costing::{
    absorption, basis_apportionment, cost_calculations, cvp, job_costing, overhead_absorption,
    price_reduction,
};
use equity::equity_changes;
use finance::insurance_claims;
use serde_json::{json, Value};
use store::{BookScope, Graph, StoreError};
use treasury::{cash_optimization, treasury_analytics};

/// A graph the API can use for the whole life of the process. The driver pools its own
/// connections, so one handle serves every request.
pub type GraphHandle = Arc<dyn Graph + Send + Sync>;

/// Where a request is allowed to read and write.
///
/// Both headers are required. A Book-less caller is not handled yet: the Python layer stored a
/// null book, and answering that case means a second statement shape in the store rather than
/// quietly treating "no Book" as a Book named nothing.
fn scope_of(headers: &HeaderMap) -> Result<BookScope, Box<Response>> {
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

fn answer(result: Result<Value, StoreError>) -> Response {
    match result {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(error) => fail(error),
    }
}

fn query_param(query: Option<&str>, name: &str) -> Option<String> {
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

type GraphState = State<GraphHandle>;

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
fn overlay(query: Value, body: Option<Value>) -> Value {
    let mut merged = query.as_object().cloned().unwrap_or_default();
    if let Some(Value::Object(fields)) = body {
        for (key, value) in fields {
            merged.insert(key, value);
        }
    }
    Value::Object(merged)
}

fn body_or_query(query: Option<&str>) -> Value {
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
    records_router(graph.clone()).merge(service_root_router(graph))
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
