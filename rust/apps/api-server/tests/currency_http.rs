//! currency over HTTP: the currencies, the rates between them and the conversions over them.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::{Graph, StoreError};
use tower::ServiceExt;

struct Scripted {
    queue: Mutex<Vec<Vec<Value>>>,
}

impl Scripted {
    fn queue(scripts: Vec<Vec<Value>>) -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(scripts),
        })
    }
}

impl Graph for Scripted {
    fn run(&self, _cypher: &str, _params: Value) -> Result<Vec<Value>, StoreError> {
        let mut queue = self.queue.lock().unwrap();
        if queue.is_empty() {
            return Ok(Vec::new());
        }
        Ok(queue.remove(0))
    }
}

async fn call(
    graph: Arc<Scripted>,
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
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn send(graph: Arc<Scripted>, uri: &str, body: Value) -> (StatusCode, Value) {
    call(graph, Method::POST, uri, Some(body)).await
}

async fn get(graph: Arc<Scripted>, uri: &str) -> (StatusCode, Value) {
    call(graph, Method::GET, uri, None).await
}

#[tokio::test]
async fn the_currencies_a_book_deals_in_are_listed() {
    let graph = Scripted::queue(vec![
        // the seeding check: this Book already deals in currencies
        vec![json!({"id": "c-usd", "code": "USD", "decimal_places": 2})],
        // the listing itself
        vec![
            json!({"id": "c-usd", "code": "USD", "decimal_places": 2, "is_active": true}),
            json!({"id": "c-jpy", "code": "JPY", "decimal_places": 0, "is_active": true}),
        ],
    ]);
    let (status, body) = get(graph, "/currencies").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.as_array().expect("a list").len(), 2);
    assert_eq!(body[1]["code"], json!("JPY"));
}

#[tokio::test]
async fn a_conversion_lands_on_the_targets_decimal_places() {
    let graph = Scripted::queue(vec![
        // the Book already deals in currencies, so nothing is seeded
        vec![json!({"id": "c-usd", "code": "USD", "decimal_places": 2})],
        // the newest rate for each pair
        vec![
            json!({"id": "r-1", "from_currency": "USD", "to_currency": "JPY", "rate": "149.5", "effective_date": "2026-03-01"}),
        ],
        // the currencies again, for the target's decimal places
        vec![json!({"id": "c-jpy", "code": "JPY", "decimal_places": 0})],
    ]);
    let (status, body) = send(
        graph,
        "/convert",
        json!({"from_currency": "usd", "to_currency": "jpy", "amount": 10.55}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["converted_amount"], json!("1577.000000"));
    assert_eq!(body["rate_used"], json!("149.500000"));
}

#[tokio::test]
async fn a_pair_with_no_rate_between_it_is_refused_over_http() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "c-usd", "code": "USD", "decimal_places": 2})],
        vec![
            json!({"id": "r-1", "from_currency": "USD", "to_currency": "GBP", "rate": "0.8", "effective_date": "2026-03-01"}),
        ],
        vec![json!({"id": "c-usd", "code": "USD", "decimal_places": 2})],
    ]);
    let (status, body) = send(
        graph,
        "/convert",
        json!({"from_currency": "CHF", "to_currency": "JPY", "amount": 10}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"],
        json!("No exchange rate found for CHF to JPY")
    );
}

#[tokio::test]
async fn a_batch_answers_every_line_it_was_sent() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "c-usd", "code": "USD", "decimal_places": 2})],
        vec![
            json!({"id": "r-1", "from_currency": "USD", "to_currency": "GBP", "rate": "0.8", "effective_date": "2026-03-01"}),
        ],
        vec![json!({"id": "c-gbp", "code": "GBP", "decimal_places": 2})],
    ]);
    // the batch arrives as a bare JSON list, which FastAPI took as the whole body
    let (status, body) = send(
        graph,
        "/convert/batch",
        json!([
            {"from_currency": "USD", "to_currency": "GBP", "amount": 100},
            {"from_currency": "CHF", "to_currency": "JPY", "amount": 5}
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], json!(2));
    assert_eq!(body["successful"], json!(1));
    assert_eq!(body["failed"], json!(1));
    assert_eq!(
        body["results"][0]["result"]["converted_amount"],
        json!("80.000000")
    );
}

#[tokio::test]
async fn an_amount_is_written_the_way_its_currency_is_written() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "c-usd", "code": "USD"})],
        vec![json!({"id": "c-usd", "code": "USD", "symbol": "$", "decimal_places": 2})],
    ]);
    let (status, body) = get(graph, "/format/USD/1234.5").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["formatted"], json!("$1,234.50"));
    assert_eq!(body["symbol"], json!("$"));
}

#[tokio::test]
async fn a_rate_alert_is_answered_without_a_book_or_a_record() {
    let graph = Scripted::queue(vec![]);
    let (status, body) = send(
        graph,
        "/alerts/rate?from_currency=usd&to_currency=gbp&target_rate=0.85&direction=above",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["alert_id"], json!("rate_alert_USD_GBP"));
    assert_eq!(body["status"], json!("active"));
}

#[tokio::test]
async fn the_currency_statistics_count_what_the_book_holds() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "c-usd", "code": "USD", "is_active": true})],
        vec![
            json!({"id": "c-usd", "code": "USD", "is_active": true}),
            json!({"id": "c-eur", "code": "EUR", "is_active": false}),
        ],
        vec![
            json!({"id": "r-1", "from_currency": "USD", "to_currency": "EUR", "rate": "0.9", "effective_date": "2026-02-01"}),
            json!({"id": "r-2", "from_currency": "USD", "to_currency": "EUR", "rate": "0.92", "effective_date": "2026-03-01"}),
        ],
    ]);
    let (status, body) = get(graph, "/currency-stats").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total_currencies"], json!(2));
    assert_eq!(body["active_currencies"], json!(1));
    assert_eq!(body["total_rates"], json!(2));
    assert_eq!(body["latest_rate_date"], json!("2026-03-01"));
}

#[tokio::test]
async fn the_newest_stated_rate_is_the_one_a_pair_answers() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "c-usd", "code": "USD"})],
        vec![
            json!({"id": "r-old", "from_currency": "USD", "to_currency": "GBP", "rate": "0.8", "effective_date": "2026-01-01"}),
            json!({"id": "r-new", "from_currency": "USD", "to_currency": "GBP", "rate": "0.85", "effective_date": "2026-03-01"}),
        ],
    ]);
    let (status, body) = get(graph, "/rates/USD/GBP").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], json!("r-new"));
    assert_eq!(body["rate"], json!("0.85"));
}

#[tokio::test]
async fn a_multi_currency_transaction_is_summed_into_its_base() {
    let graph = Scripted::queue(vec![
        // the seeding check
        vec![json!({"id": "c-usd", "code": "USD", "decimal_places": 2})],
        // the base currency has to be one the Book deals in
        vec![
            json!({"id": "c-usd", "code": "USD", "decimal_places": 2}),
            json!({"id": "c-gbp", "code": "GBP", "decimal_places": 2}),
        ],
        vec![
            json!({"id": "r-1", "from_currency": "USD", "to_currency": "GBP", "rate": "0.8", "effective_date": "2026-03-01"}),
        ],
    ]);
    let (status, body) = send(
        graph,
        "/validate-transaction",
        json!({
            "base_currency": "USD",
            "lines": [
                {"currency": "GBP", "amount": 80, "account": "1000", "description": "sales"},
                {"currency": "USD", "amount": 20}
            ]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total_in_base"], json!("120.000000"));
    assert_eq!(body["line_count"], json!(2));
}
