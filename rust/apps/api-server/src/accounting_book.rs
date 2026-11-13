//! accounting-service on HTTP: the chart of accounts, the journal, and the reports over it.

use accounting::bookkeeping as papers;
use accounting::ledger_book as book;
use accounting::subsidiary_journals as journals;
use axum::{
    extract::{Path, RawQuery, State},
    http::HeaderMap,
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

use crate::records::{answer, query_object, scope_of, GraphHandle};

type GraphState = State<GraphHandle>;

fn body_of(payload: Option<Json<Value>>) -> Value {
    payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}))
}

async fn create_account(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book::create_account(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn list_accounts(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book::list_accounts(graph.as_ref(), &scope))
}

async fn get_account(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(account_number): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book::get_account(graph.as_ref(), &scope, &account_number))
}

async fn update_account(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(account_number): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book::update_account(
        graph.as_ref(),
        &scope,
        &account_number,
        &body_of(payload),
    ))
}

async fn delete_account(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(account_number): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book::delete_account(
        graph.as_ref(),
        &scope,
        &account_number,
    ))
}

async fn period_activity(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(account_number): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book::account_period_activity(
        graph.as_ref(),
        &scope,
        &account_number,
        &query_object(query.as_deref()),
    ))
}

async fn create_entry(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book::create_entry(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn list_entries(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book::list_entries(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn get_entry(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(entry_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book::get_entry(graph.as_ref(), &scope, &entry_id))
}

async fn update_entry(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(entry_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book::update_entry(
        graph.as_ref(),
        &scope,
        &entry_id,
        &body_of(payload),
    ))
}

async fn delete_entry(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(entry_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book::delete_entry(graph.as_ref(), &scope, &entry_id))
}

async fn integrity(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book::verify_chain(graph.as_ref(), &scope))
}

async fn ledger(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(account_number): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book::ledger_report(
        graph.as_ref(),
        &scope,
        &account_number,
        &query_object(query.as_deref()),
    ))
}

/// `GET /accounts/{number}/postings`: the lines that moved one account, walked to through the ledger's edges.
///
/// The limit is the caller's, with a sane cap: an account can have been moved more times than one answer should carry,
/// and paging it is the client's business, but an unpaged read must still not ask for the whole ledger.
async fn account_postings(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(account_number): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let asked = query_object(query.as_deref())
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(200);
    answer(book::account_postings(
        graph.as_ref(),
        &scope,
        &account_number,
        asked.min(5000) as usize,
    ))
}

async fn trial_balance(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book::trial_balance(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn income_statement(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book::income_statement(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn balance_sheet(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(book::balance_sheet(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn create_sale(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(journals::create_sale(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn list_sales(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(journals::list_sales(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn get_sale(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(entry_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(journals::get_sale(graph.as_ref(), &scope, &entry_id))
}

async fn create_purchase(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(journals::create_purchase(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn list_purchases(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(journals::list_purchases(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn create_receipt(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(journals::create_receipt(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn list_receipts(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(journals::list_receipts(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn create_disbursement(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(journals::create_disbursement(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn list_disbursements(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(journals::list_disbursements(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn create_sales_return(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(journals::create_sales_return(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn create_purchases_return(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(journals::create_purchases_return(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn create_vendor_bill(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(journals::create_vendor_bill(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn receivables(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(journals::receivables_ledger(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn payables(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(journals::payables_ledger(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn fixed_asset_ledger(RawQuery(query): RawQuery) -> Response {
    answer(Ok(journals::fixed_assets_ledger(&query_object(
        query.as_deref(),
    ))))
}

async fn inventory(RawQuery(query): RawQuery) -> Response {
    answer(Ok(journals::inventory_ledger(&query_object(
        query.as_deref(),
    ))))
}

async fn create_petty_cash_fund(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::create_petty_cash_fund(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn list_petty_cash_funds(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::list_petty_cash_funds(graph.as_ref(), &scope))
}

async fn create_petty_cash_entry(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::create_petty_cash_entry(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn list_petty_cash_entries(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::list_petty_cash_entries(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn create_reconciliation(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::create_bank_reconciliation(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn list_reconciliations(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::list_bank_reconciliations(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn latest_reconciliation(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(bank_account): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::latest_bank_reconciliation(
        graph.as_ref(),
        &scope,
        &bank_account,
    ))
}

async fn create_statement(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::create_statement_of_affairs(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn list_statements(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::list_statements_of_affairs(graph.as_ref(), &scope))
}

async fn statement_on(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(as_of_date): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::statement_of_affairs_on(
        graph.as_ref(),
        &scope,
        &as_of_date,
    ))
}

async fn create_capital(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::create_capital_calculation(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn list_capital(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::list_capital_calculations(graph.as_ref(), &scope))
}

async fn get_capital(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(calc_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::get_capital_calculation(
        graph.as_ref(),
        &scope,
        &calc_id,
    ))
}

async fn create_control(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::create_control_account(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn list_control(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::list_control_accounts(
        graph.as_ref(),
        &scope,
        &query_object(query.as_deref()),
    ))
}

async fn create_receipts_payments(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::create_receipts_payments(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn list_receipts_payments(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::list_receipts_payments(graph.as_ref(), &scope))
}

async fn create_conversion(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::create_single_entry_conversion(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn list_conversions(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::list_single_entry_conversions(
        graph.as_ref(),
        &scope,
    ))
}

async fn create_estimation(
    State(graph): GraphState,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::create_profit_estimation(
        graph.as_ref(),
        &scope,
        &body_of(payload),
    ))
}

async fn list_estimations(State(graph): GraphState, headers: HeaderMap) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(papers::list_profit_estimations(graph.as_ref(), &scope))
}

/// The service's own paths, and the same routes under the record prefix.
pub fn router(graph: GraphHandle) -> Router {
    let mut router = Router::new();
    for prefix in ["/accounting", "/records/accounting"] {
        router = router
            .route(
                &format!("{prefix}/accounts"),
                post(create_account).get(list_accounts),
            )
            .route(
                &format!("{prefix}/accounts/:account_number"),
                get(get_account).put(update_account).delete(delete_account),
            )
            .route(
                &format!("{prefix}/accounts/:account_number/period-activity"),
                get(period_activity),
            )
            // the ledger's own edges answer this one: the lines that moved the account, walked to from the account
            .route(
                &format!("{prefix}/accounts/:account_number/postings"),
                get(account_postings),
            )
            .route(
                &format!("{prefix}/journal-entries"),
                post(create_entry).get(list_entries),
            )
            .route(
                &format!("{prefix}/journal-entries/:entry_id"),
                get(get_entry).put(update_entry).delete(delete_entry),
            )
            .route(&format!("{prefix}/ledger/integrity"), get(integrity))
            .route(&format!("{prefix}/ledgers/:account_number"), get(ledger))
            .route(
                &format!("{prefix}/sales-journal"),
                post(create_sale).get(list_sales),
            )
            .route(&format!("{prefix}/sales-journal/:entry_id"), get(get_sale))
            .route(
                &format!("{prefix}/purchases-journal"),
                post(create_purchase).get(list_purchases),
            )
            .route(
                &format!("{prefix}/cash-receipts-journal"),
                post(create_receipt).get(list_receipts),
            )
            .route(
                &format!("{prefix}/cash-disbursements-journal"),
                post(create_disbursement).get(list_disbursements),
            )
            .route(
                &format!("{prefix}/sales-returns-journal"),
                post(create_sales_return),
            )
            .route(
                &format!("{prefix}/purchases-returns-journal"),
                post(create_purchases_return),
            )
            .route(&format!("{prefix}/vendor-bills"), post(create_vendor_bill))
            .route(
                &format!("{prefix}/subsidiary-ledgers/accounts-receivable"),
                get(receivables),
            )
            .route(
                &format!("{prefix}/subsidiary-ledgers/accounts-payable"),
                get(payables),
            )
            .route(
                &format!("{prefix}/subsidiary-ledgers/fixed-assets"),
                get(fixed_asset_ledger),
            )
            .route(
                &format!("{prefix}/subsidiary-ledgers/inventory"),
                get(inventory),
            )
            .route(
                &format!("{prefix}/petty-cash-funds"),
                post(create_petty_cash_fund).get(list_petty_cash_funds),
            )
            .route(
                &format!("{prefix}/petty-cash-entries"),
                post(create_petty_cash_entry).get(list_petty_cash_entries),
            )
            .route(
                &format!("{prefix}/bank-reconciliation"),
                post(create_reconciliation).get(list_reconciliations),
            )
            .route(
                &format!("{prefix}/bank-reconciliation/latest/:bank_account"),
                get(latest_reconciliation),
            )
            .route(
                &format!("{prefix}/statements-of-affairs"),
                post(create_statement).get(list_statements),
            )
            .route(
                &format!("{prefix}/statements-of-affairs/:as_of_date"),
                get(statement_on),
            )
            .route(
                &format!("{prefix}/capital-calculations"),
                post(create_capital).get(list_capital),
            )
            .route(
                &format!("{prefix}/capital-calculations/:calc_id"),
                get(get_capital),
            )
            .route(
                &format!("{prefix}/control-accounts"),
                post(create_control).get(list_control),
            )
            .route(
                &format!("{prefix}/receipts-payments"),
                post(create_receipts_payments).get(list_receipts_payments),
            )
            .route(
                &format!("{prefix}/single-entry-conversions"),
                post(create_conversion).get(list_conversions),
            )
            .route(
                &format!("{prefix}/profit-estimations"),
                post(create_estimation).get(list_estimations),
            )
            .route(&format!("{prefix}/trial-balance"), get(trial_balance))
            .route(&format!("{prefix}/income-statement"), get(income_statement))
            .route(&format!("{prefix}/balance-sheet"), get(balance_sheet));
    }
    router.with_state(graph)
}
