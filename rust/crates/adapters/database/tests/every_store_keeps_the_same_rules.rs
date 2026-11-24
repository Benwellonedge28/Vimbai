//! One suite, every store Vimbai offers.
//!
//! A provider is only a provider if it answers these the same way. The in-memory graph, the file store and SQLite run
//! everywhere; Postgres runs wherever a server is available, which preflight provides. A test that skips is a test
//! nobody runs, so the Postgres case says out loud that it was skipped rather than passing quietly.

#![forbid(unsafe_code)]

mod conformance;

use std::path::PathBuf;

use vimbai_store::fake::MemoryGraph;
use vimbai_store::file::FileGraph;
use vimbai_store::postgres::PostgresGraph;
use vimbai_store::sqlite::SqliteGraph;

/// A file path no other test shares, in the directory the test runner gives us.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vimbai-conformance-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir.join(name)
}

#[test]
fn the_memory_graph_keeps_the_rules() {
    let graph = MemoryGraph::new();
    conformance::every_rule(&graph, "book-memory");
    conformance::edges_are_data_or_refused(&graph, "book-memory-edges")
        .expect("edges are data here");
}

#[test]
fn the_file_store_keeps_the_rules() {
    let path = scratch("conformance-file.json");
    let _ = std::fs::remove_file(&path);
    let graph = FileGraph::open(&path).expect("a file store");
    conformance::every_rule(&graph, "book-file");
    conformance::edges_are_data_or_refused(&graph, "book-file-edges").expect("edges are data here");
}

#[test]
fn sqlite_keeps_the_rules() {
    let path = scratch("conformance.db");
    let _ = std::fs::remove_file(&path);
    let graph = SqliteGraph::open(&path).expect("a sqlite store");
    conformance::every_rule(&graph, "book-sqlite");
    conformance::edges_are_data_or_refused(&graph, "book-sqlite-edges")
        .expect("edges are data here");
}

#[test]
fn postgres_keeps_the_rules() {
    let Ok(dsn) = std::env::var("VIMBAI_TEST_POSTGRES") else {
        eprintln!("postgres: skipped, VIMBAI_TEST_POSTGRES is not set");
        return;
    };
    let graph = PostgresGraph::connect(&dsn, 2).expect("a postgres store");
    let book = format!("book-postgres-{}", std::process::id());
    conformance::every_rule(&graph, &book);
    conformance::edges_are_data_or_refused(&graph, &format!("{book}-edges"))
        .expect("edges are data here");
}
