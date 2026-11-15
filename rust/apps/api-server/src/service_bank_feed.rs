//! bank-feed-service on HTTP.
//!
//! Bank connections, the rules that classify what a feed imports, manual sync, and the statistics over both. The rules live
//! in `vimbai_banking::bank_feed`; what was missing was the addresses. The service that published them was retired, its
//! gateway entry kept pointing at a container that no longer existed, and fifteen addresses a client had been given answered
//! a 502.
//!
//! They are mounted *with* the prefix rather than stripped, which is the one thing here worth knowing: the bare paths this
//! surface published - `/rules`, `/transactions`, `/connections` - are already claimed by alerts, sales-ledger-control and
//! banking-integration, and a stripping entry would hand a caller of one of those the other's answer. So the prefix is part
//! of the address, and nothing existing moves.
#![forbid(unsafe_code)]

use axum::{
    extract::{Path, RawQuery, State},
    http::HeaderMap,
    response::Response,
    routing::{get, post, put},
    Json, Router,
};
use banking::bank_feed as service;
use serde_json::{json, Value};
use store::{BookScope, Graph, StoreError};

use crate::records::{
    answer, body_or_query, overlay, query_object, query_param, scope_of, GraphHandle,
};

type GraphState = State<GraphHandle>;

/// Answer one request through the crate, or hand back the caller's refusal.
fn through<F>(graph: GraphHandle, headers: HeaderMap, ask: F) -> Response
where
    F: FnOnce(&(dyn Graph + Send + Sync), &BookScope) -> Result<Value, StoreError>,
{
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(refusal) => return *refusal,
    };
    answer(ask(graph.as_ref(), &scope))
}

/// The organisation a request names: in the query for a read, in the body for a write.
fn organization(asked: &Value, query: Option<&str>) -> String {
    if let Some(found) = query_param(query, "organization_id") {
        return found;
    }
    asked
        .get("organization_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn today(asked: &Value, query: Option<&str>) -> String {
    if let Some(found) = query_param(query, "today") {
        return found;
    }
    asked
        .get("today")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

async fn root(State(graph): GraphState, headers: HeaderMap) -> Response {
    through(graph, headers, |_, _| {
        Ok(json!({"service": "bank-feed-service", "status": "healthy", "engine": "rust"}))
    })
}

async fn list_connections(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let organization = organization(&Value::Null, query.as_deref());
    through(graph, headers, |graph, scope| {
        service::list_connections(graph, scope, &organization)
    })
}

async fn post_connection(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Option<Json<Value>>,
) -> Response {
    let asked = overlay(
        query_object(query.as_deref()),
        body.map(|Json(value)| value),
    );
    let organization = organization(&asked, query.as_deref());
    through(graph, headers, |graph, scope| {
        service::create_connection(graph, scope, &asked, &organization)
    })
}

async fn get_connection(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    through(graph, headers, |graph, scope| {
        service::get_connection(graph, scope, &id)
    })
}

async fn delete_connection(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    through(graph, headers, |graph, scope| {
        service::delete_connection(graph, scope, &id)
    })
}

async fn connection_balance(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let today = today(&Value::Null, query.as_deref());
    through(graph, headers, |graph, scope| {
        service::account_balance(graph, scope, &id, &today)
    })
}

async fn list_rules(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let asked = query_object(query.as_deref());
    through(graph, headers, |graph, scope| {
        service::list_feed_rules(graph, scope, &asked)
    })
}

async fn post_rule(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Option<Json<Value>>,
) -> Response {
    let asked = overlay(
        body_or_query(query.as_deref()),
        body.map(|Json(value)| value),
    );
    through(graph, headers, |graph, scope| {
        service::create_feed_rule(graph, scope, &asked)
    })
}

async fn put_rule(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(id): Path<String>,
    RawQuery(query): RawQuery,
    body: Option<Json<Value>>,
) -> Response {
    let asked = overlay(
        query_object(query.as_deref()),
        body.map(|Json(value)| value),
    );
    through(graph, headers, |graph, scope| {
        service::update_feed_rule(graph, scope, &id, &asked)
    })
}

async fn delete_rule(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    through(graph, headers, |graph, scope| {
        service::delete_feed_rule(graph, scope, &id)
    })
}

async fn get_transaction(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    through(graph, headers, |graph, scope| {
        service::get_feed_transaction(graph, scope, &id)
    })
}

/// Reconcile a transaction against the entry it turned out to be: the status and the link are one act, so one address.
async fn reconcile_transaction(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(id): Path<String>,
    RawQuery(query): RawQuery,
    body: Option<Json<Value>>,
) -> Response {
    let asked = overlay(
        query_object(query.as_deref()),
        body.map(|Json(value)| value),
    );
    through(graph, headers, |graph, scope| {
        let mut marked = asked.clone();
        marked["status"] = json!("reconciled");
        let updated = service::update_transaction_status(graph, scope, &id, &marked)?;
        let matched = text_of(&asked, "matched_entry_id");
        if matched.is_empty() {
            return Ok(updated);
        }
        service::link_transaction(
            graph,
            scope,
            &id,
            &json!({ "journal_entry_id": matched, "today": asked.get("today").cloned().unwrap_or(json!("")) }),
        )
    })
}

fn text_of(asked: &Value, key: &str) -> String {
    asked
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

async fn start_sync(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Option<Json<Value>>,
) -> Response {
    let asked = overlay(
        query_object(query.as_deref()),
        body.map(|Json(value)| value),
    );
    through(graph, headers, |graph, scope| {
        service::start_sync(graph, scope, &asked)
    })
}

async fn sync_history(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let asked = query_object(query.as_deref());
    through(graph, headers, |graph, scope| {
        service::sync_history(graph, scope, &asked)
    })
}

async fn get_sync(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    through(graph, headers, |graph, scope| {
        service::get_sync(graph, scope, &id)
    })
}

async fn webhook(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(provider): Path<String>,
    RawQuery(query): RawQuery,
    body: Option<Json<Value>>,
) -> Response {
    let asked = overlay(
        query_object(query.as_deref()),
        body.map(|Json(value)| value),
    );
    through(graph, headers, |graph, scope| {
        service::handle_webhook(graph, scope, &provider, &asked)
    })
}

async fn statistics(
    State(graph): GraphState,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let organization = organization(&Value::Null, query.as_deref());
    through(graph, headers, |graph, scope| {
        service::statistics(graph, scope, &organization)
    })
}

/// Every address bank-feed-service published, under the prefix that is part of the address.
pub fn router(graph: GraphHandle) -> Router {
    Router::new()
        .route("/bank-feed/", get(root))
        .route(
            "/bank-feed/connections",
            get(list_connections).post(post_connection),
        )
        .route(
            "/bank-feed/connections/:connection_id",
            get(get_connection).delete(delete_connection),
        )
        .route(
            "/bank-feed/connections/:connection_id/balance",
            get(connection_balance),
        )
        .route("/bank-feed/rules", get(list_rules).post(post_rule))
        .route(
            "/bank-feed/rules/:rule_id",
            put(put_rule).delete(delete_rule),
        )
        .route(
            "/bank-feed/transactions/:transaction_id",
            get(get_transaction),
        )
        .route(
            "/bank-feed/transactions/:transaction_id/reconcile",
            post(reconcile_transaction),
        )
        .route("/bank-feed/sync/start", post(start_sync))
        .route("/bank-feed/sync/history", get(sync_history))
        .route("/bank-feed/sync/:sync_id", get(get_sync))
        .route("/bank-feed/webhooks/:provider", post(webhook))
        .route("/bank-feed/statistics", get(statistics))
        .with_state(graph)
}
