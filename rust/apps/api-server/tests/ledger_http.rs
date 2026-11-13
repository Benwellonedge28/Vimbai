//! The ledger kernel's four services over HTTP: the paths the Python services published, the shapes they
//! answered, and the two places the port deliberately answers differently.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::{BookScope, Graph, StoreError};
use tower::ServiceExt;

struct Scripted {
    rows: Vec<Value>,
    queue: Mutex<Vec<Vec<Value>>>,
    seen: Mutex<Vec<(String, Value)>>,
}

impl Scripted {
    fn returning(rows: Vec<Value>) -> Arc<Self> {
        Arc::new(Self {
            rows,
            queue: Mutex::new(Vec::new()),
            seen: Mutex::new(Vec::new()),
        })
    }

    fn queue(scripts: Vec<Vec<Value>>) -> Arc<Self> {
        Arc::new(Self {
            rows: Vec::new(),
            queue: Mutex::new(scripts),
            seen: Mutex::new(Vec::new()),
        })
    }

    fn last(&self) -> (String, Value) {
        self.seen
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or_else(|| (String::new(), Value::Null))
    }
}

impl Graph for Scripted {
    fn run(&self, cypher: &str, params: Value) -> Result<Vec<Value>, StoreError> {
        self.seen.lock().unwrap().push((cypher.to_owned(), params));
        let mut queue = self.queue.lock().unwrap();
        if queue.is_empty() {
            return Ok(self.rows.clone());
        }
        Ok(queue.remove(0))
    }
}

