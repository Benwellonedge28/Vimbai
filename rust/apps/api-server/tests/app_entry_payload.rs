#![forbid(unsafe_code)]

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::fake::MemoryGraph;
use tower::ServiceExt;

struct Walk {
    router: axum::Router,
    user: String,
    book: String,
}

impl Walk {
    fn stranger() -> Self {
        Self {
            router: vimbai_api::records::router(Arc::new(MemoryGraph::new())),
            user: String::new(),
            book: String::new(),
        }
    }

    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json");
        if !self.user.is_empty() {
            builder = builder.header("x-user-id", &self.user);
        }
        if !self.book.is_empty() {
            builder = builder.header("x-book-id", &self.book);
        }
        let body = match body {
            Some(value) => Body::from(value.to_string()),
            None => Body::empty(),
        };
        let response = self
            .router
            .clone()
            .oneshot(builder.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 4_000_000)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

/// The journal form's own payload, posted where the form posts it, answered in the shape the form parses.
///
/// `createJournalEntry` posts `JournalEntry.toJson()` and then parses the reply with `JournalEntry.fromJson`, which declares
/// `debit` and `credit` as `double`. The engine answers `"250.000000"` - a string, because an exact decimal does not survive
/// a JSON number - so the post succeeded and the phone threw while reading its own success. This holds both halves: the
/// engine keeps answering the form's payload, and the money it answers with is a decimal string the app now reads.
#[tokio::test]
async fn the_journal_form_posts_a_payload_the_engine_answers_and_can_read_back() {
    let mut walk = Walk::stranger();
    let (status, made) = walk
        .call(
            Method::POST,
            "/identity/users/register",
            Some(json!({"email": "probe@example.com", "password": "Str0ng-pass-9", "first_name": "Probe"})),
        )
        .await;
    assert!(status.is_success(), "{made}");
    walk.user = made["id"].as_str().expect("an id").to_string();
    let (_, book) = walk
        .call(
            Method::POST,
            "/books",
            Some(json!({"name": "Probe", "currency": "USD", "tier": "business"})),
        )
        .await;
    walk.book = book["book"]["id"].as_str().unwrap().to_string();

    let (code, reply) = walk
        .call(
            Method::POST,
            "/accounting/journal-entries",
            Some(json!({
                "id": "3f2b6a1e-0000-4000-8000-000000000000",
                "entry_date": "2026-11-15T20:41:00.123456",
                "description": "first sale from the phone",
                "reference_number": "INV-1",
                "source_module": "MobileApp",
                "status": null,
                "lines": [
                    {"account_number": "1000", "description": null, "debit": 250.0, "credit": 0.0},
                    {"account_number": "4000", "description": null, "debit": 0.0, "credit": 250.0}
                ],
                "created_at": null,
                "updated_at": null
            })),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{reply}");
    assert_eq!(
        reply["description"],
        json!("first sale from the phone"),
        "{reply}"
    );
    // what the form writes is kept, and an entry the form does not mark posted stays a draft rather than moving money
    assert_eq!(reply["source_module"], json!("MobileApp"), "{reply}");
    assert_eq!(reply["reference_number"], json!("INV-1"), "{reply}");
    assert_eq!(reply["status"], json!("draft"), "{reply}");
    assert!(
        !reply["id"].as_str().unwrap_or_default().is_empty(),
        "{reply}"
    );
    assert!(reply["entry_date"].is_string(), "{reply}");
    assert!(reply["created_at"].is_string(), "{reply}");
    let lines = reply["lines"].as_array().expect("lines");
    assert_eq!(lines.len(), 2, "{reply}");
    // the money is a decimal string, not a float: this is the shape that used to throw on the phone
    for line in lines {
        assert!(line["account_number"].is_string(), "{line}");
        let debit = line["debit"].as_str().unwrap_or_else(|| panic!("{line}"));
        let credit = line["credit"].as_str().unwrap_or_else(|| panic!("{line}"));
        assert!(debit.parse::<f64>().is_ok(), "{line}");
        assert!(credit.parse::<f64>().is_ok(), "{line}");
    }
    let posted: f64 = lines
        .iter()
        .map(|line| line["debit"].as_str().unwrap().parse::<f64>().unwrap())
        .sum();
    assert_eq!(posted, 250.0, "{reply}");
}

/// The budget screen's own variance payload, answered at the address it posts to.
#[tokio::test]
async fn the_variance_analysis_the_budget_screen_asks_for_is_answered_in_the_shape_it_reads() {
    let walk = Walk::stranger();
    let (code, reply) = walk
        .call(
            Method::POST,
            "/budgeting/variance-analysis",
            Some(json!({
                "company_id": "book-1",
                "budget_id": "budget-1",
                "actual_results": [
                    {"account_id": "5000", "description": "Rent", "budget": 1000.0, "actual": 1200.0},
                    {"account_id": "5100", "description": "Wages", "budget": 2000.0, "actual": 1900.0}
                ],
                "tolerance_percentage": 10.0
            })),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{reply}");
    // the frame the screen reads before it reads any figure
    assert!(
        reply["items_outside_tolerance"].is_array() || reply["items_outside_tolerance"].is_null(),
        "{reply}"
    );
    assert!(reply.get("total_variance").is_some(), "{reply}");
    assert!(reply.get("favorable_variance").is_some(), "{reply}");
    assert!(reply.get("unfavorable_variance").is_some(), "{reply}");
}

/// A stranger who has registered, opened a Book and posted an entry of 250.
async fn stranger_with_a_posted_entry() -> Walk {
    let mut walk = Walk::stranger();
    let (status, made) = walk
        .call(
            Method::POST,
            "/identity/users/register",
            Some(json!({"email": "reports@example.com", "password": "Str0ng-pass-9", "first_name": "Reports"})),
        )
        .await;
    assert!(status.is_success(), "{made}");
    walk.user = made["id"].as_str().expect("an id").to_string();
    let (_, book) = walk
        .call(
            Method::POST,
            "/books",
            Some(json!({"name": "Reports", "currency": "USD", "tier": "business"})),
        )
        .await;
    walk.book = book["book"]["id"].as_str().unwrap().to_string();
    let (code, reply) = walk
        .call(
            Method::POST,
            "/accounting/journal-entries",
            Some(json!({
                "id": "3f2b6a1e-0000-4000-8000-000000000002",
                "entry_date": "2026-11-15T20:41:00.000000",
                "description": "a sale",
                "source_module": "MobileApp",
                "status": "posted",
                "lines": [
                    {"account_number": "1000", "debit": 250.0, "credit": 0.0},
                    {"account_number": "4000", "debit": 0.0, "credit": 250.0}
                ]
            })),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{reply}");
    walk
}

fn holds(report: &Value, keys: &[&str]) {
    for key in keys {
        assert!(
            report.get(*key).is_some(),
            "the screen reads `{key}`: {report}"
        );
    }
}

/// The three report screens each read a fixed frame; a frame that changes shape blanks the screen rather than failing loudly,
/// so each key they read is named here against a Book that has actually posted something.
#[tokio::test]
async fn the_report_screens_frames_are_answered_under_the_keys_they_read() {
    let walk = stranger_with_a_posted_entry().await;

    let (code, trial) = walk
        .call(
            Method::GET,
            "/accounting/trial-balance?as_of_date=2026-12-31T00:00:00.000",
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{trial}");
    holds(
        &trial,
        &[
            "report_date",
            "accounts",
            "total_debits",
            "total_credits",
            "is_balanced",
        ],
    );

    let (code, income) = walk
        .call(
            Method::GET,
            "/accounting/income-statement?start_date=2026-01-01T00:00:00.000&end_date=2026-12-31T00:00:00.000",
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{income}");
    holds(
        &income,
        &[
            "start_date",
            "end_date",
            "revenues",
            "expenses",
            "net_income",
        ],
    );

    let (code, sheet) = walk
        .call(
            Method::GET,
            "/accounting/balance-sheet?as_of_date=2026-12-31T00:00:00.000",
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{sheet}");
    holds(
        &sheet,
        &[
            "as_of_date",
            "assets",
            "liabilities",
            "equity",
            "total_assets",
            "total_liabilities_equity",
        ],
    );
}

/// The cash flow page reads a statement the service maps out of the engine's reply, key by key.
#[tokio::test]
async fn the_cash_flow_page_reads_keys_the_engine_answers() {
    let walk = stranger_with_a_posted_entry().await;
    let (code, made) = walk
        .call(
            Method::POST,
            "/cash-flow-statement/generate",
            Some(json!({
                "company_id": walk.book,
                "method": "indirect",
                "period_start": "2026-01-01T00:00:00.000",
                "period_end": "2026-12-31T00:00:00.000",
                "beginning_cash": 100.0,
                "operating_activities": [
                    {"description": "a sale", "amount": 250.0, "is_inflow": true}
                ],
                "investing_activities": [],
                "financing_activities": []
            })),
        )
        .await;
    assert!(code.is_success(), "{made}");
    holds(
        &made,
        &[
            "period_start",
            "period_end",
            "operating_activities",
            "net_operating",
            "net_change",
            "beginning_cash",
            "ending_cash",
        ],
    );
    // the page turns each line into a description and an amount, so both must survive the trip
    let line = &made["operating_activities"][0];
    assert!(line["description"].is_string(), "{made}");
    assert!(line["amount"].is_string(), "{made}");
}

/// The chart of accounts screen reads a fixed set of names off each account.
#[tokio::test]
async fn the_chart_of_accounts_reads_keys_the_engine_answers() {
    let walk = stranger_with_a_posted_entry().await;
    let (code, reply) = walk.call(Method::GET, "/accounting/accounts", None).await;
    assert_eq!(code, StatusCode::OK, "{reply}");
    let rows = reply["accounts"].as_array().or_else(|| reply.as_array());
    let rows = rows.unwrap_or_else(|| panic!("the screen reads a list: {reply}"));
    assert!(!rows.is_empty(), "{reply}");
    holds(
        &rows[0],
        &["account_number", "name", "account_type", "normal_balance"],
    );
}

/// The personal finance screens list what the engine answers, whichever shape it uses.
#[tokio::test]
async fn the_personal_finance_lists_answer_rows_the_screens_can_read() {
    let walk = stranger_with_a_posted_entry().await;
    let (code, opened) = walk
        .call(
            Method::POST,
            "/personal-finance/debts",
            Some(json!({
                "name": "Car loan",
                "principal": 1000.0,
                "annual_rate": 12.0,
                "term_months": 12,
                "started_at": "2026-01-01"
            })),
        )
        .await;
    assert!(code.is_success(), "{opened}");
    let (code, debts) = walk
        .call(Method::GET, "/personal-finance/debts", None)
        .await;
    assert_eq!(code, StatusCode::OK, "{debts}");
    // the engine answers a bare array; the app now reads either that or a wrapped one
    let rows = debts
        .as_array()
        .cloned()
        .or_else(|| debts["debts"].as_array().cloned())
        .unwrap_or_else(|| panic!("a list of debts: {debts}"));
    assert_eq!(rows.len(), 1, "{debts}");
    assert_eq!(rows[0]["name"], json!("Car loan"), "{debts}");

    let (code, recurring) = walk
        .call(Method::GET, "/personal-finance/recurring", None)
        .await;
    assert_eq!(code, StatusCode::OK, "{recurring}");
    assert!(
        recurring.is_array() || recurring["recurring"].is_array(),
        "{recurring}"
    );

    let (code, holdings) = walk
        .call(Method::GET, "/personal-finance/investments", None)
        .await;
    assert_eq!(code, StatusCode::OK, "{holdings}");
    assert!(
        holdings.is_array() || holdings["investments"].is_array(),
        "{holdings}"
    );
}

/// The group savings page reads its lists out of the names the engine wraps them in.
#[tokio::test]
async fn the_group_savings_lists_are_wrapped_under_the_names_the_page_reads() {
    let walk = stranger_with_a_posted_entry().await;
    let (code, made) = walk
        .call(
            Method::POST,
            "/family-community-group/groups?name=Kitchen%20Club&description=saves&contribution_frequency=monthly&contribution_amount=25",
            None,
        )
        .await;
    assert!(code.is_success(), "{made}");
    let (code, listed) = walk
        .call(Method::GET, "/family-community-group/groups", None)
        .await;
    assert_eq!(code, StatusCode::OK, "{listed}");
    let groups = listed["groups"]
        .as_array()
        .unwrap_or_else(|| panic!("the page reads `groups`: {listed}"));
    assert_eq!(groups.len(), 1, "{listed}");
    assert_eq!(groups[0]["name"], json!("Kitchen Club"), "{listed}");
}

/// A bank feed row names the connection `bank_connection_id` and the day `date`; the screen reads both.
#[tokio::test]
async fn a_bank_feed_row_carries_the_names_the_screen_falls_back_to() {
    let walk = stranger_with_a_posted_entry().await;
    let (code, connection) = walk
        .call(
            Method::POST,
            "/bank-feed/connections?bank_name=Stanbic&account_number=12345&account_type=checking",
            None,
        )
        .await;
    assert!(code.is_success(), "{connection}");
    let connection_id = connection["connection"]["id"]
        .as_str()
        .or_else(|| connection["id"].as_str())
        .unwrap_or_else(|| panic!("a connection: {connection}"))
        .to_string();

    let (code, imported) = walk
        .call(
            Method::POST,
            "/bank-feed/transactions/import",
            Some(json!({
                "bank_connection_id": connection_id,
                "external_id": "txn-1",
                "date": "2026-11-15",
                "amount": 250.0,
                "description": "a sale",
                "transaction_type": "credit"
            })),
        )
        .await;
    assert!(code.is_success(), "{imported}");

    let (code, rows) = walk
        .call(
            Method::GET,
            &format!("/bank-feed/transactions?connection_id={connection_id}"),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{rows}");
    let row = &rows[0];
    holds(
        row,
        &[
            "id",
            "bank_connection_id",
            "date",
            "amount",
            "transaction_type",
        ],
    );
    assert_eq!(row["bank_connection_id"], json!(connection_id), "{rows}");
}
