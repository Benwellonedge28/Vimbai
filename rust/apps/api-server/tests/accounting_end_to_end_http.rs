#![forbid(unsafe_code)]
//! Accounting, end to end, the way a person meets it.
//!
//! Everything else in this suite proves one address. This walks one Book through a whole trade and asks the reports after
//! each act, because "does accounting work" is not a question any single address can answer: it is whether what a person
//! records arrives in the statements without being asked to, and whether the statements agree with each other.
//!
//! The walk: open a Book the way the app opens one, sell on credit, take the money in, buy stock on credit, pay the
//! supplier, pay a cost in cash, charge a month's depreciation, and close the month. After each act the income statement,
//! the balance sheet, the trial balance and the general ledger are read and held against what the acts say should be there.
//! Nothing in the walk triggers a report: every figure is a view over what was recorded.
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

    /// A person opens a Book: the chart, the measures and the rules come with it.
    async fn open_a_book() -> Self {
        let mut walk = Self::stranger();
        let (status, made) = walk
            .call(
                Method::POST,
                "/identity/users/register",
                Some(
                    json!({"email": "trader@example.com", "password": "Str0ng-pass-9",
                            "first_name": "Trader"}),
                ),
            )
            .await;
        assert!(status.is_success(), "a person registers: {made}");
        walk.user = made["id"].as_str().expect("an id").to_string();

        let (status, book) = walk
            .call(
                Method::POST,
                "/books",
                Some(json!({"name": "The Trade", "currency": "USD", "tier": "business"})),
            )
            .await;
        assert!(status.is_success(), "a Book opens: {book}");
        walk.book = book["book"]["id"].as_str().expect("a Book").to_string();
        walk
    }

    /// A figure inside a list the report answers, found by a named key.
    async fn figure(&self, path: &str, list: &str, key: &str, name: &str, field: &str) -> String {
        let (status, body) = self.call(Method::GET, path, None).await;
        assert!(status.is_success(), "{path} answers: {body}");
        let rows = body[list].as_array().unwrap_or(&Vec::new()).clone();
        let row = rows
            .iter()
            .find(|row| row[key] == json!(name))
            .unwrap_or_else(|| panic!("{path} holds {name}: {body}"));
        match &row[field] {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        }
    }
}

/// What a statement's rows come to, as a whole number of cents: the reports list their accounts rather than only totalling
/// them, so the walk adds the list the way a reader would.
fn summed(report: &Value, list: &str) -> i128 {
    report[list]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| cents(row["amount"].as_str().unwrap_or("0")))
                .sum()
        })
        .unwrap_or(0)
}

/// A figure as a whole number of cents, so a comparison does not depend on how many zeros the engine prints.
fn cents(figure: &str) -> i128 {
    let trimmed = figure.trim();
    let (whole, part) = match trimmed.split_once('.') {
        Some((whole, part)) => (whole, part),
        None => (trimmed, ""),
    };
    let negative = whole.starts_with('-');
    let whole: i128 = whole.trim_start_matches('-').parse().unwrap_or(0);
    let mut part = part.to_string();
    while part.len() < 2 {
        part.push('0');
    }
    let part: i128 = part[..2].parse().unwrap_or(0);
    let total = whole * 100 + part;
    if negative {
        -total
    } else {
        total
    }
}

