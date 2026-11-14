//! The equity services published their opening moves one segment deeper than the collection.
//!
//! `/reserves/create` sits beside `POST /reserves`, and a caller uses the address the service published, not the one the
//! engine happened to keep. Each of these walks in at the published address and then reads the record back through the
//! service's own listing, so a 200 that wrote the wrong kind of record fails here rather than in a client.

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

    async fn post(&self, uri: &str, payload: Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(Method::POST)
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

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("x-user-id", "user-1")
            .header("x-book-id", "book-1")
            .body(Body::empty())
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
async fn a_reserve_is_opened_at_the_address_the_service_published() {
    let book = Book::new();
    let (status, created) = book
        .post(
            "/general-reserve/reserves/create",
            json!({
                "company_id": "c-1",
                "reserve_name": "Plant replacement",
                "initial_balance": "2500.00",
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {created}");
    let (status, listed) = book.get("/general-reserve/reserves").await;
    assert!(status.is_success(), "{status}: {listed}");
    assert!(
        listed.to_string().contains("Plant replacement"),
        "the reserve was not listed after being opened: {listed}"
    );
}

#[tokio::test]
async fn a_profit_appropriation_is_made_at_the_address_the_service_published() {
    let book = Book::new();
    let (status, created) = book
        .post(
            "/retained-profits/appropriations/create",
            json!({
                "company_id": "c-1",
                "financial_year": "2026",
                "period_start": "2026-01-01",
                "period_end": "2026-12-31",
                "net_profit": "1000.00",
                "opening_retained_earnings": "0.00",
                "appropriations": {"general reserve": 1000},
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {created}");
    let (_, listed) = book.get("/retained-profits/appropriations").await;
    assert!(listed.to_string().contains("1000"), "{listed}");
}

#[tokio::test]
async fn an_interim_dividend_is_declared_at_the_address_the_service_published() {
    let book = Book::new();
    let (status, created) = book
        .post(
            "/retained-profits/interim-dividends/declare",
            json!({
                "company_id": "c-1",
                "amount": "600.00",
                "per_share_amount": "0.15",
                "total_shares": 4000,
                "declaration_date": "2026-06-01",
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {created}");
    let (_, listed) = book.get("/retained-profits/interim-dividends").await;
    assert!(listed.to_string().contains("600"), "{listed}");
}

#[tokio::test]
async fn a_prior_year_adjustment_is_posted_at_the_address_the_service_published() {
    let book = Book::new();
    let (status, created) = book
        .post(
            "/retained-profits/adjustments/create",
            json!({
                "company_id": "c-1",
                "amount": "75.00",
                "reason": "stock correction",
                "financial_year": "2025",
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {created}");
    let (_, listed) = book.get("/retained-profits/adjustments").await;
    assert!(listed.to_string().contains("75"), "{listed}");
}

#[tokio::test]
async fn an_ordinary_dividend_is_declared_at_the_address_the_service_published() {
    let book = Book::new();
    let (status, created) = book
        .post(
            "/ordinary-shares/dividends/declare",
            json!({
                "company_id": "c-1",
                "dividend_type": "final",
                "per_share_amount": "0.25",
                "total_shares": 4000,
                "record_date": "2026-03-01",
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {created}");
    // the total is the per-share amount times the shares on the register, not the per-share amount itself
    assert!(
        created.to_string().contains("1000"),
        "a 0.25 dividend on 4000 shares is 1000: {created}"
    );
}

#[tokio::test]
async fn a_preference_class_is_created_at_the_address_the_service_published() {
    let book = Book::new();
    let (status, created) = book
        .post(
            "/preference-shares/classes/create",
            json!({
                "company_id": "c-1",
                "name": "Preference A",
                "nominal_value": "1.00",
                "issue_price": "1.00",
                "fixed_dividend_rate": "0.08",
                "dividend_type": "fixed",
                "participation_rights": "non-participating",
                "liquidation_priority": 1,
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {created}");
    let (_, listed) = book.get("/preference-shares/classes").await;
    assert!(
        listed.to_string().contains("Preference A"),
        "the class was not listed after being created: {listed}"
    );
}

#[tokio::test]
async fn the_doubtful_provision_is_not_the_aged_provision() {
    let book = Book::new();
    let (status, added) = book
        .post(
            "/provision-doubtful-debts/debtors/add",
            json!({
                "debtor_id": "d-1",
                "debtor_name": "Shaky Ltd",
                "invoice_number": "INV-1",
                "invoice_date": "2026-01-01",
                "due_date": "2026-04-01",
                "amount": "4000.00",
                "risk_assessment": "high",
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {added}");
    let (status, provision) = book
        .post(
            "/provision-doubtful-debts/provision/create",
            json!({
                "period_end": "2026-12-31",
                "provision_opening": "0.00",
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {provision}");
    let (_, listed) = book.get("/provision-doubtful-debts/provisions").await;
    assert!(listed.to_string().contains("Shaky"), "{listed}");
    // the aged provision's own listing is a different list and must not have gained this debtor
    let (_, aged) = book.get("/provision-history").await;
    assert!(
        !aged.to_string().contains("Shaky"),
        "the doubtful debtor landed in the aged provision's history: {aged}"
    );
}
