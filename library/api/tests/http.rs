//! The HTTP surface answers the same payload the library does.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn call(uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = match body {
        Some(payload) => Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap(),
        None => Request::builder().uri(uri).body(Body::empty()).unwrap(),
    };
    let response = vimbai_api::router().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test]
async fn health_names_the_engine_and_counts_the_calculators() {
    let (status, body) = call("/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["engine"], "rust");
    assert!(body["calculators"].as_u64().unwrap() >= 2);
}

#[tokio::test]
async fn the_list_matches_what_answers() {
    let (status, body) = call("/calculators", None).await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<String> = body["calculators"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"target-costing".to_string()));
    assert!(names.contains(&"internal-control-evaluation".to_string()));
}

#[tokio::test]
async fn target_costing_keeps_an_amount_larger_than_a_float_can_hold() {
    let (status, body) = call(
        "/calc/target-costing",
        Some(json!({
            "company_id": "c1",
            "product_name": "Widget",
            "target_selling_price": "123456789012345678901234.56",
            "desired_profit_margin_pct": "12.5",
            "current_cost": "100000000000000000000000.00",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // a tenth of a percent of this number would be invisible in an f64
    assert_eq!(body["target_profit"], "15432098626543209862654.32");
    assert_eq!(body["target_cost"], "108024690385802469038580.24");
}

#[tokio::test]
async fn an_unknown_calculator_says_so_and_lists_the_rest() {
    let (status, body) = call("/calc/nope", Some(json!({}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["calculators"].as_array().unwrap().len() >= 2);
}

#[tokio::test]
async fn controls_still_answer_through_the_wire() {
    let (status, body) = call(
        "/calc/internal-control-evaluation",
        Some(json!({
            "audit_id": "a1",
            "controls_to_test": [
                {"id": "C1", "deficiency": "none"},
                {"id": "C2", "deficiency": "material"},
            ],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["effective_controls"], 1);
    assert_eq!(body["overall_assessment"], "ineffective_with_deficiencies");
}
