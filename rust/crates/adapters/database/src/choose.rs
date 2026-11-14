//! Where a deployment's records live, decided once.
//!
//! The engine, the worker and anything else that runs against a deployment all have to open the *same* store, and for a
//! while they did not: the engine preferred Postgres and the worker only knew a file or Neo4j, so a worker beside a
//! Postgres engine started with no store at all and ran nobody's rules. Two copies of "which store" is how that happens, so
//! there is one copy here and every binary asks it.
//!
//! The order is the deployment decision (ADR-007): Postgres is the system of record, Neo4j stays reachable for anyone still
//! running it, and a machine with neither is told so plainly instead of being given an empty store that looks like data loss.

use std::sync::Arc;

use crate::postgres::PostgresGraph;
use crate::{Graph, Neo4jGraph, StoreError};

/// A store behind a handle, so one of them serves every caller in a process.
pub type GraphHandle = Arc<dyn Graph + Send + Sync>;

/// The store a deployment is configured for, or the reason there is none.
///
/// `VIMBAI_POSTGRES_URL` wins over Neo4j, deliberately and in this one place: an engine and a worker on the same host must
/// not each pick by their own reading of the environment.
pub fn deployment_from_env() -> Result<GraphHandle, StoreError> {
    if std::env::var("VIMBAI_POSTGRES_URL").is_ok() {
        return Ok(Arc::new(PostgresGraph::from_env()?) as GraphHandle);
    }
    Ok(Arc::new(Neo4jGraph::from_env()?) as GraphHandle)
}

/// The name of the store [`deployment_from_env`] would open, without opening it.
///
/// Health output and logs want to say which one is in use; naming it is not the same as connecting to it.
pub fn deployment_kind_from_env() -> &'static str {
    if std::env::var("VIMBAI_POSTGRES_URL").is_ok() {
        "postgres"
    } else if std::env::var("NEO4J_URI").is_ok() {
        "neo4j"
    } else {
        "none"
    }
}
