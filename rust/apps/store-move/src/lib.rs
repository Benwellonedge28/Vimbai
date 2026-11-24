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

/// What a move would carry, worked out before anything is written.
///
/// A move is a decision about a business's books, so the operator is told what it will carry and whether the destination
/// is empty before it happens, rather than after. `by_kind` is sorted by how much each kind holds, because the one that
/// matters when a number looks wrong is the biggest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cutover {
    pub records: usize,
    pub edges: usize,
    pub by_kind: Vec<(String, usize)>,
    pub destination_empty: bool,
}

impl Cutover {
    pub fn to_text(&self) -> String {
        let mut text = format!(
            "{} records and {} edges in the source; the destination is {}\n",
            self.records,
            self.edges,
            if self.destination_empty {
                "empty"
            } else {
                "NOT empty"
            }
        );
        for (kind, count) in &self.by_kind {
            text.push_str(&format!("  {count:>7}  {kind}\n"));
        }
        text
    }
}

/// Count what a move would carry, and whether the destination already holds anything.
/// Refuse a destination that cannot hold what the source carries, before anything is written.
///
/// Detection is allowed to advise and never to decide: a move into a store that cannot keep relationships would arrive
/// looking like a faithful copy of a smaller Book, so the operator is told what the destination lacks rather than being
/// handed a migration that quietly dropped something.
/// Refuse a store that is only read from, as a place to write books into.
///
/// Neo4j is not a deployment store any more (ADR-007). It stays a source, so a business that still runs one can lift its
/// books out, but pointing a move at it would put a Book somewhere the engine will not open.
fn refuse_a_frozen_destination(to: &str) -> Result<(), StoreError> {
    if to == "neo4j" {
        return Err(StoreError::Invalid(
            "neo4j is not a destination: it is read from, not written to, so a Book can be lifted out of one but not \
             moved into one. Name postgres:<dsn>, sqlite:<path> or file:<path> instead."
                .into(),
        ));
    }
    Ok(())
}

fn refuse_a_destination_that_cannot_hold_it(from: &Store, to: &Store) -> Result<(), StoreError> {
    let carried = from.graph().capabilities();
    let offered = to.graph().capabilities();
    match offered.lacking(&carried) {
        Some(missing) => Err(StoreError::Invalid(format!(
            "{} cannot hold what {} carries: it has no {missing}. Move into a store that keeps it, or the books would \
             arrive missing it and report success.",
            offered.name, carried.name
        ))),
        None => Ok(()),
    }
}

pub fn cutover(from: &str, to: &str, limit_per_kind: usize) -> Result<Cutover, StoreError> {
    refuse_a_frozen_destination(to)?;
    refuse_a_destination_that_cannot_hold_it(&Store::open(from)?, &Store::open(to)?)?;
    let source = Store::open(from)?;
    let destination = Store::open(to)?;
    let taken = Dump::take(source.graph(), limit_per_kind)?;
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for entry in &taken.records {
        *counts.entry(entry.kind.clone()).or_insert(0) += 1;
    }
    let mut by_kind: Vec<(String, usize)> = counts.into_iter().collect();
    by_kind.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    let already = Dump::take(destination.graph(), 1)?;
    Ok(Cutover {
        records: taken.records.len(),
        edges: taken.edges.len(),
        by_kind,
        destination_empty: already.records.is_empty() && already.edges.is_empty(),
    })
}

/// What a move did, written down so it can be read afterwards.
///
/// A move that leaves no record of itself is a claim, not evidence. The receipt names both stores, what arrived, what did
/// not match, and whether it was a rehearsal, so the person who runs it can show what happened rather than remember it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub from: String,
    pub to: String,
    pub records: usize,
    pub edges: usize,
    pub differences: Vec<String>,
    pub rehearsed: bool,
    pub written_at: String,
}

impl Receipt {
    pub fn faithful(&self) -> bool {
        self.differences.is_empty()
    }

    pub fn to_text(&self) -> String {
        let mut text = format!(
            "move {} {}\nfrom: {}\nto: {}\nrecords: {}\nedges: {}\nverdict: {}\n",
            if self.rehearsed {
                "rehearsal"
            } else {
                "cutover"
            },
            self.written_at,
            self.from,
            self.to,
            self.records,
            self.edges,
            if self.faithful() {
                "the destination holds what the source held"
            } else {
                "the destination does NOT hold what the source held"
            }
        );
        for line in &self.differences {
            text.push_str(&format!("  {line}\n"));
        }
        text
    }

    pub fn write(&self, path: &std::path::Path) -> Result<(), StoreError> {
        std::fs::write(path, self.to_text())
            .map_err(|error| StoreError::Graph(format!("cannot write {}: {error}", path.display())))
    }
}

/// Move a store, refusing to pour it into one that already holds books.
///
/// Restoring into a store that already holds records does not merge two books, it mixes them: two Books' entries sit side
/// by side under ids that mean different things on each side, and the destination's own chain no longer reads as one
/// history. So an occupied destination is refused unless the operator says in words that they meant it.
pub fn move_guarded(
    from: &str,
    to: &str,
    limit_per_kind: usize,
    text_written_to: Option<&std::path::Path>,
    into_occupied: bool,
    rehearsed: bool,
) -> Result<Receipt, StoreError> {
    let ahead = cutover(from, to, limit_per_kind)?;
    if !ahead.destination_empty && !into_occupied {
        return Err(StoreError::Invalid(format!(
            "{to} already holds {} records and {} edges. A move into an occupied store mixes two books rather than \
             merging them, so name an empty destination, or pass --into-occupied if you mean to write into this one.",
            ahead.records, ahead.edges
        )));
    }
    let (restored, differences) = move_all(from, to, limit_per_kind, text_written_to)?;
    Ok(Receipt {
        from: from.to_string(),
        to: to.to_string(),
        records: restored.records,
        edges: restored.edges,
        differences,
        rehearsed,
        written_at: now(),
    })
}

/// The moment the receipt was written, to the second, so two moves on one day can be told apart.
fn now() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default();
    format!("{seconds}")
}
