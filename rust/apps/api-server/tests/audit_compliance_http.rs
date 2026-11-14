#![forbid(unsafe_code)]
//! audit-compliance-service over HTTP: an event appended and verified, a version kept, lineage traced, and the
//! planning, report, trail, compliance and analytics readings answered. These tests are about the wiring - that
//! each path reaches its own handler with the Book it was asked in - so they check the answers a caller would see
//! rather than re-testing arithmetic the crate's own tests cover.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::fake::MemoryGraph;
use tower::ServiceExt;

struct Book {
    router: axum::Router,
}

impl Book {
    fn new() -> Self {
        Self {
            router: vimbai_api::records::router(Arc::new(MemoryGraph::new())),
        }
    }

    async fn call(&self, method: Method, uri: &str, payload: Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-user-id", "user-1")
            .header("x-book-id", "book-1")
            .body(Body::from(payload.to_string()))
            .unwrap();
        self.answer(request).await
    }

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("x-user-id", "user-1")
            .header("x-book-id", "book-1")
            .body(Body::empty())
            .unwrap();
        self.answer(request).await
    }

    async fn answer(&self, request: Request<Body>) -> (StatusCode, Value) {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

async fn an_event(book: &Book, kind: &str, resource: &str, actor: &str) -> Value {
    let (status, created) = book
        .call(
            Method::POST,
            "/events",
            json!({
                "event_type": kind,
                "resource_type": "invoice",
                "resource_id": resource,
                "user_id": actor,
                "action_details": {"note": "posted"},
                "timestamp": "2026-03-01T10:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    created
}

#[tokio::test]
async fn an_event_is_appended_listed_and_verified() {
    let book = Book::new();
    let created = an_event(&book, "create", "inv-1", "user-9").await;
    assert_eq!(created["resource_id"], json!("inv-1"));
    // the actor is the event's user, not the caller
    assert_eq!(created["user_id"], json!("user-9"));
    let id = created["id"].as_str().expect("an id").to_string();

    let (status, listing) = book.get("/events").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listing["total"], json!(1));
    assert_eq!(listing["events"][0]["id"], json!(id));

    let (status, fetched) = book.get(&format!("/events/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["id"], json!(id));

    let (status, checked) = book.get(&format!("/integrity/verify-event/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(checked["matches"], json!(true));

    let (status, chain) = book.get("/integrity/verify").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(chain["is_valid"], json!(true));
    assert_eq!(chain["chain_length"], json!(1));
}

#[tokio::test]
async fn another_books_event_is_missing() {
    let book = Book::new();
    let created = an_event(&book, "create", "inv-1", "user-9").await;
    let id = created["id"].as_str().expect("an id").to_string();
    let request = Request::builder()
        .method(Method::GET)
        .uri(format!("/events/{id}"))
        .header("x-user-id", "user-2")
        .header("x-book-id", "book-2")
        .body(Body::empty())
        .unwrap();
    let (status, _) = book.answer(request).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_version_is_kept_and_its_history_read() {
    let book = Book::new();
    let (status, first) = book
        .call(
            Method::POST,
            "/versions/invoice/inv-1",
            json!({"state": {"total": 100}, "changed_by": "user-1"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["version"], json!(1));

    let (_, second) = book
        .call(
            Method::POST,
            "/versions/invoice/inv-1",
            json!({"state": {"total": 120}, "changed_by": "user-1"}),
        )
        .await;
    assert_eq!(second["version"], json!(2));

    let (status, history) = book.get("/versions/invoice/inv-1").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        history["versions"].as_array().map(|held| held.len()),
        Some(2)
    );
    assert_eq!(history["versions"][1]["state"]["total"], json!(120));
}

#[tokio::test]
async fn lineage_is_traced_between_a_correlated_event_and_the_one_it_names() {
    let book = Book::new();
    let first = an_event(&book, "create", "inv-1", "user-1").await;
    let first_id = first["id"].as_str().expect("an id").to_string();
    let (status, _) = book
        .call(
            Method::POST,
            "/events",
            json!({
                "event_type": "update",
                "resource_type": "invoice",
                "resource_id": "inv-1",
                "user_id": "user-1",
                "correlation_id": first_id,
                "timestamp": "2026-03-01T11:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, traced) = book.get("/lineage/inv-1").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(traced["total_events"], json!(2));
    assert_eq!(traced["edges"].as_array().map(|held| held.len()), Some(1));

    let (status, nothing) = book.get("/lineage/inv-404").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(nothing["message"], json!("No lineage data found"));
}

#[tokio::test]
async fn the_plan_and_the_formal_report_are_answered_from_the_callers_figures() {
    let book = Book::new();
    let (status, plan) = book
        .call(
            Method::POST,
            "/planning/plan",
            json!({"audit_id": "A-1", "company_id": "co-1", "fiscal_year": 2026, "prior_year_findings": []}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    assert_eq!(plan["overall_risk"], json!("low"));
    assert_eq!(
        plan["materiality"]["performance_materiality"],
        json!(375000.0)
    );

    let (status, report) = book
        .call(
            Method::POST,
            "/reports/generate",
            json!({
                "audit_id": "A-1",
                "company_id": "co-1",
                "fiscal_year": 2026,
                "opinion": "qualified",
                "findings": [{"id": "F-1", "description": "Late filing", "severity": "high"}],
                "today": "2026-06-30"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["opinion_type"], json!("Qualified Opinion"));
    assert_eq!(report["report_date"], json!("2026-06-30"));
    assert_eq!(report["findings_summary"][0]["severity"], json!("high"));
}

#[tokio::test]
async fn the_compliance_assessments_are_answered() {
    let book = Book::new();
    let (status, monitored) = book
        .call(
            Method::POST,
            "/compliance/monitor",
            json!({
                "company_id": "co-1",
                "now": "2026-03-31T09:00:00Z",
                "checks": [
                    {"check_id": "C-1", "regulation": "SOX", "status": "Compliant"},
                    {"check_id": "C-2", "regulation": "GDPR", "status": "Non-Compliant", "description": "No consent log"}
                ]
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{monitored}");
    assert_eq!(
        monitored["compliance_summary"]["compliance_rate"],
        json!(50.0)
    );
    assert_eq!(monitored["critical_issues"], json!(["No consent log"]));

    let (status, sox) = book
        .call(
            Method::POST,
            "/compliance/sox",
            json!({
                "company_id": "co-1",
                "fiscal_year": 2026,
                "controls": [{"id": "C-1"}, {"id": "C-2", "tested": false}]
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{sox}");
    assert_eq!(sox["overall_status"], json!("needs_attention"));

    let (status, gdpr) = book
        .call(
            Method::POST,
            "/compliance/gdpr",
            json!({"company_id": "co-1", "consent_records": 10, "retention_policies": []}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{gdpr}");
    assert_eq!(gdpr["compliance_score"], json!(0.7));
}

#[tokio::test]
async fn the_compliance_report_and_analytics_read_the_trail() {
    let book = Book::new();
    an_event(&book, "create", "inv-1", "user-9").await;
    an_event(&book, "update", "inv-1", "user-9").await;

    let (status, reported) = book
        .call(
            Method::POST,
            "/compliance/report",
            json!({
                "start_date": "2026-03-01",
                "end_date": "2026-03-31",
                "include_integrity_checks": true,
                "now": "2026-04-01T09:00:00Z"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{reported}");
    assert_eq!(reported["total_events"], json!(2));
    assert_eq!(reported["events_by_user"]["user-9"], json!(2));
    assert_eq!(reported["integrity_verified"], json!(true));

    let (status, summary) = book.get("/analytics/summary").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(summary["total_events"], json!(2));
    assert_eq!(summary["unique_resources"], json!(1));
    assert_eq!(summary["top_resources"][0], json!(["invoice:inv-1", 2]));

    let (status, found) = book.get("/search?query=POSTED").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(found["total"], json!(2));
}

#[tokio::test]
async fn a_search_with_no_query_is_refused() {
    let book = Book::new();
    let (status, _) = book.get("/search?query=").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}
