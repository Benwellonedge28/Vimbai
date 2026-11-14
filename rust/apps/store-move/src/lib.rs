#![forbid(unsafe_code)]
//! Moving a store from one database to another, as an operator runs it.
//!
//! The store is a trait so the data can live in a different database without the services noticing. This is the other
//! half of that: taking the data out of one and putting it into the other, with a comparison that says whether the move
//! was faithful before anybody points traffic at the new one.
//!
//! A store is named as `file:<path>` for the on-device JSON store, `sqlite:<path>` for the device store, `postgres:<dsn>`
//! for the deployment store, or `neo4j` for the older server store (configured by `NEO4J_URI`, `NEO4J_USER`,
//! `NEO4J_PASSWORD`, `NEO4J_DATABASE`, the same variables the engine reads).

use vimbai_store::dump::{Dump, Restored};
use vimbai_store::file::FileGraph;
use vimbai_store::neo4j_client::Neo4jGraph;
use vimbai_store::postgres::PostgresGraph;
use vimbai_store::sqlite::SqliteGraph;
use vimbai_store::{Graph, StoreError};

/// One side of a move, as the command line names it.
pub enum Store {
    File(Box<FileGraph>),
    Sqlite(Box<SqliteGraph>),
    Postgres(Box<PostgresGraph>),
    Neo4j(Box<Neo4jGraph>),
}

impl Store {
    pub fn open(name: &str) -> Result<Self, StoreError> {
        match name {
            "neo4j" => Ok(Self::Neo4j(Box::new(Neo4jGraph::from_env()?))),
            "memory" => Err(StoreError::Invalid(
                "a memory store cannot be a source or a destination: it holds nothing once the process ends"
                    .into(),
            )),
            other => match other.split_once(':') {
                Some(("file", path)) if !path.trim().is_empty() => {
                    Ok(Self::File(Box::new(FileGraph::open(path)?)))
                }
                Some(("sqlite", path)) if !path.trim().is_empty() => {
                    Ok(Self::Sqlite(Box::new(SqliteGraph::open(path)?)))
                }
                Some(("postgres", dsn)) if !dsn.trim().is_empty() => Ok(Self::Postgres(Box::new(
                    PostgresGraph::connect(dsn, 4)?,
                ))),
                _ => Err(StoreError::Invalid(format!(
                    "{other} is not a store: use file:<path>, sqlite:<path>, postgres:<dsn> or neo4j"
                ))),
            },
        }
    }

    pub fn graph(&self) -> &dyn Graph {
        match self {
            Self::File(graph) => graph.as_ref(),
            Self::Sqlite(graph) => graph.as_ref(),
            Self::Postgres(graph) => graph.as_ref(),
            Self::Neo4j(graph) => graph.as_ref(),
        }
    }
}

/// Take everything out of one store and write it, as it is, into another.
///
/// The dump is compared on arrival and the answer is returned either way, because a move that reports only success is a
/// move nobody can check. Nothing is deleted from the source: a migration that empties the old database has no way back.
pub fn move_all(
    from: &str,
    to: &str,
    limit_per_kind: usize,
    text_written_to: Option<&std::path::Path>,
) -> Result<(Restored, Vec<String>), StoreError> {
    let source = Store::open(from)?;
    let destination = Store::open(to)?;
    let taken = Dump::take(source.graph(), limit_per_kind)?;
    if let Some(path) = text_written_to {
        std::fs::write(path, taken.to_text()).map_err(|error| {
            StoreError::Graph(format!("cannot write {}: {error}", path.display()))
        })?;
    }
    let restored = taken.restore(destination.graph())?;
    let after = Dump::take(destination.graph(), limit_per_kind)?;
    Ok((restored, after.differences(&taken)))
}
