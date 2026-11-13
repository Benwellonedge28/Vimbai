#![forbid(unsafe_code)]
//! data-warehouse-service: the tables a Book describes, the queries it runs against them, and the loads that filled them.
//!
//! What this crate can honestly claim is narrower than the shape of a warehouse, and the port keeps that line visible:
//!
//! * a dimension and a fact table are DESCRIPTIONS - a name, columns, the dimensions a fact hangs off and the measures it
//!   carries - plus the count of rows the load reported. Nothing here stores business rows, so a table's row count is
//!   what the loader said it did;
//! * an aggregate query is answered against the described measures, and its results stay empty. The shipped service
//!   never computed them either; storing a query with results it cannot produce would be a lie in the record;
//! * a query names its fact table by NAME, and an unknown name is missing, not empty. That distinction is the useful one:
//!   a query that cannot run should not look like a query that found nothing;
//! * an ETL job is recorded already finished, with zero rows processed, because the shipped service simulated the run. A
//!   job is kept as the record of a load, not as work in flight;
//! * describing a table twice makes two tables. There is no uniqueness rule to break, and inventing one here would
//!   refuse the second description of a table loaded under two names.

use serde_json::{json, Value};

use vimbai_common::ids::service_id;
use vimbai_store::read::{number, refused, text};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A table describing one subject - customers, accounts, periods.
pub const DIMENSION: RecordKind = RecordKind::new("WarehouseDimension", "OWNS_DIMENSION");
/// A table of measures taken against those subjects.
pub const FACT: RecordKind = RecordKind::new("WarehouseFact", "OWNS_FACT");
/// An aggregate query that was run against a fact table.
pub const QUERY: RecordKind = RecordKind::new("WarehouseQuery", "OWNS_QUERY");
/// A load that filled something.
pub const LOAD: RecordKind = RecordKind::new("WarehouseLoad", "OWNS_WAREHOUSE_LOAD");

fn list_of(payload: &Value, key: &str) -> Value {
    payload.get(key).cloned().unwrap_or_else(|| json!([]))
}

/// `POST /dimensions`: describe a dimension table.
pub fn create_dimension(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.trim().is_empty() {
        return Err(refused("name is required"));
    }
    Records::new(graph, DIMENSION).create(
        scope,
        &json!({
            "id": if text(payload, "id").is_empty() { service_id() } else { text(payload, "id") },
            "name": name,
            "columns": list_of(payload, "columns"),
            "row_count": 0,
            "created_at": payload.get("now").cloned().unwrap_or(Value::Null),
        }),
    )
}

/// `GET /dimensions`: the dimension tables this Book described.
pub fn dimensions(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    Ok(Value::Array(
        Records::new(graph, DIMENSION).list(scope, 500)?,
    ))
}

/// `POST /facts`: describe a fact table.
pub fn create_fact(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.trim().is_empty() {
        return Err(refused("name is required"));
    }
    Records::new(graph, FACT).create(
        scope,
        &json!({
            "id": if text(payload, "id").is_empty() { service_id() } else { text(payload, "id") },
            "name": name,
            "dimensions": list_of(payload, "dimensions"),
            "measures": list_of(payload, "measures"),
            "row_count": 0,
            "created_at": payload.get("now").cloned().unwrap_or(Value::Null),
        }),
    )
}

/// `GET /facts`: the fact tables this Book described.
pub fn facts(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    Ok(Value::Array(Records::new(graph, FACT).list(scope, 500)?))
}

/// The fact table with this name, if the Book described one.
fn fact_named(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    name: &str,
) -> Result<Option<Value>, StoreError> {
    Ok(Records::new(graph, FACT)
        .list(scope, 500)?
        .into_iter()
        .find(|fact| text(fact, "name") == name))
}

/// `POST /query`: run an aggregate query against a fact table the Book described.
pub fn run_query(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let fact_table = text(payload, "fact_table");
    if fact_named(graph, scope, &fact_table)?.is_none() {
        // naming a table nobody described is a mistake, not an empty answer
        return Err(StoreError::NotFound);
    }
    Records::new(graph, QUERY).create(
        scope,
        &json!({
            "id": if text(payload, "id").is_empty() { service_id() } else { text(payload, "id") },
            "fact_table": fact_table,
            "group_by": list_of(payload, "group_by"),
            "measures": list_of(payload, "measures"),
            "filters": payload.get("filters").cloned().unwrap_or_else(|| json!({})),
            // nothing aggregates business rows yet, so the record says so instead of inventing figures
            "results": [],
            "created_at": payload.get("now").cloned().unwrap_or(Value::Null),
        }),
    )
}

