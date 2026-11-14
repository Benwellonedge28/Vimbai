#![forbid(unsafe_code)]
//! The engine, serving a Book out of Postgres.
//!
//! P8.6 says a deployment keeps its data in Postgres. The store's own tests prove the store; these prove the thing an
//! operator actually runs: the router, given a Postgres store, writes a record, reads it back, keeps two Books apart, and
//! still holds it after the store is reopened. Env-var selection is a separate question and a separate test, because two
//! tests in one process reading the same variable can only race.
//!
//! They look for `VIMBAI_TEST_POSTGRES` and skip when it is absent, so a machine with no Postgres still runs the suite
//! honestly instead of failing on something it was never given.
//!
//! Each one is a plain `#[test]` that builds its own runtime, rather than a `#[tokio::test]`: the schema has to be created
//! before the runtime exists, because the Postgres client is synchronous and refuses to be driven from inside a runtime.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use store::postgres::PostgresGraph;

/// A namespace of this test's own, dropped when the guard goes.
struct Database {
    dsn: String,
    schema: String,
}

impl Database {
    fn open() -> Option<Self> {
        let base = std::env::var("VIMBAI_TEST_POSTGRES").ok()?;
        let schema = format!("vimbai_engine_{}_{}", std::process::id(), unique());
        let mut admin = postgres::Client::connect(&base, postgres::NoTls)
            .unwrap_or_else(|error| panic!("cannot reach {base}: {error}"));
        admin
            .batch_execute(&format!("CREATE SCHEMA {schema};"))
            .unwrap_or_else(|error| panic!("cannot create {schema}: {error}"));
        let dsn = format!("{base} options='-c search_path={schema}'");
        Some(Self { dsn, schema })
    }

    fn store(&self) -> Arc<PostgresGraph> {
        Arc::new(PostgresGraph::connect(&self.dsn, 4).expect("a deployment store"))
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        let base = self.dsn.split(" options=").next().unwrap().to_owned();
        if let Ok(mut admin) = postgres::Client::connect(&base, postgres::NoTls) {
            let _ = admin.batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE;", self.schema));
        }
    }
}

fn unique() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(7)
}

async fn call(
    store: vimbai_api::records::GraphHandle,
    book: &str,
    method: Method,
    uri: &str,
    payload: Option<Value>,
) -> (StatusCode, Value) {
    let router = vimbai_api::records::router(store);
    let body = match payload {
        Some(value) => Body::from(value.to_string()),
        None => Body::empty(),
    };
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-user-id", "user-1")
        .header("x-book-id", book)
        .body(body)
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn a_rule(name: &str) -> Value {
    json!({
        "name": name,
        "category": "fraud",
        "severity": "critical",
        "condition": {"type": "threshold", "field": "amount", "operator": "gt", "value": 10000},
        "cooldown_seconds": 3600,
    })
}

#[test]
fn the_engine_writes_reads_and_keeps_a_book_apart_in_postgres() {
    let Some(database) = Database::open() else {
        eprintln!("no VIMBAI_TEST_POSTGRES, so the deployment engine tests skip");
        return;
    };
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    runtime.block_on(async {
        let store = database.store();

        let (status, written) = call(
            store.clone(),
            "book-1",
            Method::POST,
            "/rules",
            Some(a_rule("Large payment")),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{written}");
        assert_eq!(written["name"], json!("Large payment"));

        let (status, listed) = call(store.clone(), "book-1", Method::GET, "/rules", None).await;
        assert_eq!(status, StatusCode::OK, "{listed}");
        assert_eq!(
            listed.as_array().map(Vec::len),
            Some(1),
            "the engine did not read back what it just wrote: {listed}"
        );
        assert_eq!(listed[0]["name"], json!("Large payment"));

        // another Book is a different set of records, which is the property the whole scope exists for
        let (status, other) = call(store.clone(), "book-2", Method::GET, "/rules", None).await;
        assert_eq!(status, StatusCode::OK, "{other}");
        assert_eq!(
            other.as_array().map(Vec::len),
            Some(0),
            "one Book saw another Book's records: {other}"
        );

        // a second store on the same database is what a restarted engine looks like
        let (status, after_restart) =
            call(database.store(), "book-1", Method::GET, "/rules", None).await;
        assert_eq!(status, StatusCode::OK, "{after_restart}");
        assert_eq!(
            after_restart.as_array().map(Vec::len),
            Some(1),
            "the record did not survive the store being reopened: {after_restart}"
        );
    });
}

#[test]
fn a_book_records_its_own_sequence_in_postgres() {
    let Some(database) = Database::open() else {
        eprintln!("no VIMBAI_TEST_POSTGRES, so the deployment engine tests skip");
        return;
    };
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    runtime.block_on(async {
        let store = database.store();

        // the high-water mark a syncing device quotes back: it has to come out of the store, not out of a process's memory
        let (status, first) = call(
            store.clone(),
            "book-9",
            Method::POST,
            "/rules",
            Some(a_rule("First")),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{first}");
        let (status, second) = call(
            store.clone(),
            "book-9",
            Method::POST,
            "/rules",
            Some(a_rule("Second")),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{second}");
        assert_ne!(
            first["id"], second["id"],
            "two records were given the same id"
        );

        let (status, listed) = call(store, "book-9", Method::GET, "/rules", None).await;
        assert_eq!(status, StatusCode::OK, "{listed}");
        let rows = listed.as_array().cloned().unwrap_or_default();
        assert_eq!(rows.len(), 2, "{listed}");
        assert_eq!(
            rows[0]["name"],
            json!("Second"),
            "a listing is newest first: {listed}"
        );
    });
}

/// Which store the engine picks when more than one is configured.
///
/// This lives in its own test because it changes a process-wide variable, and a second test reading the environment at the
/// same time could only race. Postgres is the system of record for a deployment (ADR-007), so it is chosen ahead of Neo4j
/// even when Neo4j's own variables are set - the older store stays reachable, but it does not take the deployment first.
#[test]
fn the_engine_chooses_postgres_ahead_of_neo4j() {
    let Some(database) = Database::open() else {
        eprintln!("no VIMBAI_TEST_POSTGRES, so the store-selection test skips");
        return;
    };
    std::env::set_var("VIMBAI_POSTGRES_URL", &database.dsn);
    // a Neo4j that is configured but must not be chosen: the address is one nothing listens on, so a caller reaching it
    // would fail rather than quietly succeed against the wrong store
    std::env::set_var("NEO4J_URI", "bolt://127.0.0.1:17687");

    let graph = vimbai_api::records::from_env().expect("a configured deployment store");
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let (status, listed) =
        runtime.block_on(async { call(graph, "book-1", Method::GET, "/rules", None).await });
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(
        listed.as_array().map(Vec::len),
        Some(0),
        "the engine answered from something other than the store it was told to use: {listed}"
    );

    std::env::remove_var("VIMBAI_POSTGRES_URL");
    std::env::remove_var("NEO4J_URI");
}
