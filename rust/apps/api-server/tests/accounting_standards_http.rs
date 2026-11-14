//! accounting-standards-service answers at the prefix it publishes.
//!
//! The standards catalogue and the organisation records already lived in the accounting crate, hung off a table the
//! router builds for two prefixes. What these tests pin is that the addresses the service published are the ones that
//! answer: /accounting-standards/standards and its comparison and categories, the configuration a Book keeps for an
//! organisation, activating a standard, and the compliance check with the history it leaves behind.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::fake::MemoryGraph;
use tower::ServiceExt;

async fn call(
    graph: Arc<MemoryGraph>,
    method: Method,
    uri: &str,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    let router = vimbai_api::records::router(graph);
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1");
    let body = match payload {
        Some(value) => Body::from(value.to_string()),
        None => Body::empty(),
    };
    let response = router.oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 2_000_000)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn the_published_standards_address_lists_the_catalogue() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, body) = call(graph, Method::GET, "/accounting-standards/standards", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let standards = body["standards"]
        .as_array()
        .or_else(|| body.as_array())
        .expect("a list of standards");
    assert!(
        standards.len() > 3,
        "the catalogue is more than a placeholder: {body}"
    );
    assert!(
        standards.iter().any(|standard| standard["code"] == "IFRS"),
        "IFRS is in the catalogue: {body}"
    );
}

#[tokio::test]
async fn a_region_filter_narrows_the_catalogue_rather_than_being_ignored() {
    let graph = Arc::new(MemoryGraph::new());
    let (_, all) = call(
        Arc::clone(&graph),
        Method::GET,
        "/accounting-standards/standards",
        None,
    )
    .await;
    let (status, us) = call(
        graph,
        Method::GET,
        "/accounting-standards/standards?region=us",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{us}");
    let count = |body: &Value| {
        body["standards"]
            .as_array()
            .or_else(|| body.as_array())
            .map(Vec::len)
            .unwrap_or(0)
    };
    assert!(
        count(&us) < count(&all),
        "us should be a subset: {us} vs {all}"
    );
}

#[tokio::test]
async fn categories_and_comparison_answer_at_the_addresses_they_publish() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, categories) = call(
        Arc::clone(&graph),
        Method::GET,
        "/accounting-standards/standards/categories",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{categories}");
    assert!(
        !categories.is_null(),
        "the categories answer with something"
    );

    let (status, compared) = call(
        graph,
        Method::GET,
        "/accounting-standards/standards/comparison?standard_1=ifrs&standard_2=us_gaap",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{compared}");
    assert!(
        compared["standard_1"].is_object() && compared["standard_2"].is_object(),
        "both sides of the comparison come back: {compared}"
    );
}

#[tokio::test]
async fn an_organisation_keeps_its_configuration_and_can_activate_a_standard() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, configured) = call(
        Arc::clone(&graph),
        Method::POST,
        "/accounting-standards/organizations/org-1/configuration",
        Some(json!({
            "organization_name": "Kiln Works",
            "primary_standard": "ifrs",
            "selected_standards": ["ifrs"],
            "reporting_currency": "USD",
            "fiscal_year_end": "12-31",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{configured}");

    let (status, read_back) = call(
        Arc::clone(&graph),
        Method::GET,
        "/accounting-standards/organizations/org-1/configuration",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{read_back}");
    assert!(
        read_back.to_string().contains("ifrs"),
        "the standard that was configured comes back: {read_back}"
    );

    let (status, activated) = call(
        Arc::clone(&graph),
        Method::POST,
        "/accounting-standards/organizations/org-1/standards/uk_gaap/activate",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{activated}");
    assert_eq!(activated["status"], "activated");

    // the toggle lands on the configuration the caller can read back, not on a second record
    let (_, after) = call(
        Arc::clone(&graph),
        Method::GET,
        "/accounting-standards/organizations/org-1/configuration",
        None,
    )
    .await;
    assert!(
        after.to_string().contains("uk_gaap"),
        "the activated standard is in the configuration: {after}"
    );
}

#[tokio::test]
async fn a_compliance_check_is_recorded_and_then_listed_in_the_history() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, checked) = call(
        Arc::clone(&graph),
        Method::POST,
        "/accounting-standards/organizations/org-1/compliance/check",
        Some(json!({
            "standard_type": "ifrs",
            "check_date": "2026-09-30",
            "validation_results": [
                {"rule": "balance_sheet_balances", "passed": true},
                {"rule": "revenue_recognised_on_delivery", "passed": false},
            ],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{checked}");

    let (status, history) = call(
        graph,
        Method::GET,
        "/accounting-standards/organizations/org-1/compliance/history",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{history}");
    assert!(
        history.to_string().contains("2026-09-30"),
        "the check just made is in the history: {history}"
    );
}

#[tokio::test]
async fn a_standard_is_read_by_type_with_its_requirements_and_measurement_guide() {
    let graph = Arc::new(MemoryGraph::new());
    let (status, one) = call(
        Arc::clone(&graph),
        Method::GET,
        "/accounting-standards/standards/ifrs",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{one}");
    assert!(one.to_string().contains("ifrs"), "{one}");

    let (status, requirements) = call(
        Arc::clone(&graph),
        Method::GET,
        "/accounting-standards/standards/ifrs/requirements",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{requirements}");

    let (status, guide) = call(
        graph,
        Method::GET,
        "/accounting-standards/standards/ifrs/measurement-guide",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{guide}");
    assert!(!guide.is_null());
}
