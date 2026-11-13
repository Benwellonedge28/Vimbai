//! data-warehouse-service and etl-service over HTTP: describing tables, querying them, and the pipelines that fill them.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::fake::MemoryGraph;
use tower::ServiceExt;

fn router() -> axum::Router {
    vimbai_api::records::router(Arc::new(MemoryGraph::new()))
}

async fn call(
    router: &axum::Router,
    method: Method,
    uri: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn the_warehouse_is_described_queried_and_filled() {
    let router = router();

    // tables start empty, and an unnamed one is refused
    let (status, dimension) = call(
        &router,
        Method::POST,
        "/dimensions",
        json!({"name": "customers", "columns": [{"name": "id", "type": "int"}]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{dimension}");
    assert_eq!(dimension["row_count"], json!(0));
    let (status, _) = call(&router, Method::POST, "/dimensions", json!({"name": ""})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (_, dims) = call(&router, Method::GET, "/dimensions", json!({})).await;
    assert_eq!(dims.as_array().map(Vec::len), Some(1));

    let (status, fact) = call(
        &router,
        Method::POST,
        "/facts",
        json!({"name": "sales", "dimensions": ["customers"],
               "measures": [{"name": "amount", "type": "decimal", "agg": "sum"}]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{fact}");
    assert_eq!(fact["row_count"], json!(0));

    // a query against a described table is recorded with no figures claimed
    let (status, answered) = call(
        &router,
        Method::POST,
        "/query",
        json!({"fact_table": "sales", "group_by": ["month"], "measures": ["amount"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answered}");
    assert_eq!(answered["fact_table"], json!("sales"));
    assert_eq!(answered["results"].as_array().map(Vec::len), Some(0));
    // a table nobody described is missing, not empty
    let (status, _) = call(
        &router,
        Method::POST,
        "/query",
        json!({"fact_table": "revenue"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // a load is recorded finished, having touched no rows
    let (status, load) = call(
        &router,
        Method::POST,
        "/etl",
        json!({"source": "raw_sales", "target": "sales"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{load}");
    assert_eq!(load["status"], json!("completed"));
    assert_eq!(load["rows_processed"], json!(0));
    let (_, loads) = call(&router, Method::GET, "/etl", json!({})).await;
    assert_eq!(loads.as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn a_pipeline_is_described_run_filtered_and_deleted() {
    let router = router();

    let (status, made) = call(
        &router,
        Method::POST,
        "/pipelines",
        json!({"name": "sales to warehouse", "description": "nightly",
               "steps": [{"name": "extract", "step_type": "extract", "source": "raw_sales"},
                         {"name": "load", "step_type": "load", "target": "sales"}],
               "schedule": "0 2 * * *"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["status"], json!("pending"));
    assert_eq!(made["steps"].as_array().map(Vec::len), Some(2));
    let id = made["id"].as_str().expect("an id").to_string();

    let (status, ran) = call(
        &router,
        Method::POST,
        &format!("/pipelines/{id}/run"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{ran}");
    assert_eq!(ran["status"], json!("completed"));
    assert_eq!(ran["steps_completed"], json!(2));
    assert!(ran["run_at"].as_str().is_some() || ran["run_at"].is_number());

    let (status, after) = call(&router, Method::GET, &format!("/pipelines/{id}"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{after}");
    assert_eq!(after["status"], json!("completed"));
    assert_eq!(after["steps"][0]["status"], json!("completed"));

    // the listing filter is the listing's business, and an empty pipeline runs without walking anything
    let (_, empty) = call(
        &router,
        Method::POST,
        "/pipelines",
        json!({"name": "placeholder"}),
    )
    .await;
    let empty_id = empty["id"].as_str().expect("an id").to_string();
    let (_, swept) = call(
        &router,
        Method::POST,
        &format!("/pipelines/{empty_id}/run"),
        json!({}),
    )
    .await;
    assert_eq!(swept["steps_completed"], json!(0));
    let (_, completed) = call(
        &router,
        Method::GET,
        "/pipelines?status=completed",
        json!({}),
    )
    .await;
    let (_, pending) = call(&router, Method::GET, "/pipelines?status=pending", json!({})).await;
    assert_eq!(completed.as_array().map(Vec::len), Some(2));
    assert_eq!(pending.as_array().map(Vec::len), Some(0));

    // a pipeline that is not there is missing, and deleting twice finds nothing the second time
    let (status, _) = call(&router, Method::GET, "/pipelines/nope", json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, gone) = call(
        &router,
        Method::DELETE,
        &format!("/pipelines/{id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{gone}");
    assert_eq!(gone["deleted"], json!(true));
    let (status, _) = call(
        &router,
        Method::DELETE,
        &format!("/pipelines/{id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
