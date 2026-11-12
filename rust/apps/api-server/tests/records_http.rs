//! The record routes: a caller with a Book, a graph that answers, and what comes out.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use store::{BookScope, Graph, StoreError};
use tower::ServiceExt;

/// A graph that answers with the rows a test handed it and remembers what it was asked.
struct Scripted {
    rows: Vec<Value>,
    seen: Mutex<Vec<(String, Value)>>,
}

impl Scripted {
    fn returning(rows: Vec<Value>) -> Arc<Self> {
        Arc::new(Self {
            rows,
            seen: Mutex::new(Vec::new()),
        })
    }

    fn statements(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
}

impl Graph for Scripted {
    fn run(&self, cypher: &str, params: Value) -> Result<Vec<Value>, StoreError> {
        self.seen.lock().unwrap().push((cypher.to_owned(), params));
        Ok(self.rows.clone())
    }
}

async fn call(graph: Arc<Scripted>, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let router = vimbai_api::records::router(graph);
    let mut request = match body {
        Some(payload) => Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap(),
        None => Request::builder().uri(uri).body(Body::empty()).unwrap(),
    };
    request
        .headers_mut()
        .insert("x-user-id", "user-1".parse().expect("a valid header value"));
    request
        .headers_mut()
        .insert("x-book-id", "book-1".parse().expect("a valid header value"));

    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test]
async fn a_calculation_answers_on_the_path_the_python_service_published() {
    let graph = Scripted::returning(vec![json!({"x": {
        "id": "calc-1",
        "company_id": "c-1",
        "product_or_process": "Widgets",
        "period": "2026-11",
        "total_cost": "600.000000",
        "unit_cost": "200.000000",
        "quantity": 3,
        "components": [
            {"name": "resin", "amount": "100", "cost_type": "direct_materials"}
        ]
    }})]);
    let (status, body) = call(
        graph.clone(),
        "/process-costing/calculate",
        Some(json!({
            "company_id": "c-1",
            "product_or_process": "Widgets",
            "quantity": 3,
            "components": [
                {"name": "resin", "amount": 100, "cost_type": "direct_materials"},
                {"name": "labour", "amount": 500, "cost_type": "direct_labor"}
            ]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_cost"], json!(600.0));
    assert_eq!(body["unit_cost"], json!(200.0));
    assert_eq!(graph.statements(), 1);
}

#[tokio::test]
async fn the_same_answer_is_available_under_records() {
    let graph = Scripted::returning(vec![json!({"x": {
        "id": "calc-1", "company_id": "c-1", "total_cost": "10", "unit_cost": "10", "components": []
    }})]);
    let (status, body) = call(graph, "/records/process-costing/summary/c-1", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["company_id"], json!("c-1"));
    assert_eq!(body["total_calculations"], json!(1));
}

#[tokio::test]
async fn a_request_without_a_book_is_refused_before_anything_runs() {
    let graph = Scripted::returning(vec![]);
    let router = vimbai_api::records::router(graph.clone());
    let request = Request::builder()
        .method("GET")
        .uri("/process-costing/summary/c-1")
        .header("x-user-id", "user-1")
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(graph.statements(), 0);
}

#[tokio::test]
async fn a_breakdown_of_a_missing_calculation_is_a_404_not_an_empty_report() {
    let graph = Scripted::returning(vec![]);
    let (status, _) = call(graph, "/process-costing/breakdown/c-1/calc-9", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_scope_headers_become_the_parameters_the_store_queries_with() {
    let graph = Scripted::returning(vec![]);
    let (status, _) = call(graph.clone(), "/process-costing/summary/c-1", None).await;
    assert_eq!(status, StatusCode::OK);
    let (cypher, params) = graph.seen.lock().unwrap().last().cloned().unwrap();
    assert_eq!(params["user_id"], json!("user-1"), "{cypher}");
    assert_eq!(params["book_id"], json!("book-1"), "{cypher}");
    let _ = BookScope::new("user-1", "book-1");
}

#[tokio::test]
async fn both_costing_services_are_answered_by_the_one_implementation() {
    // process-costing and product-costing were generated from the same template: same models, same
    // endpoints, same CostCalculation label. One implementation answers both, so the pair cannot
    // drift apart as they are maintained.
    let payload = json!({
        "company_id": "c-1",
        "product_or_process": "Widgets",
        "quantity": 4,
        "components": [
            {"name": "resin", "amount": 0.1, "cost_type": "direct_materials"},
            {"name": "labour", "amount": "0.2", "cost_type": "direct_labor"}
        ]
    });
    let row = json!({"x": {
        "id": "calc-1",
        "company_id": "c-1",
        "product_or_process": "Widgets",
        "total_cost": "0.300000",
        "unit_cost": "0.075000",
        "quantity": 4,
        "components": []
    }});

    let (process_status, process_body) = call(
        Scripted::returning(vec![row.clone()]),
        "/process-costing/calculate",
        Some(payload.clone()),
    )
    .await;
    let (product_status, product_body) = call(
        Scripted::returning(vec![row]),
        "/product-costing/calculate",
        Some(payload),
    )
    .await;

    assert_eq!(process_status, StatusCode::OK);
    assert_eq!(product_status, StatusCode::OK);
    assert_eq!(process_body, product_body);
    assert_eq!(process_body["unit_cost"], json!(0.075));
}
