//! accounting-service on HTTP: the chart of accounts, the journal, and the reports over it.

use accounting::ledger_book as book;
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
            .route(&format!("{prefix}/trial-balance"), get(trial_balance))
            .route(&format!("{prefix}/income-statement"), get(income_statement))
            .route(&format!("{prefix}/balance-sheet"), get(balance_sheet));
    }
    router.with_state(graph)
}