#[tokio::test]
async fn a_book_trades_from_nothing_to_a_closed_month_and_the_reports_agree() {
    let walk = Walk::open_a_book().await;

    // ---- nothing has happened yet, and the reports say so rather than showing a zero as a fact --------------------
    let (status, before) = walk
        .call(
            Method::GET,
            "/accounting/income-statement?start_date=2026-11-01&end_date=2026-11-30",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "a fresh Book's statement: {before}");
    assert_eq!(cents(before["net_income"].as_str().unwrap_or("0")), 0);

    // ---- the person sets up their environment: where their money sits --------------------------------------------
    // This is the only thing a person sets up before recording. They name the account they already hold at a bank or in
    // their hand, and the Book keeps a ledger for it, so nothing later asks them for an account number.
    let (status, money) = walk
        .call(
            Method::POST,
            "/cashbook/accounts",
            Some(
                json!({"account_code": "BANK-1", "account_name": "Bank", "account_type": "bank",
                        "opening_balance": "0.00", "currency": "USD"}),
            ),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the person registers where their money sits: {money}"
    );
    assert!(
        !money["account_number"].as_str().unwrap_or("").is_empty(),
        "registering a money account settles the ledger it posts to: {money}"
    );

    // ---- a sale on credit: the customer owes it, and revenue arrives with no report asked for ---------------------
    let (status, sale) = walk
        .call(
            Method::POST,
            "/accounting/sales-journal",
            Some(json!({"invoice_number": "INV-1", "customer_id": "acme",
                        "entry_date": "2026-11-03", "total_amount": "1000.00",
                        "tax_amount": "0.00"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "a sale is recorded: {sale}");

    let (status, statement) = walk
        .call(
            Method::GET,
            "/accounting/income-statement?start_date=2026-11-01&end_date=2026-11-30",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{statement}");
    assert_eq!(
        summed(&statement, "revenues"),
        100_000,
        "the sale is revenue without anything being triggered: {statement}"
    );

    // the customer owes it, and the balance sheet carries the receivable
    let owed = walk
        .figure(
            "/accounting/aged-receivables",
            "customers",
            "customer_id",
            "acme",
            "owed",
        )
        .await;
    assert_eq!(cents(&owed), 100_000, "the customer owes the sale");

    // ---- the customer pays: cash arrives, the debt falls, and nothing is recorded twice --------------------------
    let (status, receipt) = walk
        .call(
            Method::POST,
            "/accounting/cash-receipts-journal",
            Some(json!({"receipt_number": "RCT-1", "customer_id": "acme",
                        "entry_date": "2026-11-10", "amount": "1000.00",
                        "document_reference": "INV-1"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "a receipt is recorded: {receipt}");

    let (status, aged) = walk
        .call(Method::GET, "/accounting/aged-receivables", None)
        .await;
    assert_eq!(status, StatusCode::OK, "{aged}");
    let still_owed = aged["customers"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["customer_id"] == json!("acme")))
        .map(|row| cents(row["owed"].as_str().unwrap_or("0")))
        .unwrap_or(0);
    assert_eq!(still_owed, 0, "a paid invoice is not still owed: {aged}");

    // ---- stock bought on credit, then paid for ----------------------------------------------------------------
    let (status, purchase) = walk
        .call(
            Method::POST,
            "/accounting/purchases-journal",
            Some(
                json!({"purchase_order_number": "PO-1", "vendor_id": "supplies-ltd",
                        "entry_date": "2026-11-04", "total_amount": "400.00",
                        "tax_amount": "0.00"}),
            ),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "a purchase is recorded: {purchase}");

    let (status, paid) = walk
        .call(
            Method::POST,
            "/accounting/cash-disbursements-journal",
            Some(
                json!({"payment_number": "PAY-1", "vendor_id": "supplies-ltd",
                        "entry_date": "2026-11-12", "amount": "400.00",
                        "document_reference": "PO-1"}),
            ),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "a payment is recorded: {paid}");

    // ---- a cost paid in cash, said as a person says it ---------------------------------------------------------
    let (status, fact) = walk
        .call(
            Method::POST,
            "/accounting/facts",
            Some(json!({"what": "spent", "account": "rent", "money": "bank",
                        "amount": "150.00", "entry_date": "2026-11-05"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "a cost is recorded: {fact}");

    // ---- a thing the business now owns: paid for, and kept in the register ----------------------------------------
    let (status, bought) = walk
        .call(
            Method::POST,
            "/accounting/facts",
            Some(
                json!({"what": "spent", "account": "equipment", "money": "bank",
                        "amount": "300.00", "entry_date": "2026-11-02"}),
            ),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a thing bought is recorded: {bought}"
    );

    let (status, asset) = walk
        .call(
            Method::POST,
            "/fixed-assets-register/assets",
            Some(json!({"asset_code": "EQ-1", "asset_name": "Display fridge", "category": "equipment",
                        "acquisition_date": "2026-11-01", "acquisition_cost": "300.00",
                        "useful_life_years": 5, "depreciation_method": "straight_line",
                        "salvage_value": "0.00"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "the asset is registered: {asset}");

    // ---- the month's depreciation charges itself, and closing the month is an act ---------------------------------
    // Nobody asks for this: the rule a new Book starts with is switched on, and running it is the whole act - it reads
    // the register, works out the month's wear, and posts each asset's charge. That is what "autonomous" has to mean.
    let (status, rules) = walk.call(Method::GET, "/automation/rules", None).await;
    assert_eq!(status, StatusCode::OK, "the Book's rules read: {rules}");
    let depreciation_rule = rules["rules"]
        .as_array()
        .or_else(|| rules.as_array())
        .and_then(|rows| {
            rows.iter()
                .find(|row| row.to_string().contains("depreciat"))
                .cloned()
        })
        .unwrap_or_else(|| panic!("a new Book starts with a depreciation rule: {rules}"));
    assert_eq!(
        depreciation_rule["enabled"],
        json!(true),
        "the rule runs without anyone switching it on: {depreciation_rule}"
    );

    // The register works out what the month's wear is, and the engine posts it. The rule above does both in one act each
    // month; here the month is stated so the walk can name the figures it expects.
    let asset_id = asset["id"].as_str().unwrap_or_default();
    let (status, month) = walk
        .call(
            Method::POST,
            &format!("/fixed-assets-register/assets/{asset_id}/depreciate?period=2026-11"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "the month is recorded: {month}");

    let (status, charged) = walk
        .call(
            Method::POST,
            "/accounting/depreciation",
            Some(json!({"period": "2026-11"})),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the month's wear is charged: {charged}"
    );

    let (status, closed) = walk
        .call(
            Method::POST,
            "/accounting/closed-periods",
            Some(json!({"period": "2026-11", "closes_on": "2026-11-30",
                        "closed_by": "trader", "note": "the month is done"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "the month closes: {closed}");

    // ---- and now the reports, read back, all of them, agreeing -------------------------------------------------
    let (status, statement) = walk
        .call(
            Method::GET,
            "/accounting/income-statement?start_date=2026-11-01&end_date=2026-11-30",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{statement}");
    assert_eq!(
        summed(&statement, "revenues"),
        100_000,
        "revenue is the sale and only the sale: {statement}"
    );
    // 150 of rent and 5 of the fridge's wear. The 400 of stock is not a cost this month: it is stock the business holds,
    // and it becomes a cost when the goods it bought are sold. That distinction is the whole point of a balance sheet.
    assert_eq!(
        summed(&statement, "expenses"),
        15_500,
        "costs are what the month used up: {statement}"
    );
    assert_eq!(
        cents(statement["net_income"].as_str().unwrap_or("0")),
        84_500,
        "profit is the difference, not a figure anybody typed: {statement}"
    );

    // the balance sheet: 1000 in, less 400, 150 and 300 out, is 150 of cash; the 400 of stock is still held; the fridge is
    // worth 295 after its first month; and equity carries the profit, which is what is left once costs are taken off
    let (status, sheet) = walk
        .call(
            Method::GET,
            "/accounting/balance-sheet?as_of_date=2026-11-30",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{sheet}");
    assert_eq!(
        cents(sheet["total_assets"].as_str().unwrap_or("0")),
        84_500,
        "the money held is what came in less what went out: {sheet}"
    );
    assert_eq!(
        cents(sheet["total_assets"].as_str().unwrap_or("0")),
        cents(sheet["total_liabilities"].as_str().unwrap_or("0"))
            + cents(sheet["total_equity"].as_str().unwrap_or("0")),
        "the two sides agree: {sheet}"
    );

    // the trial balance proves the postings balance
    let (status, trial) = walk
        .call(
            Method::GET,
            "/accounting/trial-balance?as_of_date=2026-11-30",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{trial}");
    assert_eq!(
        trial["is_balanced"],
        json!(true),
        "every posting balances: {trial}"
    );

    // the general ledger runs the whole Book for the month and proves it both ways
    let (status, ledger) = walk
        .call(
            Method::GET,
            "/accounting/general-ledger?from=2026-11-01&to=2026-11-30",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{ledger}");
    assert_eq!(
        cents(ledger["total_debit"].as_str().unwrap_or("0")),
        cents(ledger["total_credit"].as_str().unwrap_or("0")),
        "the ledger's own totals agree: {ledger}"
    );

    // and the month being closed is a fact a reader can see, with who closed it
    let (status, periods) = walk
        .call(Method::GET, "/accounting/closed-periods", None)
        .await;
    assert_eq!(status, StatusCode::OK, "{periods}");
    let closed_months = periods["closed_months"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        closed_months.iter().any(|month| month == &json!("2026-11")),
        "the month reads as closed: {periods}"
    );

    // ---- a late entry into the closed month is refused, and says the way back -------------------------------------
    let (status, late) = walk
        .call(
            Method::POST,
            "/accounting/facts",
            Some(
                json!({"what": "spent", "account": "office", "money": "bank",
                        "amount": "10.00", "entry_date": "2026-11-20"}),
            ),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a closed month refuses a late entry: {late}"
    );
    assert!(
        late.to_string().to_lowercase().contains("closed")
            || late.to_string().to_lowercase().contains("reopen"),
        "the refusal says why and the way back: {late}"
    );
}
