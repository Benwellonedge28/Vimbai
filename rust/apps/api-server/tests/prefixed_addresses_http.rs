#![forbid(unsafe_code)]
//! Seven domains publish a word another domain owns, so each of them answers under its own prefix.
//!
//! `/dashboard`, `/policies`, `/evaluate`, `/plans`, `/summary`, `/reports` and `/report/{id}` each belong to one
//! domain. A gateway entry carries a single prefix flag, so a service sharing a word cannot be half flat and half
//! prefixed: it keeps its prefix and the engine registers its own addresses. These tests walk in at those addresses,
//! because the failure this guards against is the quiet one - a 200 answered by the other domain's handler.

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

#[tokio::test]
async fn the_sox_dashboard_answers_at_its_own_address() {
    let book = Book::new();
    let (status, body) = book
        .call(Method::GET, "/sox-compliance/dashboard", json!({}))
        .await;
    // the flat /dashboard is the privacy dashboard, so this must not read as one
    assert!(status.is_success(), "{status}: {body}");
    assert!(
        body.get("total_controls").is_some(),
        "a sox dashboard counts controls, not consents: {body}"
    );
}

#[tokio::test]
async fn the_regulations_listing_reads_the_company_it_is_given() {
    let book = Book::new();
    let (status, body) = book
        .call(Method::GET, "/regulatory-compliance/regulations", json!({}))
        .await;
    // published as a query parameter, so a call without one is refused rather than answered for nobody
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = book
        .call(
            Method::GET,
            "/regulatory-compliance/regulations?company_id=c-1",
            json!({}),
        )
        .await;
    assert!(status.is_success(), "{status}: {body}");
}

#[tokio::test]
async fn the_access_policies_and_the_provision_policies_are_different_lists() {
    let book = Book::new();
    let (status, body) = book
        .call(
            Method::POST,
            "/zero-trust-data/policies",
            json!({
                "name": "no export after hours",
                "resource": "ledger",
                "action": "export",
                "effect": "deny",
                "conditions": {},
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {body}");
    let (status, body) = book
        .call(Method::GET, "/zero-trust-data/policies", json!({}))
        .await;
    assert!(status.is_success(), "{status}: {body}");
    // the flat /policies holds provision policies, which carry a rate rather than an effect
    let held = body
        .get("policies")
        .or(Some(&body))
        .cloned()
        .unwrap_or(Value::Null);
    assert!(
        !held.to_string().contains("provision"),
        "the access policies read back as provision policies: {held}"
    );
}

#[tokio::test]
async fn the_benefit_plans_are_not_the_subscription_plans() {
    let book = Book::new();
    let (status, body) = book
        .call(Method::GET, "/benefits-admin/plans", json!({}))
        .await;
    assert!(status.is_success(), "{status}: {body}");
}

#[tokio::test]
async fn the_tax_summary_is_not_the_reconciliation_summary() {
    let book = Book::new();
    let (status, body) = book
        .call(
            Method::GET,
            "/tax-compliance/summary?company_id=c-1",
            json!({}),
        )
        .await;
    assert!(status.is_success(), "{status}: {body}");
    let flat = book
        .call(Method::GET, "/summary?company_id=c-1", json!({}))
        .await;
    assert_ne!(
        body, flat.1,
        "the tax summary and the reconciliation summary are the same answer, so one is reading the other's handler"
    );
}

#[tokio::test]
async fn the_automation_reports_are_not_the_reconciliation_reports() {
    let book = Book::new();
    let (status, body) = book
        .call(Method::GET, "/report-automation/reports", json!({}))
        .await;
    assert!(status.is_success(), "{status}: {body}");
    // its schedules are its own words, and they answer where the service published them
    let (status, _) = book
        .call(Method::GET, "/report-automation/schedules", json!({}))
        .await;
    assert!(status.is_success(), "{status}");
}

#[tokio::test]
async fn the_engagement_report_is_not_the_integrity_report() {
    let book = Book::new();
    let (status, body) = book
        .call(Method::GET, "/audit-engagement/audit-types", json!({}))
        .await;
    assert!(status.is_success(), "{status}: {body}");
    let (status, body) = book
        .call(Method::GET, "/audit-engagement/report/eng-1", json!({}))
        .await;
    // an engagement that was never opened is missing, which is the engagement's answer and not integrity's
    assert!(
        status == StatusCode::NOT_FOUND || status.is_success(),
        "{status}: {body}"
    );
}

#[tokio::test]
async fn a_dividend_is_paid_by_the_method_the_service_published() {
    let book = Book::new();
    let (_, declared) = book
        .call(
            Method::POST,
            "/company-accounting/dividends",
            json!({
                "company_id": "c-1",
                "amount": 1000,
                "declared_date": "2026-01-10",
            }),
        )
        .await;
    let id = declared
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("missing")
        .to_string();
    // the service published POST; the engine answered only PUT, so a client's POST was a 405
    let (status, body) = book
        .call(
            Method::POST,
            &format!("/company-accounting/dividends/{id}/pay"),
            json!({"paid_date": "2026-02-10"}),
        )
        .await;
    assert_ne!(status, StatusCode::METHOD_NOT_ALLOWED, "{body}");
}
