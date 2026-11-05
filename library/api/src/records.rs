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

use axum::{
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use store::{
    absorption, cash_flow, insurance_claims, job_costing, treasury_analytics, BookScope, Graph,
    StoreError,
};

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
        assert_eq!(value["closing_stock_value"], json!(50.0));
    }
}
