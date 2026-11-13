#![forbid(unsafe_code)]
//! The two ways a service writes a record.
//!
//! `store_record` answers with exactly what was written back. `remember` answers with the same record plus any
//! field the store left out, filled in from what was sent, because a service that echoes its own request should
//! echo the request. Both were copied into several modules; they live here once.

use serde_json::Value;

use crate::{created_record, BookScope, Graph, RecordKind, Records, StoreError};

/// Store a record and answer with it exactly as it was written back.
pub fn store_record<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    kind: RecordKind,
    body: Value,
) -> Result<Value, StoreError> {
    let saved = Records::new(graph, kind).create(scope, &body)?;
    Ok(created_record(&saved))
}

/// Store a record and answer with it, with anything the store left out filled in from what was sent.
pub fn remember<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    kind: RecordKind,
    body: Value,
) -> Result<Value, StoreError> {
    let mut answer = store_record(graph, scope, kind, body.clone())?;
    if let (Some(answer), Some(fields)) = (answer.as_object_mut(), body.as_object()) {
        for (key, value) in fields {
            answer.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    Ok(answer)
}
