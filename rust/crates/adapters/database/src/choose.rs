//! Where a deployment's records live, decided once.
//!
//! The engine, the worker and anything else that runs against a deployment all have to open the *same* store, and for a
//! while they did not: the engine preferred Postgres and the worker only knew a file or Neo4j, so a worker beside a
//! Postgres engine started with no store at all and ran nobody's rules. Two copies of "which store" is how that happens, so
//! there is one copy here and every binary asks it.
//!
//! Postgres is the deployment store (ADR-007). A deployment is not offered a choice and is not given a fallback: a host
//! with no `VIMBAI_POSTGRES_URL` is told so in words, because a store chosen by what happened to be installed is a store
//! nobody decided on. Neo4j is no longer a deployment store. A business that still runs one can lift its books out of it
//! with `vimbai-store-move`, which reads Neo4j and writes to Postgres, SQLite or a file.

use std::sync::Arc;

use crate::postgres::PostgresGraph;
use crate::{Graph, StoreError};

/// A store behind a handle, so one of them serves every caller in a process.
pub type GraphHandle = Arc<dyn Graph + Send + Sync>;

/// The store a deployment is configured for, or the reason there is none.
///
/// There is one answer and it is `VIMBAI_POSTGRES_URL`: an engine and a worker on the same host must not each pick by
/// their own reading of the environment.
pub fn deployment_from_env() -> Result<GraphHandle, StoreError> {
    if std::env::var("VIMBAI_POSTGRES_URL").is_ok() {
        return Ok(Arc::new(PostgresGraph::from_env()?) as GraphHandle);
    }
    Err(StoreError::Invalid(
        "no deployment store is configured. Set VIMBAI_POSTGRES_URL to the database this deployment's books live in. \
         Neo4j is no longer a deployment store: a business still running one lifts its books out with vimbai-store-move \
         neo4j postgres:<dsn> before the engine is pointed at the new database."
            .into(),
    ))
}

/// The name of the store [`deployment_from_env`] would open, without opening it.
///
/// Health output and logs want to say which one is in use; naming it is not the same as connecting to it.
pub fn deployment_kind_from_env() -> &'static str {
    if std::env::var("VIMBAI_POSTGRES_URL").is_ok() {
        "postgres"
    } else {
        "none"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The environment is process-wide, so a test that changes it takes a lock first.
    fn alone<T>(body: impl FnOnce() -> T) -> T {
        static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _held = ONE_AT_A_TIME.lock().expect("not poisoned");
        let postgres = std::env::var("VIMBAI_POSTGRES_URL").ok();
        let neo4j = std::env::var("NEO4J_URI").ok();
        std::env::remove_var("VIMBAI_POSTGRES_URL");
        std::env::remove_var("NEO4J_URI");
        let answer = body();
        match postgres {
            Some(dsn) => std::env::set_var("VIMBAI_POSTGRES_URL", dsn),
            None => std::env::remove_var("VIMBAI_POSTGRES_URL"),
        }
        match neo4j {
            Some(uri) => std::env::set_var("NEO4J_URI", uri),
            None => std::env::remove_var("NEO4J_URI"),
        }
        answer
    }

    #[test]
    fn a_host_with_only_neo4j_is_told_to_set_postgres_rather_than_given_neo4j() {
        alone(|| {
            // a Neo4j server being configured is not a reason to open one: the engine names the setting it wants
            std::env::set_var("NEO4J_URI", "http://127.0.0.1:1");
            // a handle is not printable, so the refusal is taken apart by hand
            let said = match deployment_from_env() {
                Ok(_) => panic!("a host with no deployment store must not be given one"),
                Err(error) => error.to_string(),
            };
            std::env::remove_var("NEO4J_URI");

            assert!(said.contains("VIMBAI_POSTGRES_URL"), "{said}");
            assert!(said.contains("no longer a deployment store"), "{said}");
            assert!(
                said.contains("vimbai-store-move"),
                "it says how to get out: {said}"
            );
            assert_eq!(deployment_kind_from_env(), "none");
        });
    }
}