/// `POST /etl`: record a load of one named thing into another, as the shipped service did - finished, no rows counted.
pub fn load(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    // a load's moment is the one fact the record exists to carry, so the clock answers when the caller does not
    let now = match payload.get("now") {
        Some(stated) if !stated.is_null() => stated.clone(),
        _ => json!(vimbai_common::clock::moment()),
    };
    Records::new(graph, LOAD).create(
        scope,
        &json!({
            "id": if text(payload, "id").is_empty() { service_id() } else { text(payload, "id") },
            "source": text(payload, "source"),
            "target": text(payload, "target"),
            "status": "completed",
            "rows_processed": 0,
            "started_at": now,
            "completed_at": now,
        }),
    )
}

/// `GET /etl`: the loads this Book recorded.
pub fn loads(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    Ok(Value::Array(Records::new(graph, LOAD).list(scope, 500)?))
}

/// The row a load reports. Exposed so a caller can see what a table claims without reading the whole record.
pub fn rows_reported(load_row: &Value) -> f64 {
    number(load_row, "rows_processed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn talked() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_dimension_and_a_fact_are_descriptions_that_start_empty() {
        let graph = MemoryGraph::new();
        let dimension = create_dimension(
            &graph,
            &talked(),
            &json!({"name": "customers", "columns": [{"name": "id", "type": "int"}], "now": 1000.0}),
        )
        .expect("a dimension");
        assert_eq!(dimension["name"], json!("customers"));
        assert_eq!(dimension["row_count"], json!(0));
        assert_eq!(dimension["columns"].as_array().map(Vec::len), Some(1));

        let fact = create_fact(
            &graph,
            &talked(),
            &json!({"name": "sales", "dimensions": ["customers"],
                    "measures": [{"name": "amount", "type": "decimal", "agg": "sum"}]}),
        )
        .expect("a fact");
        assert_eq!(fact["row_count"], json!(0));
        assert_eq!(fact["dimensions"][0], json!("customers"));

        // describing the same table twice makes two tables: there is no uniqueness rule to break
        create_dimension(&graph, &talked(), &json!({"name": "customers"})).expect("a second");
        assert_eq!(
            dimensions(&graph, &talked())
                .expect("dims")
                .as_array()
                .map(Vec::len),
            Some(2)
        );
        assert_eq!(
            facts(&graph, &talked())
                .expect("facts")
                .as_array()
                .map(Vec::len),
            Some(1)
        );

        // an unnamed table is refused rather than stored nameless
        assert!(create_dimension(&graph, &talked(), &json!({"name": "  "})).is_err());
        assert!(create_fact(&graph, &talked(), &json!({})).is_err());
    }

    #[test]
    fn a_query_needs_a_table_that_was_described_and_keeps_its_results_empty() {
        let graph = MemoryGraph::new();
        create_fact(&graph, &talked(), &json!({"name": "sales"})).expect("a fact");
        let answered = run_query(
            &graph,
            &talked(),
            &json!({"fact_table": "sales", "group_by": ["month"], "measures": ["amount"]}),
        )
        .expect("a query");
        assert_eq!(answered["fact_table"], json!("sales"));
        // no business rows are stored here, so no figures are claimed
        assert_eq!(answered["results"].as_array().map(Vec::len), Some(0));
        assert_eq!(answered["group_by"][0], json!("month"));

        // a table nobody described is missing, not empty
        let unknown = run_query(&graph, &talked(), &json!({"fact_table": "revenue"}));
        assert!(unknown.is_err());
        // and another Book's description does not answer for it
        let elsewhere = BookScope::new("user-2", "book-2");
        assert!(run_query(&graph, &elsewhere, &json!({"fact_table": "sales"})).is_err());
    }

    #[test]
    fn a_load_is_recorded_finished_with_no_rows_counted() {
        let graph = MemoryGraph::new();
        let job = load(
            &graph,
            &talked(),
            &json!({"source": "raw_sales", "target": "sales", "now": "2026-01-01T00:00:00Z"}),
        )
        .expect("a load");
        assert_eq!(job["status"], json!("completed"));
        assert_eq!(job["started_at"], job["completed_at"]);
        assert_eq!(rows_reported(&job), 0.0);
        assert_eq!(
            loads(&graph, &talked())
                .expect("loads")
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        // another Book sees none of it
        let elsewhere = BookScope::new("user-2", "book-2");
        assert_eq!(
            loads(&graph, &elsewhere)
                .expect("loads")
                .as_array()
                .map(Vec::len),
            Some(0)
        );
    }
}