async fn call(
    graph: Arc<Scripted>,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let router = vimbai_api::records::router(graph);
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1");
    let request = match body {
        Some(payload) => builder
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };

    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

fn book() -> BookScope {
    BookScope::new("user-1", "book-1")
}

// --- double entry principles: the reference, and the entry that has to balance ---

#[tokio::test]
async fn a_journal_that_balances_is_accepted_and_one_a_cent_out_is_not() {
    let graph = Scripted::returning(vec![]);
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/validate-journal-entry",
        Some(json!({"entries": [
            {"account_code": "1000", "account_name": "Cash", "debit": 250, "credit": 0},
            {"account_code": "4000", "account_name": "Sales", "debit": 0, "credit": 250},
        ]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], json!(true));
    assert_eq!(body["total_debit"], json!("250.000000"));
    assert_eq!(body["difference"], json!("0.000000"));

    // the shipped service excused a drift under a cent; the port decides on the amounts themselves
    let (status, body) = call(
        graph,
        Method::POST,
        "/validate-journal-entry",
        Some(json!({"entries": [
            {"account_code": "1000", "account_name": "Cash", "debit": 250, "credit": 0},
            {"account_code": "4000", "account_name": "Sales", "debit": 0, "credit": 249.99},
        ]})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("double entry violation"));
}

#[tokio::test]
async fn the_account_type_catalogue_holds_sixteen_types() {
    let graph = Scripted::returning(vec![]);
    let (status, body) = call(graph, Method::GET, "/account-types", None).await;
    assert_eq!(status, StatusCode::OK);
    let listed = body["account_types"].as_array().expect("a catalogue");
    assert_eq!(listed.len(), 16);
    assert_eq!(listed[0]["type"], json!("current_asset"));
    assert_eq!(listed[0]["normal_balance"], json!("debit"));
    assert_eq!(listed[13]["type"], json!("administrative_expense"));
    assert_eq!(listed[14]["type"], json!("selling_expense"));
    assert_eq!(listed[15]["type"], json!("financial_expense"));
}

#[tokio::test]
async fn the_rules_answer_for_a_type_the_shipped_route_could_not() {
    let graph = Scripted::returning(vec![]);
    let (status, body) = call(graph, Method::GET, "/account-rules/current_asset", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["category"], json!("asset"));
    assert_eq!(body["normal_balance"], json!("debit"));
    assert!(!body["examples"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn a_debit_and_credit_is_worked_out_from_the_account_side() {
    let graph = Scripted::returning(vec![]);
    // the shipped route took the whole payload in the query string of a POST
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/calculate-entry?account_type=current_asset&amount=100&transaction_type=purchase",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["debit"], json!("100.000000"));
    assert_eq!(body["credit"], json!("0.000000"));

    let (status, body) = call(
        graph,
        Method::POST,
        "/calculate-entry?account_type=current_liability&amount=100",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["debit"], json!("0.000000"));
    assert_eq!(body["credit"], json!("100.000000"));
}

#[tokio::test]
async fn a_sale_suggests_receivables_and_revenue() {
    let graph = Scripted::returning(vec![]);
    let (status, body) = call(
        graph,
        Method::POST,
        "/transaction-analyzer?description=sale%20of%20goods&amount=120",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["suggested_entry"]["entries"][0]["account_name"],
        json!("Accounts Receivable")
    );
    assert_eq!(
        body["suggested_entry"]["entries"][0]["debit"],
        json!("120.000000")
    );
}

#[tokio::test]
async fn the_remaining_reference_answers_still_answer() {
    let graph = Scripted::returning(vec![]);
    let (status, body) = call(graph.clone(), Method::GET, "/accounting-equation", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["equation"], json!("Assets = Liabilities + Equity"));

    let (status, body) = call(graph, Method::POST, "/journal-entries/sale", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["entries"].as_array().unwrap().len(), 2);
}

// --- control account reconciliation ---

#[tokio::test]
async fn a_reported_break_is_kept_with_its_variance() {
    let graph = Scripted::returning(vec![json!({"id": "error-1"})]);
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/errors/report",
        Some(json!({
            "error_type": "amount_mismatch",
            "control_account_type": "sales_ledger",
            "description": "invoice posted twice",
            "amount": 900,
            "expected_amount": 1200,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (cypher, params) = graph.last();
    assert!(
        cypher.contains("CREATE (x:ReconciliationError)"),
        "{cypher}"
    );
    assert_eq!(params["props"]["variance"], json!("300.000000"));
    assert_eq!(params["props"]["status"], json!("pending"));
    assert_eq!(params["props"]["book_id"], json!("book-1"));
}

#[tokio::test]
async fn reconciling_snapshots_the_pending_breaks_against_that_account() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "e-1", "control_account_type": "sales_ledger", "status": "pending"}),
            json!({"id": "e-2", "control_account_type": "purchases_ledger", "status": "pending"}),
        ],
        vec![json!({"id": "report-1"})],
    ]);
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/reconcile/sales_ledger?control_balance=1000&ledger_total=1010&as_of_date=2026-03-31",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (cypher, params) = graph.last();
    assert!(
        cypher.contains("CREATE (x:ReconciliationReport)"),
        "{cypher}"
    );
    assert_eq!(params["props"]["error_count"], json!(1));
    assert_eq!(params["props"]["difference"], json!("-10.000000"));
    assert_eq!(params["props"]["status"], json!("unreconciled"));
}

#[tokio::test]
async fn an_unknown_control_account_is_refused() {
    let graph = Scripted::returning(vec![]);
    let (status, _) = call(graph, Method::POST, "/reconcile/petty_cash", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn the_error_list_narrows_to_the_status_asked_for() {
    let graph = Scripted::returning(vec![
        json!({"id": "e-1", "error_type": "posting_error", "status": "pending"}),
        json!({"id": "e-2", "error_type": "posting_error", "status": "resolved"}),
    ]);
    let (status, body) = call(graph, Method::GET, "/errors?status=pending", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], json!(1));
    assert_eq!(body["errors"][0]["id"], json!("e-1"));
}

#[tokio::test]
async fn the_summary_counts_only_pending_variance() {
    let graph = Scripted::returning(vec![
        json!({"id": "e-1", "error_type": "posting_error", "status": "pending", "variance": "10.00"}),
        json!({"id": "e-2", "error_type": "rounding_error", "status": "resolved", "variance": "90.00"}),
    ]);
    let (status, body) = call(graph, Method::GET, "/summary", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["pending_errors"], json!(1));
    assert_eq!(body["total_variance"], json!("10.000000"));
}

// --- financial integrity ---

#[tokio::test]
async fn the_hash_check_verifies_against_the_stored_digest() {
    let graph = Scripted::returning(vec![json!({"id": "check-1"})]);
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/check/hash?company_id=c-1&entity_type=journal_entry&entity_id=j-1&data=abc&expected_hash=ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["passed"], json!(true));
    assert_eq!(
        body["actual_hash"],
        json!("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
}

#[tokio::test]
async fn a_balance_check_passes_within_its_tolerance() {
    let graph = Scripted::returning(vec![json!({"id": "check-2"})]);
    let (status, body) = call(
        graph,
        Method::POST,
        "/check/balance?company_id=c-1&account_id=a-1&debits=100&credits=100.005",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["passed"], json!(true));
}

#[tokio::test]
async fn the_integrity_report_averages_one_companys_checks() {
    let graph = Scripted::returning(vec![
        json!({"id": "k-1", "company_id": "c-1", "passed": true}),
        json!({"id": "k-2", "company_id": "c-1", "passed": false}),
        json!({"id": "k-3", "company_id": "c-2", "passed": true}),
    ]);
    let (status, body) = call(graph, Method::GET, "/report/c-1", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_checks"], json!(2));
    assert_eq!(body["pass_rate"], json!("50.000000"));
}

// --- the document state machine ---

#[tokio::test]
async fn a_document_is_created_in_draft_and_moves_only_where_the_map_allows() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "doc-1"})],
        vec![json!({"id": "doc-1", "current_state": "draft", "history": []})],
        vec![json!({"id": "doc-1"})],
    ]);
    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/documents",
        Some(json!({"company_id": "c-1", "document_type": "invoice", "current_state": "posted"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (cypher, params) = graph.last();
    assert!(cypher.contains("CREATE (x:FinancialDocument)"), "{cypher}");
    assert_eq!(params["props"]["current_state"], json!("draft"));

    let (status, body) = call(
        graph.clone(),
        Method::POST,
        "/documents/doc-1/transition?to_state=pending_approval&user_id=u-2&notes=for%20review",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["current_state"], json!("pending_approval"));
    assert_eq!(body["history_count"], json!(1));
}

#[tokio::test]
async fn a_move_out_of_a_terminal_state_is_refused() {
    let graph = Scripted::returning(vec![json!({
        "id": "doc-2", "current_state": "cancelled", "history": []
    })]);
    let (status, body) = call(
        graph,
        Method::POST,
        "/documents/doc-2/transition?to_state=posted",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("invalid transition"));
}

#[tokio::test]
async fn the_state_catalogue_names_the_moves() {
    let graph = Scripted::returning(vec![]);
    let (status, body) = call(graph, Method::GET, "/states", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["states"].as_array().unwrap().len(), 6);
    assert_eq!(body["transitions"]["posted"], json!(["archived"]));
}

#[tokio::test]
async fn the_records_hang_off_the_caller_and_their_book() {
    let graph = Scripted::returning(vec![
        json!({"id": "doc-3", "current_state": "draft", "history": []}),
    ]);
    let (status, body) = call(graph.clone(), Method::GET, "/documents/doc-3", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], json!("doc-3"));

    let (cypher, params) = graph.last();
    assert!(cypher.contains("OWNS_FINANCIAL_DOCUMENT"), "{cypher}");
    assert_eq!(params["user_id"], json!(book().user_id));
    assert_eq!(params["book_id"], json!(book().book_id));
}

// --- an account's postings: the read the ledger's edges exist for ---

#[tokio::test]
async fn an_accounts_postings_are_read_by_walking_in_from_the_account() {
    let graph = Scripted::returning(vec![json!({
        "l": {"id": "line-1", "line_no": 1, "account_number": "1000", "debit": "250.000000",
              "credit": "0.000000", "description": "sale"},
        "entry_id": "je-1", "entry_date": "2026-03-15", "entry_description": "sale",
    })]);
    let (status, answer) = call(
        graph.clone(),
        Method::GET,
        "/accounting/accounts/1000/postings",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(
        answer["postings"].as_array().map(Vec::len),
        Some(1),
        "{answer}"
    );
    assert_eq!(answer["postings"][0]["entry_id"], json!("je-1"));
    assert_eq!(answer["total_debit"], json!("250.000000"));
    assert_eq!(answer["total_credit"], json!("0.000000"));

    // the statement walks in from the account through the line's own edge, rather than listing every entry
    let (statement, params) = graph.last();
    assert!(
        statement.contains("-[:DEBITED|CREDITED]->(a:Account"),
        "{statement}"
    );
    assert_eq!(params["account_number"], json!("1000"));
}
